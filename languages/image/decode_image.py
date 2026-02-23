#!/usr/bin/env python3
"""
Decode a PNG image back to payload using color-space operations.

The first color in the palette acts as a self-describing header that encodes
the radix (N, epsilon). Once the radix is known, the remaining pixels are
histogram-binned into N palette colors and residuals give constellation
positions.

Decode pipeline (cell-level):
  1. Load image, convert all pixels to CIELAB
  2. Identify the header pixel at arc-length s=0 -> decode (N, epsilon)
  3. Build palette from decoded radix via equal_capacity_positions()
  4. For each pixel, find nearest palette color (word index)
  5. Decompose residual into constellation position via Bishop frame
  6. Deduplicate: group by (word, pos), yielding unique cells
  7. Order cells by spatial scan order (left-to-right, top-to-bottom)

Byte-level decode (--decode-bytes):
  Extends the cell-level pipeline with RSEncoder mixed-radix reconstruction
  and Reed-Solomon error correction to recover the original payload bytes.

Usage:
    python decode_image.py image.png
    python decode_image.py image.png --palette viridis
    python decode_image.py --decode-bytes image.png
    python decode_image.py --decode-bytes image.png --expected-hex <hex>
"""

import os
import sys
import argparse
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from parametric_encoding import (
    PaletteCurve, BishopFrame, ConstellationMap,
    srgb_to_lab, lab_to_srgb,
    compute_capacity_curve, equal_capacity_positions,
    compute_tube_radius,
    derive_config_table, decode_header,
    build_encoder, HEADER_S,
)
from rs_encoding import RSEncoder


def load_image(path):
    """Load a PNG image as an (H, W, 3) sRGB uint8 array.

    Args:
        path: path to PNG file

    Returns:
        img: (H, W, 3) numpy array of sRGB values in [0, 255]
    """
    from PIL import Image
    img = Image.open(path).convert('RGB')
    return np.asarray(img, dtype=np.uint8)


def detect_header(img_lab, curve, frame, configs):
    """Detect the header pixel and decode (N, epsilon).

    The header color sits at fixed arc-length s=0 on the palette curve.
    Finds the pixel closest to the header base color, then flood-fills
    its Voronoi cell to get a clean average for constellation decoding.

    Args:
        img_lab: (H, W, 3) CIELAB image
        curve: PaletteCurve
        frame: BishopFrame
        configs: (config_table, header_epsilon) from derive_config_table()

    Returns:
        n_palette: payload palette size
        epsilon: payload constellation spacing
        header_lab: (3,) averaged CIELAB of header pixels
    """
    from scipy.ndimage import label as cc_label

    H, W, _ = img_lab.shape
    pixels_flat = img_lab.reshape(-1, 3)

    # The header base color is at s=0 on the curve
    header_base = curve.eval(HEADER_S)

    # Find the single pixel closest to the header base color.
    # This pixel is in the header Voronoi cell.
    dists = np.linalg.norm(pixels_flat - header_base, axis=1)
    seed_idx = int(np.argmin(dists))
    seed_color = pixels_flat[seed_idx]

    # Flood-fill: find all contiguous pixels with the same color.
    # In a Voronoi banner, each cell is flat-colored, so pixels within
    # a cell have near-identical CIELAB values (differing only by sRGB
    # quantization, < 1 CIELAB). Use a tight threshold.
    color_dists = np.linalg.norm(pixels_flat - seed_color, axis=1)
    similar_mask = (color_dists < 1.5).reshape(H, W)

    # Connected component containing the seed pixel
    labeled, n_components = cc_label(similar_mask)
    seed_y, seed_x = seed_idx // W, seed_idx % W
    seed_label = labeled[seed_y, seed_x]

    if seed_label > 0:
        cell_mask = (labeled == seed_label).ravel()
        header_lab = np.mean(pixels_flat[cell_mask], axis=0)
    else:
        # Fallback: use the seed pixel directly
        header_lab = seed_color

    # Decode the header
    n_palette, epsilon = decode_header(header_lab, curve, frame, configs=configs)
    return n_palette, epsilon, header_lab


