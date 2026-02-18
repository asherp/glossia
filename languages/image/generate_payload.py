#!/usr/bin/env python3
"""
Generate payload_<palette>.yaml files from parametric curve analysis.

For each palette defined in palette.yaml, this script:
  1. Builds the PaletteCurve and BishopFrame
  2. Runs select_encoding_params() to find optimal (N, epsilon)
  3. Computes CIELAB and sRGB coordinates at each palette position
  4. Writes a payload_<palette>.yaml with token entries and color metadata

The generated files serve two purposes:
  - Glossia build system: payload tokens with POS tags (N: 1.0)
  - Portability: CIELAB/sRGB coordinates so other systems can
    reproduce the palette without running the curve engine

Can also bootstrap palette.yaml from official colormap sources:
  python generate_payload.py --bootstrap          # write all colormaps to palette.yaml
  python generate_payload.py --bootstrap viridis  # add just viridis

Usage:
  python generate_payload.py                     # generate payloads for all palettes
  python generate_payload.py viridis             # single palette
  python generate_payload.py --list              # show available palettes
  python generate_payload.py --bootstrap         # bootstrap palette.yaml from colormaps
"""

import argparse
import os
import sys

import numpy as np
import yaml

from parametric_encoding import (
    PaletteCurve,
    BishopFrame,
    select_encoding_params,
    lab_to_srgb,
    srgb_to_lab,
)

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PALETTE_YAML = os.path.join(SCRIPT_DIR, "palette.yaml")


# ---------------------------------------------------------------------------
# Official colormap provenance
# ---------------------------------------------------------------------------

# viscm control points (CAM02-UCS) from the original design tools.
# These are for attribution only — we derive CIELAB control points
# from the canonical 256-entry RGB LUTs for our spline curves.
#
# Sources:
#   viridis, plasma, inferno, magma:
#     BIDS/colormap (CC0), github.com/BIDS/colormap
#     Designed by Stéfan van der Walt, Nathaniel Smith, Eric Firing
#     via viscm in buggy-CAM02-UCS colorspace
#
#   mako, rocket:
#     seaborn (BSD-3), github.com/mwaskom/seaborn
#     Designed by Michael Waskom via viscm in CAM02-UCS
#     Control points: gist.github.com/mwaskom/03550b565e2f0cd45837cada8173ec99
#
#   cividis:
#     Nuñez, Anderton, Renslow (2018), PLoS ONE 13(7): e0199239
#     Optimized from viridis for CVD — no spline control points, LUT-defined
#
#   turbo:
#     Anton Mikhailov, Google (2019)
#     research.google/blog/turbo-an-improved-rainbow-colormap-for-visualization/
#     Polynomial design, no viscm control points, LUT-defined

