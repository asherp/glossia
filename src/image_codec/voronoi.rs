/// Voronoi diagram computation via half-plane intersection.
///
/// For N seed points, computes N convex polygon cells by clipping a bounding
/// rectangle against perpendicular bisectors. O(N²) — fast enough for the
/// ≤128 cells used in palette encoding.
///
/// Includes Lloyd relaxation: iteratively move seeds to cell centroids for
/// visually pleasing, roughly equal-area cells.

/// A 2D point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    fn midpoint(&self, other: &Point) -> Point {
        Point::new((self.x + other.x) / 2.0, (self.y + other.y) / 2.0)
    }

    /// Squared distance to another point.
    #[allow(dead_code)]
    fn dist_sq(&self, other: &Point) -> f64 {
        (self.x - other.x).powi(2) + (self.y - other.y).powi(2)
    }
}

/// A convex polygon represented as an ordered list of vertices.
#[derive(Debug, Clone)]
pub struct Polygon {
    pub vertices: Vec<Point>,
}

impl Polygon {
    /// Create a rectangular polygon.
    pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Polygon {
            vertices: vec![
                Point::new(x0, y0),
                Point::new(x1, y0),
                Point::new(x1, y1),
                Point::new(x0, y1),
            ],
        }
    }

    /// Clip this polygon to the half-plane: all points closer to `keep` than to `clip`.
    ///
    /// The half-plane boundary is the perpendicular bisector of the segment
    /// from `keep` to `clip`. We keep the side containing `keep`.
    pub fn clip_to_half_plane(&self, keep: &Point, clip: &Point) -> Polygon {
        if self.vertices.is_empty() {
            return Polygon { vertices: vec![] };
        }

        // Perpendicular bisector passes through midpoint, normal = clip - keep.
        let mid = keep.midpoint(clip);
        // Normal pointing toward `keep`: (keep - clip)
        let nx = keep.x - clip.x;
        let ny = keep.y - clip.y;

        // A point p is on the `keep` side iff dot(p - mid, normal) >= 0
        let side = |p: &Point| -> f64 {
            (p.x - mid.x) * nx + (p.y - mid.y) * ny
        };

        let n = self.vertices.len();
        let mut out = Vec::with_capacity(n + 2);

        for i in 0..n {
            let a = &self.vertices[i];
            let b = &self.vertices[(i + 1) % n];
            let sa = side(a);
            let sb = side(b);

            if sa >= 0.0 {
                // a is inside
                out.push(*a);
                if sb < 0.0 {
                    // edge a→b exits — add intersection
                    out.push(intersect(a, b, sa, sb));
                }
            } else if sb >= 0.0 {
                // a is outside, b is inside — edge enters — add intersection
                out.push(intersect(a, b, sa, sb));
            }
            // Both outside: skip
        }

        Polygon { vertices: out }
    }

    /// Signed area of the polygon (positive if CCW).
    pub fn area(&self) -> f64 {
        let n = self.vertices.len();
        if n < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for i in 0..n {
            let a = &self.vertices[i];
            let b = &self.vertices[(i + 1) % n];
            sum += a.x * b.y - b.x * a.y;
        }
        sum / 2.0
    }

    /// Centroid of the polygon.
    pub fn centroid(&self) -> Point {
        let n = self.vertices.len();
        if n == 0 {
            return Point::new(0.0, 0.0);
        }
        if n == 1 {
            return self.vertices[0];
        }
        if n == 2 {
            return self.vertices[0].midpoint(&self.vertices[1]);
        }

        let a = self.area();
        if a.abs() < 1e-12 {
            // Degenerate — return average
            let sx: f64 = self.vertices.iter().map(|p| p.x).sum();
            let sy: f64 = self.vertices.iter().map(|p| p.y).sum();
            return Point::new(sx / n as f64, sy / n as f64);
        }

        let mut cx = 0.0;
        let mut cy = 0.0;
        for i in 0..n {
            let pi = &self.vertices[i];
            let pj = &self.vertices[(i + 1) % n];
            let cross = pi.x * pj.y - pj.x * pi.y;
            cx += (pi.x + pj.x) * cross;
            cy += (pi.y + pj.y) * cross;
        }
        let factor = 1.0 / (6.0 * a);
        Point::new(cx * factor, cy * factor)
    }

    /// Format vertices as SVG points string: "x1,y1 x2,y2 ..."
    pub fn svg_points(&self) -> String {
        self.vertices
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Linear interpolation to find the intersection of edge a→b with a half-plane.
fn intersect(a: &Point, b: &Point, sa: f64, sb: f64) -> Point {
    let t = sa / (sa - sb);
    Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
}

/// Compute Voronoi cells for the given seed points within a bounding rectangle.
///
/// Returns one `Polygon` per seed, in the same order.
pub fn voronoi_cells(seeds: &[Point], width: f64, height: f64) -> Vec<Polygon> {
    let n = seeds.len();
    let mut cells = Vec::with_capacity(n);

    for i in 0..n {
        let mut cell = Polygon::rect(0.0, 0.0, width, height);
        for j in 0..n {
            if i == j {
                continue;
            }
            cell = cell.clip_to_half_plane(&seeds[i], &seeds[j]);
            if cell.vertices.is_empty() {
                break;
            }
        }
        cells.push(cell);
    }

    cells
}

/// Generate N seed points using a simple LCG-based PRNG.
///
/// Distributes points with jittered grid initialization for reasonable
/// starting positions before Lloyd relaxation.
pub fn generate_seeds(n: usize, width: f64, height: f64, seed: u64) -> Vec<Point> {
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = (n + cols - 1) / cols;
    let cell_w = width / cols as f64;
    let cell_h = height / rows as f64;

    // Simple LCG for reproducibility (no external deps)
    let mut rng_state = seed.wrapping_add(1);
    let mut next_f64 = || -> f64 {
        // LCG parameters from Numerical Recipes
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng_state >> 33) as f64 / (1u64 << 31) as f64
    };

    let mut points = Vec::with_capacity(n);
    for idx in 0..n {
        let col = idx % cols;
        let row = idx / cols;
        // Jittered grid: center of cell + random offset
        let jx = (next_f64() - 0.5) * cell_w * 0.6;
        let jy = (next_f64() - 0.5) * cell_h * 0.6;
        let x = (col as f64 + 0.5) * cell_w + jx;
        let y = (row as f64 + 0.5) * cell_h + jy;
        points.push(Point::new(
            x.clamp(1.0, width - 1.0),
            y.clamp(1.0, height - 1.0),
        ));
    }

    points
}

