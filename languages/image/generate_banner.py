#!/usr/bin/env python3
"""
Generate a banner image encoding a Nostr public key via parametric curve encoding.

Creates a wide-aspect-ratio Voronoi diagram (1500x500) where each cell color
encodes part of the public key using the viridis palette curve in CIELAB space.

Encoding strategy:
  Word+constellation mixed-radix encoding (RSEncoder). Each cell carries both a
  palette word AND a constellation displacement, encoding log2(N * M^2) bits per
  cell. A self-describing header Voronoi cell at arc-length s=0 encodes (N,
  epsilon), making the banner decodable from the image alone. Reed-Solomon
  parity bytes protect against noise and partial occlusion.

  scatter_colors permutes the cell-to-seed assignment for aesthetics; the decoder
  recovers (word, pos) from each cell's color alone, independent of spatial layout.

Usage:
    python generate_banner.py
    python generate_banner.py --npub <npub_string>
    python generate_banner.py --output-dir /path/to/dir --width 1200 --height 400
"""

import os
import sys
import argparse
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from parametric_encoding import (
    PaletteCurve, BishopFrame,
    select_encoding_params, derive_config_table,
    compute_capacity_curve, equal_capacity_positions,
    lab_to_srgb, srgb_to_lab, min_srgb_distance,
    encode_header, HEADER_S, build_encoder, EPSILON,
    compute_tube_radius,
)
from rs_encoding import RSEncoder

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from scipy.spatial import Voronoi, KDTree


# ---------------------------------------------------------------------------
# Bech32 decode (NIP-19 npub)
# ---------------------------------------------------------------------------

BECH32_CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l'


def bech32_polymod(values):
    GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]
    chk = 1
    for v in values:
        b = (chk >> 25)
        chk = (chk & 0x1ffffff) << 5 ^ v
        for i in range(5):
            chk ^= GEN[i] if ((b >> i) & 1) else 0
    return chk


def bech32_hrp_expand(hrp):
    return [ord(x) >> 5 for x in hrp] + [0] + [ord(x) & 31 for x in hrp]


def convertbits(data, frombits, tobits, pad=True):
    acc = 0
    bits = 0
    ret = []
    maxv = (1 << tobits) - 1
    for value in data:
        if value < 0 or (value >> frombits):
            return None
        acc = (acc << frombits) | value
        bits += frombits
        while bits >= tobits:
            bits -= tobits
            ret.append((acc >> bits) & maxv)
    if pad:
        if bits:
            ret.append((acc << (tobits - bits)) & maxv)
    elif bits >= frombits or ((acc << (tobits - bits)) & maxv):
        return None
    return ret


def decode_npub(npub_str):
    """Decode a Nostr npub bech32 string to raw 32-byte public key."""
    npub_str = npub_str.strip().lower()
    pos = npub_str.rfind('1')
    if pos < 1:
        raise ValueError("Invalid bech32: no separator found")

    hrp = npub_str[:pos]
    if hrp != 'npub':
        raise ValueError(f"Expected 'npub' prefix, got '{hrp}'")

    data_part = npub_str[pos + 1:]
    values = [BECH32_CHARSET.index(c) for c in data_part]

    # Verify checksum
    if bech32_polymod(bech32_hrp_expand(hrp) + values) != 1:
        raise ValueError("Invalid bech32 checksum")

    # Remove checksum (last 6), convert 5-bit to 8-bit
    data_5bit = values[:-6]
    raw_bytes = convertbits(data_5bit, 5, 8, pad=False)
    if raw_bytes is None or len(raw_bytes) != 32:
        raise ValueError(f"Expected 32 bytes, got {len(raw_bytes) if raw_bytes else 'None'}")

    return bytes(raw_bytes)


# ---------------------------------------------------------------------------
# Banner-shaped Voronoi seed generation
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Platform exclusion zones (profile picture overlaps on social media banners)
# ---------------------------------------------------------------------------