COLORMAP_CATALOG = {
    "viridis": {
        "description": "Deep purple-blue through teal to yellow-green (CC0, van der Walt & Smith 2015)",
        "source": "matplotlib",
        "license": "CC0",
        "viscm_params": {
            "xp": [22.674, 11.222, -14.357, -47.188, -34.590, -6.052],
            "yp": [-20.103, -33.082, -42.245, -5.596, 42.507, 40.134],
            "min_JK": 18.867, "max_JK": 92.5,
            "colorspace": "buggy-CAM02-UCS",
        },
    },
    "plasma": {
        "description": "Dark purple through magenta-pink to orange-yellow (CC0, van der Walt & Smith 2015)",
        "source": "matplotlib",
        "license": "CC0",
        "viscm_params": {
            "xp": [-5.490, 14.791, 82.555, 29.155, -4.132, -13.002],
            "yp": [-35.948, -42.273, -28.845, 52.034, 36.833, 40.792],
            "min_JK": 16.831, "max_JK": 95.0,
            "colorspace": "buggy-CAM02-UCS",
        },
    },
    "inferno": {
        "description": "Near-black through dark purple and red-orange to bright yellow (CC0, van der Walt & Smith 2015)",
        "source": "matplotlib",
        "license": "CC0",
        "viscm_params": {
            "xp": [0.629, 5.946, 109.508, -21.829, -6.574],
            "yp": [-9.071, -75.152, 50.040, 52.941, 24.553],
            "min_JK": 1.144, "max_JK": 98.218,
            "colorspace": "buggy-CAM02-UCS",
        },
    },
    "magma": {
        "description": "Near-black through dark purple and salmon-pink to pale white (CC0, van der Walt & Smith 2015)",
        "source": "matplotlib",
        "license": "CC0",
        "viscm_params": {
            "xp": [0.629, 5.946, 87.724, -4.688],
            "yp": [-9.071, -75.152, 44.097, 18.526],
            "min_JK": 1.144, "max_JK": 98.218,
            "colorspace": "buggy-CAM02-UCS",
        },
    },
    "cividis": {
        "description": "Dark blue through olive-grey to yellow, colorblind-safe (Nunez et al. 2018)",
        "source": "matplotlib",
        "license": "CC0",
        "viscm_params": None,  # LUT-defined via optimization, no control points
    },
    "turbo": {
        "description": "Dark blue through cyan, green, yellow, red to dark red (Mikhailov, Google 2019)",
        "source": "matplotlib",
        "license": "Apache-2.0",
        "viscm_params": None,  # Polynomial design, no viscm control points
    },
    "mako": {
        "description": "Deep purple-black through teal-blue to pale cyan-white (BSD-3, Waskom)",
        "source": "seaborn",
        "license": "BSD-3-Clause",
        "viscm_params": {
            "xp": [5.20, 9.03, -1.93, -20.01, -25.21, -16.45, -8.50],
            "yp": [0.375, -13.32, -28.12, -13.60, 3.39, 7.50, 2.84],
            "min_JK": 5, "max_JK": 95,
            "colorspace": "CAM02-UCS",
        },
    },
    "rocket": {
        "description": "Near-black through dark magenta and salmon to pale white (BSD-3, Waskom)",
        "source": "seaborn",
        "license": "BSD-3-Clause",
        "viscm_params": {
            "xp": [-1.10, 19.45, 38.08, 33.15, 12.87, 1.91],
            "yp": [-12.23, -11.13, 3.94, 20.10, 16.00, 4.76],
            "min_JK": 5, "max_JK": 95,
            "colorspace": "CAM02-UCS",
        },
    },
}


def get_colormap_lut(name, n=256):
    """Get the canonical 256-entry sRGB LUT for a colormap.

    Returns:
        (n, 3) array of sRGB values in [0, 1]
    """
    info = COLORMAP_CATALOG[name]

    if info["source"] == "matplotlib":
        import matplotlib.cm as cm
        cmap = cm.get_cmap(name)
        t = np.linspace(0, 1, n)
        return cmap(t)[:, :3]

    elif info["source"] == "seaborn":
        import seaborn as sns
        colors = sns.color_palette(name, n)
        return np.array(colors)

    else:
        raise ValueError(f"Unknown source: {info['source']}")


def sample_cielab_control_points(name, n_ctrl=6):
    """Sample n_ctrl CIELAB control points from a colormap's official LUT.

    We sample the canonical 256-entry RGB LUT at equally-spaced indices,
    convert to CIELAB, and return control points suitable for our
    PaletteCurve spline interpolation.

    Args:
        name: colormap name (key in COLORMAP_CATALOG)
        n_ctrl: number of control points to sample

    Returns:
        (n_ctrl, 3) array of CIELAB [L*, a*, b*] control points
    """
    rgb_01 = get_colormap_lut(name)
    # Convert [0,1] -> [0,255] for srgb_to_lab
    rgb_255 = rgb_01 * 255.0
    labs = srgb_to_lab(rgb_255)

    indices = np.linspace(0, 255, n_ctrl).astype(int)
    return labs[indices]


# ---------------------------------------------------------------------------
# Palette YAML generation
# ---------------------------------------------------------------------------

def load_palettes(yaml_path=PALETTE_YAML):
    """Load all palette definitions from palette.yaml."""
    with open(yaml_path) as f:
        config = yaml.safe_load(f)
    return config["palettes"]


