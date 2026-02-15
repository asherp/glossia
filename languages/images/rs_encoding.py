#!/usr/bin/env python3
"""
Reed-Solomon error correction layer for parametric curve image encoding.

Wraps the parametric encoder with RS codes so we can fairly compare
against QR codes (which use RS internally).

Architecture:
    payload bytes -> RS encode -> bitstream -> pack into cells
    cells -> unpack bitstream -> RS decode -> payload bytes

Each Voronoi cell carries `bits_per_cell` bits:
    - log2(N) bits for palette word index
    - 2 * log2(M) bits for constellation position
    Total: e.g. log2(16) + 2*log2(8) = 4 + 6 = 10 bits/cell

QR code comparison:
    QR V2 (25x25) with ECC-L: 32 bytes in 625 modules (1 bit each)
    Our V+RS:                  32 bytes in ~40 cells  (10 bits each)

Usage:
    from rs_encoding import RSEncoder
    rse = RSEncoder.from_palette('palette.yaml', ecc_ratio=0.5)
    cells_lab, meta = rse.encode_bytes(b'my secret key here')
    recovered = rse.decode_bytes(cells_lab)
"""

import os
import numpy as np
from reedsolo import RSCodec, ReedSolomonError

from parametric_encoding import (
    PaletteCurve, BishopFrame, Constellation, ConstellationMap,
    compute_tube_radius, encode, decode, build_encoder,
    lab_to_srgb, srgb_to_lab, EPSILON,
)


# ---------------------------------------------------------------------------
# QR code capacity reference (Version -> modules, data bytes at each ECC)
# Source: ISO/IEC 18004, Table 7
# ---------------------------------------------------------------------------
QR_CAPACITY = {
    # version: (side, {ecc_level: data_bytes})
    1:  (21,  {'L': 17,  'M': 14,  'Q': 11,  'H': 7}),
    2:  (25,  {'L': 32,  'M': 26,  'Q': 20,  'H': 14}),
    3:  (29,  {'L': 53,  'M': 42,  'Q': 32,  'H': 24}),
    4:  (33,  {'L': 78,  'M': 62,  'Q': 46,  'H': 34}),
    5:  (37,  {'L': 106, 'M': 84,  'Q': 60,  'H': 44}),
    6:  (41,  {'L': 134, 'M': 106, 'Q': 74,  'H': 58}),
    7:  (45,  {'L': 154, 'M': 122, 'Q': 86,  'H': 64}),
    8:  (49,  {'L': 192, 'M': 152, 'Q': 108, 'H': 84}),
    9:  (53,  {'L': 230, 'M': 180, 'Q': 130, 'H': 98}),
    10: (57,  {'L': 271, 'M': 213, 'Q': 151, 'H': 119}),
}

# ECC overhead ratios for QR (approximate)
QR_ECC_OVERHEAD = {'L': 0.07, 'M': 0.15, 'Q': 0.25, 'H': 0.30}


def qr_version_for_bytes(n_bytes, ecc_level='L'):
    """Find the smallest QR version that can hold n_bytes at given ECC."""
    for ver in sorted(QR_CAPACITY.keys()):
        side, caps = QR_CAPACITY[ver]
        if caps[ecc_level] >= n_bytes:
            return ver, side, caps[ecc_level]
    return None, None, None


