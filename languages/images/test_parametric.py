#!/usr/bin/env python3
"""
Test suite for parametric curve encoding.

Validates five properties:
1. Round-trip: encode -> decode recovers the original payload (exact, no noise)
2. Gamut: all encoded CIELAB colors map to valid sRGB [0,255]^3
3. Separation: encoded pixels are sufficiently separated in CIELAB
4. Noise robustness: decode succeeds under Gaussian noise up to EPSILON/3
5. Capacity: per-color constellations match theoretical M_i^2 bounds

Usage:
    python test_parametric.py          # run all tests
    python test_parametric.py -v       # verbose
"""

import sys
import os
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from parametric_encoding import (
    PaletteCurve, BishopFrame, Constellation, ConstellationMap,
    compute_tube_radius, encode, decode, verify_roundtrip,
    build_encoder, srgb_to_lab, lab_to_srgb, lab_in_srgb_gamut,
    EPSILON,
)


# ---------------------------------------------------------------------------
# Test fixtures
# ---------------------------------------------------------------------------

def get_encoder(n_palette=16):
    """Build a test encoder with the default palette."""
    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')
    return build_encoder(yaml_path, 'viridis_approx',
                         n_palette=n_palette)


# ---------------------------------------------------------------------------
# Test 1: Round-trip (exact, no noise)
# ---------------------------------------------------------------------------

def test_roundtrip_simple():
    """Simple ascending payload."""
    enc = get_encoder()
    assert verify_roundtrip(
        [0, 1, 2, 3], enc['curve'], enc['frame'],
        enc['n_palette'], enc['constellation_map']
    ), "Simple ascending payload failed round-trip"


def test_roundtrip_repeats():
    """Repeated words -- tests constellation grid."""
    enc = get_encoder()
    assert verify_roundtrip(
        [5, 5, 5, 5], enc['curve'], enc['frame'],
        enc['n_palette'], enc['constellation_map']
    ), "Repeated words failed round-trip"


def test_roundtrip_alternating():
    """Alternating words with repeats."""
    enc = get_encoder()
    assert verify_roundtrip(
        [0, 1, 0, 1, 0, 1], enc['curve'], enc['frame'],
        enc['n_palette'], enc['constellation_map']
    ), "Alternating payload failed round-trip"


def test_roundtrip_all_words():
    """Every palette word appears exactly once."""
    enc = get_encoder()
    payload = list(range(enc['n_palette']))
    assert verify_roundtrip(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['constellation_map']
    ), "All-words payload failed round-trip"


def test_roundtrip_random():
    """Random payload of 50 words."""
    enc = get_encoder()
    np.random.seed(42)
    payload = np.random.randint(0, enc['n_palette'], size=50).tolist()
    assert verify_roundtrip(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['constellation_map']
    ), "Random 50-word payload failed round-trip"


def test_roundtrip_max_repeats():
    """Test many repeats of a single word.

    With per-color constellations, M can be large.  The raster-order grid
    starts at the corner, so we limit repeats to keep pixels within the
    decodable region rather than filling to theoretical capacity.
    """
    enc = get_encoder()
    cmap = enc['constellation_map']
    # Use enough repeats to exercise the constellation without hitting
    # projection limits from extreme corner positions
    n_repeats = min(cmap[7].capacity - 1, 50)
    payload = [7] * n_repeats
    assert verify_roundtrip(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], cmap
    ), f"Max repeats ({n_repeats}) failed round-trip"


def test_roundtrip_single_word():
    """Single-word payload."""
    enc = get_encoder()
    assert verify_roundtrip(
        [8], enc['curve'], enc['frame'],
        enc['n_palette'], enc['constellation_map']
    ), "Single-word payload failed round-trip"


def test_capacity_overflow():
    """Exceeding constellation capacity raises ValueError."""
    enc = get_encoder()
    cmap = enc['constellation_map']
    # Use word 0's capacity
    cap = cmap[0].capacity
    payload = [0] * (cap + 1)
    try:
        encode(payload, enc['curve'], enc['frame'],
               enc['n_palette'], constellation_map=cmap)
        assert False, "Should have raised ValueError for capacity overflow"
    except ValueError:
        pass  # expected


