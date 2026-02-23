#!/usr/bin/env python3
"""
Parametric curve encoding for visual Glossia.

Encodes payloads into an image by treating a color palette as a parametric curve
in CIELAB color space. Each pixel carries two pieces of information:

  - Tangential position on the curve  -> identifies the payload word
  - 2D displacement in the normal plane -> encodes sequence position

The normal plane at each curve point is spanned by a Bishop frame (U1, U2),
giving an M x M constellation of sequence positions per palette color.

Usage:
  from parametric_encoding import build_encoder, encode, decode

  enc = build_encoder('palette.yaml')
  pixels_lab, meta = encode(payload, enc['curve'], enc['frame'],
                            enc['n_palette'],
                            constellation_map=enc['constellation_map'])
  recovered = decode(pixels_lab, enc['curve'], enc['frame'],
                     enc['n_palette'],
                     constellation_map=enc['constellation_map'])
"""

import numpy as np
from scipy.interpolate import CubicSpline
from scipy.optimize import minimize_scalar
import yaml
import os

# CIELAB Just-Noticeable Difference — fixed constellation grid spacing
EPSILON = 2.3

# ---------------------------------------------------------------------------
# CIELAB <-> sRGB conversion (no external color library required)
# ---------------------------------------------------------------------------

# D65 illuminant reference white in XYZ
D65_XN, D65_YN, D65_ZN = 0.95047, 1.00000, 1.08883

# sRGB -> XYZ matrix (D65)
_SRGB_TO_XYZ = np.array([
    [0.4124564, 0.3575761, 0.1804375],
    [0.2126729, 0.7151522, 0.0721750],
    [0.0193339, 0.1191920, 0.9503041],
])

# XYZ -> sRGB matrix (inverse of above)
_XYZ_TO_SRGB = np.linalg.inv(_SRGB_TO_XYZ)


def _srgb_gamma_expand(c):
    """sRGB gamma expansion: [0,1] nonlinear -> [0,1] linear."""
    c = np.asarray(c, dtype=np.float64)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def _srgb_gamma_compress(c):
    """sRGB gamma compression: [0,1] linear -> [0,1] nonlinear."""
    c = np.asarray(c, dtype=np.float64)
    return np.where(c <= 0.0031308, 12.92 * c, 1.055 * c ** (1.0 / 2.4) - 0.055)


def _lab_f(t):
    """CIELAB forward nonlinearity."""
    delta = 6.0 / 29.0
    return np.where(t > delta ** 3, np.cbrt(t), t / (3 * delta ** 2) + 4.0 / 29.0)


def _lab_f_inv(t):
    """CIELAB inverse nonlinearity."""
    delta = 6.0 / 29.0
    return np.where(t > delta, t ** 3, 3 * delta ** 2 * (t - 4.0 / 29.0))


def srgb_to_lab(rgb):
    """Convert sRGB [0-255] to CIELAB.

    Args:
        rgb: (..., 3) array of sRGB values in [0, 255]

    Returns:
        (..., 3) array of [L*, a*, b*]
    """
    rgb = np.asarray(rgb, dtype=np.float64)
    # Normalize to [0, 1] and gamma-expand
    linear = _srgb_gamma_expand(rgb / 255.0)
    # To XYZ
    xyz = linear @ _SRGB_TO_XYZ.T
    # Normalize by D65
    xyz_n = xyz / np.array([D65_XN, D65_YN, D65_ZN])
    f = _lab_f(xyz_n)
    L = 116.0 * f[..., 1] - 16.0
    a = 500.0 * (f[..., 0] - f[..., 1])
    b = 200.0 * (f[..., 1] - f[..., 2])
    return np.stack([L, a, b], axis=-1)


def lab_to_srgb(lab):
    """Convert CIELAB to sRGB [0-255].

    Args:
        lab: (..., 3) array of [L*, a*, b*]

    Returns:
        (..., 3) array of sRGB values in [0, 255] (clamped)
    """
    lab = np.asarray(lab, dtype=np.float64)
    L, a, b = lab[..., 0], lab[..., 1], lab[..., 2]
    fy = (L + 16.0) / 116.0
    fx = a / 500.0 + fy
    fz = fy - b / 200.0
    f = np.stack([fx, fy, fz], axis=-1)
    xyz_n = _lab_f_inv(f)
    xyz = xyz_n * np.array([D65_XN, D65_YN, D65_ZN])
    linear = xyz @ _XYZ_TO_SRGB.T
    srgb = _srgb_gamma_compress(np.clip(linear, 0, None))
    return np.clip(np.round(srgb * 255.0), 0, 255).astype(np.uint8)


def lab_in_srgb_gamut(lab, tolerance=0.001):
    """Check if a CIELAB color maps to a valid sRGB color.

    Checks that the linear sRGB values are within [0, 1], with a small
    tolerance for numerical noise. Colors outside this range get clipped
    when converted to uint8, causing potentially large CIELAB round-trip
    errors.

    Args:
        lab: (..., 3) array of [L*, a*, b*]
        tolerance: allowed overshoot in linear sRGB [0, 1]
            (default: 0.001, ~0.26 of a uint8 step)

    Returns:
        boolean array, True if in gamut
    """
    lab = np.asarray(lab, dtype=np.float64)
    L, a, b = lab[..., 0], lab[..., 1], lab[..., 2]
    fy = (L + 16.0) / 116.0
    fx = a / 500.0 + fy
    fz = fy - b / 200.0
    f = np.stack([fx, fy, fz], axis=-1)
    xyz_n = _lab_f_inv(f)
    xyz = xyz_n * np.array([D65_XN, D65_YN, D65_ZN])
    linear = xyz @ _XYZ_TO_SRGB.T
    return np.all((linear >= -tolerance) & (linear <= 1.0 + tolerance), axis=-1)


# ---------------------------------------------------------------------------
# Palette curve: cubic spline in CIELAB, arc-length parameterized
# ---------------------------------------------------------------------------

