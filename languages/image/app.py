#!/usr/bin/env python3
"""
Flask demo for parametric curve image encoding.

Encode payload word sequences into colored pixel image and decode them back.
Visualizes the palette, encoded image, and round-trip verification.

Usage:
    python app.py              # runs on http://localhost:5001
    python app.py --port 8080  # custom port
"""

import os
import sys
import io
import base64
import argparse
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from parametric_encoding import (
    PaletteCurve, BishopFrame,
    encode, decode, lab_to_srgb,
    select_encoding_params, encode_header, decode_header,
    derive_config_table,
)

from visualize_encoding import render_voronoi_image, generate_voronoi_seeds

from flask import Flask, request, jsonify, render_template_string
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

app = Flask(__name__)

# ---------------------------------------------------------------------------
# Optimal config cache — derived from curve geometry, cached by palette name
# ---------------------------------------------------------------------------

YAML_PATH = os.path.join(os.path.dirname(__file__), 'palette.yaml')
CONFIG_CACHE = {}


def get_optimal_config(palette_name='viridis_approx'):
    """Build curve + frame and derive optimal (N, epsilon) for a palette.

    Caches by palette name since the optimal config depends only on the
    curve geometry.
    """
    if palette_name not in CONFIG_CACHE:
        curve = PaletteCurve.from_yaml(YAML_PATH, palette_name)
        frame = BishopFrame(curve)
        configs = derive_config_table(curve, frame)
        opt = select_encoding_params(curve, frame, configs=configs)
        CONFIG_CACHE[palette_name] = {
            'curve': curve,
            'frame': frame,
            'opt': opt,
            'configs': configs,
        }
    return CONFIG_CACHE[palette_name]


# ---------------------------------------------------------------------------
# Image rendering helpers
# ---------------------------------------------------------------------------

def generate_circular_seeds(n, img_size, seed=None, relax_iters=5):
    """Generate well-spaced seed points inside a circular boundary.

    Seeds are placed in polar coordinates (r, theta) using sqrt(r) sampling
    for uniform area density, then Lloyd-relaxed within the disk.

    Args:
        n: number of seed points
        img_size: canvas dimension (circle centered at img_size/2)
        seed: random seed for reproducibility
        relax_iters: Lloyd relaxation iterations

    Returns:
        (n, 2) array of (x, y) seed coordinates within the disk
    """
    from scipy.spatial import KDTree

    rng = np.random.RandomState(seed)
    cx, cy = img_size / 2.0, img_size / 2.0
    radius = img_size / 2.0 - 2  # slight inset

    # Uniform sampling in a disk: r = R * sqrt(U), theta = 2*pi*V
    r = radius * np.sqrt(rng.uniform(0, 1, n))
    theta = rng.uniform(0, 2 * np.pi, n)
    seeds = np.column_stack([
        cx + r * np.cos(theta),
        cy + r * np.sin(theta),
    ])

    # Lloyd relaxation constrained to the disk
    for _ in range(relax_iters):
        tree = KDTree(seeds)
        # Dense sample grid within bounding box, filter to disk
        sx = np.linspace(0, img_size, min(img_size, 200))
        sy = np.linspace(0, img_size, min(img_size, 200))
        gx, gy = np.meshgrid(sx, sy)
        sample_pts = np.column_stack([gx.ravel(), gy.ravel()])
        # Keep only points inside the disk
        dist_from_center = np.sqrt((sample_pts[:, 0] - cx)**2 +
                                   (sample_pts[:, 1] - cy)**2)
        mask_disk = dist_from_center <= radius
        sample_pts = sample_pts[mask_disk]

        _, indices = tree.query(sample_pts)

        new_seeds = seeds.copy()
        for i in range(n):
            mask = indices == i
            if np.any(mask):
                new_seeds[i] = sample_pts[mask].mean(axis=0)

        # Project back into the disk if needed
        dx = new_seeds[:, 0] - cx
        dy = new_seeds[:, 1] - cy
        dist = np.sqrt(dx**2 + dy**2)
        outside = dist > radius
        if np.any(outside):
            scale = radius / dist[outside]
            new_seeds[outside, 0] = cx + dx[outside] * scale
            new_seeds[outside, 1] = cy + dy[outside] * scale
        seeds = new_seeds

    return seeds