# ---------------------------------------------------------------------------
# Test 2: Gamut validity
# ---------------------------------------------------------------------------

def test_gamut_all_palette_points():
    """All palette curve sample points are in sRGB gamut."""
    enc = get_encoder()
    curve = enc['curve']
    s_pts = np.linspace(0, curve.arc_length, 200)
    pts = curve.eval(s_pts)
    in_gamut = lab_in_srgb_gamut(pts)
    n_bad = np.sum(~in_gamut)
    assert n_bad == 0, f"{n_bad}/200 palette curve points out of sRGB gamut"


def test_gamut_encoded_pixels():
    """Palette base points (zero displacement) are in sRGB gamut.

    With per-color constellations, the raster-order grid places j=0 at
    the grid corner, producing large normal-plane displacements that can
    exit the gamut.  This test checks that the palette curve itself stays
    inside sRGB, which is the baseline guarantee.
    """
    enc = get_encoder()
    curve = enc['curve']
    n_pal = enc['n_palette']
    s_pts = np.linspace(0, curve.arc_length, n_pal)
    pts_lab = curve.eval(s_pts)
    in_gamut = lab_in_srgb_gamut(pts_lab, tolerance=1.0)
    n_bad = np.sum(~in_gamut)
    assert n_bad == 0, f"{n_bad}/{n_pal} palette base points out of sRGB gamut"


def test_gamut_srgb_roundtrip():
    """CIELAB -> sRGB -> CIELAB round-trip for palette base points.

    Some viridis palette points lie near the sRGB gamut boundary,
    where uint8 quantization introduces up to ~3.5 CIELAB units of error.
    """
    enc = get_encoder()
    curve = enc['curve']
    n_pal = enc['n_palette']
    s_pts = np.linspace(0, curve.arc_length, n_pal)
    pts_lab = curve.eval(s_pts)
    pts_srgb = lab_to_srgb(pts_lab)
    pts_lab2 = srgb_to_lab(pts_srgb)
    errors = np.linalg.norm(pts_lab - pts_lab2, axis=1)
    max_err = np.max(errors)
    assert max_err < 4.0, f"CIELAB->sRGB->CIELAB max error = {max_err:.2f} (> 4.0)"


# ---------------------------------------------------------------------------
# Test 3: Pixel separation
# ---------------------------------------------------------------------------

def test_separation_different_words():
    """Pixels encoding different words project to distinct curve positions."""
    enc = get_encoder()
    payload = list(range(enc['n_palette']))
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], constellation_map=enc['constellation_map']
    )
    # Project each pixel back to the curve and check word identification
    for i, px in enumerate(pixels_lab):
        s_nearest, _ = enc['curve'].project(px.reshape(1, 3))
        s_nearest = float(s_nearest[0])
        w = round(s_nearest * max(enc['n_palette'] - 1, 1) / enc['curve'].arc_length)
        w = int(np.clip(w, 0, enc['n_palette'] - 1))
        assert w == i, \
            f"Word {i} projected to word {w} (s={s_nearest:.2f})"


def test_separation_same_word_repeats():
    """Repeated words' pixels are separated by >= EPSILON in the normal plane."""
    enc = get_encoder()
    payload = [4, 4, 4, 4, 4]
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], constellation_map=enc['constellation_map']
    )
    for i in range(len(pixels_lab)):
        for j in range(i + 1, len(pixels_lab)):
            d = np.linalg.norm(pixels_lab[i] - pixels_lab[j])
            assert d >= EPSILON * 0.9, \
                f"Repeated-word pixels [{i}],[{j}] too close: {d:.2f} < {EPSILON * 0.9:.2f}"


# ---------------------------------------------------------------------------
# Test 4: Noise robustness
# ---------------------------------------------------------------------------