class PaletteCurve:
    """A smooth parametric curve through CIELAB control points.

    The curve is a cubic spline interpolation through K control points,
    reparameterized by arc length so that |gamma'(s)| ~ 1 everywhere.
    """

    def __init__(self, points_lab, n_samples=2000):
        """Build curve from CIELAB control points.

        Args:
            points_lab: (K, 3) array of control points in CIELAB
            n_samples: number of samples for arc-length computation
        """
        points_lab = np.asarray(points_lab, dtype=np.float64)
        assert points_lab.ndim == 2 and points_lab.shape[1] == 3
        self.control_points = points_lab
        self.K = len(points_lab)
        self.n_samples = n_samples

        # Cubic spline through control points (parameter u in [0, 1])
        u = np.linspace(0, 1, self.K)
        self._spline_L = CubicSpline(u, points_lab[:, 0])
        self._spline_a = CubicSpline(u, points_lab[:, 1])
        self._spline_b = CubicSpline(u, points_lab[:, 2])

        # Compute arc-length table: s(u) by numerical integration
        u_fine = np.linspace(0, 1, n_samples)
        pts = self._eval_raw(u_fine)  # (n_samples, 3)
        diffs = np.diff(pts, axis=0)
        seg_lengths = np.linalg.norm(diffs, axis=1)
        self._s_table = np.concatenate([[0], np.cumsum(seg_lengths)])
        self._u_table = u_fine
        self.arc_length = self._s_table[-1]

        # Build spline from s -> u for arc-length reparameterization
        self._s_to_u_spline = CubicSpline(self._s_table, self._u_table)

    def _eval_raw(self, u):
        """Evaluate the raw spline at parameter u (not arc-length)."""
        u = np.asarray(u, dtype=np.float64)
        L = self._spline_L(u)
        a = self._spline_a(u)
        b = self._spline_b(u)
        return np.stack([L, a, b], axis=-1)

    def _eval_raw_deriv(self, u):
        """Evaluate the raw spline derivative at parameter u."""
        u = np.asarray(u, dtype=np.float64)
        dL = self._spline_L(u, 1)
        da = self._spline_a(u, 1)
        db = self._spline_b(u, 1)
        return np.stack([dL, da, db], axis=-1)

    def _eval_raw_deriv2(self, u):
        """Evaluate the raw spline second derivative at parameter u."""
        u = np.asarray(u, dtype=np.float64)
        dL = self._spline_L(u, 2)
        da = self._spline_a(u, 2)
        db = self._spline_b(u, 2)
        return np.stack([dL, da, db], axis=-1)

    def _s_to_u(self, s):
        """Convert arc-length parameter s to raw parameter u."""
        s = np.clip(s, 0, self.arc_length)
        return self._s_to_u_spline(s)

    def eval(self, s):
        """Evaluate gamma(s) at arc-length parameter s.

        Args:
            s: scalar or array of arc-length values in [0, L]

        Returns:
            (..., 3) CIELAB coordinates
        """
        u = self._s_to_u(s)
        return self._eval_raw(u)

    def tangent(self, s):
        """Unit tangent T(s) = gamma'(s) / |gamma'(s)| at arc-length s.

        Returns:
            (..., 3) unit tangent vectors
        """
        u = self._s_to_u(s)
        deriv = self._eval_raw_deriv(u)
        norms = np.linalg.norm(deriv, axis=-1, keepdims=True)
        norms = np.maximum(norms, 1e-12)
        return deriv / norms

    def curvature_normal(self, s):
        """Frenet normal N(s) and curvature kappa(s).

        Returns:
            N: (..., 3) principal normal (unit vector toward center of curvature)
            kappa: (...) curvature values
        """
        u = self._s_to_u(s)
        d1 = self._eval_raw_deriv(u)
        d2 = self._eval_raw_deriv2(u)
        speed = np.linalg.norm(d1, axis=-1, keepdims=True)
        speed = np.maximum(speed, 1e-12)

        # T = d1/|d1|, then dT/du = (d2*|d1| - d1*(d1.d2/|d1|)) / |d1|^2
        T = d1 / speed
        # dT/ds = (1/|d1|) * dT/du
        # curvature vector = dT/ds = (d2 - (d2.T)T) / |d1|^2
        proj = np.sum(d2 * T, axis=-1, keepdims=True) * T
        kappa_vec = (d2 - proj) / speed ** 2
        kappa = np.linalg.norm(kappa_vec, axis=-1)
        N = np.zeros_like(kappa_vec)
        mask = kappa > 1e-10
        if np.any(mask):
            N[mask] = kappa_vec[mask] / kappa[mask][..., None]
        return N, kappa

    def project(self, points_lab):
        """Find the nearest point on the curve for each input point.

        Args:
            points_lab: (..., 3) CIELAB points

        Returns:
            s_nearest: (...) arc-length parameters of nearest curve points
            dist: (...) distances to curve
        """
        points_lab = np.asarray(points_lab, dtype=np.float64)
        original_shape = points_lab.shape[:-1]
        points_flat = points_lab.reshape(-1, 3)

        # Coarse search over sampled curve points
        n_search = min(self.n_samples, 2000)
        s_search = np.linspace(0, self.arc_length, n_search)
        curve_pts = self.eval(s_search)  # (n_search, 3)

        s_results = np.zeros(len(points_flat))
        dist_results = np.zeros(len(points_flat))

        for i, p in enumerate(points_flat):
            dists = np.linalg.norm(curve_pts - p, axis=1)
            best_idx = np.argmin(dists)

            # Refine with local optimization
            s_lo = s_search[max(0, best_idx - 2)]
            s_hi = s_search[min(n_search - 1, best_idx + 2)]

            result = minimize_scalar(
                lambda s: np.sum((self.eval(s) - p) ** 2),
                bounds=(s_lo, s_hi),
                method='bounded'
            )
            s_results[i] = result.x
            dist_results[i] = np.sqrt(result.fun)

        return s_results.reshape(original_shape), dist_results.reshape(original_shape)

    @classmethod
    def from_yaml(cls, yaml_path, palette_name='viridis_approx', n_samples=2000):
        """Load a palette curve from a YAML config file.

        Args:
            yaml_path: path to palette.yaml
            palette_name: key in the palettes dict
            n_samples: curve sampling density

        Returns:
            PaletteCurve instance
        """
        with open(yaml_path) as f:
            config = yaml.safe_load(f)
        pts = np.array(config['palettes'][palette_name]['control_points_lab'])
        return cls(pts, n_samples=n_samples)

    @classmethod
    def from_control_points(cls, control_points_lab, n_samples=2000):
        """Build directly from a list/array of CIELAB points."""
        return cls(np.array(control_points_lab), n_samples=n_samples)


# ---------------------------------------------------------------------------
# Bishop frame (rotation-minimizing frame) via double-reflection method
# ---------------------------------------------------------------------------