def bootstrap_palette_yaml(names=None, output_path=PALETTE_YAML, n_ctrl=6):
    """Write palette.yaml with CIELAB control points from official colormaps.

    Samples the canonical LUTs from matplotlib/seaborn, converts to CIELAB,
    and writes control points with full provenance metadata.

    Args:
        names: list of colormap names (default: all in COLORMAP_CATALOG)
        output_path: where to write palette.yaml
        n_ctrl: control points per palette
    """
    if names is None:
        names = list(COLORMAP_CATALOG.keys())

    lines = []
    lines.append("# Palette control points for parametric curve encoding")
    lines.append("#")
    lines.append("# CIELAB control points sampled from official colormap LUTs.")
    lines.append("# The curve is built by cubic spline interpolation through these points,")
    lines.append("# then arc-length reparameterized for uniform perceptual spacing.")
    lines.append("#")
    lines.append("# Auto-generated by: python generate_payload.py --bootstrap")
    lines.append("#")
    lines.append("# Sources:")
    lines.append("#   viridis, plasma, inferno, magma:")
    lines.append("#     BIDS/colormap (CC0) — github.com/BIDS/colormap")
    lines.append("#     van der Walt, Smith & Firing, SciPy 2015")
    lines.append("#   cividis:")
    lines.append("#     Nunez, Anderton & Renslow, PLoS ONE 13(7) 2018")
    lines.append("#   turbo:")
    lines.append("#     Mikhailov, Google 2019 (Apache-2.0)")
    lines.append("#   mako, rocket:")
    lines.append("#     seaborn (BSD-3) — github.com/mwaskom/seaborn")
    lines.append("")
    lines.append("palettes:")

    for name in names:
        info = COLORMAP_CATALOG[name]
        pts = sample_cielab_control_points(name, n_ctrl)
        desc = info["description"]
        lic = info["license"]

        print(f"  {name}: {n_ctrl} CIELAB control points ({lic})")

        lines.append(f"  {name}:")
        lines.append(f'    description: "{desc}"')
        lines.append(f"    license: {lic}")
        lines.append(f"    control_points_lab:")
        for p in pts:
            lines.append(f"      - [{p[0]:5.1f}, {p[1]:6.1f}, {p[2]:6.1f}]")

        # Include viscm provenance if available
        vp = info.get("viscm_params")
        if vp:
            lines.append(f"    # viscm control points ({vp['colorspace']}):")
            lines.append(f"    #   xp: {vp['xp']}")
            lines.append(f"    #   yp: {vp['yp']}")
            lines.append(f"    #   J'K' range: [{vp['min_JK']}, {vp['max_JK']}]")

        lines.append("")

    with open(output_path, "w") as f:
        f.write("\n".join(lines))

    return output_path


# ---------------------------------------------------------------------------
# Payload YAML generation
# ---------------------------------------------------------------------------

def srgb_to_css_hex(r, g, b):
    """Convert sRGB (0-255) to CSS hex string '#RRGGBB'."""
    return f"#{int(r):02x}{int(g):02x}{int(b):02x}"


def lab_to_token_name(L, a, b):
    """Convert CIELAB coordinates to a token name string.

    Format: "L*_a*_b*" with 2 decimal places, e.g. "15.03_40.54_-32.50".
    This is collision-free (float precision), valid as a YAML key and
    Glossia token (no spaces), and self-describing (any renderer can
    convert LAB→sRGB without needing the wordlist YAML).
    """
    return f"{L:.2f}_{a:.2f}_{b:.2f}"