def render_voronoi_svg(pixels_srgb, img_size=400, seed=None, circular=False):
    """Render encoded colors as an SVG Voronoi diagram.

    Uses scipy.spatial.Voronoi for exact polygon computation.
    Returns an SVG string that can be embedded directly in HTML.

    Args:
        pixels_srgb: (N, 3) array of sRGB colors
        img_size: canvas size in pixels
        seed: random seed for reproducibility
        circular: if True, use circular boundary with transparent outside
    """
    from scipy.spatial import Voronoi

    n = len(pixels_srgb)
    pixels_srgb = np.asarray(pixels_srgb, dtype=np.uint8)

    if circular:
        seeds = generate_circular_seeds(n, img_size, seed=seed, relax_iters=5)
    else:
        seeds = generate_voronoi_seeds(n, img_size, img_size, seed=seed,
                                        relax_iters=5)

    # Add mirror points around the boundary so all cells are finite
    mirror_pts = np.vstack([
        seeds,
        np.column_stack([seeds[:, 0], -seeds[:, 1]]),           # top mirror
        np.column_stack([seeds[:, 0], 2 * img_size - seeds[:, 1]]),  # bottom
        np.column_stack([-seeds[:, 0], seeds[:, 1]]),           # left
        np.column_stack([2 * img_size - seeds[:, 0], seeds[:, 1]]),  # right
    ])

    vor = Voronoi(mirror_pts)

    # Build SVG
    lines = []
    lines.append(f'<svg xmlns="http://www.w3.org/2000/svg" '
                 f'viewBox="0 0 {img_size} {img_size}" '
                 f'width="100%" height="100%" '
                 f'style="border-radius:6px;">')

    if circular:
        # Define a circular clip path; everything outside is transparent
        cr = img_size / 2.0
        lines.append('<defs>')
        lines.append(f'  <clipPath id="circle-clip">')
        lines.append(f'    <circle cx="{cr:.1f}" cy="{cr:.1f}" r="{cr - 1:.1f}"/>')
        lines.append(f'  </clipPath>')
        lines.append('</defs>')
        lines.append(f'<g clip-path="url(#circle-clip)">')
        # Background inside the circle
        lines.append(f'<rect width="{img_size}" height="{img_size}" '
                     f'fill="#0f0f23"/>')
    else:
        lines.append(f'<rect width="{img_size}" height="{img_size}" '
                     f'fill="#0f0f23"/>')

    # Draw Voronoi cells for the original seeds (indices 0..n-1)
    for i in range(n):
        region_idx = vor.point_region[i]
        region = vor.regions[region_idx]
        if -1 in region or len(region) == 0:
            continue

        vertices = vor.vertices[region]
        # Clip to canvas
        vertices = np.clip(vertices, 0, img_size)
        points_str = ' '.join(f'{x:.1f},{y:.1f}' for x, y in vertices)

        r, g, b = int(pixels_srgb[i][0]), int(pixels_srgb[i][1]), int(pixels_srgb[i][2])
        lines.append(
            f'<polygon points="{points_str}" '
            f'fill="rgb({r},{g},{b})" '
            f'stroke="#14142a" stroke-width="1.5" '
            f'stroke-linejoin="round"/>'
        )

    if circular:
        lines.append('</g>')

    lines.append('</svg>')
    return '\n'.join(lines)


def perturb_lab(pixels_lab, noise_sigma=0.0, brightness=0.0,
                color_temp=0.0, saturation=1.0, rng=None):
    """Apply environmental perturbations to CIELAB colors.

    All operations are native CIELAB, matching the encoding space.

    Args:
        pixels_lab: (N, 3) array of [L*, a*, b*] colors
        noise_sigma: Gaussian noise std-dev in CIELAB units (0 = none)
        brightness: L* additive offset (-40 to +40), simulates exposure change
        color_temp: warm/cool shift. Positive = warm (tungsten-like, +b*),
                    negative = cool (shade-like, -b*). Range ~ -30 to +30.
                    Also applies a slight a* shift (0.15 * color_temp).
        saturation: chroma multiplier (1.0 = unchanged, <1 desaturated,
                    >1 oversaturated). Scales a* and b* jointly.
        rng: numpy RandomState for reproducible noise

    Returns:
        (N, 3) perturbed CIELAB array (clamped to valid L* range)
    """
    if rng is None:
        rng = np.random.RandomState()

    out = np.array(pixels_lab, dtype=np.float64)

    # 1. Saturation: scale chroma (a*, b*) around the L* axis
    if saturation != 1.0:
        out[:, 1] *= saturation
        out[:, 2] *= saturation

    # 2. Color temperature: shift along b* (yellow-blue) with slight a*
    if color_temp != 0.0:
        out[:, 2] += color_temp           # b* shift
        out[:, 1] += color_temp * 0.15    # slight green-magenta coupling

    # 3. Brightness: uniform L* offset
    if brightness != 0.0:
        out[:, 0] += brightness

    # 4. Gaussian noise in all three channels
    if noise_sigma > 0:
        out += rng.normal(0, noise_sigma, out.shape)

    # Clamp L* to [0, 100]
    out[:, 0] = np.clip(out[:, 0], 0, 100)

    return out