class BishopFrame:
    """Rotation-minimizing frame along a PaletteCurve.

    At each sampled arc-length s, provides orthonormal {T(s), U1(s), U2(s)}:
      - T: unit tangent
      - U1, U2: smoothly varying normal-plane basis vectors

    Computed via the double-reflection method (Wang et al. 2008).
    """

    def __init__(self, curve, n_frames=500):
        """Compute the Bishop frame along the curve.

        Args:
            curve: PaletteCurve instance
            n_frames: number of frame sample points
        """
        self.curve = curve
        self.n_frames = n_frames
        self.s_samples = np.linspace(0, curve.arc_length, n_frames)

        # Compute tangent at all sample points
        T = curve.tangent(self.s_samples)  # (n_frames, 3)

        # Initialize U1(0): use Frenet normal if curvature > 0, else arbitrary
        N0, kappa0 = curve.curvature_normal(self.s_samples[0])
        if isinstance(kappa0, np.ndarray):
            kappa0 = kappa0.item()
        if kappa0 > 1e-6:
            u1_0 = N0.flatten()
        else:
            # Arbitrary perpendicular to T[0]
            u1_0 = self._arbitrary_perp(T[0])
        u1_0 = u1_0 / np.linalg.norm(u1_0)

        # U2(0) = T(0) x U1(0)
        u2_0 = np.cross(T[0], u1_0)
        u2_0 = u2_0 / np.linalg.norm(u2_0)

        # Propagate via double-reflection (Wang et al. 2008)
        U1 = np.zeros((n_frames, 3))
        U2 = np.zeros((n_frames, 3))
        U1[0] = u1_0
        U2[0] = u2_0

        for i in range(n_frames - 1):
            # Double-reflection method
            v1 = curve.eval(self.s_samples[i + 1]) - curve.eval(self.s_samples[i])
            c1 = np.dot(v1, v1)
            if c1 < 1e-20:
                U1[i + 1] = U1[i]
                U2[i + 1] = U2[i]
                continue

            # First reflection: reflect U1[i] and T[i] across the plane
            # perpendicular to v1 at the midpoint
            rL = U1[i] - (2.0 / c1) * np.dot(v1, U1[i]) * v1
            tL = T[i] - (2.0 / c1) * np.dot(v1, T[i]) * v1

            # Second reflection: reflect across the plane perpendicular to
            # (T[i+1] - tL) to align with T[i+1]
            v2 = T[i + 1] - tL
            c2 = np.dot(v2, v2)
            if c2 < 1e-20:
                U1[i + 1] = rL
            else:
                U1[i + 1] = rL - (2.0 / c2) * np.dot(v2, rL) * v2

            # Normalize and compute U2
            U1[i + 1] = U1[i + 1] / np.linalg.norm(U1[i + 1])
            U2[i + 1] = np.cross(T[i + 1], U1[i + 1])
            U2[i + 1] = U2[i + 1] / np.linalg.norm(U2[i + 1])

        self._T = T
        self._U1 = U1
        self._U2 = U2

        # Build interpolation splines for continuous evaluation
        from scipy.interpolate import CubicSpline as CS
        self._T_spline = [CS(self.s_samples, T[:, j]) for j in range(3)]
        self._U1_spline = [CS(self.s_samples, U1[:, j]) for j in range(3)]
        self._U2_spline = [CS(self.s_samples, U2[:, j]) for j in range(3)]

    @staticmethod
    def _arbitrary_perp(v):
        """Find an arbitrary unit vector perpendicular to v."""
        v = np.asarray(v, dtype=np.float64)
        # Pick the coordinate axis least aligned with v
        abs_v = np.abs(v)
        if abs_v[0] <= abs_v[1] and abs_v[0] <= abs_v[2]:
            candidate = np.array([1.0, 0.0, 0.0])
        elif abs_v[1] <= abs_v[2]:
            candidate = np.array([0.0, 1.0, 0.0])
        else:
            candidate = np.array([0.0, 0.0, 1.0])
        perp = candidate - np.dot(candidate, v) * v / np.dot(v, v)
        return perp / np.linalg.norm(perp)

    def eval_frame(self, s):
        """Evaluate the Bishop frame at arc-length s.

        Args:
            s: scalar or 1D array of arc-length values

        Returns:
            T:  (..., 3) unit tangent
            U1: (..., 3) first normal
            U2: (..., 3) second normal
        """
        s = np.asarray(s, dtype=np.float64)
        s = np.clip(s, 0, self.curve.arc_length)
        scalar = s.ndim == 0
        s = np.atleast_1d(s)

        T = np.stack([sp(s) for sp in self._T_spline], axis=-1)
        U1 = np.stack([sp(s) for sp in self._U1_spline], axis=-1)
        U2 = np.stack([sp(s) for sp in self._U2_spline], axis=-1)

        # Re-orthonormalize (spline interpolation can drift slightly)
        T = T / np.linalg.norm(T, axis=-1, keepdims=True)
        U1 = U1 - np.sum(U1 * T, axis=-1, keepdims=True) * T
        U1 = U1 / np.linalg.norm(U1, axis=-1, keepdims=True)
        U2 = np.cross(T, U1)
        U2 = U2 / np.linalg.norm(U2, axis=-1, keepdims=True)

        if scalar:
            return T[0], U1[0], U2[0]
        return T, U1, U2


# ---------------------------------------------------------------------------
# Tube geometry: sRGB gamut boundary distance in the normal plane
# ---------------------------------------------------------------------------

def compute_tube_radius(curve, frame, n_palette=None, s_values=None,
                        n_angles=16, max_radius=60.0, step=0.5):
    """Compute the tube radius at each palette point.

    The tube radius r(s) is the maximum displacement along any direction
    in the normal plane that stays within the sRGB gamut.

    Args:
        curve: PaletteCurve
        frame: BishopFrame
        n_palette: if given, compute at N equally-spaced palette points
        s_values: explicit arc-length values to evaluate (overrides n_palette)
        n_angles: number of angular samples in the normal plane
        max_radius: maximum radius to search
        step: radial step for ray marching

    Returns:
        s_pts: (M,) arc-length values
        radii: (M,) minimum tube radius at each point
    """
    if s_values is None:
        if n_palette is None:
            n_palette = 100
        s_pts = np.linspace(0, curve.arc_length, n_palette)
    else:
        s_pts = np.asarray(s_values)

    radii = np.zeros(len(s_pts))
    angles = np.linspace(0, 2 * np.pi, n_angles, endpoint=False)

    for i, s in enumerate(s_pts):
        base = curve.eval(s)
        _, U1, U2 = frame.eval_frame(s)
        min_r = max_radius
        for theta in angles:
            direction = np.cos(theta) * U1 + np.sin(theta) * U2
            # Binary search for gamut boundary
            lo, hi = 0.0, max_radius
            while hi - lo > step:
                mid = (lo + hi) / 2
                test_pt = base + mid * direction
                if lab_in_srgb_gamut(test_pt):
                    lo = mid
                else:
                    hi = mid
            min_r = min(min_r, lo)
        radii[i] = min_r

    return s_pts, radii


# ---------------------------------------------------------------------------
# 2D Constellation: mapping sequence positions to normal-plane grid
# ---------------------------------------------------------------------------

class Constellation:
    """M x M grid of sequence positions in the normal plane.

    Grid point (a, b) maps to displacement:
        alpha_a = (a - (M-1)/2) * epsilon
        alpha_b = (b - (M-1)/2) * epsilon

    Sequence position j = a * M + b (raster order).
    """

    def __init__(self, M, epsilon):
        self.M = M
        self.epsilon = epsilon
        self.capacity = M * M  # sequence positions per palette color

    @classmethod
    def from_radius(cls, radius, epsilon):
        """Create constellation from tube radius and step size.

        The grid is MxM with spacing epsilon. The farthest grid point
        (corner) is at distance (M-1)/2 * sqrt(2) * epsilon from center.
        To stay within the tube radius, we inscribe the square grid inside
        the circle: M <= sqrt(2) * radius / epsilon + 1.

        Args:
            radius: tube radius in CIELAB units
            epsilon: minimum step between constellation points

        Returns:
            Constellation instance
        """
        M = int(np.sqrt(2) * radius / epsilon) + 1
        M = max(M, 1)
        return cls(M, epsilon)

    def position_to_grid(self, j):
        """Map sequence position to grid coordinates (a, b).

        Args:
            j: sequence position (int or array)

        Returns:
            a, b: grid coordinates
        """
        j = np.asarray(j, dtype=int)
        a = j // self.M
        b = j % self.M
        return a, b

    def grid_to_position(self, a, b):
        """Map grid coordinates to sequence position.

        Args:
            a, b: grid coordinates

        Returns:
            j: sequence position
        """
        return np.asarray(a, dtype=int) * self.M + np.asarray(b, dtype=int)

    def grid_to_displacement(self, a, b):
        """Map grid coordinates to (alpha1, alpha2) displacements.

        Args:
            a, b: grid coordinates

        Returns:
            alpha1, alpha2: displacement magnitudes in CIELAB units
        """
        a, b = np.asarray(a, dtype=np.float64), np.asarray(b, dtype=np.float64)
        center = (self.M - 1) / 2.0
        alpha1 = (a - center) * self.epsilon
        alpha2 = (b - center) * self.epsilon
        return alpha1, alpha2

    def displacement_to_grid(self, alpha1, alpha2):
        """Snap continuous displacements to nearest grid coordinates.

        Args:
            alpha1, alpha2: displacement magnitudes in CIELAB units

        Returns:
            a, b: grid coordinates (clamped to valid range)
        """
        center = (self.M - 1) / 2.0
        a = np.round(alpha1 / self.epsilon + center).astype(int)
        b = np.round(alpha2 / self.epsilon + center).astype(int)
        a = np.clip(a, 0, self.M - 1)
        b = np.clip(b, 0, self.M - 1)
        return a, b

    def position_to_displacement(self, j):
        """Map sequence position to displacement vector components."""
        a, b = self.position_to_grid(j)
        return self.grid_to_displacement(a, b)

    def displacement_to_position(self, alpha1, alpha2):
        """Snap displacements and recover sequence position."""
        a, b = self.displacement_to_grid(alpha1, alpha2)
        return self.grid_to_position(a, b)


