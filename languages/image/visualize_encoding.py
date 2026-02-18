#!/usr/bin/env python3
"""
Visualization tools for parametric curve encoding.

Generates:
1. 3D scatter plot in CIELAB showing curve, Bishop frame, constellation, and tube
2. 2D image: rendered encoded payload as a pixel grid
3. Tube radius profile along the curve

Usage:
    python visualize_encoding.py                     # default demo
    python visualize_encoding.py --payload "0,3,3,7,15,8,8,8"
    python visualize_encoding.py --output my_plot.png
    python visualize_encoding.py --mode image --payload "0,1,2,3,4,5"
"""

import sys
import os
import argparse
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from parametric_encoding import (
    PaletteCurve, BishopFrame, Constellation, ConstellationMap,
    compute_tube_radius, encode, decode, build_encoder,
    lab_to_srgb, srgb_to_lab, EPSILON,
)

import matplotlib
matplotlib.use('Agg')  # non-interactive backend
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d import Axes3D
from scipy.spatial import KDTree


def plot_3d_curve_and_encoding(enc, payload, output_path='encoding_3d.png',
                                show_frame=True, show_constellation=True,
                                show_tube=True, title=None):
    """3D scatter plot of the palette curve with encoded pixels.

    Args:
        enc: encoder dict from build_encoder()
        payload: list of payload word indices
        output_path: where to save the figure
        show_frame: draw Bishop frame vectors at sample points
        show_constellation: draw constellation grids at a few palette points
        show_tube: shade the tube boundary
        title: plot title
    """
    curve = enc['curve']
    frame = enc['frame']

    fig = plt.figure(figsize=(14, 10))
    ax = fig.add_subplot(111, projection='3d')

    # --- The curve ---
    n_curve = 300
    s_curve = np.linspace(0, curve.arc_length, n_curve)
    pts_curve = curve.eval(s_curve)
    # Color the curve by its own sRGB color
    rgb_curve = lab_to_srgb(pts_curve) / 255.0
    for i in range(n_curve - 1):
        ax.plot(pts_curve[i:i+2, 1], pts_curve[i:i+2, 2], pts_curve[i:i+2, 0],
                color=rgb_curve[i], linewidth=3, solid_capstyle='round')

    # --- Control points ---
    cp = curve.control_points
    cp_rgb = lab_to_srgb(cp) / 255.0
    ax.scatter(cp[:, 1], cp[:, 2], cp[:, 0],
               c=cp_rgb, s=100, edgecolors='black', linewidths=1.5,
               zorder=10, label='Control points')

    # --- Bishop frame vectors at selected points ---
    if show_frame:
        n_frame_show = 12
        s_frame = np.linspace(0, curve.arc_length, n_frame_show + 2)[1:-1]
        pts_f = curve.eval(s_frame)
        T, U1, U2 = frame.eval_frame(s_frame)
        scale = 5.0  # arrow length in CIELAB units
        for i in range(len(s_frame)):
            p = pts_f[i]
            # T in red, U1 in green, U2 in blue
            ax.quiver(p[1], p[2], p[0], T[i,1]*scale, T[i,2]*scale, T[i,0]*scale,
                      color='red', arrow_length_ratio=0.2, linewidth=1.0, alpha=0.6)
            ax.quiver(p[1], p[2], p[0], U1[i,1]*scale, U1[i,2]*scale, U1[i,0]*scale,
                      color='green', arrow_length_ratio=0.2, linewidth=1.0, alpha=0.6)
            ax.quiver(p[1], p[2], p[0], U2[i,1]*scale, U2[i,2]*scale, U2[i,0]*scale,
                      color='blue', arrow_length_ratio=0.2, linewidth=1.0, alpha=0.6)

    # --- Constellation grids at a few palette points ---
    if show_constellation:
        n_pal = enc['n_palette']
        show_indices = [0, n_pal // 4, n_pal // 2, 3 * n_pal // 4, n_pal - 1]
        cmap = enc['constellation_map']

        for wi in show_indices:
            constellation = cmap[wi]
            M = constellation.M
            s_w = wi * curve.arc_length / max(n_pal - 1, 1)
            base = curve.eval(s_w)
            _, U1, U2 = frame.eval_frame(s_w)
            # Draw grid
            for a in range(M):
                for b in range(M):
                    alpha1, alpha2 = constellation.grid_to_displacement(a, b)
                    pt = base + alpha1 * U1 + alpha2 * U2
                    rgb = lab_to_srgb(pt.reshape(1, 3))[0] / 255.0
                    ax.scatter(pt[1], pt[2], pt[0], c=[rgb], s=8, alpha=0.3)

    # --- Encoded payload pixels ---
    if payload:
        pixels_lab, meta = encode(
            payload, curve, frame,
            enc['n_palette'],
            constellation_map=enc['constellation_map']
        )
        pixels_rgb = lab_to_srgb(pixels_lab) / 255.0
        ax.scatter(pixels_lab[:, 1], pixels_lab[:, 2], pixels_lab[:, 0],
                   c=pixels_rgb, s=80, edgecolors='white', linewidths=0.8,
                   zorder=15, label=f'Payload ({len(payload)} pixels)')

    # --- Tube boundary (wireframe circles at sample points) ---
    if show_tube:
        s_tube, radii = enc['tube_radii']
        n_show = 8
        indices = np.linspace(0, len(s_tube) - 1, n_show, dtype=int)
        theta = np.linspace(0, 2 * np.pi, 24)
        for idx in indices:
            s = s_tube[idx]
            r = radii[idx]
            base = curve.eval(s)
            _, U1, U2 = frame.eval_frame(s)
            circle = np.array([
                base + r * (np.cos(t) * U1 + np.sin(t) * U2) for t in theta
            ])
            ax.plot(circle[:, 1], circle[:, 2], circle[:, 0],
                    color='gray', alpha=0.2, linewidth=0.5)

    ax.set_xlabel('a* (green-red)')
    ax.set_ylabel('b* (blue-yellow)')
    ax.set_zlabel('L* (lightness)')
    if title:
        ax.set_title(title)
    else:
        cmap = enc['constellation_map']
        ax.set_title(f'Parametric Encoding in CIELAB\n'
                     f'N={enc["n_palette"]}, M={cmap.M_min}..{cmap.M_max}, '
                     f'ε={EPSILON:.1f}')
    ax.legend(loc='upper left')

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved 3D plot to {output_path}")


def plot_tube_radius_profile(enc, output_path='tube_profile.png'):
    """Plot tube radius along the curve.

    Args:
        enc: encoder dict
        output_path: where to save
    """
    s_pts, radii = enc['tube_radii']
    cmap = enc['constellation_map']
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 6), sharex=True)

    # Tube radius
    r_min = float(np.min(radii))
    ax1.fill_between(s_pts, radii, alpha=0.3, color='steelblue')
    ax1.plot(s_pts, radii, color='steelblue', linewidth=2)
    ax1.axhline(y=r_min, color='red', linestyle='--',
                label=f'r_min = {r_min:.1f}')
    ax1.axhline(y=EPSILON, color='orange', linestyle=':',
                label=f'ε = {EPSILON:.1f}')
    ax1.set_ylabel('Tube radius (CIELAB Δ)')
    ax1.set_title('Tube Radius Profile Along Palette Curve')
    ax1.legend()
    ax1.grid(True, alpha=0.3)

    # Per-color constellation capacity
    ax2.fill_between(s_pts, cmap.capacities, alpha=0.3, color='forestgreen')
    ax2.plot(s_pts, cmap.capacities, color='forestgreen', linewidth=2,
             label=f'Per-color M² ({cmap.capacity_min}..{cmap.capacity_max})')
    ax2.set_xlabel('Arc length s (CIELAB Δ)')
    ax2.set_ylabel('Constellation capacity (M²)')
    ax2.legend()
    ax2.grid(True, alpha=0.3)

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved tube profile to {output_path}")