class RSEncoder:
    """Parametric curve encoder with Reed-Solomon error correction.

    The RS layer works over GF(256) at the byte level:
        1. Take raw payload bytes
        2. RS encode: adds `nsym` parity bytes
        3. Convert to bitstream
        4. Pack bits into cells: each cell = (word_index, constellation_pos)
        5. Encode cells into CIELAB colors via parametric curve

    Decoding reverses the process, with RS correcting up to nsym//2 byte errors.
    """

    def __init__(self, enc, nsym=None, ecc_ratio=0.5):
        """
        Args:
            enc: encoder dict from build_encoder()
            nsym: number of RS parity symbols (bytes). If None, computed
                  from ecc_ratio.
            ecc_ratio: fraction of overhead for ECC (0.5 = 50% overhead,
                       corrects ~25% of symbols). Only used if nsym is None.
        """
        self.enc = enc
        self.curve = enc['curve']
        self.frame = enc['frame']
        self.n_palette = enc['n_palette']
        self.constellation_map = enc['constellation_map']

        # Bits per cell: use M_min for uniform bit packing
        self.word_bits = int(np.log2(self.n_palette))
        self.pos_bits = int(2 * np.log2(self.constellation_map.M_min))
        self.bits_per_cell = self.word_bits + self.pos_bits

        # RS codec
        self.ecc_ratio = ecc_ratio
        self._nsym_override = nsym
        self._last_rs_total_bytes = None  # set during encode, used by decode
        self._last_nsym = None

    def _get_codec(self, payload_len):
        """Get RS codec sized for a given payload length."""
        if self._nsym_override is not None:
            nsym = self._nsym_override
        else:
            nsym = max(2, int(np.ceil(payload_len * self.ecc_ratio)))
            # Must be even for symmetric correction
            if nsym % 2 == 1:
                nsym += 1
        return RSCodec(nsym), nsym

    def encode_bytes(self, payload_bytes):
        """Encode raw bytes into CIELAB Voronoi cells with RS protection.

        Args:
            payload_bytes: bytes or bytearray

        Returns:
            pixels_lab: (N_cells, 3) CIELAB colors
            meta: dict with encoding metadata
        """
        payload_bytes = bytes(payload_bytes)
        codec, nsym = self._get_codec(len(payload_bytes))

        # RS encode: payload_bytes -> encoded_bytes (with parity)
        encoded = codec.encode(payload_bytes)
        encoded_bytes = bytes(encoded)

        # Store for decode
        self._last_rs_total_bytes = len(encoded_bytes)
        self._last_nsym = nsym

        # Convert to bitstream
        bits = []
        for byte in encoded_bytes:
            for bit in range(7, -1, -1):
                bits.append((byte >> bit) & 1)

        # Pack bits into cells
        cells = []  # list of (word_index, constellation_position)
        for i in range(0, len(bits), self.bits_per_cell):
            chunk = bits[i:i + self.bits_per_cell]
            # Pad last chunk with zeros if needed
            while len(chunk) < self.bits_per_cell:
                chunk.append(0)

            # First word_bits -> word index
            word_idx = 0
            for b in chunk[:self.word_bits]:
                word_idx = (word_idx << 1) | b

            # Remaining pos_bits -> constellation position
            pos_idx = 0
            for b in chunk[self.word_bits:]:
                pos_idx = (pos_idx << 1) | b

            # Clamp
            word_idx = min(word_idx, self.n_palette - 1)
            c = self.constellation_map[word_idx]
            pos_idx = min(pos_idx, c.capacity - 1)

            cells.append((word_idx, pos_idx))

        # Encode cells into CIELAB colors
        n_cells = len(cells)
        pixels_lab = np.zeros((n_cells, 3))
        for i, (w, j) in enumerate(cells):
            s_w = w * self.curve.arc_length / max(self.n_palette - 1, 1)
            base = self.curve.eval(s_w)
            _, U1, U2 = self.frame.eval_frame(s_w)
            c = self.constellation_map[w]
            alpha1, alpha2 = c.position_to_displacement(j)
            pixels_lab[i] = base + alpha1 * U1 + alpha2 * U2

        meta = {
            'payload_bytes': len(payload_bytes),
            'rs_parity_bytes': nsym,
            'rs_total_bytes': len(encoded_bytes),
            'total_bits': len(encoded_bytes) * 8,
            'n_cells': n_cells,
            'bits_per_cell': self.bits_per_cell,
            'max_correctable_bytes': nsym // 2,
            'max_correctable_pct': 100.0 * (nsym // 2) / len(encoded_bytes),
            'ecc_overhead_pct': 100.0 * nsym / len(payload_bytes),
            'word_bits': self.word_bits,
            'pos_bits': self.pos_bits,
        }
        return pixels_lab, meta

    def decode_bytes(self, pixels_lab):
        """Decode CIELAB Voronoi cells back to raw bytes with RS correction.

        Args:
            pixels_lab: (N_cells, 3) CIELAB colors

        Returns:
            payload_bytes: recovered bytes (or None if uncorrectable)
            meta: dict with decode metadata (errors_corrected, etc.)
        """
        pixels_lab = np.asarray(pixels_lab)
        n_cells = len(pixels_lab)

        # Decode each cell to (word_index, constellation_position)
        # Use joint (word, position) search: for each palette word, compute
        # the pixel's constellation position and residual distance, then pick
        # the (word, position) with minimum residual. This is necessary because
        # large constellation displacements can push a pixel closer to an
        # adjacent palette color's base than to its own.
        cells = []
        for i in range(n_cells):
            pixel = pixels_lab[i]
            best_w = 0
            best_j = 0
            best_residual = np.inf

            for w in range(self.n_palette):
                s_w = w * self.curve.arc_length / max(self.n_palette - 1, 1)
                base = self.curve.eval(s_w)
                _, U1, U2 = self.frame.eval_frame(s_w)
                diff = pixel - base
                alpha1 = float(np.dot(diff, U1))
                alpha2 = float(np.dot(diff, U2))

                # Snap to nearest grid point using per-color constellation
                c = self.constellation_map[w]
                j = c.displacement_to_position(alpha1, alpha2)
                j = int(j)

                # Reconstruct the snapped point and measure residual
                a1_snap, a2_snap = c.position_to_displacement(j)
                reconstructed = base + a1_snap * U1 + a2_snap * U2
                residual = np.linalg.norm(pixel - reconstructed)

                if residual < best_residual:
                    best_residual = residual
                    best_w = w
                    best_j = j

            cells.append((best_w, best_j))

        # Unpack cells to bitstream
        bits = []
        for w, j in cells:
            # Word index -> word_bits
            for bit in range(self.word_bits - 1, -1, -1):
                bits.append((w >> bit) & 1)
            # Position -> pos_bits
            for bit in range(self.pos_bits - 1, -1, -1):
                bits.append((j >> bit) & 1)

        # Convert bitstream to bytes, truncating to exactly the RS codeword
        # length (the last cell may have padding bits beyond the codeword)
        rs_total = self._last_rs_total_bytes
        nsym = self._last_nsym
        if rs_total is None or nsym is None:
            raise RuntimeError(
                "Must call encode_bytes() before decode_bytes() "
                "to establish RS parameters")

        raw_bytes = bytearray()
        for i in range(0, rs_total * 8, 8):
            byte = 0
            for bit in range(8):
                idx = i + bit
                if idx < len(bits):
                    byte = (byte << 1) | bits[idx]
                else:
                    byte = byte << 1
            raw_bytes.append(byte)

        assert len(raw_bytes) == rs_total, \
            f"Expected {rs_total} bytes, got {len(raw_bytes)}"

        # RS decode with the exact same nsym used during encode
        codec = RSCodec(nsym)
        try:
            decoded_msg, decoded_msgecc, errata_pos = codec.decode(
                bytes(raw_bytes))
            return bytes(decoded_msg), {
                'success': True,
                'errors_corrected': len(errata_pos),
                'cells_decoded': n_cells,
            }
        except ReedSolomonError as e:
            return None, {
                'success': False,
                'error': str(e),
                'cells_decoded': n_cells,
            }

    def compare_with_qr(self, n_bytes, ecc_levels=None):
        """Compare our RS-encoded Voronoi with QR codes for a given payload.

        Args:
            n_bytes: payload size in bytes

        Returns:
            dict with comparison data
        """
        if ecc_levels is None:
            ecc_levels = ['L', 'M', 'Q', 'H']

        # Our encoding
        codec, nsym = self._get_codec(n_bytes)
        rs_total = n_bytes + nsym
        n_cells = int(np.ceil(rs_total * 8 / self.bits_per_cell))

        our = {
            'payload_bytes': n_bytes,
            'payload_bits': n_bytes * 8,
            'rs_parity_bytes': nsym,
            'total_bytes': rs_total,
            'total_bits': rs_total * 8,
            'n_cells': n_cells,
            'bits_per_cell': self.bits_per_cell,
            'max_correctable_bytes': nsym // 2,
            'max_correctable_pct': 100.0 * (nsym // 2) / rs_total,
            'ecc_overhead_pct': 100.0 * nsym / n_bytes,
            'visual_elements': n_cells,
        }

        # QR comparison
        qr = {}
        for level in ecc_levels:
            ver, side, cap = qr_version_for_bytes(n_bytes, level)
            if ver is not None:
                qr[level] = {
                    'version': ver,
                    'side': side,
                    'modules': side * side,
                    'data_capacity': cap,
                    'ecc_overhead_pct': QR_ECC_OVERHEAD[level] * 100,
                    'visual_elements': side * side,
                }
            else:
                qr[level] = None

        return {'voronoi': our, 'qr': qr}

    @classmethod
    def from_palette(cls, yaml_path, palette='viridis_approx',
                     n_palette=16, nsym=None, ecc_ratio=0.5):
        """Build an RSEncoder from a palette YAML file."""
        enc = build_encoder(yaml_path, palette,
                            n_palette=n_palette)
        return cls(enc, nsym=nsym, ecc_ratio=ecc_ratio)


# ---------------------------------------------------------------------------
# Noise tolerance comparison
# ---------------------------------------------------------------------------

def noise_comparison(rse, payload_bytes, sigmas, n_trials=50,
                     brightness=0, color_temp=0, saturation=1.0):
    """Run noise sweep comparing RS-decoded accuracy vs raw accuracy.

    Returns list of dicts with results per sigma.
    """
    from app import perturb_lab

    payload = bytes(payload_bytes)
    cells_lab, encode_meta = rse.encode_bytes(payload)

    # Also encode as raw words for comparison (no RS)
    raw_words = []
    for i in range(len(cells_lab)):
        pixel = cells_lab[i]
        best_w = 0
        best_dist = np.inf
        for w in range(rse.n_palette):
            s_w = w * rse.curve.arc_length / max(rse.n_palette - 1, 1)
            base = rse.curve.eval(s_w)
            d = np.linalg.norm(pixel - base)
            if d < best_dist:
                best_dist = d
                best_w = w
        raw_words.append(best_w)

    results = []
    for sigma in sigmas:
        rs_successes = 0
        raw_word_accs = []

        for trial in range(n_trials):
            rng = np.random.RandomState(trial)
            perturbed = perturb_lab(cells_lab, noise_sigma=sigma,
                                   brightness=brightness,
                                   color_temp=color_temp,
                                   saturation=saturation, rng=rng)

            # RS decode
            recovered, meta = rse.decode_bytes(perturbed)
            if recovered is not None and recovered == payload:
                rs_successes += 1

            # Raw word accuracy (no RS)
            raw_decoded = decode(
                perturbed, rse.curve, rse.frame,
                rse.n_palette,
                constellation_map=rse.constellation_map
            )
            correct = sum(1 for a, b in zip(raw_words, raw_decoded) if a == b)
            raw_word_accs.append(100.0 * correct / len(raw_words))

        results.append({
            'sigma': sigma,
            'rs_success_rate': 100.0 * rs_successes / n_trials,
            'raw_word_accuracy_mean': np.mean(raw_word_accs),
            'raw_word_accuracy_p5': np.percentile(raw_word_accs, 5),
        })

    return results, encode_meta


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

if __name__ == '__main__':
    import sys

    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')

    # Demo: encode a 32-byte key (Nostr nsec equivalent)
    key = bytes(range(32))  # dummy 32-byte key
    print(f"Payload: {len(key)} bytes ({len(key)*8} bits)")
    print()

    for ecc_ratio in [0.3, 0.5, 0.75, 1.0]:
        rse = RSEncoder.from_palette(yaml_path, ecc_ratio=ecc_ratio)
        comparison = rse.compare_with_qr(len(key))
        v = comparison['voronoi']

        print(f"--- ECC ratio {ecc_ratio:.0%} ---")
        print(f"  RS parity: {v['rs_parity_bytes']} bytes")
        print(f"  Corrects up to: {v['max_correctable_bytes']} byte errors "
              f"({v['max_correctable_pct']:.1f}%)")
        print(f"  Voronoi cells: {v['n_cells']} "
              f"({v['bits_per_cell']} bits each)")

        for level in ['L', 'M', 'Q', 'H']:
            qr = comparison['qr'][level]
            if qr:
                ratio = qr['visual_elements'] / v['visual_elements']
                print(f"  vs QR-{level} V{qr['version']} ({qr['side']}x{qr['side']}): "
                      f"{qr['visual_elements']} modules -> {ratio:.0f}x more elements")
        print()

    # Round-trip test
    print("=== Round-trip test ===")
    rse = RSEncoder.from_palette(yaml_path, ecc_ratio=0.5)
    cells_lab, meta = rse.encode_bytes(key)
    recovered, dec_meta = rse.decode_bytes(cells_lab)
    print(f"Cells: {meta['n_cells']}, "
          f"RS parity: {meta['rs_parity_bytes']} bytes")
    print(f"Round-trip: {'PASS' if recovered == key else 'FAIL'}")
    print(f"Errors corrected: {dec_meta.get('errors_corrected', 0)}")