class ConstellationMap:
    """Per-color constellations keyed by palette index.

    Each palette color gets its own Constellation sized to the local
    tube radius: M_i = floor(2 * r(s_i) / epsilon) + 1.  This exploits
    the full capacity of fat-tube regions instead of being limited by
    the global minimum radius.
    """

    def __init__(self, radii, epsilon=EPSILON):
        """Build one Constellation per palette color from local tube radii.

        Args:
            radii: 1-D array of tube radii, one per palette color
            epsilon: grid spacing in CIELAB units (default: EPSILON)
        """
        radii = np.asarray(radii, dtype=np.float64)
        self.epsilon = epsilon
        self.constellations = [
            Constellation.from_radius(float(r), epsilon) for r in radii
        ]
        self.M_values = np.array([c.M for c in self.constellations])
        self.capacities = np.array([c.capacity for c in self.constellations])

    def __getitem__(self, palette_index):
        """Return the Constellation for a given palette color."""
        return self.constellations[palette_index]

    def __len__(self):
        return len(self.constellations)

    @property
    def M_min(self):
        return int(np.min(self.M_values))

    @property
    def M_max(self):
        return int(np.max(self.M_values))

    @property
    def capacity_min(self):
        return int(np.min(self.capacities))

    @property
    def capacity_max(self):
        return int(np.max(self.capacities))

    def total_capacity(self):
        """Sum of all per-color capacities."""
        return int(np.sum(self.capacities))


# ---------------------------------------------------------------------------
# Capacity-weighted palette placement (non-uniform spacing)
# ---------------------------------------------------------------------------

def compute_capacity_curve(curve, frame, n_samples=200,
                           n_angles=16, max_radius=60.0, step=0.5):
    """Compute the cumulative capacity function C(s) along the palette curve.

    The capacity density at arc-length s is r(s)^2, proportional to the
    constellation area available in the normal plane at that point.
    Integrating gives a monotonically increasing function C(s) that
    measures accumulated gamut area from the curve start.

    Dividing C into N equal segments places palette colors so that each
    color commands equal gamut area, concentrating colors where the tube
    is fattest and raising M_min across the palette.

    The density r(s)^2 is independent of epsilon, so the capacity curve
    can be computed once and reused across different epsilon values.

    Args:
        curve: PaletteCurve
        frame: BishopFrame
        n_samples: density of samples along the curve (more = finer placement)
        n_angles: angular samples for tube radius computation
        max_radius: maximum tube radius search bound (CIELAB)
        step: radial step precision (CIELAB)

    Returns:
        s_dense: (n_samples,) arc-length sample points
        radii: (n_samples,) tube radius at each sample point
        C: (n_samples,) cumulative capacity (units: CIELAB^2 * arc-length)
    """
    s_dense = np.linspace(0, curve.arc_length, n_samples)
    _, radii = compute_tube_radius(curve, frame, s_values=s_dense,
                                    n_angles=n_angles, max_radius=max_radius,
                                    step=step)

    # Capacity density = r(s)^2  (proportional to constellation area M^2,
    # since M = floor(2r/eps)+1 ~ 2r/eps, so M^2 ~ 4r^2/eps^2)
    density = radii ** 2

    # Cumulative via trapezoidal integration
    ds = np.diff(s_dense)
    avg_density = (density[:-1] + density[1:]) / 2
    C = np.concatenate([[0], np.cumsum(avg_density * ds)])

    return s_dense, radii, C


def equal_capacity_positions(s_dense, C, N, mode='centroid'):
    """Place N palette colors at equal-capacity divisions of the curve.

    Inverts the cumulative capacity function C(s) at N equally-spaced
    capacity values, so each color owns an equal share of the total
    gamut area along the curve.

    With uniform arc-length spacing, colors in thin-tube regions have
    small constellations (low M_i) that bottleneck bit packing. This
    function pushes colors toward fat-tube regions, raising M_min and
    equalizing per-color capacity.

    Two placement modes:
      - 'centroid': color i at (i+0.5)/N through capacity (default).
        No color lands at the curve endpoints, avoiding the thinnest
        tube regions and raising M_min.
      - 'boundary': color i at i/(N-1) through capacity.
        First and last colors land at curve endpoints (s=0 and s=L).

    Args:
        s_dense: (M,) arc-length sample points
        C: (M,) cumulative capacity values (from compute_capacity_curve)
        N: number of palette colors to place
        mode: 'centroid' (default) or 'boundary'

    Returns:
        s_palette: (N,) arc-length positions for palette colors
    """
    C_total = C[-1]

    if mode == 'centroid':
        # Color i at the centroid of its capacity segment — avoids
        # pinning colors at thin-tube curve endpoints
        target_C = np.array([(i + 0.5) * C_total / N for i in range(N)])
    else:
        # Boundary: C(s_i) = i * C_total / (N-1), endpoints included
        target_C = np.linspace(0, C_total, N)

    # Invert C(s) via linear interpolation
    s_palette = np.interp(target_C, C, s_dense)

    return s_palette


def generate_payload_tokens(N):
    """Generate N payload token names for the image codec.

    Token names are positional indices (c00, c01, ...) into the palette
    curve. The actual CIELAB color of each token depends on the curve,
    spacing mode, and epsilon — not the token name. With adaptive
    spacing, token c05 at N=16 maps to a different arc-length position
    (and thus different color) than c05 at N=64.

    Args:
        N: number of palette colors

    Returns:
        list of token name strings
    """
    width = max(2, len(str(N - 1)))
    return [f"c{i:0{width}d}" for i in range(N)]


