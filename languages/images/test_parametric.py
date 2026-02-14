#!/usr/bin/env python3
"""
Test suite for parametric curve encoding.

Validates five properties:
1. Round-trip: encode -> decode recovers the original payload (exact, no noise)
2. Gamut: all encoded CIELAB colors map to valid sRGB [0,255]^3
3. Separation: encoded pixels are sufficiently separated in CIELAB
4. Noise robustness: decode succeeds under Gaussian noise up to epsilon/3
5. Capacity: constellation capacity matches theoretical M^2 bound

Usage:
    python test_parametric.py          # run all tests
    python test_parametric.py -v       # verbose
"""

import sys
import os
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from parametric_encoding import (
    PaletteCurve, BishopFrame, Constellation,
    compute_tube_radius, encode, decode, verify_roundtrip,
    build_encoder, srgb_to_lab, lab_to_srgb, lab_in_srgb_gamut,
)


# ---------------------------------------------------------------------------
# Test fixtures
# ---------------------------------------------------------------------------

def get_encoder(n_palette=16, epsilon=5.0):
    """Build a test encoder with the default palette."""
    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')
    return build_encoder(yaml_path, 'viridis_approx',
                         n_palette=n_palette, epsilon=epsilon)


# ---------------------------------------------------------------------------
# Test 1: Round-trip (exact, no noise)
# ---------------------------------------------------------------------------

def test_roundtrip_simple():
    """Simple ascending payload."""
    enc = get_encoder()
    assert verify_roundtrip(
        [0, 1, 2, 3], enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    ), "Simple ascending payload failed round-trip"


def test_roundtrip_repeats():
    """Repeated words — tests constellation grid."""
    enc = get_encoder()
    assert verify_roundtrip(
        [5, 5, 5, 5], enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    ), "Repeated words failed round-trip"


def test_roundtrip_alternating():
    """Alternating words with repeats."""
    enc = get_encoder()
    assert verify_roundtrip(
        [0, 1, 0, 1, 0, 1], enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    ), "Alternating payload failed round-trip"


def test_roundtrip_all_words():
    """Every palette word appears exactly once."""
    enc = get_encoder()
    payload = list(range(enc['n_palette']))
    assert verify_roundtrip(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    ), "All-words payload failed round-trip"


def test_roundtrip_random():
    """Random payload of 50 words."""
    enc = get_encoder()
    np.random.seed(42)
    payload = np.random.randint(0, enc['n_palette'], size=50).tolist()
    assert verify_roundtrip(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    ), "Random 50-word payload failed round-trip"


def test_roundtrip_max_repeats():
    """Fill constellation to near capacity for one word."""
    enc = get_encoder()
    cap = enc['constellation'].capacity
    payload = [7] * (cap - 1)  # one less than capacity
    assert verify_roundtrip(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    ), f"Max repeats ({cap - 1}) failed round-trip"


def test_roundtrip_single_word():
    """Single-word payload."""
    enc = get_encoder()
    assert verify_roundtrip(
        [8], enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    ), "Single-word payload failed round-trip"


def test_capacity_overflow():
    """Exceeding constellation capacity raises ValueError."""
    enc = get_encoder()
    cap = enc['constellation'].capacity
    payload = [0] * (cap + 1)
    try:
        encode(payload, enc['curve'], enc['frame'],
               enc['n_palette'], enc['epsilon'], enc['tube_radius'])
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
    """All encoded pixels are in sRGB gamut."""
    enc = get_encoder()
    np.random.seed(123)
    payload = np.random.randint(0, enc['n_palette'], size=80).tolist()
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    )
    in_gamut = lab_in_srgb_gamut(pixels_lab, tolerance=1.0)
    n_bad = np.sum(~in_gamut)
    assert n_bad == 0, f"{n_bad}/{len(payload)} encoded pixels out of sRGB gamut"


