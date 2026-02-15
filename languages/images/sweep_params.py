#!/usr/bin/env python3
"""
Parameter sweep for 32-byte Nostr pubkey encoding.

Sweeps (N, epsilon, ECC ratio, img_size) and computes:
  - Number of Voronoi cells needed
  - Pixels per cell (= img_size^2 / n_cells)
  - Noise reduction factor sqrt(px/cell)
  - sigma95 per single pixel (epsilon / 5.6)
  - sigma95 effective (with cell averaging)
  - Comparison against QR codes

Usage:
    python sweep_params.py
"""

import os
import sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))

from parametric_encoding import build_encoder, ConstellationMap, EPSILON
from rs_encoding import RSEncoder, qr_version_for_bytes

YAML_PATH = os.path.join(os.path.dirname(__file__), 'palette.yaml')
PAYLOAD_BYTES = 32  # Nostr pubkey

# QR reference for 32 bytes
QR_REF = {}
for level in ['L', 'M', 'Q', 'H']:
    ver, side, cap = qr_version_for_bytes(PAYLOAD_BYTES, level)
    if ver:
        QR_REF[level] = {'version': ver, 'side': side, 'modules': side * side}


def sweep():
    """Sweep parameter space and return results sorted by effective sigma95."""

    # Parameter ranges
    N_values = [4, 8, 16]
    epsilon_values = [2.3, 5.0, 8.0, 10.0, 15.0, 20.0, 25.0, 30.0]
    ecc_ratios = [0.5, 1.0]
    img_sizes = [100, 200, 400, 800]

    results = []

    for N in N_values:
        # Build encoder once per N (curve/frame are epsilon-independent)
        enc = build_encoder(YAML_PATH, 'viridis_approx', n_palette=N)
        _, radii = enc['tube_radii']

        for eps in epsilon_values:
            # Build constellation map with this epsilon
            cmap = ConstellationMap(radii, epsilon=eps)

            if cmap.M_min < 2:
                continue  # Too few constellation positions, skip

            # Bits per cell (uniform packing uses M_min)
            word_bits = int(np.log2(N))
            pos_bits = int(2 * np.log2(cmap.M_min))
            bits_per_cell = word_bits + pos_bits

            if bits_per_cell < 2:
                continue

            for ecc_ratio in ecc_ratios:
                # RS overhead
                nsym = max(2, int(np.ceil(PAYLOAD_BYTES * ecc_ratio)))
                if nsym % 2 == 1:
                    nsym += 1
                rs_total_bytes = PAYLOAD_BYTES + nsym
                rs_total_bits = rs_total_bytes * 8
                n_cells = int(np.ceil(rs_total_bits / bits_per_cell))

                max_correctable = nsym // 2
                max_correctable_pct = 100.0 * max_correctable / rs_total_bytes

                # sigma95 per single pixel
                sigma95_single = eps / 5.6

                for img_size in img_sizes:
                    total_pixels = img_size * img_size
                    px_per_cell = total_pixels / n_cells
                    noise_reduction = np.sqrt(px_per_cell)
                    sigma95_eff = sigma95_single * noise_reduction

                    results.append({
                        'N': N,
                        'epsilon': eps,
                        'ecc_ratio': ecc_ratio,
                        'img_size': img_size,
                        'M_min': cmap.M_min,
                        'M_max': cmap.M_max,
                        'bits_per_cell': bits_per_cell,
                        'n_cells': n_cells,
                        'px_per_cell': int(px_per_cell),
                        'noise_reduction': noise_reduction,
                        'sigma95_single': sigma95_single,
                        'sigma95_eff': sigma95_eff,
                        'rs_parity': nsym,
                        'max_correctable_pct': max_correctable_pct,
                    })

    return results


