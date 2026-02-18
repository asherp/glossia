/// Natural cubic spline with 1st and 2nd derivative evaluation.
///
/// Replaces `scipy.interpolate.CubicSpline` from the Python reference.
/// Thomas algorithm for tridiagonal solve (~20 lines).

/// Natural cubic spline through (x, y) data points.
///
/// Each interval [x_i, x_{i+1}] uses:
///   S_i(t) = a_i + b_i*h + c_i*h^2 + d_i*h^3
/// where h = x - x_i.
#[derive(Debug, Clone)]
pub struct CubicSpline {
    x: Vec<f64>,
    a: Vec<f64>, // y-values (function values at knots)
    b: Vec<f64>,
    c: Vec<f64>,
    d: Vec<f64>,
}

impl CubicSpline {
    /// Build a natural cubic spline through the given data points.
    ///
    /// Natural boundary conditions: S''(x_0) = S''(x_n) = 0.
    pub fn new(x: &[f64], y: &[f64]) -> Self {
        let n = x.len();
        assert!(n >= 2, "Need at least 2 points for cubic spline");
        assert_eq!(n, y.len(), "x and y must have the same length");

        if n == 2 {
            // Linear interpolation for 2 points
            let h = x[1] - x[0];
            let slope = (y[1] - y[0]) / h;
            return CubicSpline {
                x: x.to_vec(),
                a: y.to_vec(),
                b: vec![slope, slope],
                c: vec![0.0, 0.0],
                d: vec![0.0, 0.0],
            };
        }

        let a = y.to_vec();
        let mut h = vec![0.0; n - 1];
        for i in 0..n - 1 {
            h[i] = x[i + 1] - x[i];
        }

        // Solve for c (second derivatives / 2) via Thomas algorithm
        // Tridiagonal system: natural boundary c[0] = c[n-1] = 0
        let m = n - 2; // interior points
        let mut lower = vec![0.0; m];
        let mut diag = vec![0.0; m];
        let mut upper = vec![0.0; m];
        let mut rhs = vec![0.0; m];

        for i in 0..m {
            let ii = i + 1; // index into original arrays
            if i > 0 {
                lower[i] = h[ii - 1];
            }
            diag[i] = 2.0 * (h[ii - 1] + h[ii]);
            if i < m - 1 {
                upper[i] = h[ii];
            }
            rhs[i] = 3.0 * ((a[ii + 1] - a[ii]) / h[ii] - (a[ii] - a[ii - 1]) / h[ii - 1]);
        }

        // Thomas algorithm (forward elimination + back substitution)
        let c_interior = thomas_solve(&lower, &diag, &upper, &rhs);

        let mut c = vec![0.0; n];
        for i in 0..m {
            c[i + 1] = c_interior[i];
        }

        // Compute b and d from c
        let mut b = vec![0.0; n];
        let mut d = vec![0.0; n];
        for i in 0..n - 1 {
            b[i] = (a[i + 1] - a[i]) / h[i] - h[i] * (2.0 * c[i] + c[i + 1]) / 3.0;
            d[i] = (c[i + 1] - c[i]) / (3.0 * h[i]);
        }
        // Last segment values (for evaluation at the rightmost point)
        b[n - 1] = b[n - 2] + 2.0 * c[n - 2] * h[n - 2] + 3.0 * d[n - 2] * h[n - 2] * h[n - 2];

        CubicSpline {
            x: x.to_vec(),
            a,
            b,
            c,
            d,
        }
    }