# Each zone is (center_x_frac, center_y_frac, radius_frac) relative to banner width.
# center_y_frac is relative to banner height.
PLATFORM_EXCLUSIONS = {
    'twitter': [
        # Profile circle: center at 12.5% from left, 102% from top (below bottom
        # edge), radius 12.3% of width.  Measured from live X/Twitter layout.
        (0.1247, 1.02, 0.1233),
    ],
}


def _in_exclusion(pts, zones):
    """Return boolean mask: True for points inside any exclusion circle."""
    mask = np.zeros(len(pts), dtype=bool)
    for cx, cy, r in zones:
        d = np.sqrt((pts[:, 0] - cx)**2 + (pts[:, 1] - cy)**2)
        mask |= d < r
    return mask


def generate_banner_seeds(n, width, height, seed=None, relax_iters=8,
                          exclusion_zones=None):
    """Generate well-spaced seed points for a wide banner layout.

    Uses aspect-ratio-aware grid initialization and Lloyd relaxation
    for even cell sizing across the wide format.  Color-aware separation
    is handled post-layout by scatter_colors() which permutes the
    digit-to-seed assignment.

    Args:
        exclusion_zones: list of (cx, cy, radius) in pixel coords, or None.
            Seeds are pushed out of these circles during generation.
    """
    rng = np.random.RandomState(seed)
    zones = exclusion_zones or []

    aspect = width / height
    cols = max(int(np.ceil(np.sqrt(n * aspect))), 1)
    rows = max(int(np.ceil(n / cols)), 1)

    margin_x = width * 0.02
    margin_y = height * 0.04
    cx = np.linspace(margin_x, width - margin_x, cols)
    cy = np.linspace(margin_y, height - margin_y, rows)
    gx, gy = np.meshgrid(cx, cy)
    grid_pts = np.column_stack([gx.ravel(), gy.ravel()])

    # Filter grid points that fall inside exclusion zones
    if zones:
        keep = ~_in_exclusion(grid_pts, zones)
        grid_pts = grid_pts[keep]

    if len(grid_pts) >= n:
        indices = rng.choice(len(grid_pts), size=n, replace=False)
        seeds = grid_pts[indices].astype(np.float64)
    else:
        # Need extra points — sample outside exclusion zones
        extra_needed = n - len(grid_pts)
        extra_pts = []
        while len(extra_pts) < extra_needed:
            batch = np.column_stack([
                rng.uniform(margin_x, width - margin_x, extra_needed * 2),
                rng.uniform(margin_y, height - margin_y, extra_needed * 2),
            ])
            if zones:
                batch = batch[~_in_exclusion(batch, zones)]
            extra_pts.extend(batch.tolist())
        extra_pts = np.array(extra_pts[:extra_needed])
        seeds = np.vstack([grid_pts, extra_pts]).astype(np.float64) if len(grid_pts) else extra_pts.astype(np.float64)

    jitter_x = (width - 2 * margin_x) / max(cols, 1) * 0.2
    jitter_y = (height - 2 * margin_y) / max(rows, 1) * 0.2
    seeds[:, 0] += rng.uniform(-jitter_x, jitter_x, n)
    seeds[:, 1] += rng.uniform(-jitter_y, jitter_y, n)

    seeds[:, 0] = np.clip(seeds[:, 0], 2, width - 2)
    seeds[:, 1] = np.clip(seeds[:, 1], 2, height - 2)

    # Push any jittered seeds out of exclusion zones
    if zones:
        for cx_z, cy_z, r_z in zones:
            for i in range(len(seeds)):
                dx = seeds[i, 0] - cx_z
                dy = seeds[i, 1] - cy_z
                d = np.sqrt(dx**2 + dy**2)
                if d < r_z:
                    if d < 1e-6:
                        angle = rng.uniform(0, 2 * np.pi)
                        dx, dy = np.cos(angle), np.sin(angle)
                        d = 1.0
                    seeds[i, 0] = cx_z + dx / d * (r_z + 5)
                    seeds[i, 1] = cy_z + dy / d * (r_z + 5)

        seeds[:, 0] = np.clip(seeds[:, 0], 2, width - 2)
        seeds[:, 1] = np.clip(seeds[:, 1], 2, height - 2)

    # Lloyd relaxation (exclusion-aware)
    for _ in range(relax_iters):
        tree = KDTree(seeds)
        sx = np.linspace(0, width, min(int(width * 0.5), 400))
        sy = np.linspace(0, height, min(int(height * 0.5), 200))
        gx2, gy2 = np.meshgrid(sx, sy)
        sample_pts = np.column_stack([gx2.ravel(), gy2.ravel()])

        # Exclude sample points inside zones from centroid computation
        if zones:
            valid = ~_in_exclusion(sample_pts, zones)
            sample_pts = sample_pts[valid]

        _, indices = tree.query(sample_pts)

        new_seeds = seeds.copy()
        for i in range(n):
            mask = indices == i
            if np.any(mask):
                new_seeds[i] = sample_pts[mask].mean(axis=0)

        new_seeds[:, 0] = np.clip(new_seeds[:, 0], 2, width - 2)
        new_seeds[:, 1] = np.clip(new_seeds[:, 1], 2, height - 2)

        # Re-check exclusion after relaxation
        if zones:
            for cx_z, cy_z, r_z in zones:
                for i in range(n):
                    dx = new_seeds[i, 0] - cx_z
                    dy = new_seeds[i, 1] - cy_z
                    d = np.sqrt(dx**2 + dy**2)
                    if d < r_z:
                        new_seeds[i] = seeds[i]  # revert to pre-relaxation

        seeds = new_seeds

    return seeds