def min_srgb_distance(curve, s_palette):
    """Compute minimum pairwise Euclidean distance in 8-bit sRGB space.

    This is the bottleneck for camera-based decoding: two palette colors
    that are close in sRGB cannot be distinguished from a photograph,
    regardless of their CIELAB separation.

    Args:
        curve: PaletteCurve
        s_palette: array of arc-length positions

    Returns:
        Minimum pairwise sRGB distance (Euclidean in [0-255]^3 space).
        Returns inf for fewer than 2 colors.
    """
    N = len(s_palette)
    if N < 2:
        return float('inf')

    labs = curve.eval(s_palette)
    srgbs = lab_to_srgb(labs).astype(int)

    min_d = float('inf')
    for i in range(N):
        for j in range(i + 1, N):
            d = float(np.sqrt(np.sum((srgbs[i] - srgbs[j]) ** 2)))
            if d < min_d:
                min_d = d
    return min_d


# Default minimum sRGB distance for camera-decodable images.
# At d=15, palette colors are distinguishable in phone photos under
# normal lighting. This constraint caps N (e.g., viridis: N=16 instead
# of N=128) but ensures the image works like a QR code — scannable
# from a photo.
MIN_SRGB_DISTANCE = 15.0


def select_encoding_params(curve, frame,
                           configs=None,
                           n_capacity_samples=200,
                           min_srgb_dist=MIN_SRGB_DISTANCE):
    """Select the optimal (N, epsilon) for a palette curve.

    The optimal config maximizes bits per cell (bpc), the curve's
    intrinsic channel capacity:

        bpc = log2(N * M_min^2)

    N is not restricted to powers of 2 — mixed-radix encoding
    extracts all N * M^2 distinguishable states per cell.  This is a
    property of the curve geometry alone — message length and image
    resolution don't affect the ranking.  For entropy-preserving
    encoding, each cell should carry the maximum number of bits the
    palette supports.  The caller computes n_cells from the message
    size: n_cells = ceil(total_bits / bpc).

    Among configs with equal bpc, prefers higher epsilon for noise
    robustness.

    The min_srgb_dist constraint ensures camera-decodable images:
    every pair of palette colors must be at least this far apart in
    8-bit sRGB Euclidean distance. This is the bottleneck for
    photograph-based decode — CIELAB precision is irrelevant if the
    8-bit sRGB rendering is ambiguous.

    Args:
        curve: PaletteCurve
        frame: BishopFrame
        configs: (config_table, header_epsilon) from derive_config_table().
                 Derived if not provided.
        n_capacity_samples: samples for capacity curve computation
        min_srgb_dist: minimum pairwise sRGB distance (default 15.0).
                       Set to 0 to disable (text-only decode).

    Returns:
        dict with keys:
            N, epsilon, s_palette, bits_per_cell, states_per_cell,
            M_min, M_max, word_bits, pos_bits, constellation_map, radii_at_palette,
            tokens, configs, all_configs, srgb_dist_min
        Returns None if no valid configuration found.
    """
    if configs is None:
        configs = derive_config_table(curve, frame,
                                       n_capacity_samples=n_capacity_samples)
    config_table, header_eps = configs

    s_dense, radii_dense, C = compute_capacity_curve(
        curve, frame, n_samples=n_capacity_samples)

    best = None
    results = []

    from itertools import groupby
    for N, group in groupby(config_table, key=lambda c: c[0]):
        if N < 2:
            list(group)
            continue

        s_pal = equal_capacity_positions(s_dense, C, N)
        radii_at_pal = np.interp(s_pal, s_dense, radii_dense)

        # Check sRGB distance constraint before considering any epsilon.
        # This is a property of N and the palette positions alone —
        # it doesn't depend on epsilon or constellation parameters.
        srgb_d = min_srgb_distance(curve, s_pal)
        if srgb_d < min_srgb_dist:
            # Skip all epsilons for this N — colors too close in sRGB
            # for camera decode.
            list(group)  # consume the group iterator
            continue

        for _, eps in group:
            cmap = ConstellationMap(radii_at_pal, epsilon=eps)

            if cmap.M_min < 2:
                continue

            # True capacity: log2(N * M_min^2) — works for any N,
            # not just powers of 2.  Mixed-radix encoding extracts
            # all states_per_cell = N * M_min^2 distinguishable values.
            states_per_cell = N * cmap.M_min * cmap.M_min
            bits_per_cell = float(np.log2(states_per_cell))

            if bits_per_cell < 2:
                continue

            result = {
                'N': N,
                'epsilon': eps,
                's_palette': s_pal,
                'bits_per_cell': bits_per_cell,
                'states_per_cell': states_per_cell,
                'M_min': cmap.M_min,
                'M_max': cmap.M_max,
                'word_bits': float(np.log2(N)),
                'pos_bits': float(2 * np.log2(cmap.M_min)),
                'constellation_map': cmap,
                'radii_at_palette': radii_at_pal,
                'tokens': generate_payload_tokens(N),
                'configs': configs,
                'srgb_dist_min': srgb_d,
            }
            results.append(result)

            if best is None or (bits_per_cell, eps) > (best['bits_per_cell'], best['epsilon']):
                best = result

    if best is not None:
        best['all_configs'] = results

    return best


# ---------------------------------------------------------------------------
# Self-describing header: first color encodes the radix
# ---------------------------------------------------------------------------

# The header color sits at a FIXED position on the curve (s=0, the fattest
# tube region) so the decoder can always find it without knowing N.  It uses
# a derived robust epsilon so decoding the header doesn't require knowing
# the payload epsilon.
#
# The header's constellation position encodes an index into a config table
# of (N_payload, epsilon_payload) pairs DERIVED from the curve geometry.
# Both encoder and decoder compute the same table from the same curve, so
# no hardcoded palette sizes or epsilon values are needed.
#
# Analogy: variable-length integer encoding declares its radix first.
# Here, the first palette color declares the color radix (N) and the
# constellation grid spacing (epsilon) for all subsequent colors.

HEADER_S = 0.0  # Arc-length position: always curve start (fattest tube)


