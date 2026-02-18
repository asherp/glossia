/// Rotation-minimizing frame (Bishop frame) via double-reflection method.
///
/// Wang et al. 2008: "Computation of Rotation Minimizing Frames".

use super::color::Vec3;
use super::curve::PaletteCurve;
use super::spline::CubicSpline;

/// Rotation-minimizing frame along a PaletteCurve.
///
/// At each sampled arc-length s, provides orthonormal {T(s), U1(s), U2(s)}:
///   - T: unit tangent
///   - U1, U2: smoothly varying normal-plane basis vectors
#[derive(Debug, Clone)]
pub struct BishopFrame {
    s_samples: Vec<f64>,
    t_splines: [CubicSpline; 3],
    u1_splines: [CubicSpline; 3],
    u2_splines: [CubicSpline; 3],
}

impl BishopFrame {
    /// Compute the Bishop frame along the curve.
    pub fn new(curve: &PaletteCurve, n_frames: usize) -> Self {
        let s_samples: Vec<f64> = (0..n_frames)
            .map(|i| i as f64 * curve.arc_length / (n_frames - 1) as f64)
            .collect();

        // Compute tangent at all sample points
        let t_all: Vec<Vec3> = s_samples.iter().map(|&s| curve.tangent(s)).collect();

        // Initialize U1(0)
        let (n0, kappa0) = curve.curvature_normal(s_samples[0]);
        let u1_0 = if kappa0 > 1e-6 {
            n0.normalize()
        } else {
            arbitrary_perp(&t_all[0]).normalize()
        };
        let u2_0 = t_all[0].cross(&u1_0).normalize();

        // Propagate via double-reflection (Wang et al. 2008)
        let mut u1 = vec![Vec3::new(0.0, 0.0, 0.0); n_frames];
        let mut u2 = vec![Vec3::new(0.0, 0.0, 0.0); n_frames];
        u1[0] = u1_0;
        u2[0] = u2_0;

        for i in 0..n_frames - 1 {
            let v1 = curve.eval(s_samples[i + 1]).sub(&curve.eval(s_samples[i]));
            let c1 = v1.dot(&v1);

            if c1 < 1e-20 {
                u1[i + 1] = u1[i];
                u2[i + 1] = u2[i];
                continue;
            }

            // First reflection
            let r_l = u1[i].sub(&v1.scale(2.0 / c1 * v1.dot(&u1[i])));
            let t_l = t_all[i].sub(&v1.scale(2.0 / c1 * v1.dot(&t_all[i])));

            // Second reflection
            let v2 = t_all[i + 1].sub(&t_l);
            let c2 = v2.dot(&v2);

            u1[i + 1] = if c2 < 1e-20 {
                r_l
            } else {
                r_l.sub(&v2.scale(2.0 / c2 * v2.dot(&r_l)))
            };

            // Normalize and compute U2
            u1[i + 1] = u1[i + 1].normalize();
            u2[i + 1] = t_all[i + 1].cross(&u1[i + 1]).normalize();
        }

        // Build interpolation splines
        let t_splines = build_component_splines(&s_samples, &t_all);
        let u1_splines = build_component_splines(&s_samples, &u1);
        let u2_splines = build_component_splines(&s_samples, &u2);

        BishopFrame {
            s_samples,
            t_splines,
            u1_splines,
            u2_splines,
        }
    }

    /// Evaluate the Bishop frame at arc-length s.
    ///
    /// Returns (T, U1, U2) — all re-orthonormalized after spline interpolation.
    pub fn eval_frame(&self, s: f64) -> (Vec3, Vec3, Vec3) {
        let t = eval_spline_vec3(&self.t_splines, s);
        let u1_raw = eval_spline_vec3(&self.u1_splines, s);
        let u2_raw = eval_spline_vec3(&self.u2_splines, s);

        // Re-orthonormalize (spline interpolation can drift)
        let t = t.normalize();
        let u1 = u1_raw.sub(&t.scale(u1_raw.dot(&t))).normalize();
        let u2 = t.cross(&u1).normalize();

        // Check that u2 is roughly aligned with the raw interpolated u2
        // (if not, flip to maintain consistent handedness)
        let u2 = if u2.dot(&u2_raw) < 0.0 { u2.scale(-1.0) } else { u2 };

        (t, u1, u2)
    }
}