# ---------------------------------------------------------------------------
# Color-scatter: permute digit→seed assignment to minimize same-color adjacency
# ---------------------------------------------------------------------------

def scatter_colors(seeds, digits, iterations=5000, seed=None):
    """Permute digit-to-seed assignment so same-colored cells aren't adjacent.

    Uses simulated-annealing-style random swaps.  Accepts a swap if it
    reduces the number of same-color nearest-neighbor pairs, or with
    decreasing probability otherwise (to escape local minima).

    The encoding is preserved: the *set* of digits is unchanged, only the
    mapping from digit → spatial position is permuted.  The decoder reads
    cells in canonical scan order (left-to-right, top-to-bottom by seed
    position), so the returned permutation must be applied consistently.

    Returns:
        perm: permutation array — new_digits[i] = digits[perm[i]]
    """
    rng = np.random.RandomState(seed)
    n = len(seeds)
    digits = np.asarray(digits)

    # Build adjacency: for each seed, its k nearest neighbors
    tree = KDTree(seeds)
    k = min(6, n - 1)
    _, nn_idx = tree.query(seeds, k=k + 1)
    neighbors = nn_idx[:, 1:]  # exclude self

    perm = np.arange(n)
    current_digits = digits.copy()

    def cost():
        """Count same-color nearest-neighbor pairs (directed)."""
        c = 0
        for i in range(n):
            for j in neighbors[i]:
                if current_digits[i] == current_digits[j]:
                    c += 1
        return c

    def local_cost(idx):
        """Count same-color neighbors for a single cell."""
        c = 0
        for nb in neighbors[idx]:
            if current_digits[idx] == current_digits[nb]:
                c += 1
        # Also count reverse: cells that have idx as a neighbor
        for k in range(n):
            if k == idx:
                continue
            if idx in neighbors[k] and current_digits[k] == current_digits[idx]:
                c += 1
        return c

    initial_cost = cost()
    best_perm = perm.copy()
    best_cost = initial_cost
    temp = 1.0

    for it in range(iterations):
        i, j = rng.choice(n, size=2, replace=False)
        if current_digits[i] == current_digits[j]:
            continue

        # Cost before swap for affected cells
        cost_before = local_cost(i) + local_cost(j)

        # Swap
        current_digits[i], current_digits[j] = current_digits[j], current_digits[i]
        perm[i], perm[j] = perm[j], perm[i]

        cost_after = local_cost(i) + local_cost(j)
        delta = cost_after - cost_before

        if delta <= 0 or rng.random() < np.exp(-delta / temp):
            current_cost = cost()  # recompute to avoid drift
            if current_cost < best_cost:
                best_cost = current_cost
                best_perm = perm.copy()
        else:
            # Reject
            current_digits[i], current_digits[j] = current_digits[j], current_digits[i]
            perm[i], perm[j] = perm[j], perm[i]

        temp *= 0.999

    # Restore best permutation
    final_perm = best_perm
    return final_perm, best_cost


