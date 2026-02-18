/// Cubic spline curve through CIELAB control points, arc-length reparameterized.
///
/// Replaces `PaletteCurve` from the Python reference.

use super::color::{Vec3, Lab};
use super::spline::CubicSpline;

/// A smooth parametric curve through CIELAB control points.
///
/// The curve is a cubic spline interpolation through K control points,
/// reparameterized by arc length so that |gamma'(s)| ~ 1 everywhere.
#[derive(Debug, Clone)]
pub struct PaletteCurve {
    pub control_points: Vec<Vec3>,
    spline_l: CubicSpline,
    spline_a: CubicSpline,
    spline_b: CubicSpline,
    /// Arc-length values at each sample point (monotonically increasing).
    s_table: Vec<f64>,
    /// Raw parameter u at each sample point.
    u_table: Vec<f64>,
    /// Spline from s -> u for arc-length reparameterization.
    s_to_u: CubicSpline,
    /// Total arc length of the curve.
    pub arc_length: f64,
}

impl PaletteCurve {
    /// Build curve from CIELAB control points.
    pub fn new(points_lab: &[Vec3], n_samples: usize) -> Self {
        let k = points_lab.len();
        assert!(k >= 2, "Need at least 2 control points");

        // Cubic spline through control points (parameter u in [0, 1])
        let u: Vec<f64> = (0..k).map(|i| i as f64 / (k - 1) as f64).collect();
        let l_vals: Vec<f64> = points_lab.iter().map(|p| p[0]).collect();
        let a_vals: Vec<f64> = points_lab.iter().map(|p| p[1]).collect();
        let b_vals: Vec<f64> = points_lab.iter().map(|p| p[2]).collect();

        let spline_l = CubicSpline::new(&u, &l_vals);
        let spline_a = CubicSpline::new(&u, &a_vals);
        let spline_b = CubicSpline::new(&u, &b_vals);

        // Compute arc-length table
        let u_fine: Vec<f64> = (0..n_samples).map(|i| i as f64 / (n_samples - 1) as f64).collect();
        let pts: Vec<Vec3> = u_fine.iter().map(|&ui| {
            Vec3::new(spline_l.eval(ui), spline_a.eval(ui), spline_b.eval(ui))
        }).collect();

        let mut s_table = vec![0.0; n_samples];
        for i in 1..n_samples {
            let diff = pts[i].sub(&pts[i - 1]);
            s_table[i] = s_table[i - 1] + diff.norm();
        }
        let arc_length = s_table[n_samples - 1];

        // Build spline from s -> u
        let s_to_u = CubicSpline::new(&s_table, &u_fine);

        PaletteCurve {
            control_points: points_lab.to_vec(),
            spline_l,
            spline_a,
            spline_b,
            s_table,
            u_table: u_fine,
            s_to_u,
            arc_length,
        }
    }

    /// Convert arc-length parameter s to raw parameter u.
    fn s_to_u_param(&self, s: f64) -> f64 {
        let s = s.clamp(0.0, self.arc_length);
        self.s_to_u.eval(s)
    }

    /// Evaluate gamma(s) at arc-length parameter s.
    pub fn eval(&self, s: f64) -> Vec3 {
        let u = self.s_to_u_param(s);
        Vec3::new(
            self.spline_l.eval(u),
            self.spline_a.eval(u),
            self.spline_b.eval(u),
        )
    }

    /// Evaluate gamma(s) as a Lab color.
    pub fn eval_lab(&self, s: f64) -> Lab {
        let v = self.eval(s);
        Lab::from_vec3(&v)
    }

    /// Evaluate the raw spline derivative at parameter u.
    fn eval_raw_deriv(&self, u: f64) -> Vec3 {
        Vec3::new(
            self.spline_l.eval_deriv(u),
            self.spline_a.eval_deriv(u),
            self.spline_b.eval_deriv(u),
        )
    }

    /// Evaluate the raw spline second derivative at parameter u.
    fn eval_raw_deriv2(&self, u: f64) -> Vec3 {
        Vec3::new(
            self.spline_l.eval_deriv2(u),
            self.spline_a.eval_deriv2(u),
            self.spline_b.eval_deriv2(u),
        )
    }

    /// Unit tangent T(s) = gamma'(s) / |gamma'(s)| at arc-length s.
    pub fn tangent(&self, s: f64) -> Vec3 {
        let u = self.s_to_u_param(s);
        let deriv = self.eval_raw_deriv(u);
        deriv.normalize()
    }