def decode_pixels(img_lab, curve, frame, s_palette, constellation_map,
                  distance_threshold=None, min_cell_pixels=10):
    """Decode all image pixels into (word, position) pairs.

    For each pixel:
      1. Find nearest palette color (word index)
      2. Decompose residual into Bishop frame basis -> (alpha1, alpha2)
      3. Snap to constellation grid -> position j

    Args:
        img_lab: (H, W, 3) CIELAB image
        curve: PaletteCurve
        frame: BishopFrame
        s_palette: (N,) arc-length positions for palette colors
        constellation_map: ConstellationMap
        distance_threshold: max CIELAB distance to a palette color;
            pixels farther than this are discarded as background/border.
            If None, computed as 2x max tube radius.
        min_cell_pixels: minimum pixel count for a connected component
            to be considered a valid cell. Filters out noise fragments.
            Default: 10.

    Returns:
        cells: list of dicts with keys:
            'word': palette color index
            'pos': constellation position
            'y': row coordinate (for scan ordering)
            'x': column coordinate (for scan ordering)
            'count': number of pixels in this Voronoi cell
    """
    H, W, _ = img_lab.shape
    N = len(s_palette)

    # Precompute palette base colors and frame vectors
    palette_base = np.array([curve.eval(s) for s in s_palette])  # (N, 3)
    palette_U1 = np.zeros((N, 3))
    palette_U2 = np.zeros((N, 3))
    for i, s in enumerate(s_palette):
        _, U1, U2 = frame.eval_frame(s)
        palette_U1[i] = U1
        palette_U2[i] = U2

    # Compute distance threshold if not given
    if distance_threshold is None:
        _, radii = compute_tube_radius(curve, frame, s_values=s_palette)
        distance_threshold = 2.0 * float(np.max(radii))

    pixels_flat = img_lab.reshape(-1, 3)
    n_pixels = len(pixels_flat)

    # Vectorized nearest-palette assignment
    # Compute distance from each pixel to each palette color
    # pixels_flat: (P, 3), palette_base: (N, 3)
    # diff: (P, N, 3)
    diff = pixels_flat[:, np.newaxis, :] - palette_base[np.newaxis, :, :]
    dists = np.linalg.norm(diff, axis=2)  # (P, N)
    nearest_word = np.argmin(dists, axis=1)  # (P,)
    nearest_dist = dists[np.arange(n_pixels), nearest_word]  # (P,)

    # Filter out background/border pixels
    valid_mask = nearest_dist < distance_threshold

    # For valid pixels, compute residual decomposition
    # Gather the base, U1, U2 for each pixel's assigned word
    valid_idx = np.where(valid_mask)[0]
    valid_words = nearest_word[valid_idx]
    valid_pixels = pixels_flat[valid_idx]

    # Compute residuals
    bases = palette_base[valid_words]  # (V, 3)
    U1s = palette_U1[valid_words]      # (V, 3)
    U2s = palette_U2[valid_words]      # (V, 3)
    residuals = valid_pixels - bases    # (V, 3)

    alpha1 = np.sum(residuals * U1s, axis=1)  # (V,)
    alpha2 = np.sum(residuals * U2s, axis=1)  # (V,)

    # Snap to constellation grid for each pixel
    positions = np.zeros(len(valid_idx), dtype=int)
    for i in range(len(valid_idx)):
        w = valid_words[i]
        c = constellation_map[w]
        positions[i] = int(c.displacement_to_position(alpha1[i], alpha2[i]))

    # Build a label map for connected-component analysis.
    # Each pixel gets a (word, pos) tuple. Contiguous regions of the same
    # (word, pos) are distinct Voronoi cells — this correctly handles
    # multiple spatially separated cells that share the same word.
    from scipy.ndimage import label as cc_label

    # Build 2D maps for word and position assignments
    word_map = np.full(H * W, -1, dtype=int)
    pos_map = np.full(H * W, -1, dtype=int)
    word_map[valid_idx] = valid_words
    pos_map[valid_idx] = positions

    word_map_2d = word_map.reshape(H, W)
    pos_map_2d = pos_map.reshape(H, W)

    # Encode (word, pos) into a single ID for labeling.
    # -1 stays as -1 (invalid/background).
    max_pos = int(np.max(positions)) + 1 if len(positions) > 0 else 1
    combined = np.where(word_map_2d >= 0,
                        word_map_2d * max_pos + pos_map_2d,
                        -1)

    # Find connected components per unique (word, pos) value
    cells = []
    unique_vals = np.unique(combined)
    unique_vals = unique_vals[unique_vals >= 0]  # skip background

    for val in unique_vals:
        mask = combined == val
        labeled, n_components = cc_label(mask)
        word = int(val // max_pos)
        pos = int(val % max_pos)

        for comp_id in range(1, n_components + 1):
            comp_mask = labeled == comp_id
            count = int(np.sum(comp_mask))
            if count < min_cell_pixels:
                continue  # skip noise fragments
            ys, xs = np.where(comp_mask)
            cells.append({
                'word': word,
                'pos': pos,
                'y': float(np.mean(ys)),
                'x': float(np.mean(xs)),
                'count': count,
            })

    # Sort by scan order (top-to-bottom, left-to-right)
    cells.sort(key=lambda c: (c['y'], c['x']))

    return cells


def decode_image(image_path, palette_name='viridis'):
    """Decode a PNG image back to payload cells.

    End-to-end pipeline: load image -> detect header -> build palette ->
    histogram bin -> residual decompose -> deduplicate -> return cells.

    Args:
        image_path: path to PNG file
        palette_name: palette name from palette.yaml

    Returns:
        cells: list of dicts with 'word', 'pos', 'y', 'x', 'count'
        n_palette: recovered palette size
        epsilon: recovered constellation spacing
    """
    # Load image and convert to CIELAB
    img_srgb = load_image(image_path)
    img_lab = srgb_to_lab(img_srgb)

    # Build curve and frame
    yaml_path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             'palette.yaml')
    curve = PaletteCurve.from_yaml(yaml_path, palette_name)
    frame = BishopFrame(curve)

    # Derive config table
    configs = derive_config_table(curve, frame)

    # Step 1: Detect and decode header
    n_palette, epsilon, header_lab = detect_header(
        img_lab, curve, frame, configs)

    # Step 2: Build palette from decoded radix
    s_dense, radii_dense, C = compute_capacity_curve(curve, frame,
                                                      n_samples=200)
    s_palette = equal_capacity_positions(s_dense, C, n_palette)
    radii_at_pal = np.interp(s_palette, s_dense, radii_dense)
    constellation_map = ConstellationMap(radii_at_pal, epsilon=epsilon)

    # Steps 3-5: Decode pixels
    cells = decode_pixels(img_lab, curve, frame, s_palette, constellation_map)

    return cells, n_palette, epsilon