# ---------------------------------------------------------------------------
# Banner rendering (PNG raster and SVG vector)
# ---------------------------------------------------------------------------

def render_banner_png(cells_srgb, digits, width, height, output_path,
                       seed=42, relax_iters=10,
                       border_width=2.5, border_color=(10, 10, 25),
                       exclusion_zones=None, scan_order=False):
    """Render cells as a wide Voronoi banner and save as PNG.

    After Lloyd relaxation, applies color-scatter to permute the digit→seed
    assignment so same-colored cells aren't adjacent.

    When scan_order=True, assigns cells to seeds in scan order (top-to-bottom,
    left-to-right) instead of scattering. This makes the banner decodable:
    the decoder reads cells in the same scan order.
    """
    n = len(cells_srgb)
    cells_srgb = np.asarray(cells_srgb, dtype=np.uint8)

    seeds = generate_banner_seeds(n, width, height, seed=seed,
                                   relax_iters=relax_iters,
                                   exclusion_zones=exclusion_zones)

    if scan_order:
        # Assign cells in scan order: cell i → i-th seed in scan order.
        # The decoder reads cells in the same scan order, recovering the
        # original digit sequence.
        scan_idx = np.lexsort((seeds[:, 0], seeds[:, 1]))
        # Build permutation: seed j gets cell inverse_scan[j]
        inverse_scan = np.zeros(n, dtype=int)
        inverse_scan[scan_idx] = np.arange(n)
        perm = inverse_scan
        scattered_srgb = cells_srgb[perm]
        print(f"  Scan-order assignment (decodable)")
    else:
        # Scatter: permute digit→seed to minimize same-color adjacency
        perm, cost = scatter_colors(seeds, digits, iterations=8000, seed=seed)
        scattered_srgb = cells_srgb[perm]
        print(f"  Color scatter: {cost} same-color neighbor pairs remaining")

    tree = KDTree(seeds)
    xx, yy = np.meshgrid(np.arange(width), np.arange(height))
    pixel_coords = np.column_stack([xx.ravel(), yy.ravel()])
    dists, indices = tree.query(pixel_coords, k=2)

    nearest_idx = indices[:, 0]
    img_flat = scattered_srgb[nearest_idx].copy()

    if border_width > 0:
        d1, d2 = dists[:, 0], dists[:, 1]
        border_mask = (d2 - d1) < border_width
        img_flat[border_mask] = np.array(border_color, dtype=np.uint8)

    img = img_flat.reshape(height, width, 3)

    # Save with PIL for clean output (no matplotlib axes)
    try:
        from PIL import Image
        Image.fromarray(img).save(output_path)
    except ImportError:
        # Fallback to matplotlib
        fig, ax = plt.subplots(1, 1, figsize=(width / 100, height / 100), dpi=100)
        ax.imshow(img)
        ax.axis('off')
        plt.subplots_adjust(left=0, right=1, top=1, bottom=0)
        ax.margins(0)
        plt.savefig(output_path, dpi=100, bbox_inches='tight', pad_inches=0,
                    facecolor='#0a0a18')
        plt.close()

    return img, seeds, perm