def derive_config_table(curve, frame, n_capacity_samples=200,
                        min_epsilon=2.0, N_max=128,
                        min_srgb_dist=MIN_SRGB_DISTANCE):
    """Derive valid (N, epsilon) configurations from the curve geometry.

    Instead of hardcoded palette sizes and epsilon values, this function
    computes feasible configurations directly from the tube radius profile.

    For each N from 2 to N_max, it places palette colors at equal-capacity
    centroid positions and finds r_min (the minimum tube radius across
    those positions).  For each target M_min (power of 2), it computes
    the epsilon that achieves exactly that M_min at the tightest point:

        epsilon = 2 * r_min / (M_target - 1)

    N is not restricted to powers of 2.  Mixed-radix encoding allows
    any N; the true capacity per cell is log2(N * M^2) bits.

    Camera-decodability filter: only configs where all N palette colors
    are at least min_srgb_dist apart in 8-bit sRGB are included.  This
    dramatically reduces the table (viridis: 401 -> ~50 entries).

    Sorting: configs are sorted by bits-per-cell descending, then epsilon
    descending.  Combined with center-out grid mapping in encode_header /
    decode_header, this ensures the optimal config (index 0) gets zero
    constellation displacement, keeping it robust to sRGB quantization.

    The header epsilon is also derived: the largest epsilon at s=0 that
    still provides enough constellation capacity to index the table.

    Both encoder and decoder call this function on the same curve and
    get identical results, so the config table is an implicit contract
    — no hardcoded constants needed.

    Args:
        curve: PaletteCurve
        frame: BishopFrame
        n_capacity_samples: capacity curve density
        min_epsilon: minimum epsilon (below ~2 CIELAB is subthreshold)
        N_max: largest palette size to consider
        min_srgb_dist: minimum pairwise sRGB distance for camera decode.
                       Set to 0 to disable filtering.

    Returns:
        configs: list of (N, epsilon) tuples sorted by bpc descending
        header_epsilon: derived robust epsilon for header decoding
    """
    s_dense, radii_dense, C = compute_capacity_curve(
        curve, frame, n_samples=n_capacity_samples)

    # Target M_min values: powers of 2 for clean constellation grids.
    # M=2 -> 4 positions, M=4 -> 16, M=8 -> 64, M=16 -> 256, M=32 -> 1024
    M_TARGETS = [2, 4, 8, 16, 32]

    # Cache sRGB distance per N (expensive to compute)
    srgb_dist_cache = {}

    configs_with_bpc = []  # (bpc, eps, N, eps_val)
    for N in range(2, N_max + 1):
        # Place N colors at equal-capacity centroids
        s_pal = equal_capacity_positions(s_dense, C, N)
        r_at_pal = np.interp(s_pal, s_dense, radii_dense)
        r_min = float(np.min(r_at_pal))

        # Camera-decodability: check sRGB distance once per N
        if min_srgb_dist > 0:
            if N not in srgb_dist_cache:
                srgb_dist_cache[N] = min_srgb_distance(curve, s_pal)
            if srgb_dist_cache[N] < min_srgb_dist:
                continue

        for M_target in M_TARGETS:
            # epsilon that gives M_min = M_target at the tightest point:
            # M = floor(2*r_min/eps) + 1 = M_target  =>  eps = 2*r_min/(M_target-1)
            #
            # Nudge eps down by one ULP to prevent floating-point truncation
            # from rounding floor(2*r_min/eps) to M_target-2 instead of M_target-1.
            eps = np.nextafter(2.0 * r_min / (M_target - 1), 0.0)

            if eps < min_epsilon:
                continue

            # True capacity: log2(N * M_target^2) — no power-of-2 assumption
            states = N * M_target * M_target
            bpc = np.log2(states)
            if bpc < 2:
                continue

            configs_with_bpc.append((bpc, eps, N, eps))

    # Sort by bpc descending, then eps descending (best configs first).
    # With center-out grid ordering in encode/decode_header, this ensures
    # the optimal config gets index 0 (grid center, zero displacement).
    configs_with_bpc.sort(key=lambda x: (-x[0], -x[1]))
    configs = [(entry[2], entry[3]) for entry in configs_with_bpc]

    # Derive header epsilon from s=0 tube radius and table size.
    # Maximize epsilon (noise robustness) while keeping enough
    # constellation capacity to index every config entry.
    r_header = float(np.interp(HEADER_S, s_dense, radii_dense))
    table_size = len(configs)
    M_header_needed = max(int(np.ceil(np.sqrt(table_size))), 2)
    header_epsilon = 2.0 * r_header / (M_header_needed - 1)

    return configs, header_epsilon


def _center_out_order(M):
    """Build a center-out spiral mapping for an M x M constellation grid.

    Returns two arrays:
        idx_to_pos[i] -> raster position j (a*M + b) for the i-th center-out slot
        pos_to_idx[j] -> center-out index for raster position j

    Index 0 maps to the grid center (smallest displacement), index 1 to the
    next closest position, etc.  This ensures that low config-table indices
    produce small displacements, keeping header colors within sRGB gamut.
    """
    center = (M - 1) / 2.0
    positions = []
    for a in range(M):
        for b in range(M):
            dist_sq = (a - center) ** 2 + (b - center) ** 2
            positions.append((dist_sq, a, b))
    # Sort by distance from center (stable sort preserves raster order for ties)
    positions.sort(key=lambda x: (x[0], x[1], x[2]))
    idx_to_pos = np.array([a * M + b for _, a, b in positions], dtype=int)
    pos_to_idx = np.zeros(M * M, dtype=int)
    for i, j in enumerate(idx_to_pos):
        pos_to_idx[j] = i
    return idx_to_pos, pos_to_idx


def encode_header(n_palette, epsilon, curve, frame, configs=None):
    """Encode (N, epsilon) into the header color at s=0.

    The header color sits at the fixed position HEADER_S on the palette
    curve.  Its constellation displacement encodes the config index,
    making the encoding self-describing.

    Uses center-out grid ordering so that low config indices (the most
    common configs) produce small displacements that stay within sRGB gamut.

    Args:
        n_palette: payload palette size
        epsilon: payload constellation spacing
        curve: PaletteCurve
        frame: BishopFrame
        configs: (config_table, header_epsilon) tuple from
                 derive_config_table().  Derived if not provided.

    Returns:
        pixel_lab: (3,) CIELAB color for the header pixel
    """
    if configs is None:
        configs = derive_config_table(curve, frame)
    config_table, header_eps = configs

    try:
        idx = config_table.index((n_palette, epsilon))
    except ValueError:
        raise ValueError(
            f"({n_palette}, {epsilon}) not in derived config table. "
            f"Valid configs: {config_table}")

    base = curve.eval(HEADER_S)
    _, U1, U2 = frame.eval_frame(HEADER_S)

    _, radii = compute_tube_radius(curve, frame,
                                    s_values=np.array([HEADER_S]))
    c = Constellation.from_radius(float(radii[0]), header_eps)

    if idx >= c.capacity:
        raise ValueError(
            f"Header constellation too small ({c.capacity} positions) "
            f"for config index {idx}")

    # Map config index to center-out grid position
    idx_to_pos, _ = _center_out_order(c.M)
    grid_pos = int(idx_to_pos[idx])
    alpha1, alpha2 = c.position_to_displacement(grid_pos)
    return base + alpha1 * U1 + alpha2 * U2


def decode_header(pixel_lab, curve, frame, configs=None):
    """Decode (N, epsilon) from the header color.

    Args:
        pixel_lab: (3,) CIELAB color (the header pixel)
        curve: PaletteCurve
        frame: BishopFrame
        configs: (config_table, header_epsilon) tuple from
                 derive_config_table().  Derived if not provided.

    Returns:
        n_palette: payload palette size
        epsilon: payload constellation spacing
    """
    if configs is None:
        configs = derive_config_table(curve, frame)
    config_table, header_eps = configs

    base = curve.eval(HEADER_S)
    _, U1, U2 = frame.eval_frame(HEADER_S)

    residual = np.asarray(pixel_lab) - base
    alpha1 = float(np.dot(residual, U1))
    alpha2 = float(np.dot(residual, U2))

    _, radii = compute_tube_radius(curve, frame,
                                    s_values=np.array([HEADER_S]))
    c = Constellation.from_radius(float(radii[0]), header_eps)
    grid_pos = int(c.displacement_to_position(alpha1, alpha2))

    # Reverse the center-out mapping to get config index
    _, pos_to_idx = _center_out_order(c.M)
    idx = int(pos_to_idx[grid_pos])

    if idx >= len(config_table):
        raise ValueError(
            f"Config index {idx} out of range (max {len(config_table)-1})")

    return config_table[idx]