/// Lloyd relaxation: iteratively move seeds to cell centroids.
///
/// Produces roughly equal-area, nicely spaced cells.
pub fn lloyd_relax(
    seeds: &mut [Point],
    width: f64,
    height: f64,
    iterations: usize,
) {
    for _ in 0..iterations {
        let cells = voronoi_cells(seeds, width, height);
        for (i, cell) in cells.iter().enumerate() {
            if cell.vertices.len() >= 3 {
                let c = cell.centroid();
                seeds[i] = Point::new(
                    c.x.clamp(1.0, width - 1.0),
                    c.y.clamp(1.0, height - 1.0),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polygon_area_unit_square() {
        let p = Polygon::rect(0.0, 0.0, 1.0, 1.0);
        let a = p.area().abs();
        assert!((a - 1.0).abs() < 1e-10, "Unit square area should be 1.0, got {}", a);
    }

    #[test]
    fn test_polygon_centroid_unit_square() {
        let p = Polygon::rect(0.0, 0.0, 1.0, 1.0);
        let c = p.centroid();
        assert!((c.x - 0.5).abs() < 1e-10, "Centroid x should be 0.5, got {}", c.x);
        assert!((c.y - 0.5).abs() < 1e-10, "Centroid y should be 0.5, got {}", c.y);
    }

    #[test]
    fn test_clip_half_plane_basic() {
        let p = Polygon::rect(0.0, 0.0, 10.0, 10.0);
        // Keep the side near (2, 5), clip toward (8, 5)
        // Bisector is x = 5
        let clipped = p.clip_to_half_plane(
            &Point::new(2.0, 5.0),
            &Point::new(8.0, 5.0),
        );
        // Result should be roughly [0,0] [5,0] [5,10] [0,10]
        assert!(!clipped.vertices.is_empty());
        let a = clipped.area().abs();
        assert!((a - 50.0).abs() < 1e-6, "Clipped area should be 50, got {}", a);
    }

    #[test]
    fn test_voronoi_two_cells_split_canvas() {
        let seeds = vec![
            Point::new(2.0, 5.0),
            Point::new(8.0, 5.0),
        ];
        let cells = voronoi_cells(&seeds, 10.0, 10.0);
        assert_eq!(cells.len(), 2);

        let a0 = cells[0].area().abs();
        let a1 = cells[1].area().abs();
        let total = a0 + a1;
        assert!((total - 100.0).abs() < 1e-6,
            "Total area should be 100, got {} ({} + {})", total, a0, a1);
        assert!((a0 - 50.0).abs() < 1e-6, "Each cell should be ~50, got {}", a0);
    }

    #[test]
    fn test_voronoi_cells_cover_canvas() {
        let seeds = generate_seeds(16, 100.0, 100.0, 42);
        let cells = voronoi_cells(&seeds, 100.0, 100.0);
        assert_eq!(cells.len(), 16);

        let total: f64 = cells.iter().map(|c| c.area().abs()).sum();
        assert!((total - 10000.0).abs() < 1.0,
            "Total cell area should be ~10000, got {}", total);
    }

    #[test]
    fn test_lloyd_relaxation_reduces_variance() {
        let mut seeds = generate_seeds(9, 100.0, 100.0, 42);

        let cells_before = voronoi_cells(&seeds, 100.0, 100.0);
        let areas_before: Vec<f64> = cells_before.iter().map(|c| c.area().abs()).collect();
        let mean_before = areas_before.iter().sum::<f64>() / areas_before.len() as f64;
        let var_before: f64 = areas_before.iter().map(|a| (a - mean_before).powi(2)).sum::<f64>()
            / areas_before.len() as f64;

        lloyd_relax(&mut seeds, 100.0, 100.0, 10);

        let cells_after = voronoi_cells(&seeds, 100.0, 100.0);
        let areas_after: Vec<f64> = cells_after.iter().map(|c| c.area().abs()).collect();
        let mean_after = areas_after.iter().sum::<f64>() / areas_after.len() as f64;
        let var_after: f64 = areas_after.iter().map(|a| (a - mean_after).powi(2)).sum::<f64>()
            / areas_after.len() as f64;

        assert!(var_after <= var_before,
            "Lloyd relaxation should reduce area variance: {} -> {}", var_before, var_after);
    }

    #[test]
    fn test_generate_seeds_deterministic() {
        let a = generate_seeds(16, 100.0, 100.0, 42);
        let b = generate_seeds(16, 100.0, 100.0, 42);
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.x, pb.x);
            assert_eq!(pa.y, pb.y);
        }
    }

    #[test]
    fn test_generate_seeds_in_bounds() {
        let seeds = generate_seeds(64, 400.0, 400.0, 0);
        for (i, p) in seeds.iter().enumerate() {
            assert!(p.x >= 0.0 && p.x <= 400.0, "Seed {} x={} out of bounds", i, p.x);
            assert!(p.y >= 0.0 && p.y <= 400.0, "Seed {} y={} out of bounds", i, p.y);
        }
    }
}