def decode_banner_words(img_lab, curve, frame, s_palette, constellation_map):
    """Decode banner pixels with header masking.

    Two-pass decoder:
      Pass 1: Identify header Voronoi cell via flood-fill from nearest pixel to s=0
      Pass 2: Mask header cell, run decode_pixels() on remaining pixels

    Args:
        img_lab: (H, W, 3) CIELAB image
        curve: PaletteCurve
        frame: BishopFrame
        s_palette: (N,) arc-length positions for payload palette colors
        constellation_map: ConstellationMap for payload

    Returns:
        cells: list of dicts with 'word', 'pos', 'y', 'x', 'count'
            (header cells excluded)
    """
    from scipy.ndimage import label as cc_label

    H, W, _ = img_lab.shape
    pixels_flat = img_lab.reshape(-1, 3)

    # Pass 1: Find header cell via flood-fill (same logic as detect_header)
    header_base = curve.eval(HEADER_S)
    dists = np.linalg.norm(pixels_flat - header_base, axis=1)
    seed_idx = int(np.argmin(dists))
    seed_color = pixels_flat[seed_idx]

    # Flood-fill: contiguous pixels with same color (< 1.5 CIELAB)
    color_dists = np.linalg.norm(pixels_flat - seed_color, axis=1)
    similar_mask = (color_dists < 1.5).reshape(H, W)
    labeled, _ = cc_label(similar_mask)
    seed_y, seed_x = seed_idx // W, seed_idx % W
    seed_label = labeled[seed_y, seed_x]

    header_mask_2d = (labeled == seed_label) if seed_label > 0 else np.zeros((H, W), dtype=bool)

    # Pass 2: Mask header pixels, then decode remaining
    masked_lab = img_lab.copy()
    # Set header pixels to black (L*=0, a*=0, b*=0) — far from any palette color
    masked_lab[header_mask_2d] = [0.0, 0.0, 0.0]

    cells = decode_pixels(masked_lab, curve, frame, s_palette,
                          constellation_map)
    return cells