def render_palette_strip(curve, s_palette, scale=40):
    """Render palette as a horizontal color strip, return PNG bytes."""
    n_pal = len(s_palette)
    pts_lab = curve.eval(s_palette)
    pts_srgb = lab_to_srgb(pts_lab)

    img = pts_srgb.reshape(1, n_pal, 3)

    # Scale down for large palettes
    effective_scale = scale if n_pal <= 32 else max(scale * 32 / n_pal, 5)
    fig_w = max(n_pal * effective_scale / 100, 3)
    fig, ax = plt.subplots(1, 1, figsize=(fig_w, 0.8))
    ax.imshow(img, interpolation='nearest', aspect='auto')
    if n_pal <= 32:
        ax.set_xticks(range(n_pal))
        ax.set_xticklabels(range(n_pal), fontsize=7, color='#c0c0c0')
    else:
        ax.set_xticks([])
    ax.set_yticks([])
    ax.tick_params(axis='x', colors='#c0c0c0')
    plt.tight_layout(pad=0.2)

    buf = io.BytesIO()
    plt.savefig(buf, format='png', dpi=150, bbox_inches='tight',
                facecolor='#1a1a2e')
    plt.close()
    buf.seek(0)
    return buf.getvalue()


def img_to_data_uri(png_bytes):
    b64 = base64.b64encode(png_bytes).decode('ascii')
    return f"data:image/png;base64,{b64}"


# ---------------------------------------------------------------------------
# HTML template
# ---------------------------------------------------------------------------