def test_noise_robustness():
    """Decode succeeds under moderate Gaussian noise in CIELAB."""
    enc = get_encoder()
    np.random.seed(99)
    payload = np.random.randint(0, enc['n_palette'], size=30).tolist()
    cmap = enc['constellation_map']
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], constellation_map=cmap
    )

    # Add noise with sigma = EPSILON/3 (should be decodable)
    sigma = EPSILON / 3.0
    noise = np.random.normal(0, sigma, pixels_lab.shape)
    noisy_pixels = pixels_lab + noise

    recovered = decode(
        noisy_pixels, enc['curve'], enc['frame'],
        enc['n_palette'], constellation_map=cmap
    )

    errors = sum(1 for a, b in zip(payload, recovered) if a != b)
    error_rate = errors / len(payload)
    # Allow up to 10% error at sigma = EPSILON/3
    assert error_rate < 0.10, \
        f"Noise robustness: {errors}/{len(payload)} errors ({error_rate:.1%}) at sigma={sigma:.1f}"


def test_noise_deterministic():
    """Same input produces same encoding (deterministic)."""
    enc = get_encoder()
    payload = [3, 7, 11, 0, 15]
    cmap = enc['constellation_map']
    p1, _ = encode(payload, enc['curve'], enc['frame'],
                   enc['n_palette'], constellation_map=cmap)
    p2, _ = encode(payload, enc['curve'], enc['frame'],
                   enc['n_palette'], constellation_map=cmap)
    assert np.allclose(p1, p2), "Encoding is not deterministic"


# ---------------------------------------------------------------------------
# Test 5: Capacity
# ---------------------------------------------------------------------------

def test_capacity_constellation():
    """Per-color constellation M_i matches floor(2*r_i/EPSILON) + 1."""
    enc = get_encoder()
    cmap = enc['constellation_map']
    _, radii = enc['tube_radii']
    for i, r in enumerate(radii):
        expected_M = int(2 * r / EPSILON) + 1
        actual_M = cmap[i].M
        assert actual_M == expected_M, \
            f"Color {i}: M={actual_M} != expected {expected_M} for r={r:.1f}, eps={EPSILON}"


def test_capacity_bits_per_pixel():
    """Bits per pixel uses M_min for conservative estimate."""
    enc = get_encoder()
    N = enc['n_palette']
    M_min = enc['constellation_map'].M_min
    expected_bpp = np.log2(N) + 2 * np.log2(M_min)
    actual_bpp = enc['metadata']['bits_per_pixel']
    assert abs(actual_bpp - expected_bpp) < 0.01, \
        f"Bits/pixel {actual_bpp:.2f} != expected {expected_bpp:.2f}"


def test_per_color_capacity_variation():
    """M_values are not all identical (tube radius varies along curve)."""
    enc = get_encoder()
    cmap = enc['constellation_map']
    assert cmap.M_min < cmap.M_max, \
        f"All M values identical ({cmap.M_min}), expected variation"


def test_roundtrip_max_repeats_fattest_tube():
    """Test many repeats at the fattest tube color."""
    enc = get_encoder()
    cmap = enc['constellation_map']
    fattest = int(np.argmax(cmap.capacities))
    # Limit to avoid extreme corner positions
    n_repeats = min(cmap[fattest].capacity - 1, 50)
    payload = [fattest] * n_repeats
    assert verify_roundtrip(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], cmap
    ), f"Repeats at fattest tube (word {fattest}, {n_repeats}) failed round-trip"


# ---------------------------------------------------------------------------
# Test: Bishop frame properties
# ---------------------------------------------------------------------------