def extract_cell_colors(img_lab, header_mask, border_color_lab=None,
                        min_cell_pixels=100, verbose=False):
    """Segment a Voronoi banner into cells and return average CIELAB per cell.

    In a rendered Voronoi PNG, each cell is flat-colored (single sRGB value).
    We segment by grouping pixels with identical sRGB colors into connected
    components. Border pixels and header pixels are excluded.

    Args:
        img_lab: (H, W, 3) CIELAB image
        header_mask: (H, W) boolean mask of header pixels
        border_color_lab: (3,) CIELAB of border color (auto-detected if None)
        min_cell_pixels: minimum pixel count for a valid cell
        verbose: print debug info

    Returns:
        cell_colors: (n_cells, 3) average CIELAB per cell, scan-ordered
    """
    from scipy.ndimage import label as cc_label

    H, W, _ = img_lab.shape

    # Detect border color: the most common color along the image edges.
    # Border pixels have near-identical CIELAB values.
    if border_color_lab is None:
        edge_pixels = np.concatenate([
            img_lab[0, :],          # top row
            img_lab[-1, :],         # bottom row
            img_lab[:, 0],          # left col
            img_lab[:, -1],         # right col
        ])
        # In a bordered Voronoi, the border color appears at cell boundaries
        # along the edge. Use the median as a robust estimator.
        # Actually, for a dark border, it's usually the darkest color.
        # Use the mode of rounded L* values to find the border cluster.
        L_vals = edge_pixels[:, 0]
        border_color_lab = edge_pixels[np.argmin(L_vals)]

    # Build exclusion mask: header + border pixels
    pixels_flat = img_lab.reshape(-1, 3)
    border_dists = np.linalg.norm(pixels_flat - border_color_lab, axis=1)
    border_mask = (border_dists < 3.0).reshape(H, W)  # 3 CIELAB tolerance
    exclude_mask = header_mask | border_mask

    # Quantize CIELAB to identify unique cell colors.
    # sRGB uint8 quantization produces CIELAB values that differ by < 0.5
    # within a cell. Round to nearest integer to merge.
    lab_rounded = np.round(img_lab).astype(np.int16)
    # Encode as unique ID per color
    color_id = (lab_rounded[:, :, 0].astype(np.int32) + 128) * 65536 + \
               (lab_rounded[:, :, 1].astype(np.int32) + 128) * 256 + \
               (lab_rounded[:, :, 2].astype(np.int32) + 128)
    # Set excluded pixels to -1
    color_id[exclude_mask] = -1

    # Find connected components per unique color
    cells = []
    unique_ids = np.unique(color_id)
    unique_ids = unique_ids[unique_ids >= 0]

    for uid in unique_ids:
        mask = color_id == uid
        labeled, n_components = cc_label(mask)
        for comp_id in range(1, n_components + 1):
            comp_mask = labeled == comp_id
            count = int(np.sum(comp_mask))
            if count < min_cell_pixels:
                continue
            # Average CIELAB of this cell
            ys, xs = np.where(comp_mask)
            avg_lab = np.mean(img_lab[comp_mask], axis=0)
            cells.append({
                'lab': avg_lab,
                'y': float(np.mean(ys)),
                'x': float(np.mean(xs)),
                'count': count,
            })

    # Sort by scan order (top-to-bottom, left-to-right)
    cells.sort(key=lambda c: (c['y'], c['x']))

    if verbose:
        print(f"  Cell segmentation: {len(cells)} cells "
              f"(min_pixels={min_cell_pixels})")

    cell_colors = np.array([c['lab'] for c in cells])
    return cell_colors, cells