    /// Frenet curvature normal N(s) and curvature kappa(s).
    pub fn curvature_normal(&self, s: f64) -> (Vec3, f64) {
        let u = self.s_to_u_param(s);
        let d1 = self.eval_raw_deriv(u);
        let d2 = self.eval_raw_deriv2(u);
        let speed = d1.norm().max(1e-12);

        let t = d1.scale(1.0 / speed);
        let proj = d2.dot(&t);
        let kappa_vec = d2.sub(&t.scale(proj)).scale(1.0 / (speed * speed));
        let kappa = kappa_vec.norm();

        let n = if kappa > 1e-10 {
            kappa_vec.scale(1.0 / kappa)
        } else {
            Vec3::new(0.0, 0.0, 0.0)
        };

        (n, kappa)
    }

    /// Find the nearest point on the curve for an input point.
    ///
    /// Uses coarse search followed by Brent's method refinement.
    ///
    /// Returns (s_nearest, distance).
    pub fn project(&self, point: &Vec3) -> (f64, f64) {
        let n_search = self.s_table.len().min(2000);
        let s_search: Vec<f64> = (0..n_search)
            .map(|i| i as f64 * self.arc_length / (n_search - 1) as f64)
            .collect();

        // Coarse search
        let mut best_idx = 0;
        let mut best_dist_sq = f64::MAX;
        for (idx, &s) in s_search.iter().enumerate() {
            let p = self.eval(s);
            let d = p.sub(point);
            let dist_sq = d.dot(&d);
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                best_idx = idx;
            }
        }

        // Refine with golden section search
        let s_lo = s_search[best_idx.saturating_sub(2)];
        let s_hi = s_search[(best_idx + 2).min(n_search - 1)];

        let s_best = golden_section_minimize(s_lo, s_hi, |s| {
            let p = self.eval(s);
            let d = p.sub(point);
            d.dot(&d)
        }, 1e-8);

        let p = self.eval(s_best);
        let d = p.sub(point);
        (s_best, d.norm())
    }

    /// Load from palette.yaml control points.
    pub fn from_control_points_lab(points: &[[f64; 3]], n_samples: usize) -> Self {
        let vecs: Vec<Vec3> = points.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect();
        Self::new(&vecs, n_samples)
    }

    /// Get the s_table for arc-length queries.
    pub fn s_table(&self) -> &[f64] {
        &self.s_table
    }
}

/// Golden section search for minimum of f on [a, b].
fn golden_section_minimize<F: Fn(f64) -> f64>(mut a: f64, mut b: f64, f: F, tol: f64) -> f64 {
    let golden_ratio = (5.0_f64.sqrt() - 1.0) / 2.0;
    let mut c = b - golden_ratio * (b - a);
    let mut d = a + golden_ratio * (b - a);
    let mut fc = f(c);
    let mut fd = f(d);

    while (b - a).abs() > tol {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - golden_ratio * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + golden_ratio * (b - a);
            fd = f(d);
        }
    }

    (a + b) / 2.0
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
    fn test_curve_endpoints() {
        let pts = viridis_control_points();
        let curve = PaletteCurve::new(&pts, 2000);

        let start = curve.eval(0.0);
        let end = curve.eval(curve.arc_length);

        // Should be close to first and last control points
        assert!((start[0] - 25.0).abs() < 0.5, "Start L* should be ~25, got {}", start[0]);
        assert!((end[0] - 82.0).abs() < 0.5, "End L* should be ~82, got {}", end[0]);
    }

    #[test]
    fn test_arc_length_positive() {
        let pts = viridis_control_points();
        let curve = PaletteCurve::new(&pts, 2000);
        assert!(curve.arc_length > 0.0, "Arc length should be positive");
    }

    #[test]
    fn test_tangent_is_unit() {
        let pts = viridis_control_points();
        let curve = PaletteCurve::new(&pts, 2000);

        for s in [0.0, curve.arc_length * 0.25, curve.arc_length * 0.5, curve.arc_length * 0.75, curve.arc_length] {
            let t = curve.tangent(s);
            assert!((t.norm() - 1.0).abs() < 0.01,
                "Tangent at s={} should be unit length, got {}", s, t.norm());
        }
    }

    #[test]
    fn test_project_on_curve_point() {
        let pts = viridis_control_points();
        let curve = PaletteCurve::new(&pts, 2000);

        // A point on the curve should project to itself
        let s_test = curve.arc_length * 0.5;
        let point = curve.eval(s_test);
        let (s_proj, dist) = curve.project(&point);

        assert!(dist < 0.1, "Point on curve should have near-zero projection distance, got {}", dist);
        assert!((s_proj - s_test).abs() < 1.0,
            "Projected s should be near original s: got {} expected {}", s_proj, s_test);
    }
}