/// Find an arbitrary unit vector perpendicular to v.
fn arbitrary_perp(v: &Vec3) -> Vec3 {
    let abs_v = [v[0].abs(), v[1].abs(), v[2].abs()];
    let candidate = if abs_v[0] <= abs_v[1] && abs_v[0] <= abs_v[2] {
        Vec3::new(1.0, 0.0, 0.0)
    } else if abs_v[1] <= abs_v[2] {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let d = candidate.dot(v) / v.dot(v);
    candidate.sub(&v.scale(d)).normalize()
}

/// Build 3 cubic splines for the x, y, z components of a Vec3 sequence.
fn build_component_splines(s: &[f64], vecs: &[Vec3]) -> [CubicSpline; 3] {
    let x: Vec<f64> = vecs.iter().map(|v| v[0]).collect();
    let y: Vec<f64> = vecs.iter().map(|v| v[1]).collect();
    let z: Vec<f64> = vecs.iter().map(|v| v[2]).collect();
    [
        CubicSpline::new(s, &x),
        CubicSpline::new(s, &y),
        CubicSpline::new(s, &z),
    ]
}

/// Evaluate 3 component splines to produce a Vec3.
fn eval_spline_vec3(splines: &[CubicSpline; 3], s: f64) -> Vec3 {
    Vec3::new(
        splines[0].eval(s),
        splines[1].eval(s),
        splines[2].eval(s),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viridis_control_points() -> Vec<Vec3> {
        vec![
            Vec3::new(25.0,   8.0, -25.0),
            Vec3::new(33.0,  -5.0, -30.0),
            Vec3::new(42.0, -25.0, -15.0),
            Vec3::new(55.0, -35.0,  10.0),
            Vec3::new(68.0, -30.0,  40.0),
            Vec3::new(82.0, -15.0,  60.0),
        ]
    }

    #[test]
    fn test_frame_orthonormal() {
        let pts = viridis_control_points();
        let curve = PaletteCurve::new(&pts, 2000);
        let frame = BishopFrame::new(&curve, 500);

        // Check orthonormality at several points
        for frac in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let s = frac * curve.arc_length;
            let (t, u1, u2) = frame.eval_frame(s);

            assert!((t.norm() - 1.0).abs() < 0.01,
                "T should be unit at s={}: norm={}", s, t.norm());
            assert!((u1.norm() - 1.0).abs() < 0.01,
                "U1 should be unit at s={}: norm={}", s, u1.norm());
            assert!((u2.norm() - 1.0).abs() < 0.01,
                "U2 should be unit at s={}: norm={}", s, u2.norm());

            assert!(t.dot(&u1).abs() < 0.01,
                "T.U1 should be 0 at s={}: got {}", s, t.dot(&u1));
            assert!(t.dot(&u2).abs() < 0.01,
                "T.U2 should be 0 at s={}: got {}", s, t.dot(&u2));
            assert!(u1.dot(&u2).abs() < 0.01,
                "U1.U2 should be 0 at s={}: got {}", s, u1.dot(&u2));
        }
    }

    #[test]
    fn test_frame_tangent_matches_curve() {
        let pts = viridis_control_points();
        let curve = PaletteCurve::new(&pts, 2000);
        let frame = BishopFrame::new(&curve, 500);

        let s = curve.arc_length * 0.5;
        let (t_frame, _, _) = frame.eval_frame(s);
        let t_curve = curve.tangent(s);

        let dot = t_frame.dot(&t_curve);
        assert!(dot > 0.99,
            "Frame tangent should match curve tangent: dot={}", dot);
    }
}