def generate_voronoi_seeds(n, width, height, seed=None, relax_iters=5):
    """Generate well-spaced seed points via Lloyd relaxation.

    Starts with jittered grid, then iterates Lloyd's algorithm
    (move each seed to its Voronoi cell centroid) for even spacing.

    Args:
        n: number of seed points
        width, height: canvas dimensions
        seed: random seed for reproducibility
        relax_iters: Lloyd relaxation iterations (0 = pure random)

    Returns:
        (n, 2) array of (x, y) seed coordinates
    """
    rng = np.random.RandomState(seed)

    # Start with jittered grid
    cols = max(int(np.ceil(np.sqrt(n * width / height))), 1)
    rows = max(int(np.ceil(n / cols)), 1)
    cx = np.linspace(0, width, cols + 2)[1:-1]
    cy = np.linspace(0, height, rows + 2)[1:-1]
    gx, gy = np.meshgrid(cx, cy)
    grid_pts = np.column_stack([gx.ravel(), gy.ravel()])

    if len(grid_pts) >= n:
        # Subsample grid + add jitter
        indices = rng.choice(len(grid_pts), size=n, replace=False)
        seeds = grid_pts[indices].astype(np.float64)
    else:
        # Pad with random points
        extra = n - len(grid_pts)
        pad = np.column_stack([
            rng.uniform(0, width, extra),
            rng.uniform(0, height, extra)
        ])
        seeds = np.vstack([grid_pts, pad]).astype(np.float64)

    # Add jitter
    jitter_x = width / max(cols, 1) * 0.25
    jitter_y = height / max(rows, 1) * 0.25
    seeds[:, 0] += rng.uniform(-jitter_x, jitter_x, n)
    seeds[:, 1] += rng.uniform(-jitter_y, jitter_y, n)

    # Clamp to canvas
    seeds[:, 0] = np.clip(seeds[:, 0], 1, width - 2)
    seeds[:, 1] = np.clip(seeds[:, 1], 1, height - 2)

    # Lloyd relaxation: move seeds toward cell centroids
    for _ in range(relax_iters):
        tree = KDTree(seeds)
        # Sample a dense grid and assign each to nearest seed
        sx = np.linspace(0, width, min(width, 200))
        sy = np.linspace(0, height, min(height, 200))
        gx2, gy2 = np.meshgrid(sx, sy)
        sample_pts = np.column_stack([gx2.ravel(), gy2.ravel()])
        _, indices = tree.query(sample_pts)

        # Compute centroids
        new_seeds = seeds.copy()
        for i in range(n):
            mask = indices == i
            if np.any(mask):
                new_seeds[i] = sample_pts[mask].mean(axis=0)

        # Clamp
        new_seeds[:, 0] = np.clip(new_seeds[:, 0], 1, width - 2)
        new_seeds[:, 1] = np.clip(new_seeds[:, 1], 1, height - 2)
        seeds = new_seeds

    return seeds