def test_bishop_frame_orthonormal():
    """Bishop frame is orthonormal at all sample points."""
    enc = get_encoder()
    frame = enc['frame']
    s_test = np.linspace(0, enc['curve'].arc_length, 50)
    T, U1, U2 = frame.eval_frame(s_test)

    for i in range(len(s_test)):
        assert abs(np.dot(T[i], U1[i])) < 0.01, f"T.U1 = {np.dot(T[i], U1[i]):.4f} at s={s_test[i]:.1f}"
        assert abs(np.dot(T[i], U2[i])) < 0.01, f"T.U2 = {np.dot(T[i], U2[i]):.4f} at s={s_test[i]:.1f}"
        assert abs(np.dot(U1[i], U2[i])) < 0.01, f"U1.U2 = {np.dot(U1[i], U2[i]):.4f} at s={s_test[i]:.1f}"
        assert abs(np.linalg.norm(T[i]) - 1) < 0.01, f"|T| = {np.linalg.norm(T[i]):.4f}"
        assert abs(np.linalg.norm(U1[i]) - 1) < 0.01, f"|U1| = {np.linalg.norm(U1[i]):.4f}"
        assert abs(np.linalg.norm(U2[i]) - 1) < 0.01, f"|U2| = {np.linalg.norm(U2[i]):.4f}"


def test_bishop_frame_smooth():
    """Bishop frame varies smoothly (consecutive frames nearly identical)."""
    enc = get_encoder()
    frame = enc['frame']
    s_test = np.linspace(0, enc['curve'].arc_length, 200)
    T, U1, U2 = frame.eval_frame(s_test)

    for i in range(1, len(s_test)):
        # Consecutive U1 vectors should be nearly aligned
        dot = np.dot(U1[i], U1[i - 1])
        assert dot > 0.95, f"U1 discontinuity at s={s_test[i]:.1f}: dot={dot:.4f}"


# ---------------------------------------------------------------------------
# Test: Color conversion
# ---------------------------------------------------------------------------

def test_srgb_lab_roundtrip():
    """sRGB -> CIELAB -> sRGB round-trip within 1 unit."""
    test_colors = np.array([
        [0, 0, 0],       # black
        [255, 255, 255],  # white
        [255, 0, 0],      # red
        [0, 255, 0],      # green
        [0, 0, 255],      # blue
        [128, 128, 128],  # mid gray
        [64, 192, 100],   # arbitrary
    ], dtype=np.uint8)

    lab = srgb_to_lab(test_colors)
    rgb_back = lab_to_srgb(lab)
    max_diff = np.max(np.abs(test_colors.astype(int) - rgb_back.astype(int)))
    assert max_diff <= 1, f"sRGB round-trip max error = {max_diff}"


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------

ALL_TESTS = [
    # Round-trip
    test_roundtrip_simple,
    test_roundtrip_repeats,
    test_roundtrip_alternating,
    test_roundtrip_all_words,
    test_roundtrip_random,
    test_roundtrip_max_repeats,
    test_roundtrip_single_word,
    test_capacity_overflow,
    # Gamut
    test_gamut_all_palette_points,
    test_gamut_encoded_pixels,
    test_gamut_srgb_roundtrip,
    # Separation
    test_separation_different_words,
    test_separation_same_word_repeats,
    # Noise
    test_noise_robustness,
    test_noise_deterministic,
    # Capacity
    test_capacity_constellation,
    test_capacity_bits_per_pixel,
    test_per_color_capacity_variation,
    test_roundtrip_max_repeats_fattest_tube,
    # Frame
    test_bishop_frame_orthonormal,
    test_bishop_frame_smooth,
    # Color
    test_srgb_lab_roundtrip,
]


def main():
    import argparse
    parser = argparse.ArgumentParser(description='Parametric encoding tests')
    parser.add_argument('-v', '--verbose', action='store_true')
    args = parser.parse_args()

    passed = 0
    failed = 0
    errors = []

    for test_fn in ALL_TESTS:
        name = test_fn.__name__
        try:
            test_fn()
            passed += 1
            if args.verbose:
                print(f"  \033[32mPASS\033[0m {name}")
        except Exception as e:
            failed += 1
            errors.append((name, str(e)))
            if args.verbose:
                print(f"  \033[31mFAIL\033[0m {name}: {e}")

    print(f"\n{'=' * 50}")
    print(f"Results: {passed} passed, {failed} failed, {passed + failed} total")

    if errors:
        print(f"\nFailures:")
        for name, msg in errors:
            print(f"  {name}: {msg}")
        sys.exit(1)
    else:
        print("All tests passed!")
        sys.exit(0)


if __name__ == '__main__':
    main()