def generate_for_palette(palette_name, control_points_lab):
    """Run curve analysis and return payload entries for one palette.

    Token names are CIELAB coordinates ("L*_a*_b*") which are inherently
    collision-free — even when two palette positions round to the same
    8-bit sRGB, their CIELAB coordinates differ by at least ~1.7 units
    (at N=128 with arc length ~220). No nudge hack needed.

    The sRGB hex rendering is stored as an inner value for convenience.

    Returns:
        dict with keys:
            tokens: list of token dicts (name, srgb_hex, s, M, capacity)
            metadata: dict with N, epsilon, bits_per_cell, arc_length, etc.
        or None if no valid configuration found.
    """
    pts = np.array(control_points_lab)
    curve = PaletteCurve.from_control_points(pts)
    frame = BishopFrame(curve)

    result = select_encoding_params(curve, frame)
    if result is None:
        return None

    N = result["N"]
    epsilon = result["epsilon"]
    s_palette = result["s_palette"]
    cmap = result["constellation_map"]

    # Evaluate colors at each palette position
    labs = curve.eval(s_palette)
    srgbs = lab_to_srgb(labs)

    # Generate CIELAB token names — collision-free by construction.
    # Each palette position has unique float-precision CIELAB coordinates.
    # No nudge/dedup needed (unlike sRGB hex keys where 8-bit rounding
    # can cause collisions in low-gradient regions like near-black).
    lab_names = []
    seen = set()
    for i in range(N):
        lab = labs[i]
        name = lab_to_token_name(float(lab[0]), float(lab[1]), float(lab[2]))
        assert name not in seen, (
            f"Palette '{palette_name}': CIELAB collision at position {i}: {name} "
            f"(this should never happen — adjacent palette positions differ by "
            f"~{float(curve.arc_length) / N:.1f} CIELAB units)"
        )
        seen.add(name)
        lab_names.append(name)

    tokens = []
    for i in range(N):
        lab = labs[i]
        srgb = srgbs[i]
        tokens.append({
            "name": lab_names[i],
            "index": i,
            "srgb_hex": srgb_to_css_hex(int(srgb[0]), int(srgb[1]), int(srgb[2])),
            "s": round(float(s_palette[i]), 4),
            "M": int(cmap[i].M),
            "capacity": int(cmap[i].capacity),
        })

    metadata = {
        "palette": palette_name,
        "N": N,
        "epsilon": round(epsilon, 4),
        "bits_per_cell": result["bits_per_cell"],
        "states_per_cell": result.get("states_per_cell", 0),
        "word_bits": result["word_bits"],
        "pos_bits": result["pos_bits"],
        "M_min": result["M_min"],
        "M_max": result["M_max"],
        "srgb_dist_min": round(result.get("srgb_dist_min", 0.0), 1),
        "arc_length": round(float(curve.arc_length), 4),
        "control_points_lab": [list(map(float, p)) for p in pts],
    }

    # Add provenance if this is a known colormap
    if palette_name in COLORMAP_CATALOG:
        cat = COLORMAP_CATALOG[palette_name]
        metadata["license"] = cat["license"]
        metadata["source"] = cat["source"]
        if cat.get("viscm_params"):
            metadata["viscm_colorspace"] = cat["viscm_params"]["colorspace"]

    return {"tokens": tokens, "metadata": metadata}


