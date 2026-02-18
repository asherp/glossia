/// Tube radius computation, capacity curve, adaptive spacing, and config table.
///
/// Ports the capacity analysis functions from the Python reference.

use super::color::{Lab, lab_in_srgb_gamut};
use super::curve::PaletteCurve;
use super::frame::BishopFrame;
use super::constellation::ConstellationMap;

/// Fixed arc-length position for the header pixel (curve start).
pub const HEADER_S: f64 = 0.0;

/// Compute the tube radius at each given arc-length position.
///
/// The tube radius r(s) is the maximum displacement along any direction
/// in the normal plane that stays within the sRGB gamut.
pub fn compute_tube_radius(
    curve: &PaletteCurve,
    frame: &BishopFrame,
    s_values: &[f64],
    n_angles: usize,
    max_radius: f64,
    step: f64,
) -> Vec<f64> {
    let angles: Vec<f64> = (0..n_angles)
        .map(|i| i as f64 * 2.0 * std::f64::consts::PI / n_angles as f64)
        .collect();

    s_values.iter().map(|&s| {
        let base = curve.eval(s);
        let (_, u1, u2) = frame.eval_frame(s);
        let mut min_r = max_radius;

        for &theta in &angles {
            let direction = u1.scale(theta.cos()).add(&u2.scale(theta.sin()));
            // Binary search for gamut boundary
            let mut lo = 0.0_f64;
            let mut hi = max_radius;
            while hi - lo > step {
                let mid = (lo + hi) / 2.0;
                let test_pt = base.add(&direction.scale(mid));
                let lab = Lab::from_vec3(&test_pt);
                if lab_in_srgb_gamut(&lab, 0.5) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            min_r = min_r.min(lo);
        }

        min_r
    }).collect()
}

/// Compute the cumulative capacity function C(s) along the palette curve.
///
/// Returns (s_dense, radii, C) where C is monotonically increasing and
/// C[-1] is the total capacity.
pub fn compute_capacity_curve(
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_samples: usize,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let s_dense: Vec<f64> = (0..n_samples)
        .map(|i| i as f64 * curve.arc_length / (n_samples - 1) as f64)
        .collect();

    let radii = compute_tube_radius(curve, frame, &s_dense, 16, 60.0, 0.5);

    // Capacity density = r(s)^2
    let density: Vec<f64> = radii.iter().map(|&r| r * r).collect();

    // Cumulative via trapezoidal integration
    let mut c = vec![0.0; n_samples];
    for i in 1..n_samples {
        let ds = s_dense[i] - s_dense[i - 1];
        let avg = (density[i - 1] + density[i]) / 2.0;
        c[i] = c[i - 1] + avg * ds;
    }

    (s_dense, radii, c)
}

/// Place N palette colors at equal-capacity divisions of the curve.
///
/// Centroid mode (default): color i at (i+0.5)/N through capacity.
pub fn equal_capacity_positions(s_dense: &[f64], c: &[f64], n: usize) -> Vec<f64> {
    let c_total = c[c.len() - 1];

    // Centroid mode: avoid curve endpoints
    let target_c: Vec<f64> = (0..n)
        .map(|i| (i as f64 + 0.5) * c_total / n as f64)
        .collect();

    // Invert C(s) via linear interpolation
    target_c.iter().map(|&tc| {
        interp(tc, c, s_dense)
    }).collect()
}

/// Linear interpolation: given monotonically increasing xs and ys,
/// find y corresponding to a given x.
fn interp(target: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if target <= xs[0] { return ys[0]; }
    if target >= xs[xs.len() - 1] { return ys[ys.len() - 1]; }

    // Binary search
    let mut lo = 0;
    let mut hi = xs.len() - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if xs[mid] > target {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    let frac = (target - xs[lo]) / (xs[hi] - xs[lo]);
    ys[lo] + frac * (ys[hi] - ys[lo])
}

/// Interpolate radii at specific arc-length positions.
pub fn interp_radii(s_positions: &[f64], s_dense: &[f64], radii_dense: &[f64]) -> Vec<f64> {
    s_positions.iter().map(|&s| interp(s, s_dense, radii_dense)).collect()
}

/// A configuration entry: (palette_size, epsilon).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfigEntry {
    pub n: usize,
    pub epsilon: f64,
}

/// Derive valid (N, epsilon) configurations from the curve geometry.
///
/// Returns (configs, header_epsilon).
pub fn derive_config_table(
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_capacity_samples: usize,
) -> (Vec<ConfigEntry>, f64) {
    let (s_dense, radii_dense, c) = compute_capacity_curve(
        curve, frame, n_capacity_samples);

    let m_targets = [2usize, 4, 8, 16, 32];
    let min_epsilon = 2.0;
    let n_max = 128;

    let mut configs = Vec::new();
    let mut n = 2usize;
    while n <= n_max {
        // Only powers of 2
        if n & (n - 1) != 0 {
            n += 1;
            continue;
        }

        let word_bits = (n as f64).log2() as usize;

        // Place N colors at equal-capacity centroids
        let s_pal = equal_capacity_positions(&s_dense, &c, n);
        let r_at_pal = interp_radii(&s_pal, &s_dense, &radii_dense);
        let r_min = r_at_pal.iter().cloned().fold(f64::MAX, f64::min);

        for &m_target in &m_targets {
            // epsilon that gives M_min = M_target at the tightest point
            // Nudge eps down by one ULP to prevent floating-point truncation
            let eps_raw = 2.0 * r_min / (m_target - 1) as f64;
            let eps = f64::from_bits(eps_raw.to_bits().wrapping_sub(1));

            if eps < min_epsilon {
                continue;
            }

            let pos_bits = 2 * (m_target as f64).log2() as usize;
            let bpc = word_bits + pos_bits;
            if bpc < 2 {
                continue;
            }

            configs.push(ConfigEntry { n, epsilon: eps });
        }

        n *= 2;
    }

    // Derive header epsilon from s=0 tube radius and table size
    let r_header = interp(HEADER_S, &s_dense, &radii_dense);
    let table_size = configs.len();
    let m_header_needed = ((table_size as f64).sqrt().ceil() as usize).max(2);
    let header_epsilon = 2.0 * r_header / (m_header_needed - 1) as f64;

    (configs, header_epsilon)
}

/// Result of encoding parameter selection.
#[derive(Debug, Clone)]
pub struct EncodingParams {
    pub n: usize,
    pub epsilon: f64,
    pub s_palette: Vec<f64>,
    pub bits_per_cell: usize,
    pub m_min: usize,
    pub m_max: usize,
    pub word_bits: usize,
    pub pos_bits: usize,
    pub constellation_map: ConstellationMap,
    pub radii_at_palette: Vec<f64>,
    pub configs: Vec<ConfigEntry>,
    pub header_epsilon: f64,
}

/// Select the optimal (N, epsilon) for a palette curve.
pub fn select_encoding_params(
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_capacity_samples: usize,
) -> Option<EncodingParams> {
    let (configs, header_epsilon) = derive_config_table(curve, frame, n_capacity_samples);
    let (s_dense, radii_dense, c) = compute_capacity_curve(curve, frame, n_capacity_samples);

    let mut best: Option<EncodingParams> = None;

    for config in &configs {
        if config.n < 2 {
            continue;
        }

        let s_pal = equal_capacity_positions(&s_dense, &c, config.n);
        let radii_at_pal = interp_radii(&s_pal, &s_dense, &radii_dense);

        let cmap = ConstellationMap::new(&radii_at_pal, config.epsilon);

        if cmap.m_min() < 2 {
            continue;
        }

        let word_bits = (config.n as f64).log2() as usize;
        let pos_bits = 2 * (cmap.m_min() as f64).log2() as usize;
        let bits_per_cell = word_bits + pos_bits;

        if bits_per_cell < 2 {
            continue;
        }

        let is_better = match &best {
            None => true,
            Some(b) => (bits_per_cell, FloatOrd(config.epsilon)) > (b.bits_per_cell, FloatOrd(b.epsilon)),
        };

        if is_better {
            best = Some(EncodingParams {
                n: config.n,
                epsilon: config.epsilon,
                s_palette: s_pal,
                bits_per_cell,
                m_min: cmap.m_min(),
                m_max: cmap.m_max(),
                word_bits,
                pos_bits,
                constellation_map: cmap,
                radii_at_palette: radii_at_pal,
                configs: configs.clone(),
                header_epsilon,
            });
        }
    }

    best
}

/// Wrapper for f64 ordering that supports total ordering.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FloatOrd(f64);

impl PartialOrd for FloatOrd {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Eq for FloatOrd {}

impl Ord for FloatOrd {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Build encoding components for a given (N, epsilon, spacing) configuration.
///
/// Convenience function analogous to Python's `build_encoder()`.
pub fn build_encoder(
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_palette: usize,
    epsilon: f64,
    adaptive: bool,
) -> (Vec<f64>, Vec<f64>, ConstellationMap) {
    let (s_palette, radii) = if adaptive {
        let (s_dense, radii_dense, c) = compute_capacity_curve(curve, frame, 200);
        let s_pal = equal_capacity_positions(&s_dense, &c, n_palette);
        let radii = interp_radii(&s_pal, &s_dense, &radii_dense);
        (s_pal, radii)
    } else {
        let s_pal: Vec<f64> = (0..n_palette)
            .map(|i| i as f64 * curve.arc_length / (n_palette - 1).max(1) as f64)
            .collect();
        let radii = compute_tube_radius(curve, frame, &s_pal, 16, 60.0, 0.5);
        (s_pal, radii)
    };

    let cmap = ConstellationMap::new(&radii, epsilon);
    (s_palette, radii, cmap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_codec::color::Vec3;

    fn viridis_curve_and_frame() -> (PaletteCurve, BishopFrame) {
        let pts = vec![
            Vec3::new(25.0,   8.0, -25.0),
            Vec3::new(33.0,  -5.0, -30.0),
            Vec3::new(42.0, -25.0, -15.0),
            Vec3::new(55.0, -35.0,  10.0),
            Vec3::new(68.0, -30.0,  40.0),
            Vec3::new(82.0, -15.0,  60.0),
        ];
        let curve = PaletteCurve::new(&pts, 2000);
        let frame = BishopFrame::new(&curve, 500);
        (curve, frame)
    }

    #[test]
    fn test_tube_radius_positive() {
        let (curve, frame) = viridis_curve_and_frame();
        let s_vals: Vec<f64> = (0..10)
            .map(|i| i as f64 * curve.arc_length / 9.0)
            .collect();
        let radii = compute_tube_radius(&curve, &frame, &s_vals, 16, 60.0, 0.5);

        for (i, &r) in radii.iter().enumerate() {
            assert!(r > 0.0, "Tube radius at index {} should be positive, got {}", i, r);
        }
    }

    #[test]
    fn test_capacity_curve_monotonic() {
        let (curve, frame) = viridis_curve_and_frame();
        let (_, _, c) = compute_capacity_curve(&curve, &frame, 50);

        for i in 1..c.len() {
            assert!(c[i] >= c[i - 1],
                "Capacity curve should be monotonically increasing at {}", i);
        }
    }

    #[test]
    fn test_equal_capacity_positions_count() {
        let (curve, frame) = viridis_curve_and_frame();
        let (s_dense, _, c) = compute_capacity_curve(&curve, &frame, 50);
        let n = 16;
        let positions = equal_capacity_positions(&s_dense, &c, n);
        assert_eq!(positions.len(), n);

        // All positions should be within curve bounds
        for &s in &positions {
            assert!(s >= 0.0 && s <= curve.arc_length,
                "Position {} out of bounds [0, {}]", s, curve.arc_length);
        }
    }

    #[test]
    fn test_derive_config_table_nonempty() {
        let (curve, frame) = viridis_curve_and_frame();
        let (configs, header_eps) = derive_config_table(&curve, &frame, 50);
        assert!(!configs.is_empty(), "Config table should not be empty");
        assert!(header_eps > 0.0, "Header epsilon should be positive");
    }
}