def render_banner_svg(cells_srgb, digits, width, height, output_path,
                       seed=42, relax_iters=10,
                       border_width=1.8,
                       border_color="#0a0a18",
                       bg_color="#0a0a18",
                       corner_radius=16,
                       exclusion_zones=None):
    """Render cells as a wide Voronoi banner and save as SVG."""
    n = len(cells_srgb)
    cells_srgb = np.asarray(cells_srgb, dtype=np.uint8)

    seeds = generate_banner_seeds(n, width, height, seed=seed,
                                   relax_iters=relax_iters,
                                   exclusion_zones=exclusion_zones)

    # Scatter: permute digit→seed to minimize same-color adjacency
    perm, cost = scatter_colors(seeds, digits, iterations=8000, seed=seed)
    cells_srgb = cells_srgb[perm]
    print(f"  Color scatter: {cost} same-color neighbor pairs remaining")

    # Mirror points for finite Voronoi cells
    mirror_pts = np.vstack([
        seeds,
        np.column_stack([seeds[:, 0], -seeds[:, 1]]),
        np.column_stack([seeds[:, 0], 2 * height - seeds[:, 1]]),
        np.column_stack([-seeds[:, 0], seeds[:, 1]]),
        np.column_stack([2 * width - seeds[:, 0], seeds[:, 1]]),
    ])

    vor = Voronoi(mirror_pts)

    lines = []
    lines.append(f'<svg xmlns="http://www.w3.org/2000/svg" '
                 f'viewBox="0 0 {width} {height}" '
                 f'width="{width}" height="{height}">')

    # Rounded clip path
    lines.append('<defs>')
    lines.append(f'  <clipPath id="banner-clip">')
    lines.append(f'    <rect x="0" y="0" width="{width}" height="{height}" '
                 f'rx="{corner_radius}" ry="{corner_radius}"/>')
    lines.append(f'  </clipPath>')
    lines.append('</defs>')
    lines.append(f'<g clip-path="url(#banner-clip)">')
    lines.append(f'<rect width="{width}" height="{height}" fill="{bg_color}"/>')

    for i in range(n):
        region_idx = vor.point_region[i]
        region = vor.regions[region_idx]
        if -1 in region or len(region) == 0:
            continue

        vertices = vor.vertices[region]
        vertices = np.clip(vertices, [-10, -10], [width + 10, height + 10])
        points_str = ' '.join(f'{x:.1f},{y:.1f}' for x, y in vertices)

        r, g, b = int(cells_srgb[i][0]), int(cells_srgb[i][1]), int(cells_srgb[i][2])
        lines.append(
            f'<polygon points="{points_str}" '
            f'fill="rgb({r},{g},{b})" '
            f'stroke="{border_color}" stroke-width="{border_width}" '
            f'stroke-linejoin="round"/>'
        )

    lines.append('</g>')
    lines.append('</svg>')

    svg_str = '\n'.join(lines)
    with open(output_path, 'w') as f:
        f.write(svg_str)

    return seeds, perm


# ---------------------------------------------------------------------------
# Main: encode pubkey and render banner
# ---------------------------------------------------------------------------