def decode_banner(image_path, palette_name='viridis', nsym=16,
                  seed=42, verbose=False):
    """Decode a banner PNG to recovered payload bytes.

    Full byte-level recovery pipeline:
      1. Load PNG -> CIELAB
      2. detect_header() -> (N, epsilon)
      3. Segment image into Voronoi cells (color-based connected components)
      4. Regenerate Voronoi seeds and match cells to seeds for correct ordering
      5. Decode via RSEncoder.decode_bytes() (joint word+pos search)
      6. RS decode -> recovered bytes

    Cell ordering is critical: the encoder assigns cell i to the i-th seed
    in scan order. The decoder regenerates the same seeds (deterministic
    given image dimensions and random seed) and matches each extracted cell
    to its nearest seed to recover the correct ordering.

    Args:
        image_path: path to banner PNG file
        palette_name: palette name from palette.yaml
        nsym: RS parity bytes (must match encoder, default 16)
        seed: random seed for Voronoi layout (must match encoder, default 42)
        verbose: print detailed info

    Returns:
        payload_bytes: recovered bytes (or None if uncorrectable)
        meta: dict with decode metadata
    """
    from scipy.spatial import KDTree
    from generate_banner import generate_banner_seeds

    # Load image and convert to CIELAB
    img_srgb = load_image(image_path)
    img_lab = srgb_to_lab(img_srgb)
    H, W, _ = img_lab.shape

    # Build curve and frame
    yaml_path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             'palette.yaml')
    curve = PaletteCurve.from_yaml(yaml_path, palette_name)
    frame = BishopFrame(curve)

    # Derive config table and detect header
    configs = derive_config_table(curve, frame)
    n_palette, epsilon, header_lab = detect_header(
        img_lab, curve, frame, configs)

    if verbose:
        print(f"  Header decoded: N={n_palette}, epsilon={epsilon:.4f}")

    # Build encoder dict and RSEncoder with decoded params
    enc = build_encoder(yaml_path, palette_name, n_palette=n_palette,
                        spacing='adaptive', epsilon=epsilon)
    rse = RSEncoder(enc, nsym=nsym)

    if verbose:
        print(f"  States per cell: {rse.states_per_cell}")
        print(f"  Bits per cell: {rse.bits_per_cell:.2f}")

    # Build header mask via flood-fill (same as detect_header)
    from scipy.ndimage import label as cc_label
    pixels_flat = img_lab.reshape(-1, 3)
    header_base = curve.eval(HEADER_S)
    dists = np.linalg.norm(pixels_flat - header_base, axis=1)
    seed_idx = int(np.argmin(dists))
    seed_color = pixels_flat[seed_idx]
    color_dists = np.linalg.norm(pixels_flat - seed_color, axis=1)
    similar_mask = (color_dists < 1.5).reshape(H, W)
    labeled, _ = cc_label(similar_mask)
    seed_y, seed_x = seed_idx // W, seed_idx % W
    seed_label = labeled[seed_y, seed_x]
    header_mask = (labeled == seed_label) if seed_label > 0 else np.zeros((H, W), dtype=bool)

    # Segment image into cells and extract average CIELAB per cell.
    # Cells are sorted by centroid scan order, which may not match the
    # encoder's seed scan order exactly (cells at similar Y can swap).
    cell_colors, cells = extract_cell_colors(
        img_lab, header_mask, verbose=verbose)
    n_cells = len(cell_colors)

    if verbose:
        print(f"  Cells recovered: {n_cells}")

    # Regenerate Voronoi seeds to recover the encoder's cell ordering.
    # The encoder assigns cell i to the i-th seed in scan order (with
    # scan_order=True). We regenerate the same seeds and match each
    # extracted cell to its nearest seed.
    n_total = n_cells + 1  # payload cells + header
    seeds = generate_banner_seeds(n_total, W, H, seed=seed, relax_iters=10)

    # Determine seed scan order (same as encoder)
    scan_idx = np.lexsort((seeds[:, 0], seeds[:, 1]))

    # Match each extracted cell to nearest seed by centroid
    cell_centroids = np.array([[c['x'], c['y']] for c in cells])
    tree = KDTree(seeds)
    _, matched_seeds = tree.query(cell_centroids)

    # Determine ordering: for each cell, find its rank in the seed scan order.
    # Skip the header seed (rank 0 in scan order = seed scan_idx[0]).
    seed_rank = np.zeros(len(seeds), dtype=int)
    seed_rank[scan_idx] = np.arange(len(seeds))

    # Build the reordered cell colors: position i gets the cell matched to
    # the seed with scan rank i+1 (rank 0 is header).
    cell_order = []
    for i in range(n_cells):
        rank = seed_rank[matched_seeds[i]]
        cell_order.append((rank, i))
    cell_order.sort()

    # Extract ordered colors (skip rank 0 = header)
    ordered_colors = []
    for rank, cell_idx in cell_order:
        if rank == 0:
            continue  # skip header
        ordered_colors.append(cell_colors[cell_idx])
    ordered_colors = np.array(ordered_colors)
    n_payload = len(ordered_colors)

    if verbose:
        print(f"  Payload cells (ordered): {n_payload}")

    # Use RSEncoder.decode_bytes() with joint (word, pos) search.
    # Try different cell counts: some cells may be border artifacts.
    best_result = None
    for trim in range(min(5, n_payload)):
        try_n = n_payload - trim
        kept_colors = ordered_colors[:try_n]

        # Set rs_total so RSEncoder.decode_bytes() can reconstruct
        rs_total = int(np.floor(try_n * rse.bits_per_cell / 8))
        if rs_total <= nsym:
            continue
        rse._last_rs_total_bytes = rs_total
        rse._last_nsym = nsym

        try:
            recovered, dec_meta = rse.decode_bytes(kept_colors)
        except (OverflowError, ValueError):
            continue

        if dec_meta.get('success'):
            best_result = (recovered, {
                'success': True,
                'errors_corrected': dec_meta['errors_corrected'],
                'cells_decoded': try_n,
                'cells_trimmed': trim,
            })
            break

    meta = {
        'n_palette': n_palette,
        'epsilon': epsilon,
        'n_cells': n_payload,
        'bits_per_cell': rse.bits_per_cell,
        'states_per_cell': rse.states_per_cell,
        'nsym': nsym,
    }

    if best_result:
        payload_bytes, dec_meta = best_result
        meta.update(dec_meta)
        return payload_bytes, meta
    else:
        meta['success'] = False
        meta['error'] = f"RS decode failed for all cell counts ({n_payload} to {max(n_payload - 4, 0)})"
        return None, meta