def encode_self_describing(payload_words, curve, frame,
                           n_palette, epsilon,
                           configs=None):
    """Encode payload with a self-describing header color.

    The first color in the output declares the radix (N) and grid
    spacing (epsilon).  The remaining colors carry the payload using
    capacity-weighted adaptive spacing.

    Args:
        payload_words: list of ints, each in [0, n_palette-1]
        curve: PaletteCurve
        frame: BishopFrame
        n_palette: number of payload palette colors
        epsilon: constellation grid spacing for payload
        configs: (config_table, header_epsilon) from derive_config_table().
                 Derived if not provided.  Pass explicitly to avoid
                 recomputing the capacity curve on every call.

    Returns:
        pixels_lab: (1 + n_payload, 3) array — header + payload colors
        metadata: dict with encoding parameters
    """
    if configs is None:
        configs = derive_config_table(curve, frame)
    config_table, _ = configs

    # Header pixel at fixed position
    header_pixel = encode_header(n_palette, epsilon, curve, frame,
                                 configs=configs)

    # Payload colors using adaptive spacing
    enc = build_encoder(n_palette=n_palette, spacing='adaptive',
                        epsilon=epsilon)
    payload_pixels, meta = encode(
        payload_words, curve, frame, n_palette,
        constellation_map=enc['constellation_map'],
        s_palette=enc['s_palette'])

    all_pixels = np.vstack([header_pixel.reshape(1, 3), payload_pixels])
    meta['header_config'] = (n_palette, epsilon)
    meta['header_config_index'] = config_table.index((n_palette, epsilon))
    return all_pixels, meta


def decode_self_describing(pixels_lab, curve, frame, configs=None):
    """Decode a self-describing encoded pixel sequence.

    Reads the header (first pixel) to learn N and epsilon, then
    decodes the remaining pixels using the declared parameters.

    Args:
        pixels_lab: (1 + n_payload, 3) array of CIELAB colors
        curve: PaletteCurve
        frame: BishopFrame
        configs: (config_table, header_epsilon) from derive_config_table().
                 Derived if not provided.

    Returns:
        payload_words: list of ints, the recovered payload
        n_palette: palette size declared by header
        epsilon: grid spacing declared by header
    """
    pixels_lab = np.asarray(pixels_lab)

    if configs is None:
        configs = derive_config_table(curve, frame)

    # Read header
    n_palette, epsilon = decode_header(pixels_lab[0], curve, frame,
                                       configs=configs)

    # Build encoder with declared parameters
    enc = build_encoder(n_palette=n_palette, spacing='adaptive',
                        epsilon=epsilon)

    # Decode payload (everything after the header)
    payload = decode(
        pixels_lab[1:], curve, frame, n_palette,
        constellation_map=enc['constellation_map'],
        s_palette=enc['s_palette'])

    return payload, n_palette, epsilon


# ---------------------------------------------------------------------------
# Encode / Decode
# ---------------------------------------------------------------------------

def encode(payload_words, curve, frame, n_palette,
           constellation_map=None, s_palette=None):
    """Encode a payload word sequence into CIELAB pixel colors.

    Args:
        payload_words: list of ints, each in [0, n_palette-1]
        curve: PaletteCurve
        frame: BishopFrame
        n_palette: number of palette colors (N)
        constellation_map: ConstellationMap (if None, computed from s_palette)
        s_palette: (N,) arc-length positions for palette colors. If None,
                   uses uniform spacing. Pass non-uniform positions from
                   equal_capacity_positions() for adaptive mode.

    Returns:
        pixels_lab: (M, 3) array of encoded CIELAB colors
        metadata: dict with encoding parameters
    """
    payload_words = np.asarray(payload_words, dtype=int)
    n_words = len(payload_words)

    # Compute palette positions if not given (uniform spacing fallback)
    if s_palette is None:
        s_palette = np.array([
            w * curve.arc_length / max(n_palette - 1, 1)
            for w in range(n_palette)
        ])
    if constellation_map is None:
        _, radii = compute_tube_radius(curve, frame, s_values=s_palette)
        constellation_map = ConstellationMap(radii)

    # Count occurrences of each word to assign sequence positions
    # Words are ordered by their position in the payload sequence.
    # Each word w gets assigned sequence positions within the constellation
    # for palette color w.
    word_counters = {}  # word_index -> next sequence position for that word
    pixels_lab = np.zeros((n_words, 3))

    for i, w in enumerate(payload_words):
        w = int(w)
        c = constellation_map[w]
        # Arc-length parameter for this palette color
        s_w = s_palette[w]
        base = curve.eval(s_w)
        _, U1, U2 = frame.eval_frame(s_w)

        # Assign sequence position within this word's constellation
        j = word_counters.get(w, 0)
        if j >= c.capacity:
            raise ValueError(
                f"Payload word {w} appears more than {c.capacity} "
                f"times (constellation capacity exceeded)."
            )
        word_counters[w] = j + 1

        # Map sequence position to displacement
        alpha1, alpha2 = c.position_to_displacement(j)
        pixel = base + alpha1 * U1 + alpha2 * U2
        pixels_lab[i] = pixel

    metadata = {
        'n_palette': n_palette,
        'epsilon': EPSILON,
        'M_min': constellation_map.M_min,
        'M_max': constellation_map.M_max,
        'capacity_min': constellation_map.capacity_min,
        'capacity_max': constellation_map.capacity_max,
        'n_payload_words': n_words,
        'max_word_repeats': max(word_counters.values()) if word_counters else 0,
    }
    return pixels_lab, metadata


def decode(pixels_lab, curve, frame, n_palette,
           constellation_map=None, s_palette=None):
    """Decode CIELAB pixel colors back to a payload word sequence.

    Args:
        pixels_lab: (M, 3) array of CIELAB colors
        curve: PaletteCurve
        frame: BishopFrame
        n_palette: number of palette colors (N)
        constellation_map: ConstellationMap (if None, computed from s_palette)
        s_palette: (N,) arc-length positions for palette colors. If None,
                   uses uniform spacing. Must match the positions used for
                   encoding.

    Returns:
        payload_words: list of ints, the recovered payload sequence
    """
    pixels_lab = np.asarray(pixels_lab, dtype=np.float64)
    n_pixels = len(pixels_lab)

    # Compute palette positions if not given (uniform spacing fallback)
    if s_palette is None:
        s_palette = np.array([
            w * curve.arc_length / max(n_palette - 1, 1)
            for w in range(n_palette)
        ])
    if constellation_map is None:
        _, radii = compute_tube_radius(curve, frame, s_values=s_palette)
        constellation_map = ConstellationMap(radii)

    # For each pixel: project onto curve, decompose residual, recover (word, position)
    decoded_entries = []  # list of (word_index, seq_position, pixel_index)

    for i, px in enumerate(pixels_lab):
        # Project onto curve
        s_nearest, dist = curve.project(px.reshape(1, 3))
        s_nearest = float(s_nearest[0])
        dist = float(dist[0])

        # Identify nearest palette color (works for both uniform and
        # non-uniform spacing — no linear formula assumption)
        w = int(np.argmin(np.abs(s_palette - s_nearest)))

        # Base point at the snapped palette position
        s_w = s_palette[w]
        base = curve.eval(s_w)
        _, U1, U2 = frame.eval_frame(s_w)

        # Decompose residual into U1, U2 components
        residual = px - base
        alpha1 = float(np.dot(residual, U1))
        alpha2 = float(np.dot(residual, U2))

        # Snap to constellation grid and recover sequence position
        c = constellation_map[w]
        j = int(c.displacement_to_position(alpha1, alpha2))
        decoded_entries.append((w, j, i))

    # Sort by (word_index, sequence_position) and emit words in order
    # Group by word, sort each group by sequence position, then interleave
    # by original pixel order to recover the original sequence.
    #
    # The encoding guarantees: for each word w, the j-th occurrence has
    # constellation position j. So we reconstruct by sorting each word's
    # occurrences by constellation position j, then interleaving all words
    # by their original pixel index i (which preserves the global sequence).
    decoded_entries.sort(key=lambda x: x[2])  # sort by pixel index
    return [entry[0] for entry in decoded_entries]