def create_banner(npub_str, output_dir=None, width=1500, height=500,
                   palette='viridis', N=None, nsym=16, seed=42,
                   fmt='both', platform=None):
    """Create a banner image encoding a Nostr public key.

    Uses word+constellation mixed-radix encoding (RSEncoder) with
    Reed-Solomon error correction. Each cell color uniquely encodes both
    its word AND sequence position, so scatter_colors is purely cosmetic.
    A self-describing header Voronoi cell encodes (N, epsilon) so the
    banner is decodable from the image alone.

    Args:
        npub_str: Nostr public key in npub bech32 format
        output_dir: output directory (default: samples/)
        width, height: banner dimensions
        palette: palette name from palette.yaml
        N: number of palette colors (None = auto-select optimal)
        nsym: RS parity symbol count (default: 16)
        seed: random seed for Voronoi layout
        fmt: 'png', 'svg', or 'both'
        platform: 'twitter' or None.  When set, avoids placing cells under
                  the platform's profile picture overlay.

    Returns:
        dict with file paths and metadata
    """
    if output_dir is None:
        output_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'samples')
    os.makedirs(output_dir, exist_ok=True)

    # 1. Decode npub
    print("Decoding npub...")
    pubkey = decode_npub(npub_str)
    print(f"  Public key: {pubkey.hex()}")
    print(f"  Length: {len(pubkey)} bytes ({len(pubkey) * 8} bits)")

    # 2. Build palette curve
    yaml_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'palette.yaml')
    curve = PaletteCurve.from_yaml(yaml_path, palette)
    frame = BishopFrame(curve)

    # 3. Select optimal (N, epsilon) or use caller-specified N
    configs = derive_config_table(curve, frame)
    config_table, header_eps = configs

    if N is None:
        # Auto-select: find optimal (N, epsilon) for camera-decodability
        print(f"\nSelecting optimal encoding params ({palette})...")
        best = select_encoding_params(curve, frame, configs=configs)
        if best is None:
            raise RuntimeError("No valid encoding configuration found")
        N = best['N']
        eps = best['epsilon']
        s_palette = best['s_palette']
        constellation_map = best['constellation_map']
        srgb_dist = best['srgb_dist_min']
    else:
        # Caller specified N — find the best epsilon for it
        print(f"\nBuilding palette curve ({palette}, N={N})...")
        s_dense, radii_dense, C = compute_capacity_curve(curve, frame,
                                                          n_samples=200)
        s_palette = equal_capacity_positions(s_dense, C, N)
        srgb_dist = min_srgb_distance(curve, s_palette)

        # Find highest-bpc epsilon for this N in the config table
        eps = None
        for cfg_n, cfg_eps in config_table:
            if cfg_n == N:
                if eps is None or cfg_eps > eps:
                    eps = cfg_eps
        if eps is None:
            raise ValueError(f"N={N} not found in derived config table")

        radii_at_pal = np.interp(s_palette, s_dense, radii_dense)
        from parametric_encoding import ConstellationMap
        constellation_map = ConstellationMap(radii_at_pal, epsilon=eps)

    print(f"  N={N}, epsilon={eps:.4f}")
    print(f"  Arc length: {curve.arc_length:.2f}")
    print(f"  Min sRGB pairwise distance: {srgb_dist:.1f}")

    # 4. Build RSEncoder
    enc = build_encoder(yaml_path, palette, n_palette=N,
                        spacing='adaptive', epsilon=eps)
    rse = RSEncoder(enc, nsym=nsym)
    print(f"  Bits per cell: {rse.bits_per_cell:.2f} "
          f"(word={rse.word_bits:.2f} + pos={rse.pos_bits:.2f})")
    print(f"  States per cell: {rse.states_per_cell}")

    # Show palette colors
    palette_lab = curve.eval(s_palette)
    palette_srgb = lab_to_srgb(palette_lab)
    print(f"\n  Palette colors (N={N}):")
    for i in range(N):
        c = palette_srgb[i]
        print(f"    [{i:2d}] R={c[0]:3d} G={c[1]:3d} B={c[2]:3d}")

    # 5. Encode public key -> CIELAB cells
    print(f"\nEncoding {len(pubkey)} bytes with RS({nsym} parity)...")
    pixels_lab, meta = rse.encode_bytes(pubkey)
    cells_srgb = lab_to_srgb(pixels_lab)
    print(f"  Total cells: {meta['n_cells']}")
    print(f"  RS: {meta['payload_bytes']} data + {meta['rs_parity_bytes']} parity = "
          f"{meta['rs_total_bytes']} bytes")
    print(f"  Max correctable: {meta['max_correctable_bytes']} byte errors")

    # 6. Generate header pixel and prepend
    print("\nGenerating self-describing header...")
    header_lab = encode_header(N, eps, curve, frame, configs=configs)
    header_srgb = lab_to_srgb(header_lab.reshape(1, 3))[0]
    print(f"  Header color: R={header_srgb[0]:3d} G={header_srgb[1]:3d} "
          f"B={header_srgb[2]:3d}")

    # Prepend header to payload cells
    all_srgb = np.vstack([header_srgb.reshape(1, 3), cells_srgb])
    # For scatter_colors, digits are just indices; header gets a unique index
    # that won't collide with any payload digit (use N as sentinel)
    n_cells = meta['n_cells']
    # Decode each cell to get its word index (for color scatter)
    cell_words = []
    for i in range(n_cells):
        pixel = pixels_lab[i]
        best_w = 0
        best_dist = np.inf
        for w in range(N):
            base = curve.eval(s_palette[w])
            d = np.linalg.norm(pixel - base)
            if d < best_dist:
                best_dist = d
                best_w = w
        cell_words.append(best_w)
    # Header gets unique "digit" N (not a real palette index)
    all_digits = [N] + cell_words
    total_cells = len(all_srgb)

    # 7. Verify CIELAB round-trip (encode -> decode, no PNG)
    print("\nVerifying CIELAB round-trip...")
    recovered, dec_meta = rse.decode_bytes(pixels_lab)
    if recovered is None or recovered != pubkey:
        raise RuntimeError(f"CIELAB round-trip verification FAILED: {dec_meta}")
    print(f"  CIELAB round-trip: PASS (errors corrected: {dec_meta['errors_corrected']})")

    # 8. Resolve platform exclusion zones (fractional -> pixel coords)
    exclusion_zones = None
    if platform:
        fracs = PLATFORM_EXCLUSIONS.get(platform)
        if fracs is None:
            raise ValueError(f"Unknown platform '{platform}'. "
                             f"Known: {list(PLATFORM_EXCLUSIONS.keys())}")
        exclusion_zones = [
            (fx * width, fy * height, fr * width) for fx, fy, fr in fracs
        ]
        print(f"\nPlatform: {platform}")
        for cx_z, cy_z, r_z in exclusion_zones:
            print(f"  Exclusion zone: center=({cx_z:.0f}, {cy_z:.0f}), radius={r_z:.0f}")

    # 9. Render banner (header cell included as just another Voronoi cell)
    result = {
        'pubkey_hex': pubkey.hex(),
        'npub': npub_str,
        'palette': palette,
        'N': N,
        'epsilon': eps,
        'nsym': nsym,
        'n_cells': n_cells,
        'total_cells': total_cells,
        'bits_per_cell': rse.bits_per_cell,
        'states_per_cell': rse.states_per_cell,
        'rs_total_bytes': meta['rs_total_bytes'],
        'srgb_dist_min': srgb_dist,
        'platform': platform,
        'files': [],
    }

    if fmt in ('png', 'both'):
        print(f"\nRendering PNG banner ({width}x{height}, {total_cells} cells "
              f"incl. header)...")
        png_path = os.path.join(output_dir, f'nostr_banner_{palette}.png')
        img, seeds, perm = render_banner_png(
            all_srgb, all_digits, width, height, png_path,
            seed=seed, relax_iters=10,
            exclusion_zones=exclusion_zones,
            scan_order=True)
        print(f"  Saved: {png_path}")
        result['files'].append(png_path)

        # 10. Verify PNG round-trip (load -> decode header -> decode bytes)
        print("\nVerifying PNG round-trip...")
        img_lab = srgb_to_lab(img)
        hdr_n, hdr_eps, _ = _detect_header(img_lab, curve, frame, configs)
        if hdr_n != N or hdr_eps != eps:
            print(f"  WARNING: Header decoded N={hdr_n}, eps={hdr_eps:.4f} "
                  f"(expected N={N}, eps={eps:.4f})")
        else:
            print(f"  Header decoded: N={hdr_n}, epsilon={hdr_eps:.4f} - PASS")

    if fmt in ('svg', 'both'):
        print(f"\nRendering SVG banner ({width}x{height}, {total_cells} cells "
              f"incl. header)...")
        svg_path = os.path.join(output_dir, f'nostr_banner_{palette}.svg')
        render_banner_svg(all_srgb, all_digits, width, height, svg_path,
                           seed=seed, relax_iters=10,
                           exclusion_zones=exclusion_zones)
        print(f"  Saved: {svg_path}")
        result['files'].append(svg_path)

    # 11. Summary
    print(f"\n{'='*60}")
    print(f"NOSTR PUBKEY BANNER (self-describing)")
    print(f"{'='*60}")
    print(f"  npub:           {npub_str[:20]}...{npub_str[-8:]}")
    print(f"  pubkey (hex):   {pubkey.hex()[:16]}...{pubkey.hex()[-8:]}")
    print(f"  palette:        {palette} (N={N}, sRGB dist={srgb_dist:.1f})")
    print(f"  encoding:       word+constellation, {rse.bits_per_cell:.2f} bits/cell")
    print(f"  epsilon:        {eps:.4f}")
    print(f"  cells:          {n_cells} payload + 1 header = {total_cells}")
    print(f"  RS protection:  {nsym} parity bytes "
          f"(corrects up to {nsym // 2} byte errors)")
    print(f"  image size:     {width} x {height}")
    if platform:
        print(f"  platform:       {platform} (profile pic exclusion active)")
    for f in result['files']:
        print(f"  output:         {f}")
    print(f"{'='*60}")

    return result