def render_voronoi_image(pixels_srgb, width, height, seed=None,
                          relax_iters=5, border_width=1.5,
                          border_color=(20, 20, 40), bg_color=(15, 15, 35)):
    """Render encoded colors as a Voronoi diagram.

    Each payload word becomes a Voronoi cell filled with its encoded color.
    Cell borders are drawn as thin dark lines.

    Args:
        pixels_srgb: (N, 3) array of sRGB colors, one per payload word
        width, height: output image dimensions in pixels
        seed: random seed for seed point placement
        relax_iters: Lloyd relaxation iterations for even spacing
        border_width: border thickness in pixels (0 = no borders)
        border_color: RGB tuple for cell borders
        bg_color: RGB tuple for background

    Returns:
        (height, width, 3) numpy array (uint8 RGB image)
    """
    n = len(pixels_srgb)
    pixels_srgb = np.asarray(pixels_srgb, dtype=np.uint8)

    # Generate seed points
    seeds = generate_voronoi_seeds(n, width, height, seed=seed,
                                    relax_iters=relax_iters)

    # Build KDTree for nearest-seed lookup
    tree = KDTree(seeds)

    # Rasterize: for each output pixel, find nearest seed
    xx, yy = np.meshgrid(np.arange(width), np.arange(height))
    pixel_coords = np.column_stack([xx.ravel(), yy.ravel()])
    dists, indices = tree.query(pixel_coords, k=2 if border_width > 0 else 1)

    if border_width > 0:
        # Cell interiors: color from nearest seed
        nearest_idx = indices[:, 0]
        img_flat = pixels_srgb[nearest_idx]

        # Borders: where the two nearest seeds are almost equidistant
        d1, d2 = dists[:, 0], dists[:, 1]
        border_mask = (d2 - d1) < border_width
        img_flat[border_mask] = np.array(border_color, dtype=np.uint8)
    else:
        nearest_idx = indices if indices.ndim == 1 else indices[:, 0]
        img_flat = pixels_srgb[nearest_idx]

    img = img_flat.reshape(height, width, 3)
    return img, seeds