    /// Find the interval index for a given x value.
    fn find_interval(&self, x: f64) -> usize {
        let n = self.x.len();
        if x <= self.x[0] {
            return 0;
        }
        if x >= self.x[n - 1] {
            return n - 2;
        }
        // Binary search
        let mut lo = 0;
        let mut hi = n - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.x[mid] > x {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        lo
    }

    /// Evaluate the spline at x.
    pub fn eval(&self, x: f64) -> f64 {
        let i = self.find_interval(x);
        let h = x - self.x[i];
        self.a[i] + self.b[i] * h + self.c[i] * h * h + self.d[i] * h * h * h
    }

    /// Evaluate the first derivative at x.
    pub fn eval_deriv(&self, x: f64) -> f64 {
        let i = self.find_interval(x);
        let h = x - self.x[i];
        self.b[i] + 2.0 * self.c[i] * h + 3.0 * self.d[i] * h * h
    }

    /// Evaluate the second derivative at x.
    pub fn eval_deriv2(&self, x: f64) -> f64 {
        let i = self.find_interval(x);
        let h = x - self.x[i];
        2.0 * self.c[i] + 6.0 * self.d[i] * h
    }
}

/// Thomas algorithm for tridiagonal systems.
///
/// Solves Ax = d where A is tridiagonal with:
///   lower[i] = A[i][i-1], diag[i] = A[i][i], upper[i] = A[i][i+1]
fn thomas_solve(lower: &[f64], diag: &[f64], upper: &[f64], rhs: &[f64]) -> Vec<f64> {
    let n = diag.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![rhs[0] / diag[0]];
    }

    let mut c_prime = vec![0.0; n];
    let mut d_prime = vec![0.0; n];

    // Forward sweep
    c_prime[0] = upper[0] / diag[0];
    d_prime[0] = rhs[0] / diag[0];

    for i in 1..n {
        let denom = diag[i] - lower[i] * c_prime[i - 1];
        c_prime[i] = if i < n - 1 { upper[i] / denom } else { 0.0 };
        d_prime[i] = (rhs[i] - lower[i] * d_prime[i - 1]) / denom;
    }

    // Back substitution
    let mut x = vec![0.0; n];
    x[n - 1] = d_prime[n - 1];
    for i in (0..n - 1).rev() {
        x[i] = d_prime[i] - c_prime[i] * x[i + 1];
    }

    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spline_interpolates_data_points() {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![0.0, 1.0, 0.0, 1.0, 0.0];
        let spline = CubicSpline::new(&x, &y);

        for i in 0..x.len() {
            let val = spline.eval(x[i]);
            assert!((val - y[i]).abs() < 1e-10,
                "Spline should interpolate exactly at knot {}: got {} expected {}",
                i, val, y[i]);
        }
    }

    #[test]
    fn test_spline_linear_data() {
        // For linear data y = 2x + 1, spline should reproduce exactly
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![1.0, 3.0, 5.0, 7.0];
        let spline = CubicSpline::new(&x, &y);

        let test_x = 1.5;
        let expected = 2.0 * test_x + 1.0;
        let val = spline.eval(test_x);
        assert!((val - expected).abs() < 1e-10,
            "Linear data should be exact: got {} expected {}", val, expected);

        // Derivative should be 2
        let deriv = spline.eval_deriv(test_x);
        assert!((deriv - 2.0).abs() < 1e-10,
            "Derivative of linear data should be 2: got {}", deriv);
    }

    #[test]
    fn test_spline_natural_boundary() {
        // Natural spline: second derivative should be 0 at endpoints
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![0.0, 1.0, 0.5, 0.8, 0.2];
        let spline = CubicSpline::new(&x, &y);

        let d2_start = spline.eval_deriv2(x[0]);
        let d2_end = spline.eval_deriv2(x[x.len() - 1]);
        assert!(d2_start.abs() < 1e-10,
            "Second derivative at start should be 0: got {}", d2_start);
        assert!(d2_end.abs() < 1e-10,
            "Second derivative at end should be 0: got {}", d2_end);
    }

    #[test]
    fn test_two_point_spline() {
        let x = vec![0.0, 1.0];
        let y = vec![0.0, 1.0];
        let spline = CubicSpline::new(&x, &y);

        assert!((spline.eval(0.5) - 0.5).abs() < 1e-10);
        assert!((spline.eval_deriv(0.5) - 1.0).abs() < 1e-10);
    }
}