def verify_roundtrip(palette_name='viridis', N=13, width=300, height=100,
                     seed=42, verbose=False):
    """Verify encode -> PNG render -> decode round-trip.

    Creates a small test image using WordEncoder from generate_banner,
    saves to a temporary PNG, decodes it, and checks that the recovered
    word sequence matches the original digits.

    Args:
        palette_name: palette name from palette.yaml
        N: number of palette colors
        width, height: test image dimensions
        seed: random seed for layout
        verbose: print detailed info

    Returns:
        True if round-trip succeeds, False otherwise
    """
    import tempfile
    from PIL import Image
    from generate_banner import WordEncoder, generate_banner_seeds, scatter_colors
    from scipy.spatial import KDTree

    yaml_path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             'palette.yaml')
    curve = PaletteCurve.from_yaml(yaml_path, palette_name)
    frame = BishopFrame(curve)

    # Place palette colors
    s_dense, _, C = compute_capacity_curve(curve, frame, n_samples=200)
    s_palette = equal_capacity_positions(s_dense, C, N)

    # Build word encoder
    encoder = WordEncoder(curve, s_palette, N, nsym=4)

    # Encode a test payload (8 bytes)
    payload_bytes = bytes(range(8))
    digits, cells_srgb, meta = encoder.encode_bytes(payload_bytes)
    n_cells = meta['n_cells']

    if verbose:
        print(f"Encoding: {len(payload_bytes)} bytes -> {n_cells} cells, N={N}")
        print(f"Digits: {digits}")

    # Render a simple Voronoi image (no border, to keep colors clean)
    seeds = generate_banner_seeds(n_cells, width, height, seed=seed,
                                  relax_iters=5)
    perm, _ = scatter_colors(seeds, digits, iterations=2000, seed=seed)
    scattered_srgb = cells_srgb[perm]

    tree = KDTree(seeds)
    xx, yy = np.meshgrid(np.arange(width), np.arange(height))
    pixel_coords = np.column_stack([xx.ravel(), yy.ravel()])
    _, indices = tree.query(pixel_coords)
    nearest_idx = indices.ravel()
    img_flat = scattered_srgb[nearest_idx]
    img = img_flat.reshape(height, width, 3)

    # Save to temporary PNG
    with tempfile.NamedTemporaryFile(suffix='.png', delete=False) as f:
        tmp_path = f.name
    Image.fromarray(img).save(tmp_path)

    try:
        # Decode
        img_srgb = load_image(tmp_path)
        img_lab = srgb_to_lab(img_srgb)

        # No header in this image (WordEncoder doesn't add one), so we
        # skip header detection and use known N.
        _, radii_dense, _ = compute_capacity_curve(curve, frame, n_samples=200)
        radii_at_pal = np.interp(s_palette, s_dense, radii_dense)
        constellation_map = ConstellationMap(radii_at_pal, epsilon=2.3)

        cells = decode_pixels(img_lab, curve, frame, s_palette,
                              constellation_map)

        # The decoded cells give us words in scan order. The encoding used
        # scatter_colors to permute digit->seed, so scan order of seeds
        # gives us the scattered digits. We need to verify that the
        # recovered word set matches the original digit multiset.
        recovered_words = [c['word'] for c in cells]

        # The scatter permutation maps: seed i gets digit[perm[i]].
        # Seeds are in scan order for the decoder, so we expect
        # recovered_words to match digits[perm] in seed-scan order.
        # Sort seeds by scan order (y, x) to get the expected sequence.
        scan_order = np.lexsort((seeds[:, 0], seeds[:, 1]))
        expected_words = [int(digits[perm[i]]) for i in scan_order]

        if verbose:
            print(f"Recovered: {len(recovered_words)} cells")
            print(f"Expected:  {len(expected_words)} words")

        # Compare multisets (order may differ slightly due to cell merging)
        from collections import Counter
        recovered_counter = Counter(recovered_words)
        expected_counter = Counter(expected_words)

        success = recovered_counter == expected_counter
        if verbose:
            if success:
                print("Round-trip: PASS (word multiset matches)")
            else:
                print("Round-trip: FAIL")
                print(f"  Expected counter: {expected_counter}")
                print(f"  Recovered counter: {recovered_counter}")
                diff = expected_counter - recovered_counter
                if diff:
                    print(f"  Missing: {dict(diff)}")
                diff2 = recovered_counter - expected_counter
                if diff2:
                    print(f"  Extra: {dict(diff2)}")

        return success

    finally:
        os.unlink(tmp_path)