def write_payload_yaml(palette_name, data, output_dir=SCRIPT_DIR):
    """Write payload_<palette>.yaml in Glossia wordlist format.

    Token names are CIELAB coordinates ("L*_a*_b*"), e.g. "15.03_40.54_-32.50".
    Each token carries:
      - N: 1.0            POS tag for the Glossia grammar (payload noun)
      - srgb: "#RRGGBB"   8-bit sRGB hex rendering (for SVG fill etc.)
      - s: <float>        arc-length position on the palette curve
      - M: <int>          constellation grid size (bits capacity at this position)

    The CIELAB coordinates are IN the key itself — no need for separate
    L/a/b inner values. This makes keys collision-free (float precision)
    even when two palette positions round to the same 8-bit sRGB.

    The Rust grammar engine reads N: 1.0 and ignores the rest (they fail
    POS tag parsing). Renderers parse the key to get CIELAB directly, or
    read the inner srgb for CSS fill colors.
    """
    meta = data["metadata"]
    tokens = data["tokens"]
    out_path = os.path.join(output_dir, f"payload_{palette_name}.yaml")

    lines = []

    # Header
    lines.append(f"# Palette Color Payload: {palette_name}")
    lines.append(f"#")
    lines.append(f"# Auto-generated by generate_payload.py from palette.yaml")
    lines.append(f"# Do not edit by hand — regenerate with:")
    lines.append(f"#   python generate_payload.py {palette_name}")
    lines.append(f"#")
    if "license" in meta:
        lines.append(f"# License: {meta['license']}")
    if "source" in meta:
        lines.append(f"# Source: {meta['source']}")
    lines.append(f"#")
    lines.append(f"# Parametric curve encoding parameters:")
    lines.append(f"#   N (palette size):  {meta['N']}")
    lines.append(f"#   epsilon (JND):     {meta['epsilon']}")
    lines.append(f"#   bits per cell:     {meta['bits_per_cell']:.2f}")
    lines.append(f"#   word bits:         {meta['word_bits']:.2f}")
    lines.append(f"#   position bits:     {meta['pos_bits']:.2f}")
    if 'states_per_cell' in meta:
        lines.append(f"#   states per cell:   {meta['states_per_cell']}")
    lines.append(f"#   M_min (grid):      {meta['M_min']}")
    lines.append(f"#   M_max (grid):      {meta['M_max']}")
    lines.append(f"#   sRGB dist min:     {meta['srgb_dist_min']}")
    lines.append(f"#   arc length:        {meta['arc_length']}")
    lines.append(f"#")
    lines.append(f"# Control points (CIELAB):")
    for cp in meta["control_points_lab"]:
        lines.append(f"#   [{cp[0]:6.1f}, {cp[1]:6.1f}, {cp[2]:6.1f}]")
    lines.append(f"#")
    lines.append(f"# Token keys are CIELAB coordinates (L*_a*_b*). The key IS the color:")
    lines.append(f"#   parse on '_' to get [L*, a*, b*] for the parametric curve pipeline.")
    lines.append(f"# Inner values carry rendering metadata:")
    lines.append(f"#   srgb — 8-bit CSS hex color for SVG fill (may have rounding collisions)")
    lines.append(f"#   s    — arc-length position on palette curve (similarity metric)")
    lines.append(f"#   M    — constellation grid size (capacity at this curve position)")
    lines.append(f"#")

    # Tokens — CIELAB keys with sRGB inner value
    for tok in tokens:
        lines.append(f'"{tok["name"]}":')
        lines.append(f"  N: 1.0")
        lines.append(f'  srgb: "{tok["srgb_hex"]}"')
        lines.append(f"  s: {tok['s']}")
        lines.append(f"  M: {tok['M']}")

    lines.append("")  # trailing newline

    with open(out_path, "w") as f:
        f.write("\n".join(lines))

    return out_path


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Generate payload_<palette>.yaml from parametric curve analysis")
    parser.add_argument(
        "palettes", nargs="*",
        help="Palette names to generate (default: all in palette.yaml)")
    parser.add_argument(
        "--list", action="store_true",
        help="List available palettes and exit")
    parser.add_argument(
        "--bootstrap", action="store_true",
        help="Bootstrap palette.yaml from official colormap LUTs")
    parser.add_argument(
        "--n-ctrl", type=int, default=6,
        help="Number of CIELAB control points per palette (default: 6)")
    parser.add_argument(
        "--output-dir", default=SCRIPT_DIR,
        help="Output directory (default: same as this script)")
    args = parser.parse_args()

    # Bootstrap mode: write palette.yaml from official colormaps
    if args.bootstrap:
        names = args.palettes or None  # None = all
        if names:
            for name in names:
                if name not in COLORMAP_CATALOG:
                    print(f"ERROR: unknown colormap '{name}'", file=sys.stderr)
                    print(f"  Available: {', '.join(COLORMAP_CATALOG.keys())}",
                          file=sys.stderr)
                    sys.exit(1)
        print(f"Bootstrapping palette.yaml ({args.n_ctrl} control points per palette)...")
        out = bootstrap_palette_yaml(names, n_ctrl=args.n_ctrl)
        print(f"Wrote {out}")
        return

    # Normal mode: generate payload YAMLs from palette.yaml
    all_palettes = load_palettes()

    if args.list:
        for name, info in all_palettes.items():
            desc = info.get("description", "")
            n_pts = len(info["control_points_lab"])
            print(f"  {name:20s}  {n_pts} control points  {desc}")
        return

    palette_names = args.palettes or list(all_palettes.keys())

    for name in palette_names:
        if name not in all_palettes:
            print(f"ERROR: unknown palette '{name}'", file=sys.stderr)
            print(f"  Available: {', '.join(all_palettes.keys())}", file=sys.stderr)
            sys.exit(1)

    for name in palette_names:
        info = all_palettes[name]
        pts = info["control_points_lab"]
        print(f"Analyzing palette '{name}' ({len(pts)} control points)...")

        data = generate_for_palette(name, pts)
        if data is None:
            print(f"  WARNING: no valid configuration found for '{name}', skipping")
            continue

        meta = data["metadata"]
        print(f"  N={meta['N']}, epsilon={meta['epsilon']:.4f}, "
              f"bits/cell={meta['bits_per_cell']:.2f} "
              f"({meta['states_per_cell']} states), "
              f"M_min={meta['M_min']}, M_max={meta['M_max']}, "
              f"sRGB_dist={meta['srgb_dist_min']}")

        out_path = write_payload_yaml(name, data, output_dir=args.output_dir)
        print(f"  Wrote {out_path}")

    print("Done.")


if __name__ == "__main__":
    main()