def render_payload_voronoi(enc, payload, output_path='encoded_voronoi.png',
                            width=400, height=400, seed=42,
                            border_width=1.5):
    """Render encoded payload as a Voronoi diagram and save.

    Args:
        enc: encoder dict
        payload: list of payload word indices
        output_path: where to save
        width, height: image dimensions
        seed: random seed for cell placement
        border_width: cell border thickness
    """
    pixels_lab, meta = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'],
        constellation_map=enc['constellation_map']
    )
    pixels_srgb = lab_to_srgb(pixels_lab)

    img, seeds = render_voronoi_image(
        pixels_srgb, width, height, seed=seed,
        border_width=border_width
    )

    fig, ax = plt.subplots(1, 1, figsize=(6, 6))
    ax.imshow(img)
    # Mark seed points
    ax.scatter(seeds[:, 0], seeds[:, 1], c='white', s=6, zorder=10,
               alpha=0.6, edgecolors='none')
    ax.set_xlim(0, width)
    ax.set_ylim(height, 0)
    ax.set_xticks([])
    ax.set_yticks([])
    ax.set_title(f'Voronoi Encoding: {len(payload)} words\n'
                 f'N={enc["n_palette"]}, ε={EPSILON:.1f}')

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight',
                facecolor='#0f0f23')
    plt.close()
    print(f"Saved Voronoi image to {output_path}")
    return img


def render_payload_image(enc, payload, output_path='encoded_image.png',
                          width=None):
    """Render the encoded payload as a 2D pixel grid image.

    Args:
        enc: encoder dict
        payload: list of payload word indices
        output_path: where to save
        width: image width in pixels (height computed to fit)
    """
    pixels_lab, meta = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'],
        constellation_map=enc['constellation_map']
    )
    pixels_srgb = lab_to_srgb(pixels_lab)

    n = len(payload)
    if width is None:
        width = max(int(np.ceil(np.sqrt(n))), 1)
    height = max(int(np.ceil(n / width)), 1)

    # Pad with black if needed
    total = width * height
    if total > n:
        pad = np.zeros((total - n, 3), dtype=np.uint8)
        pixels_srgb = np.vstack([pixels_srgb, pad])

    img = pixels_srgb[:total].reshape(height, width, 3)

    fig, ax = plt.subplots(1, 1, figsize=(max(width * 0.5, 3), max(height * 0.5, 3)))
    ax.imshow(img, interpolation='nearest', aspect='equal')
    ax.set_title(f'Encoded payload: {n} words, {width}x{height} pixels\n'
                 f'N={enc["n_palette"]}, ε={EPSILON:.1f}')
    ax.set_xticks(np.arange(-0.5, width, 1), minor=True)
    ax.set_yticks(np.arange(-0.5, height, 1), minor=True)
    ax.grid(which='minor', color='white', linewidth=0.5, alpha=0.5)
    ax.tick_params(which='minor', size=0)
    ax.set_xticks(range(width))
    ax.set_yticks(range(height))

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved pixel grid image to {output_path}")

    return img