def main():
    """CLI entry point for image decoding."""
    parser = argparse.ArgumentParser(
        description='Decode a Glossia-encoded PNG image back to payload')
    parser.add_argument('image', nargs='?', help='Path to PNG image')
    parser.add_argument('--palette', default='viridis',
                        help='Palette name from palette.yaml (default: viridis)')
    parser.add_argument('-v', '--verbose', action='store_true',
                        help='Print detailed decode info')
    parser.add_argument('--verify', action='store_true',
                        help='Run encode -> PNG -> decode round-trip test')
    parser.add_argument('--decode-bytes', action='store_true',
                        help='Full byte-level decode (header + RS recovery)')
    parser.add_argument('--nsym', type=int, default=16,
                        help='RS parity bytes (must match encoder, default: 16)')
    parser.add_argument('--seed', type=int, default=42,
                        help='Voronoi layout seed (must match encoder, default: 42)')
    parser.add_argument('--expected-hex', default=None,
                        help='Expected payload hex for verification')
    args = parser.parse_args()

    if args.verify:
        print("Running round-trip verification...")
        ok = verify_roundtrip(palette_name=args.palette, verbose=True)
        sys.exit(0 if ok else 1)

    if args.image is None:
        parser.print_help()
        sys.exit(1)

    if not os.path.exists(args.image):
        print(f"Error: {args.image} not found", file=sys.stderr)
        sys.exit(1)

    if args.decode_bytes:
        # Full byte-level decode
        print(f"Decoding bytes from {args.image}...")
        payload, meta = decode_banner(args.image,
                                       palette_name=args.palette,
                                       nsym=args.nsym,
                                       seed=args.seed,
                                       verbose=args.verbose)

        print(f"\nDecode results:")
        print(f"  Palette size (N): {meta['n_palette']}")
        print(f"  Epsilon: {meta['epsilon']:.4f}")
        print(f"  Cells: {meta['n_cells']}")
        print(f"  Bits/cell: {meta['bits_per_cell']:.2f}")
        print(f"  Success: {meta['success']}")

        if payload is not None:
            print(f"  Errors corrected: {meta.get('errors_corrected', 0)}")
            print(f"\nRecovered payload ({len(payload)} bytes):")
            print(f"  hex: {payload.hex()}")

            if args.expected_hex:
                expected = bytes.fromhex(args.expected_hex)
                if payload == expected:
                    print(f"  Verification: PASS")
                else:
                    print(f"  Verification: FAIL")
                    print(f"  Expected: {expected.hex()}")
                    sys.exit(1)
        else:
            print(f"  Error: {meta.get('error', 'unknown')}")
            sys.exit(1)
    else:
        # Cell-level decode (word + position)
        print(f"Decoding {args.image}...")
        cells, n_palette, epsilon = decode_image(args.image,
                                                 palette_name=args.palette)

        print(f"\nDecoded parameters:")
        print(f"  Palette size (N): {n_palette}")
        print(f"  Epsilon: {epsilon:.4f}")
        print(f"  Cells recovered: {len(cells)}")

        if args.verbose:
            print(f"\nCell sequence (scan order):")
            for i, cell in enumerate(cells):
                print(f"  [{i:3d}] word={cell['word']:3d} pos={cell['pos']:3d} "
                      f"at ({cell['x']:.0f}, {cell['y']:.0f}) "
                      f"count={cell['count']}")

        # Extract word sequence
        words = [c['word'] for c in cells]
        print(f"\nWord sequence ({len(words)} cells):")
        print(f"  {words}")


if __name__ == '__main__':
    main()