HTML_TEMPLATE = """
<!DOCTYPE html>
<html>
<head>
<title>Glossia Image Encoder</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: 'Menlo', 'Consolas', monospace;
    background: #0f0f23;
    color: #c0c0d0;
    min-height: 100vh;
    padding: 2rem;
  }
  h1 {
    color: #7fefff;
    font-size: 1.5rem;
    margin-bottom: 0.3rem;
  }
  .subtitle {
    color: #666680;
    font-size: 0.85rem;
    margin-bottom: 2rem;
  }
  .container {
    max-width: 800px;
    margin: 0 auto;
  }
  .panel {
    background: #1a1a2e;
    border: 1px solid #2a2a4a;
    border-radius: 8px;
    padding: 1.5rem;
    margin-bottom: 1.5rem;
  }
  .panel h2 {
    color: #a0a0ff;
    font-size: 0.95rem;
    margin-bottom: 1rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }
  label {
    display: block;
    color: #8888aa;
    font-size: 0.8rem;
    margin-bottom: 0.3rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  input[type=text], input[type=range], select {
    width: 100%;
    padding: 0.6rem 0.8rem;
    background: #0f0f23;
    border: 1px solid #3a3a5a;
    border-radius: 4px;
    color: #e0e0ff;
    font-family: inherit;
    font-size: 0.9rem;
    margin-bottom: 1rem;
  }
  input[type=range] {
    -webkit-appearance: none;
    height: 6px;
    border-radius: 3px;
    background: #2a2a4a;
    border: none;
    padding: 0;
    margin-top: 0.4rem;
  }
  input[type=range]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #7fefff;
    cursor: pointer;
  }
  .slider-value {
    color: #7fefff;
    font-size: 0.9rem;
    font-weight: bold;
    float: right;
  }
  input:focus, select:focus {
    outline: none;
    border-color: #7fefff;
  }
  .row {
    display: flex;
    gap: 1rem;
  }
  .row > div { flex: 1; }
  button {
    background: #2a4a6a;
    color: #7fefff;
    border: 1px solid #3a6a8a;
    padding: 0.7rem 1.5rem;
    border-radius: 4px;
    font-family: inherit;
    font-size: 0.9rem;
    cursor: pointer;
    margin-right: 0.5rem;
    transition: background 0.2s;
  }
  button:hover { background: #3a5a7a; }
  button.secondary {
    background: #2a2a3e;
    border-color: #4a4a6a;
    color: #a0a0c0;
  }
  .image-box {
    text-align: center;
    margin: 1rem 0;
  }
  .image-box img {
    max-width: 100%;
    border-radius: 4px;
    border: 1px solid #2a2a4a;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 0.8rem;
    margin-top: 1rem;
  }
  .stat {
    background: #0f0f23;
    border-radius: 4px;
    padding: 0.6rem;
    text-align: center;
  }
  .stat .value {
    color: #7fefff;
    font-size: 1.3rem;
    font-weight: bold;
  }
  .stat .label {
    color: #666680;
    font-size: 0.7rem;
    text-transform: uppercase;
    margin-top: 0.2rem;
  }
  .stat.derived .value { color: #ffd700; }
  .stat.derived .label { color: #998a00; }
  .roundtrip {
    margin-top: 1rem;
    padding: 0.8rem;
    border-radius: 4px;
    font-size: 0.85rem;
  }
  .roundtrip.pass { background: #0a2a1a; border: 1px solid #1a5a3a; color: #5fdf8f; }
  .roundtrip.fail { background: #2a0a0a; border: 1px solid #5a1a1a; color: #df5f5f; }
  .decoded-list {
    color: #a0a0c0;
    font-size: 0.85rem;
    margin-top: 0.5rem;
    word-break: break-all;
  }
  .pixel-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.75rem;
    margin-top: 0.5rem;
  }
  .pixel-table th {
    color: #666680;
    text-align: left;
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid #2a2a4a;
  }
  .pixel-table td {
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid #1a1a2e;
  }
  .swatch {
    display: inline-block;
    width: 16px;
    height: 16px;
    border-radius: 2px;
    vertical-align: middle;
    margin-right: 0.4rem;
    border: 1px solid #3a3a5a;
  }
  .side-by-side {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.5rem;
  }
  .side-by-side .image-box { margin: 0; }
  .side-by-side h3 {
    color: #8888aa;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 0.5rem;
    text-align: center;
  }
  .error-word { color: #df5f5f; font-weight: bold; }
  .ok-word { color: #5fdf8f; }
  .slider-row {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    margin-bottom: 0.8rem;
  }
  .slider-row label {
    min-width: 110px;
    margin-bottom: 0;
    flex-shrink: 0;
  }
  .slider-row input[type=range] {
    flex: 1;
    margin-bottom: 0;
  }
  .slider-row .slider-value {
    min-width: 50px;
    text-align: right;
    float: none;
  }
  .perturb-panel {
    background: #1a1a2e;
    border: 1px solid #3a2a2a;
    border-radius: 8px;
    padding: 1.5rem;
    margin-bottom: 1.5rem;
  }
  .perturb-panel h2 { color: #ffaa7f; }
  .header-tag {
    color: #ffd700;
    font-weight: bold;
    font-size: 0.7rem;
  }
</style>
</head>
<body>
<div class="container">
  <h1>Glossia Image Encoder</h1>
  <p class="subtitle">Parametric curve encoding in CIELAB color space &mdash; self-describing adaptive radix</p>

  <form method="POST" action="/encode" id="main-form">
    <div class="panel">
      <h2>Payload</h2>
      <label>Word indices (comma-separated, 0 to {{ derived_N - 1 }})</label>
      <input type="text" name="payload" value="{{ payload_str }}"
             placeholder="0,3,3,7,15,8,8,8,12,5" id="payload-input">

      <label>Cells <span class="slider-value" id="cells-display">{{ n_cells }}</span></label>
      <input type="range" name="n_cells" id="cells-slider"
             min="3" max="100" value="{{ n_cells }}"
             oninput="document.getElementById('cells-display').textContent=this.value"
             onchange="autoEncode()">

      <label style="display:inline-flex;align-items:center;gap:0.5rem;cursor:pointer;margin-bottom:1rem;">
        <input type="checkbox" name="circular" value="1"
               {{ 'checked' if circular }}
               style="accent-color:#7fefff;width:16px;height:16px;">
        Circular boundary
      </label>

      <div class="row">
        <div>
          <label>Palette</label>
          <select name="palette" onchange="autoEncode()">
            <option value="viridis_approx" {{ 'selected' if palette=='viridis_approx' }}>viridis_approx</option>
            <option value="warm" {{ 'selected' if palette=='warm' }}>warm</option>
            <option value="cool" {{ 'selected' if palette=='cool' }}>cool</option>
          </select>
        </div>
        <div>
          <label>Image size</label>
          <input type="text" name="img_size" value="{{ img_size }}">
        </div>
        <div>
          <label>Seed</label>
          <input type="text" name="seed" value="{{ seed }}">
        </div>
      </div>

      <div class="row" style="margin-bottom:1rem;">
        <div class="stat derived" style="padding:0.4rem 0.6rem;">
          <div class="value" style="font-size:1rem;">N={{ derived_N }}</div>
          <div class="label">palette size (derived)</div>
        </div>
        <div class="stat derived" style="padding:0.4rem 0.6rem;">
          <div class="value" style="font-size:1rem;">&epsilon;={{ "%.1f"|format(derived_eps) }}</div>
          <div class="label">grid spacing (derived)</div>
        </div>
        <div class="stat derived" style="padding:0.4rem 0.6rem;">
          <div class="value" style="font-size:1rem;">{{ derived_bpc }} bpc</div>
          <div class="label">bits/cell (derived)</div>
        </div>
      </div>

      <button type="submit">Encode</button>
      <button type="submit" formaction="/random" class="secondary">Random</button>
    </div>
  </form>

  {% if palette_img %}
  <div class="panel">
    <h2>Palette</h2>
    <div class="image-box">
      <img src="{{ palette_img }}" alt="Palette strip">
    </div>
  </div>
  {% endif %}

  {% if encoded_svg %}
  <div class="panel">
    <h2>Encoded Image</h2>

    {% if perturbed_svg %}
    <div class="side-by-side">
      <div>
        <h3>Clean</h3>
        <div class="image-box">{{ encoded_svg|safe }}</div>
      </div>
      <div>
        <h3>Perturbed</h3>
        <div class="image-box">{{ perturbed_svg|safe }}</div>
      </div>
    </div>
    {% else %}
    <div class="image-box">
      {{ encoded_svg|safe }}
    </div>
    {% endif %}

    <div class="stats">
      <div class="stat derived">
        <div class="value">{{ derived_N }}</div>
        <div class="label">N (derived)</div>
      </div>
      <div class="stat derived">
        <div class="value">{{ "%.1f"|format(derived_eps) }}</div>
        <div class="label">&epsilon; (derived)</div>
      </div>
      <div class="stat derived">
        <div class="value">{{ derived_bpc }}</div>
        <div class="label">Bits/cell</div>
      </div>
      <div class="stat">
        <div class="value">{{ n_words }}</div>
        <div class="label">Payload words</div>
      </div>
      <div class="stat">
        <div class="value">{{ n_words + 1 }}</div>
        <div class="label">Total cells</div>
      </div>
      <div class="stat">
        <div class="value">{{ meta.M_min }}..{{ meta.M_max }}</div>
        <div class="label">M range</div>
      </div>
      <div class="stat">
        <div class="value">{{ meta.capacity_min }}..{{ meta.capacity_max }}</div>
        <div class="label">Capacity range</div>
      </div>
      <div class="stat">
        <div class="value">{{ meta.max_word_repeats }}</div>
        <div class="label">Max repeats</div>
      </div>
      <div class="stat">
        <div class="value">{{ total_bits }}</div>
        <div class="label">Total bits</div>
      </div>
      <div class="stat">
        <div class="value">{{ "%.1f"|format(meta.sigma95) }}</div>
        <div class="label">&sigma;95 (single px)</div>
      </div>
      <div class="stat">
        <div class="value">{{ meta.px_per_cell }}</div>
        <div class="label">px / cell</div>
      </div>
      <div class="stat">
        <div class="value">&times;{{ "%.0f"|format(meta.noise_reduction) }}</div>
        <div class="label">&radic;(px/cell) avg</div>
      </div>
      <div class="stat">
        <div class="value">{{ "%.1f"|format(meta.effective_sigma95) }}</div>
        <div class="label">&sigma;95 (averaged)</div>
      </div>
    </div>
  </div>

  <div class="perturb-panel">
    <h2>Environment Simulation</h2>
    <div class="slider-row">
      <label>Noise &sigma; <span class="slider-value" id="noise-display">{{ noise_sigma }}</span></label>
      <input type="range" name="noise_sigma" id="noise-slider"
             min="0" max="20" step="0.5" value="{{ noise_sigma }}"
             form="main-form"
             oninput="document.getElementById('noise-display').textContent=this.value"
             onchange="autoEncode()">
    </div>
    <div class="slider-row">
      <label>Brightness <span class="slider-value" id="bright-display">{{ brightness }}</span></label>
      <input type="range" name="brightness" id="bright-slider"
             min="-40" max="40" step="1" value="{{ brightness }}"
             form="main-form"
             oninput="document.getElementById('bright-display').textContent=this.value"
             onchange="autoEncode()">
    </div>
    <div class="slider-row">
      <label>Color temp <span class="slider-value" id="temp-display">{{ color_temp }}</span></label>
      <input type="range" name="color_temp" id="temp-slider"
             min="-30" max="30" step="1" value="{{ color_temp }}"
             form="main-form"
             oninput="document.getElementById('temp-display').textContent=this.value"
             onchange="autoEncode()">
    </div>
    <div class="slider-row">
      <label>Saturation <span class="slider-value" id="sat-display">{{ saturation }}</span></label>
      <input type="range" name="saturation" id="sat-slider"
             min="0.3" max="2.0" step="0.05" value="{{ saturation }}"
             form="main-form"
             oninput="document.getElementById('sat-display').textContent=parseFloat(this.value).toFixed(2)"
             onchange="autoEncode()">
    </div>
  </div>

  {% if perturbed_svg %}
  <div class="panel">
    <h2>Decode Under Perturbation</h2>
    <div class="roundtrip {{ 'pass' if perturb_accuracy == 100.0 else 'fail' }}">
      Accuracy: {{ "%.1f"|format(perturb_accuracy) }}%
      &mdash; {{ perturb_correct }}/{{ n_words }} words recovered
      ({{ perturb_bits_recovered }}/{{ total_bits }} bits)
    </div>
    <div class="decoded-list" style="margin-top:0.8rem;">
      <strong>Word-by-word:</strong><br>
      {% for w in perturb_words %}
        <span class="{{ 'ok-word' if w.ok else 'error-word' }}">
          [{{ w.idx }}] {{ w.expected }}{%- if not w.ok %}&rarr;{{ w.got }}{% endif %}
        </span>
      {% endfor %}
    </div>
    <div style="margin-top:0.8rem;color:#8888aa;font-size:0.8rem;">
      &Delta;L*={{ "%.1f"|format(brightness) }},
      &Delta;b*={{ "%.1f"|format(color_temp) }},
      sat&times;{{ "%.2f"|format(saturation) }},
      &sigma;={{ noise_sigma }}
    </div>
  </div>
  {% endif %}

  <div class="panel">
    <h2>Round-trip Verification (Clean)</h2>
    <div class="roundtrip {{ 'pass' if roundtrip_ok else 'fail' }}">
      {{ 'PASS' if roundtrip_ok else 'FAIL' }}:
      decode(encode(payload)) {{ '==' if roundtrip_ok else '!=' }} payload
    </div>
    <div class="decoded-list">
      <strong>Decoded:</strong> {{ decoded }}
    </div>
  </div>

  <div class="panel">
    <h2>Pixel Details</h2>
    <table class="pixel-table">
      <tr><th>#</th><th>Word</th><th>Color</th><th>CIELAB</th><th>sRGB</th></tr>
      {% for p in pixels %}
      <tr>
        <td>{{ p.idx }}</td>
        <td>
          {% if p.word == 'HEADER' %}
            <span class="header-tag">HEADER</span>
          {% else %}
            {{ p.word }}
          {% endif %}
        </td>
        <td><span class="swatch" style="background:rgb({{ p.r }},{{ p.g }},{{ p.b }})"></span></td>
        <td>L*={{ "%.1f"|format(p.L) }} a*={{ "%.1f"|format(p.a) }} b*={{ "%.1f"|format(p.bstar) }}</td>
        <td>({{ p.r }}, {{ p.g }}, {{ p.b }})</td>
      </tr>
      {% endfor %}
    </table>
  </div>
  {% endif %}
</div>
<script>
function autoEncode() {
  var form = document.getElementById('main-form');
  form.action = '/random';
  form.submit();
}
</script>
</body>
</html>
"""


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------