def plot_palette_colors(enc, output_path='palette_colors.png'):
    """Visualize all palette colors as a strip with their sRGB values.

    Args:
        enc: encoder dict
        output_path: where to save
    """
    n_pal = enc['n_palette']
    curve = enc['curve']
    s_pts = np.linspace(0, curve.arc_length, n_pal)
    pts_lab = curve.eval(s_pts)
    pts_srgb = lab_to_srgb(pts_lab)

    fig, ax = plt.subplots(1, 1, figsize=(max(n_pal * 0.6, 6), 2))
    for i in range(n_pal):
        rgb = pts_srgb[i] / 255.0
        ax.add_patch(plt.Rectangle((i, 0), 1, 1, facecolor=rgb,
                                     edgecolor='white', linewidth=0.5))
    ax.set_xlim(0, n_pal)
    ax.set_ylim(0, 1)
    ax.set_xticks(np.arange(0.5, n_pal, 1))
    ax.set_xticklabels(range(n_pal), fontsize=7)
    ax.set_yticks([])
    ax.set_title(f'Palette Colors (N={n_pal})')
    ax.set_xlabel('Word index')

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved palette strip to {output_path}")


def main():
    parser = argparse.ArgumentParser(
        description='Visualize parametric curve encoding')
    parser.add_argument('--payload', type=str, default=None,
                        help='Comma-separated payload word indices')
    parser.add_argument('--random', type=int, default=None, metavar='N',
                        help='Generate N random payload words')
    parser.add_argument('--seed', type=int, default=42)
    parser.add_argument('--palette', default='viridis_approx')
    parser.add_argument('-N', '--n-palette', type=int, default=16)
    parser.add_argument('--output', type=str, default=None,
                        help='Output directory for plots')
    parser.add_argument('--mode', choices=['3d', 'image', 'voronoi', 'profile', 'palette', 'all'],
                        default='all', help='Which plot(s) to generate')
    parser.add_argument('--img-size', type=int, default=400,
                        help='Voronoi image width/height in pixels')
    args = parser.parse_args()

    # Build encoder
    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')
    enc = build_encoder(yaml_path, args.palette,
                        n_palette=args.n_palette)

    # Parse or generate payload
    if args.payload:
        payload = [int(x.strip()) for x in args.payload.split(',')]
    elif args.random:
        np.random.seed(args.seed)
        payload = np.random.randint(0, args.n_palette, size=args.random).tolist()
    else:
        # Default demo payload
        np.random.seed(args.seed)
        payload = np.random.randint(0, args.n_palette, size=24).tolist()

    # Output directory
    out_dir = args.output or os.path.dirname(__file__)

    cmap = enc['constellation_map']
    print(f"Palette: {args.palette}, N={args.n_palette}, epsilon={EPSILON}")
    print(f"Constellation M range: {cmap.M_min}..{cmap.M_max}")
    print(f"Capacity range: {cmap.capacity_min}..{cmap.capacity_max} positions/word")
    print(f"Bits/pixel: {enc['metadata']['bits_per_pixel']:.1f}")
    print(f"Payload: {payload[:10]}{'...' if len(payload) > 10 else ''} "
          f"({len(payload)} words)")
    print()

    if args.mode in ('3d', 'all'):
        plot_3d_curve_and_encoding(
            enc, payload,
            output_path=os.path.join(out_dir, 'encoding_3d.png'))

    if args.mode in ('voronoi', 'all'):
        render_payload_voronoi(
            enc, payload,
            output_path=os.path.join(out_dir, 'encoded_voronoi.png'),
            width=args.img_size, height=args.img_size, seed=args.seed)

    if args.mode in ('image', 'all'):
        render_payload_image(
            enc, payload,
            output_path=os.path.join(out_dir, 'encoded_image.png'))

    if args.mode in ('profile', 'all'):
        plot_tube_radius_profile(
            enc, output_path=os.path.join(out_dir, 'tube_profile.png'))

    if args.mode in ('palette', 'all'):
        plot_palette_colors(
            enc, output_path=os.path.join(out_dir, 'palette_colors.png'))


if __name__ == '__main__':
    main()
