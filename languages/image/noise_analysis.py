#!/usr/bin/env python3
"""
Noise analysis and capacity frontier for parametric curve encoding.

Simulates camera/sensor noise in CIELAB, measures decode error rates,
and maps the feasible (N, epsilon, error_rate) region.

Usage:
    python noise_analysis.py                          # default sweep
    python noise_analysis.py --sigma 1 2 3 5          # specific noise levels
    python noise_analysis.py --output noise_results/  # output directory
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
matplotlib.use('Agg')
import matplotlib.pyplot as plt


def measure_error_rate(enc, sigma, n_payload=50, n_trials=20, seed=0):
    """Measure decode error rate under Gaussian noise.

    Args:
        enc: encoder dict from build_encoder()
        sigma: noise standard deviation in CIELAB units
        n_payload: payload length per trial
        n_trials: number of independent trials
        seed: random seed

    Returns:
        word_error_rate: fraction of words decoded incorrectly
        position_error_rate: fraction of words at wrong sequence position
        details: dict with per-trial results
    """
    rng = np.random.RandomState(seed)
    n_pal = enc['n_palette']
    curve = enc['curve']
    frame = enc['frame']
    cmap = enc['constellation_map']

    word_errors = 0
    total_words = 0

    for trial in range(n_trials):
        payload = rng.randint(0, n_pal, size=n_payload).tolist()
        pixels_lab, _ = encode(payload, curve, frame, n_pal,
                               constellation_map=cmap)

        # Add Gaussian noise in CIELAB
        noise = rng.normal(0, sigma, pixels_lab.shape)
        noisy = pixels_lab + noise

        recovered = decode(noisy, curve, frame, n_pal,
                           constellation_map=cmap)

        # Count word identity errors
        for orig, rec in zip(payload, recovered):
            if orig != rec:
                word_errors += 1
        total_words += len(payload)

    word_error_rate = word_errors / total_words if total_words > 0 else 0

    return word_error_rate, {
        'sigma': sigma,
        'n_trials': n_trials,
        'n_payload': n_payload,
        'total_words': total_words,
        'word_errors': word_errors,
    }


def sweep_noise(enc, sigmas, n_payload=50, n_trials=20, seed=0, verbose=False):
    """Sweep noise levels and collect error rates.

    Args:
        enc: encoder dict
        sigmas: list of noise standard deviations
        n_payload: payload length per trial
        n_trials: number of trials per sigma
        seed: random seed
        verbose: print progress

    Returns:
        results: list of (sigma, word_error_rate, details) tuples
    """
    results = []
    for sigma in sigmas:
        wer, details = measure_error_rate(
            enc, sigma, n_payload, n_trials, seed)
        results.append((sigma, wer, details))
        if verbose:
            print(f"  sigma={sigma:5.1f}  word_err={wer:.4f} "
                  f"({details['word_errors']}/{details['total_words']})")
    return results


def sweep_palette_sizes_and_noise(yaml_path, palette_name, palette_sizes,
                                   sigmas, n_payload=30, n_trials=10,
                                   seed=0, verbose=False):
    """2D sweep over (N_palette, sigma) to map the capacity frontier.

    With fixed epsilon, the knob is palette size N.

    Returns:
        grid: (len(palette_sizes), len(sigmas)) array of word error rates
        meta: list of dicts with encoder metadata for each N
    """
    grid = np.zeros((len(palette_sizes), len(sigmas)))
    meta = []

    for i, n_pal in enumerate(palette_sizes):
        if verbose:
            print(f"\nN = {n_pal}:")
        enc = build_encoder(yaml_path, palette_name,
                            n_palette=n_pal)
        meta.append(enc['metadata'])
        cmap = enc['constellation_map']
        for j, sigma in enumerate(sigmas):
            wer, _ = measure_error_rate(
                enc, sigma, n_payload, n_trials, seed)
            grid[i, j] = wer
            if verbose:
                print(f"  sigma={sigma:4.1f}  M={cmap.M_min}..{cmap.M_max}  "
                      f"cap={cmap.capacity_min}..{cmap.capacity_max}  "
                      f"wer={wer:.4f}")
    return grid, meta


def plot_noise_sweep(results, enc, output_path='noise_sweep.png'):
    """Plot error rate vs. noise level.

    Args:
        results: from sweep_noise()
        enc: encoder dict
        output_path: where to save
    """
    sigmas = [r[0] for r in results]
    wers = [r[1] for r in results]

    fig, ax = plt.subplots(1, 1, figsize=(8, 5))
    ax.semilogy(sigmas, [max(w, 1e-4) for w in wers],
                'o-', color='steelblue', linewidth=2, markersize=8)

    # Mark EPSILON/3 threshold
    ax.axvline(x=EPSILON / 3, color='orange', linestyle='--',
               label=f'ε/3 = {EPSILON/3:.1f}')
    ax.axvline(x=EPSILON / 2, color='red', linestyle=':',
               label=f'ε/2 = {EPSILON/2:.1f}')
    ax.axhline(y=0.01, color='gray', linestyle=':', alpha=0.5,
               label='1% error')

    cmap = enc['constellation_map']
    ax.set_xlabel('Noise σ (CIELAB units)')
    ax.set_ylabel('Word Error Rate')
    ax.set_title(f'Noise Robustness\n'
                 f'N={enc["n_palette"]}, M={cmap.M_min}..{cmap.M_max}, '
                 f'ε={EPSILON:.1f}')
    ax.legend()
    ax.grid(True, alpha=0.3)
    ax.set_ylim(bottom=5e-5)

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved noise sweep to {output_path}")


def plot_capacity_frontier(grid, palette_sizes, sigmas, n_palette,
                           meta_list, output_path='capacity_frontier.png'):
    """Heatmap of error rates over (N, sigma) space.

    Args:
        grid: (n_N, n_sigma) error rate array
        palette_sizes, sigmas: axis values
        n_palette: default N (informational)
        meta_list: list of encoder metadata dicts
        output_path: where to save
    """
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 5))

    # Heatmap
    im = ax1.imshow(grid, aspect='auto', origin='lower',
                    extent=[sigmas[0], sigmas[-1],
                            palette_sizes[0], palette_sizes[-1]],
                    cmap='RdYlGn_r', vmin=0, vmax=0.5)
    ax1.set_xlabel('Noise σ (CIELAB)')
    ax1.set_ylabel('Palette size N')
    ax1.set_title(f'Word Error Rate\nε={EPSILON}')
    plt.colorbar(im, ax=ax1, label='Word Error Rate')

    # Overlay 1% and 5% contours
    try:
        cs = ax1.contour(sigmas, palette_sizes, grid,
                         levels=[0.01, 0.05, 0.10],
                         colors=['white', 'yellow', 'red'],
                         linewidths=1.5)
        ax1.clabel(cs, fmt='%.0f%%', fontsize=8)
    except Exception:
        pass  # contours may fail if grid is too coarse

    # Bits/pixel vs N
    bpps = [m['bits_per_pixel'] for m in meta_list]
    ax2.plot(palette_sizes, bpps, 'o-', color='steelblue', linewidth=2, markersize=8)
    ax2.set_xlabel('Palette size N')
    ax2.set_ylabel('Bits per pixel')
    ax2.set_title(f'Encoding Capacity vs. Palette Size\nε={EPSILON}')
    ax2.grid(True, alpha=0.3)

    # Annotate M range
    for i, n_pal in enumerate(palette_sizes):
        m_min = meta_list[i]['M_min']
        m_max = meta_list[i]['M_max']
        ax2.annotate(f'M={m_min}..{m_max}', (n_pal, bpps[i]),
                     textcoords='offset points', xytext=(5, 5),
                     fontsize=7)

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved capacity frontier to {output_path}")


def plot_resolution_scaling(enc, target_payloads, target_error=0.01,
                            output_path='resolution_scaling.png'):
    """Plot minimum image dimensions for various payload sizes.

    Uses the theoretical capacity bound:
        n_pixels >= n_payload_words * max_repeats_per_word / constellation_capacity

    For a square image: side = ceil(sqrt(n_pixels))

    Args:
        enc: encoder dict
        target_payloads: list of payload sizes (total words)
        target_error: target error rate (informational)
        output_path: where to save
    """
    cmap = enc['constellation_map']
    N = enc['n_palette']
    bpp = enc['metadata']['bits_per_pixel']

    fig, ax = plt.subplots(1, 1, figsize=(8, 5))

    # Simple model: each pixel encodes one word
    # So n_pixels = n_payload_words
    sides = [int(np.ceil(np.sqrt(n))) for n in target_payloads]
    bits = [n * bpp for n in target_payloads]

    ax.plot(target_payloads, sides, 'o-', color='steelblue',
            linewidth=2, markersize=8)
    ax.set_xlabel('Payload words')
    ax.set_ylabel('Image side length (pixels)')
    ax.set_title(f'Minimum Image Size\n'
                 f'N={N}, M={cmap.M_min}..{cmap.M_max}, {bpp:.1f} bits/pixel')
    ax.grid(True, alpha=0.3)

    # Secondary y-axis: total payload bits
    ax2 = ax.twinx()
    ax2.plot(target_payloads, bits, 's--', color='coral',
             linewidth=1.5, markersize=6, alpha=0.7)
    ax2.set_ylabel('Total payload bits', color='coral')

    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved resolution scaling to {output_path}")


def main():
    parser = argparse.ArgumentParser(
        description='Noise analysis for parametric curve encoding')
    parser.add_argument('--sigma', type=float, nargs='+',
                        default=None,
                        help='Noise levels to test (CIELAB units)')
    parser.add_argument('--palette', default='viridis_approx')
    parser.add_argument('-N', '--n-palette', type=int, default=16)
    parser.add_argument('--n-payload', type=int, default=30,
                        help='Payload length per trial')
    parser.add_argument('--n-trials', type=int, default=20,
                        help='Number of trials per noise level')
    parser.add_argument('--seed', type=int, default=42)
    parser.add_argument('--output', type=str, default=None,
                        help='Output directory')
    parser.add_argument('--frontier', action='store_true',
                        help='Run full (epsilon, sigma) frontier sweep')
    parser.add_argument('-v', '--verbose', action='store_true')
    args = parser.parse_args()

    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')
    out_dir = args.output or os.path.dirname(__file__)

    # Default sigma sweep
    if args.sigma is None:
        sigmas = [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.0, 10.0]
    else:
        sigmas = args.sigma

    print(f"Noise Analysis")
    print(f"==============")
    print(f"Palette: {args.palette}, N={args.n_palette}, epsilon={EPSILON}")
    print(f"Payload: {args.n_payload} words x {args.n_trials} trials")
    print(f"Noise levels: {sigmas}")
    print()

    # --- Noise sweep with fixed epsilon ---
    enc = build_encoder(yaml_path, args.palette,
                        n_palette=args.n_palette)
    cmap = enc['constellation_map']
    print(f"M range: {cmap.M_min}..{cmap.M_max}, "
          f"capacity range: {cmap.capacity_min}..{cmap.capacity_max}/word, "
          f"bits/pixel={enc['metadata']['bits_per_pixel']:.1f}")
    print()

    print("Noise sweep:")
    results = sweep_noise(enc, sigmas, args.n_payload, args.n_trials,
                          args.seed, verbose=True)
    plot_noise_sweep(results, enc,
                     output_path=os.path.join(out_dir, 'noise_sweep.png'))

    # --- Resolution scaling ---
    plot_resolution_scaling(
        enc,
        target_payloads=[10, 25, 50, 100, 200, 500, 1000],
        output_path=os.path.join(out_dir, 'resolution_scaling.png'))

    # --- Full frontier sweep (palette size x sigma) ---
    if args.frontier:
        print("\nCapacity frontier sweep (N x sigma):")
        palette_sizes = [8, 16, 32, 64, 128]
        grid, meta = sweep_palette_sizes_and_noise(
            yaml_path, args.palette, palette_sizes,
            sigmas,
            n_payload=args.n_payload,
            n_trials=max(args.n_trials // 2, 5),
            seed=args.seed, verbose=args.verbose)
        plot_capacity_frontier(
            grid, palette_sizes, sigmas, args.n_palette, meta,
            output_path=os.path.join(out_dir, 'capacity_frontier.png'))


if __name__ == '__main__':
    main()