@app.route('/', methods=['GET'])
def index():
    cfg = get_optimal_config()
    opt = cfg['opt']
    palette_img = img_to_data_uri(
        render_palette_strip(cfg['curve'], opt['s_palette']))
    return render_template_string(
        HTML_TEMPLATE,
        payload_str='0,3,3,7,15,8,8,8,12,5',
        palette='viridis_approx',
        img_size=400,
        seed=42,
        n_cells=20,
        circular=False,
        noise_sigma=0.0,
        brightness=0.0,
        color_temp=0.0,
        saturation=1.0,
        palette_img=palette_img,
        encoded_svg=None,
        perturbed_svg=None,
        meta=None,
        total_bits=0,
        roundtrip_ok=None,
        decoded=None,
        pixels=None,
        n_words=0,
        perturb_accuracy=100.0,
        perturb_correct=0,
        perturb_bits_recovered=0,
        perturb_words=[],
        derived_N=opt['N'],
        derived_eps=opt['epsilon'],
        derived_bpc=opt['bits_per_cell'],
    )


def parse_perturb_params(form):
    """Extract perturbation parameters from form data."""
    return {
        'noise_sigma': float(form.get('noise_sigma', 0.0)),
        'brightness': float(form.get('brightness', 0.0)),
        'color_temp': float(form.get('color_temp', 0.0)),
        'saturation': float(form.get('saturation', 1.0)),
    }