def print_results(results):
    """Print formatted results table sorted by effective sigma95."""

    # Sort by effective sigma95 descending
    results.sort(key=lambda r: -r['sigma95_eff'])

    # QR reference
    print("=" * 110)
    print("QR CODE REFERENCE (32-byte payload)")
    print("=" * 110)
    for level in ['L', 'M', 'Q', 'H']:
        ref = QR_REF.get(level)
        if ref:
            # QR: epsilon ~ 50 (L* threshold), sigma ~ 0.5 (B/W is trivial)
            # But QR is really about structural damage, not noise.
            # At 4 px/module on a phone screen:
            px_mod = 4
            qr_noise_red = np.sqrt(px_mod)
            print(f"  QR-{level}: V{ref['version']} ({ref['side']}x{ref['side']}) "
                  f"= {ref['modules']} modules   "
                  f"[at {px_mod} px/mod -> noise_red = x{qr_noise_red:.1f}, "
                  f"but ε_BW ~ 50 so σ95 ~ {50/5.6 * qr_noise_red:.0f}]")
    print()

    # Header
    print("=" * 110)
    print(f"{'N':>3} {'ε':>5} {'ECC':>5} {'img':>5} "
          f"{'M_min':>5} {'b/cell':>6} {'cells':>5} {'px/cell':>8} "
          f"{'×√px':>6} {'σ95_1px':>7} {'σ95_eff':>8} "
          f"{'vs QR-L':>8}")
    print("-" * 110)

    qr_l_modules = QR_REF.get('L', {}).get('modules', 625)

    for r in results[:60]:  # Top 60 configs
        ratio_vs_qr = qr_l_modules / r['n_cells']
        print(f"{r['N']:>3} {r['epsilon']:>5.1f} {r['ecc_ratio']:>5.0%} "
              f"{r['img_size']:>5} "
              f"{r['M_min']:>5} {r['bits_per_cell']:>6} "
              f"{r['n_cells']:>5} {r['px_per_cell']:>8} "
              f"{r['noise_reduction']:>6.0f} "
              f"{r['sigma95_single']:>7.1f} "
              f"{r['sigma95_eff']:>8.1f} "
              f"{ratio_vs_qr:>7.0f}×")

    print()
    print("=" * 110)
    print("KEY:")
    print("  σ95_1px  = ε / 5.6        (noise tolerance per single pixel)")
    print("  σ95_eff  = σ95_1px × √(px/cell)  (with spatial averaging)")
    print("  vs QR-L  = QR-L modules / our cells  (element ratio)")
    print()

    # Summary: best config per image size
    print("=" * 110)
    print("BEST CONFIG PER IMAGE SIZE (highest σ95_eff)")
    print("=" * 110)
    seen = set()
    for r in results:
        key = r['img_size']
        if key not in seen:
            seen.add(key)
            ratio_vs_qr = qr_l_modules / r['n_cells']
            print(f"  {r['img_size']}x{r['img_size']}: "
                  f"N={r['N']}, ε={r['epsilon']:.1f}, "
                  f"ECC={r['ecc_ratio']:.0%}, "
                  f"{r['n_cells']} cells, "
                  f"{r['px_per_cell']} px/cell, "
                  f"σ95_eff={r['sigma95_eff']:.1f}, "
                  f"{ratio_vs_qr:.0f}× fewer than QR-L")

    # Also show: best "practical" configs (sigma95_eff > 10, fewest cells)
    print()
    print("=" * 110)
    print("PRACTICAL CONFIGS (σ95_eff > 10, sorted by fewest cells)")
    print("=" * 110)
    practical = [r for r in results if r['sigma95_eff'] > 10]
    practical.sort(key=lambda r: r['n_cells'])
    seen_configs = set()
    for r in practical[:30]:
        config_key = (r['N'], r['epsilon'], r['ecc_ratio'], r['n_cells'])
        if config_key in seen_configs:
            continue
        seen_configs.add(config_key)
        ratio_vs_qr = qr_l_modules / r['n_cells']
        img_options = [r2['img_size'] for r2 in practical
                       if (r2['N'], r2['epsilon'], r2['ecc_ratio']) ==
                          (r['N'], r['epsilon'], r['ecc_ratio'])]
        print(f"  N={r['N']:>2}, ε={r['epsilon']:>5.1f}, "
              f"ECC={r['ecc_ratio']:>4.0%} -> "
              f"{r['n_cells']:>3} cells, "
              f"{r['bits_per_cell']:>2} b/cell, "
              f"{ratio_vs_qr:>5.0f}× vs QR-L  "
              f"[img sizes: {', '.join(str(s) for s in sorted(img_options))}]")


if __name__ == '__main__':
    print("Sweeping parameter space for 32-byte Nostr pubkey...")
    print(f"Payload: {PAYLOAD_BYTES} bytes ({PAYLOAD_BYTES * 8} bits)")
    print()
    results = sweep()
    print(f"Generated {len(results)} configurations\n")
    print_results(results)