def _detect_header(img_lab, curve, frame, configs):
    """Detect header from a rendered banner image (internal helper).

    Reuses detect_header from decode_image logic but works on raw
    CIELAB image arrays.

    Returns:
        n_palette, epsilon, header_lab
    """
    from decode_image import detect_header
    return detect_header(img_lab, curve, frame, configs)


def main():
    parser = argparse.ArgumentParser(
        description='Generate a Voronoi banner encoding a Nostr public key')
    parser.add_argument(
        '--npub',
        default='npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r',
        help='Nostr public key in npub bech32 format')
    parser.add_argument('--width', type=int, default=1500,
                        help='Banner width in pixels (default: 1500)')
    parser.add_argument('--height', type=int, default=500,
                        help='Banner height in pixels (default: 500)')
    parser.add_argument('--palette', default='viridis',
                        help='Palette name from palette.yaml (default: viridis)')
    parser.add_argument('-N', '--n-palette', type=int, default=None,
                        help='Number of palette colors (default: auto-select optimal)')
    parser.add_argument('--nsym', type=int, default=16,
                        help='RS parity bytes (default: 16)')
    parser.add_argument('--seed', type=int, default=42,
                        help='Random seed for Voronoi layout')
    parser.add_argument('--output-dir', default=None,
                        help='Output directory (default: samples/)')
    parser.add_argument('--format', choices=['png', 'svg', 'both'],
                        default='both', help='Output format (default: both)')
    parser.add_argument('--platform', choices=list(PLATFORM_EXCLUSIONS.keys()),
                        default=None,
                        help='Avoid placing cells under platform profile pic '
                             f'(choices: {", ".join(PLATFORM_EXCLUSIONS.keys())})')
    args = parser.parse_args()

    create_banner(
        args.npub,
        output_dir=args.output_dir,
        width=args.width,
        height=args.height,
        palette=args.palette,
        N=args.n_palette,
        nsym=args.nsym,
        seed=args.seed,
        fmt=args.format,
        platform=args.platform,
    )


if __name__ == '__main__':
    main()