@app.route('/random', methods=['POST'])
def random_payload():
    palette = request.form.get('palette', 'viridis_approx')
    img_size = int(request.form.get('img_size', 400))
    voronoi_seed = int(request.form.get('seed', 42))
    n_cells = int(request.form.get('n_cells', 20))
    circular = request.form.get('circular') == '1'
    perturb = parse_perturb_params(request.form)

    cfg = get_optimal_config(palette)
    N = cfg['opt']['N']

    np.random.seed(None)  # true random
    payload = np.random.randint(0, N, size=n_cells).tolist()
    payload_str = ','.join(str(w) for w in payload)

    return do_encode(payload_str, palette,
                     img_size, voronoi_seed, circular=circular,
                     perturb=perturb)


@app.route('/encode', methods=['POST'])
def encode_route():
    payload_str = request.form.get('payload', '0,1,2,3')
    palette = request.form.get('palette', 'viridis_approx')
    img_size = int(request.form.get('img_size', 400))
    voronoi_seed = int(request.form.get('seed', 42))
    circular = request.form.get('circular') == '1'
    perturb = parse_perturb_params(request.form)

    return do_encode(payload_str, palette,
                     img_size, voronoi_seed, circular=circular,
                     perturb=perturb)


def do_encode(payload_str, palette,
              img_size, voronoi_seed, circular=False, perturb=None):
    if perturb is None:
        perturb = {'noise_sigma': 0.0, 'brightness': 0.0,
                   'color_temp': 0.0, 'saturation': 1.0}

    cfg = get_optimal_config(palette)
    curve = cfg['curve']
    frame = cfg['frame']
    opt = cfg['opt']
    configs = cfg['configs']

    N = opt['N']
    eps = opt['epsilon']
    cmap = opt['constellation_map']
    s_palette = opt['s_palette']
    bpc = opt['bits_per_cell']

    payload = [int(x.strip()) for x in payload_str.split(',') if x.strip()]
    # Clamp to valid range
    payload = [max(0, min(w, N - 1)) for w in payload]

    # Encode header pixel (self-describing: declares N and epsilon)
    header_pixel = encode_header(N, eps, curve, frame, configs=configs)

    # Encode payload pixels
    payload_pixels, meta = encode(
        payload, curve, frame, N,
        constellation_map=cmap,
        s_palette=s_palette,
    )

    # Combine: header + payload (header is visually indistinguishable)
    all_pixels_lab = np.vstack([header_pixel.reshape(1, 3), payload_pixels])
    all_pixels_srgb = lab_to_srgb(all_pixels_lab)

    # Clean round-trip decode
    decode_header(all_pixels_lab[0], curve, frame, configs=configs)
    decoded = decode(
        all_pixels_lab[1:], curve, frame, N,
        constellation_map=cmap,
        s_palette=s_palette,
    )
    roundtrip_ok = (payload == decoded)

    # Render clean image (all pixels including header)
    encoded_svg = render_voronoi_svg(all_pixels_srgb, img_size=img_size,
                                      seed=voronoi_seed, circular=circular)
    palette_img = img_to_data_uri(render_palette_strip(curve, s_palette))

    # --- Perturbation ---
    has_perturbation = (perturb['noise_sigma'] > 0 or
                        perturb['brightness'] != 0 or
                        perturb['color_temp'] != 0 or
                        perturb['saturation'] != 1.0)

    perturbed_svg = None
    perturb_accuracy = 100.0
    perturb_correct = len(payload)
    total_bits = int(len(payload) * bpc)
    perturb_bits_recovered = total_bits
    perturb_words = []

    if has_perturbation:
        # Perturb all pixels (header + payload)
        perturbed_lab = perturb_lab(
            all_pixels_lab,
            noise_sigma=perturb['noise_sigma'],
            brightness=perturb['brightness'],
            color_temp=perturb['color_temp'],
            saturation=perturb['saturation'],
        )
        perturbed_srgb = lab_to_srgb(perturbed_lab)
        perturbed_svg = render_voronoi_svg(perturbed_srgb, img_size=img_size,
                                            seed=voronoi_seed, circular=circular)

        # Decode perturbed: header then payload
        try:
            decode_header(perturbed_lab[0], curve, frame, configs=configs)
            perturbed_decoded = decode(
                perturbed_lab[1:], curve, frame, N,
                constellation_map=cmap,
                s_palette=s_palette,
            )
        except Exception:
            perturbed_decoded = [-1] * len(payload)

        # Compute accuracy
        perturb_correct = sum(1 for a, b in zip(payload, perturbed_decoded)
                              if a == b)
        perturb_accuracy = 100.0 * perturb_correct / len(payload) if payload else 100.0
        perturb_bits_recovered = int(perturb_correct * bpc)

        perturb_words = []
        for i, (expected, got) in enumerate(zip(payload, perturbed_decoded)):
            perturb_words.append({
                'idx': i + 1,  # +1 because pixel 0 is header
                'expected': expected,
                'got': got,
                'ok': expected == got,
            })

    # Build pixel detail list (header + payload)
    pixel_details = []
    # Header pixel
    h_lab = all_pixels_lab[0]
    h_rgb = all_pixels_srgb[0]
    pixel_details.append({
        'idx': 0,
        'word': 'HEADER',
        'L': h_lab[0], 'a': h_lab[1], 'bstar': h_lab[2],
        'r': int(h_rgb[0]), 'g': int(h_rgb[1]), 'b': int(h_rgb[2]),
    })
    # Payload pixels
    for i, (lab, rgb, w) in enumerate(zip(
            all_pixels_lab[1:], all_pixels_srgb[1:], payload)):
        pixel_details.append({
            'idx': i + 1,
            'word': w,
            'L': lab[0], 'a': lab[1], 'bstar': lab[2],
            'r': int(rgb[0]), 'g': int(rgb[1]), 'b': int(rgb[2]),
        })

    n_total_cells = len(all_pixels_lab)  # header + payload
    px_per_cell = (img_size * img_size) / max(n_total_cells, 1)
    noise_reduction = np.sqrt(px_per_cell)
    sigma95 = eps / 5.6
    effective_sigma95 = sigma95 * noise_reduction

    meta_dict = {
        'M_min': cmap.M_min,
        'M_max': cmap.M_max,
        'bits_per_pixel': bpc,
        'capacity_min': cmap.capacity_min,
        'capacity_max': cmap.capacity_max,
        'max_word_repeats': meta.get('max_word_repeats', 0),
        'epsilon': eps,
        'sigma95': sigma95,
        'px_per_cell': int(px_per_cell),
        'noise_reduction': noise_reduction,
        'effective_sigma95': effective_sigma95,
    }

    return render_template_string(
        HTML_TEMPLATE,
        payload_str=payload_str,
        palette=palette,
        img_size=img_size,
        seed=voronoi_seed,
        n_cells=len(payload),
        circular=circular,
        noise_sigma=perturb['noise_sigma'],
        brightness=perturb['brightness'],
        color_temp=perturb['color_temp'],
        saturation=perturb['saturation'],
        palette_img=palette_img,
        encoded_svg=encoded_svg,
        perturbed_svg=perturbed_svg,
        meta=type('M', (), meta_dict),
        total_bits=total_bits,
        roundtrip_ok=roundtrip_ok,
        decoded=decoded,
        pixels=pixel_details,
        n_words=len(payload),
        perturb_accuracy=perturb_accuracy,
        perturb_correct=perturb_correct,
        perturb_bits_recovered=perturb_bits_recovered,
        perturb_words=perturb_words,
        derived_N=N,
        derived_eps=eps,
        derived_bpc=bpc,
    )


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Glossia Image Encoder Demo')
    parser.add_argument('--port', type=int, default=5001)
    parser.add_argument('--host', default='127.0.0.1')
    parser.add_argument('--debug', action='store_true')
    args = parser.parse_args()

    # Pre-warm the default encoder
    print("Deriving optimal config for default palette...")
    cfg = get_optimal_config()
    opt = cfg['opt']
    print(f"  N={opt['N']}, epsilon={opt['epsilon']:.1f}, "
          f"bpc={opt['bits_per_cell']:.2f}")
    print(f"Ready! Open http://{args.host}:{args.port}")

    app.run(host=args.host, port=args.port, debug=args.debug)
