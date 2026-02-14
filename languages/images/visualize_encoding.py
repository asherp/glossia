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
    PaletteCurve, BishopFrame, Constellation,
    compute_tube_radius, encode, decode, build_encoder,
    lab_to_srgb, srgb_to_lab,
)

import matplotlib
matplotlib.use('Agg')  # non-interactive backend
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d import Axes3D


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
        constellation = enc['constellation']
        M = constellation.M

        for wi in show_indices:
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
            enc['n_palette'], enc['epsilon'],
            tube_radius=enc['tube_radius']
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
        ax.set_title(f'Parametric Encoding in CIELAB\n'
                     f'N={enc["n_palette"]}, M={enc["constellation"].M}, '
                     f'ε={enc["epsilon"]:.1f}, r={enc["tube_radius"]:.1f}')
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
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 6), sharex=True)

    # Tube radius
    ax1.fill_between(s_pts, radii, alpha=0.3, color='steelblue')
    ax1.plot(s_pts, radii, color='steelblue', linewidth=2)
    ax1.axhline(y=enc['tube_radius'], color='red', linestyle='--',
                label=f'r_min = {enc["tube_radius"]:.1f}')
    ax1.axhline(y=enc['epsilon'], color='orange', linestyle=':',
                label=f'ε = {enc["epsilon"]:.1f}')
    ax1.set_ylabel('Tube radius (CIELAB Δ)')
    ax1.set_title('Tube Radius Profile Along Palette Curve')
    ax1.legend()
    ax1.grid(True, alpha=0.3)

    # Constellation capacity at each point
    eps = enc['epsilon']
    M_local = np.floor(2 * radii / eps).astype(int) + 1
    capacity_local = M_local ** 2
    ax2.fill_between(s_pts, capacity_local, alpha=0.3, color='forestgreen')
    ax2.plot(s_pts, capacity_local, color='forestgreen', linewidth=2)
    ax2.axhline(y=enc['constellation'].capacity, color='red', linestyle='--',
                label=f'Global M²={enc["constellation"].capacity}')
    ax2.set_xlabel('Arc length s (CIELAB Δ)')
    ax2.set_ylabel('Constellation capacity (M²)')
    ax2.legend()
    ax2.grid(True, alpha=0.3)

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved tube profile to {output_path}")


def render_payload_image(enc, payload, output_path='encoded_image.png',
                          width=None):
    """Render the encoded payload as a 2D pixel image.

    Args:
        enc: encoder dict
        payload: list of payload word indices
        output_path: where to save
        width: image width in pixels (height computed to fit)
    """
    pixels_lab, meta = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'],
        tube_radius=enc['tube_radius']
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
                 f'N={enc["n_palette"]}, ε={enc["epsilon"]:.1f}')
    # Grid lines
    ax.set_xticks(np.arange(-0.5, width, 1), minor=True)
    ax.set_yticks(np.arange(-0.5, height, 1), minor=True)
    ax.grid(which='minor', color='white', linewidth=0.5, alpha=0.5)
    ax.tick_params(which='minor', size=0)
    ax.set_xticks(range(width))
    ax.set_yticks(range(height))

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved encoded image to {output_path}")

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
    parser.add_argument('-e', '--epsilon', type=float, default=5.0)
    parser.add_argument('--output', type=str, default=None,
                        help='Output directory for plots')
    parser.add_argument('--mode', choices=['3d', 'image', 'profile', 'palette', 'all'],
                        default='all', help='Which plot(s) to generate')
    args = parser.parse_args()

    # Build encoder
    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')
    enc = build_encoder(yaml_path, args.palette,
                        n_palette=args.n_palette, epsilon=args.epsilon)

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

    print(f"Palette: {args.palette}, N={args.n_palette}, epsilon={args.epsilon}")
    print(f"Tube radius: {enc['tube_radius']:.1f} CIELAB")
    print(f"Constellation: {enc['constellation'].M}x{enc['constellation'].M} "
          f"= {enc['constellation'].capacity} positions/word")
    print(f"Bits/pixel: {enc['metadata']['bits_per_pixel']:.1f}")
    print(f"Payload: {payload[:10]}{'...' if len(payload) > 10 else ''} "
          f"({len(payload)} words)")
    print()

    if args.mode in ('3d', 'all'):
        plot_3d_curve_and_encoding(
            enc, payload,
            output_path=os.path.join(out_dir, 'encoding_3d.png'))

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