def verify_roundtrip(payload_words, curve, frame, n_palette,
                     constellation_map=None, s_palette=None, verbose=False):
    """Verify that encode -> decode recovers the original payload.

    Returns:
        True if round-trip succeeds, False otherwise
    """
    pixels_lab, metadata = encode(
        payload_words, curve, frame, n_palette,
        constellation_map=constellation_map, s_palette=s_palette
    )
    recovered = decode(
        pixels_lab, curve, frame, n_palette,
        constellation_map=constellation_map, s_palette=s_palette
    )

    success = list(payload_words) == recovered
    if verbose:
        print(f"Payload:   {list(payload_words)}")
        print(f"Recovered: {recovered}")
        print(f"Match: {success}")
        print(f"Metadata: {metadata}")
        if not success:
            for i, (orig, rec) in enumerate(zip(payload_words, recovered)):
                if orig != rec:
                    print(f"  Mismatch at index {i}: expected {orig}, got {rec}")
    return success


# ---------------------------------------------------------------------------
# Convenience: build everything from a palette YAML
# ---------------------------------------------------------------------------

def build_encoder(yaml_path=None, palette_name='viridis_approx',
                  control_points_lab=None, n_palette=64,
                  n_curve_samples=2000, n_frames=500,
                  spacing='uniform', epsilon=EPSILON):
    """Build all components needed for encoding/decoding.

    Args:
        yaml_path: path to palette.yaml (or None if control_points_lab given)
        palette_name: palette key in YAML
        control_points_lab: direct control points (overrides yaml_path)
        n_palette: number of palette colors
        n_curve_samples: curve sampling density
        n_frames: Bishop frame sampling density
        spacing: 'uniform' (equal arc-length) or 'adaptive' (equal capacity).
                 Adaptive mode integrates tube radius along the curve and
                 places colors where gamut area is largest, raising M_min.
        epsilon: constellation grid spacing in CIELAB units (default: 2.3 JND)

    Returns:
        dict with keys: curve, frame, n_palette, constellation_map,
                        s_palette, tube_radii, metadata
    """
    if control_points_lab is not None:
        curve = PaletteCurve.from_control_points(control_points_lab,
                                                  n_samples=n_curve_samples)
    else:
        if yaml_path is None:
            yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')
        curve = PaletteCurve.from_yaml(yaml_path, palette_name,
                                        n_samples=n_curve_samples)

    frame = BishopFrame(curve, n_frames=n_frames)

    # Place palette colors along the curve
    if spacing == 'adaptive':
        # Integrate tube radius to get cumulative capacity, then
        # divide into N equal-capacity segments
        s_dense, radii_dense, C = compute_capacity_curve(
            curve, frame, n_samples=200)
        s_palette = equal_capacity_positions(s_dense, C, n_palette)
        # Interpolate radii at the placed positions
        radii = np.interp(s_palette, s_dense, radii_dense)
    else:
        # Classic uniform arc-length spacing
        s_palette = np.array([
            w * curve.arc_length / max(n_palette - 1, 1)
            for w in range(n_palette)
        ])
        _, radii = compute_tube_radius(curve, frame, s_values=s_palette)

    constellation_map = ConstellationMap(radii, epsilon=epsilon)

    return {
        'curve': curve,
        'frame': frame,
        'n_palette': n_palette,
        'constellation_map': constellation_map,
        's_palette': s_palette,
        'tube_radii': (s_palette, radii),
        'metadata': {
            'arc_length': curve.arc_length,
            'n_palette': n_palette,
            'epsilon': epsilon,
            'spacing': spacing,
            'M_min': constellation_map.M_min,
            'M_max': constellation_map.M_max,
            'capacity_min': constellation_map.capacity_min,
            'capacity_max': constellation_map.capacity_max,
            'bits_per_pixel': (np.log2(n_palette) +
                               2 * np.log2(max(constellation_map.M_min, 1))),
        }
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    """Command-line interface for parametric encoding."""
    import argparse

    parser = argparse.ArgumentParser(
        description='Parametric curve encoding for visual Glossia')
    parser.add_argument('payload', nargs='?',
                        help='Comma-separated payload word indices')
    parser.add_argument('--palette', default='viridis_approx',
                        help='Palette name from palette.yaml')
    parser.add_argument('-N', '--n-palette', type=int, default=64,
                        help='Number of palette colors')
    parser.add_argument('--info', action='store_true',
                        help='Print encoder info and exit')
    parser.add_argument('--verify', action='store_true',
                        help='Verify round-trip for given payload')
    parser.add_argument('-v', '--verbose', action='store_true')

    args = parser.parse_args()

    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')
    enc = build_encoder(yaml_path, args.palette, n_palette=args.n_palette)

    if args.info:
        print("Parametric Curve Encoder")
        print("========================")
        for k, v in enc['metadata'].items():
            if isinstance(v, float):
                print(f"  {k}: {v:.3f}")
            else:
                print(f"  {k}: {v}")
        return

    if args.payload is None:
        parser.print_help()
        return

    payload = [int(x.strip()) for x in args.payload.split(',')]

    if args.verify:
        ok = verify_roundtrip(
            payload, enc['curve'], enc['frame'],
            enc['n_palette'],
            constellation_map=enc['constellation_map'],
            verbose=True
        )
        import sys
        sys.exit(0 if ok else 1)

    # Encode and print
    cmap = enc['constellation_map']
    pixels_lab, meta = encode(
        payload, enc['curve'], enc['frame'],
        enc['n_palette'],
        constellation_map=cmap
    )
    pixels_srgb = lab_to_srgb(pixels_lab)

    if args.verbose:
        print(f"Palette: {args.palette}, N={args.n_palette}, "
              f"epsilon={EPSILON}")
        print(f"Constellation M range: {cmap.M_min}..{cmap.M_max}")
        print(f"Capacity range: {cmap.capacity_min}..{cmap.capacity_max} positions/word")
        print(f"Bits/pixel: {enc['metadata']['bits_per_pixel']:.1f}")
        print()

    print("CIELAB pixels:")
    for i, (lab, w) in enumerate(zip(pixels_lab, payload)):
        print(f"  [{i}] word={w:3d}  L*={lab[0]:6.2f} a*={lab[1]:6.2f} b*={lab[2]:6.2f}")

    print("\nsRGB pixels:")
    for i, (rgb, w) in enumerate(zip(pixels_srgb, payload)):
        print(f"  [{i}] word={w:3d}  R={rgb[0]:3d} G={rgb[1]:3d} B={rgb[2]:3d}")


if __name__ == '__main__':
    main()