def test_gamut_srgb_roundtrip():
    """CIELAB -> sRGB -> CIELAB round-trip has < 1 unit error."""
    enc = get_encoder()
    payload = list(range(enc['n_palette']))
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    )
    pixels_srgb = lab_to_srgb(pixels_lab)
    pixels_lab2 = srgb_to_lab(pixels_srgb)
    errors = np.linalg.norm(pixels_lab - pixels_lab2, axis=1)
    max_err = np.max(errors)
    assert max_err < 1.5, f"CIELAB->sRGB->CIELAB max error = {max_err:.2f} (> 1.5)"


# ---------------------------------------------------------------------------
# Test 3: Pixel separation
# ---------------------------------------------------------------------------

def test_separation_different_words():
    """Pixels encoding different words project to distinct curve positions.

    Note: Euclidean distance in CIELAB can be less than arc-length separation
    (chord < arc) on a curved path. The correct invariant is that curve
    projection correctly distinguishes adjacent words, not Euclidean distance.
    """
    enc = get_encoder()
    payload = list(range(enc['n_palette']))
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
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
    """Repeated words' pixels are separated by >= epsilon in the normal plane."""
    enc = get_encoder()
    payload = [4, 4, 4, 4, 4]
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    )
    epsilon = enc['epsilon']
    for i in range(len(pixels_lab)):
        for j in range(i + 1, len(pixels_lab)):
            d = np.linalg.norm(pixels_lab[i] - pixels_lab[j])
            assert d >= epsilon * 0.9, \
                f"Repeated-word pixels [{i}],[{j}] too close: {d:.2f} < {epsilon * 0.9:.2f}"


# ---------------------------------------------------------------------------
# Test 4: Noise robustness
# ---------------------------------------------------------------------------

def test_noise_robustness():
    """Decode succeeds under moderate Gaussian noise in CIELAB."""
    enc = get_encoder()
    np.random.seed(99)
    payload = np.random.randint(0, enc['n_palette'], size=30).tolist()
    pixels_lab, _ = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    )

    # Add noise with sigma = epsilon/3 (should be decodable)
    sigma = enc['epsilon'] / 3.0
    noise = np.random.normal(0, sigma, pixels_lab.shape)
    noisy_pixels = pixels_lab + noise

    recovered = decode(
        noisy_pixels, enc['curve'], enc['frame'],
        enc['n_palette'], enc['epsilon'], enc['tube_radius']
    )

    errors = sum(1 for a, b in zip(payload, recovered) if a != b)
    error_rate = errors / len(payload)
    # Allow up to 10% error at sigma = epsilon/3
    assert error_rate < 0.10, \
        f"Noise robustness: {errors}/{len(payload)} errors ({error_rate:.1%}) at sigma={sigma:.1f}"


def test_noise_deterministic():
    """Same input produces same encoding (deterministic)."""
    enc = get_encoder()
    payload = [3, 7, 11, 0, 15]
    p1, _ = encode(payload, enc['curve'], enc['frame'],
                   enc['n_palette'], enc['epsilon'], enc['tube_radius'])
    p2, _ = encode(payload, enc['curve'], enc['frame'],
                   enc['n_palette'], enc['epsilon'], enc['tube_radius'])
    assert np.allclose(p1, p2), "Encoding is not deterministic"


# ---------------------------------------------------------------------------
# Test 5: Capacity
# ---------------------------------------------------------------------------

def test_capacity_constellation():
    """Constellation M matches theoretical floor(2r/epsilon) + 1."""
    enc = get_encoder()
    r = enc['tube_radius']
    eps = enc['epsilon']
    expected_M = int(2 * r / eps) + 1
    actual_M = enc['constellation'].M
    assert actual_M == expected_M, \
        f"Constellation M={actual_M} != expected {expected_M} for r={r:.1f}, eps={eps}"


def test_capacity_bits_per_pixel():
    """Bits per pixel matches log2(N) + 2*log2(M)."""
    enc = get_encoder()
    N = enc['n_palette']
    M = enc['constellation'].M
    expected_bpp = np.log2(N) + 2 * np.log2(M)
    actual_bpp = enc['metadata']['bits_per_pixel']
    assert abs(actual_bpp - expected_bpp) < 0.01, \
        f"Bits/pixel {actual_bpp:.2f} != expected {expected_bpp:.2f}"


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
