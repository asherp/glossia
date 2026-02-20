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

/// Generate N seed points inside a circular boundary.
///
/// Uses sqrt(r) polar sampling for uniform area density within the disk,
/// then adds angular jitter for variety. The circle is centered in the
/// canvas with radius = min(width, height)/2 - 2.
pub fn generate_circular_seeds(n: usize, width: f64, height: f64, seed: u64) -> Vec<Point> {
    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = width.min(height) / 2.0 - 2.0;

    // Simple LCG for reproducibility
    let mut rng_state = seed.wrapping_add(1);
    let mut next_f64 = || -> f64 {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng_state >> 33) as f64 / (1u64 << 31) as f64
    };

    let mut points = Vec::with_capacity(n);
    for _ in 0..n {
        // Uniform disk sampling: r = R * sqrt(U), theta = 2*pi*V
        let r = radius * next_f64().sqrt();
        let theta = 2.0 * std::f64::consts::PI * next_f64();
        let x = cx + r * theta.cos();
        let y = cy + r * theta.sin();
        points.push(Point::new(x, y));
    }

    points
}

/// Lloyd relaxation constrained to a circular boundary.
///
/// Like `lloyd_relax`, but after computing Voronoi centroids, projects
/// any points that drift outside the disk back onto its boundary.
pub fn lloyd_relax_circular(
    seeds: &mut [Point],
    width: f64,
    height: f64,
    iterations: usize,
) {
    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = width.min(height) / 2.0 - 2.0;

    for _ in 0..iterations {
        let cells = voronoi_cells(seeds, width, height);
        for (i, cell) in cells.iter().enumerate() {
            if cell.vertices.len() >= 3 {
                let c = cell.centroid();
                let dx = c.x - cx;
                let dy = c.y - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > radius {
                    // Project back onto disk boundary
                    let scale = radius / dist;
                    seeds[i] = Point::new(cx + dx * scale, cy + dy * scale);
                } else {
                    seeds[i] = c;
                }
            }
        }
    }
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

/// Parse a CSS hex color string to (r, g, b) in [0, 255].
///
/// Accepts "#RRGGBB" or "RRGGBB" (case-insensitive).
fn parse_hex(hex: &str) -> (f64, f64, f64) {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0) as f64;
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0) as f64;
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0) as f64;
    (r, g, b)
}

/// Pre-compute a flattened N×N pairwise sRGB Euclidean distance matrix,
/// normalized to [0, 1] (0 = identical, 1 = max distance √(255²×3) ≈ 441.67).
pub fn color_distance_matrix(hex_colors: &[&str]) -> Vec<f64> {
    let max_dist = (255.0_f64 * 255.0 * 3.0).sqrt(); // ≈ 441.67
    let n = hex_colors.len();
    let rgb: Vec<(f64, f64, f64)> = hex_colors.iter().map(|c| parse_hex(c)).collect();
    let mut dists = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in (i + 1)..n {
            let dr = rgb[i].0 - rgb[j].0;
            let dg = rgb[i].1 - rgb[j].1;
            let db = rgb[i].2 - rgb[j].2;
            let d = (dr * dr + dg * dg + db * db).sqrt() / max_dist;
            dists[i * n + j] = d;
            dists[j * n + i] = d;
        }
    }
    dists
}

/// Lloyd relaxation with color-aware repulsive forcing.
///
/// Each iteration:
/// 1. Compute Voronoi centroids (standard Lloyd step).
/// 2. For each seed pair (i, j), add a repulsive displacement when their
///    assigned colors are similar (nearby in sRGB).
/// 3. Update position = centroid + strength × Σ(repulsions), clamped to canvas.
///
/// `color_dists` is the flattened N×N matrix from `color_distance_matrix`.
/// `strength` controls repulsion magnitude (0.3 = ~30% of characteristic spacing).
pub fn lloyd_relax_color_aware(
    seeds: &mut [Point],
    width: f64,
    height: f64,
    iterations: usize,
    color_dists: &[f64],
    strength: f64,
) {
    let n = seeds.len();
    if n == 0 || strength == 0.0 {
        lloyd_relax(seeds, width, height, iterations);
        return;
    }
    let char_spacing = (width * height / n as f64).sqrt();

    for _ in 0..iterations {
        let cells = voronoi_cells(seeds, width, height);
        let mut new_positions: Vec<Point> = Vec::with_capacity(n);

        for (i, cell) in cells.iter().enumerate() {
            let centroid = if cell.vertices.len() >= 3 {
                let c = cell.centroid();
                Point::new(c.x.clamp(1.0, width - 1.0), c.y.clamp(1.0, height - 1.0))
            } else {
                seeds[i]
            };

            // Accumulate repulsive displacement from similar-colored neighbors
            let mut rx = 0.0;
            let mut ry = 0.0;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let similarity = 1.0 - color_dists[i * n + j];
                if similarity < 0.01 {
                    continue;
                }
                let dx = seeds[i].x - seeds[j].x;
                let dy = seeds[i].y - seeds[j].y;
                let spatial_dist = (dx * dx + dy * dy).sqrt();
                if spatial_dist < 1e-6 {
                    continue;
                }
                // 1/r falloff, normalized by characteristic spacing
                let force_mag = similarity * char_spacing / spatial_dist;
                rx += force_mag * dx / spatial_dist;
                ry += force_mag * dy / spatial_dist;
            }

            new_positions.push(Point::new(
                (centroid.x + strength * rx).clamp(1.0, width - 1.0),
                (centroid.y + strength * ry).clamp(1.0, height - 1.0),
            ));
        }

        for (i, p) in new_positions.into_iter().enumerate() {
            seeds[i] = p;
        }
    }
}

/// Lloyd relaxation with color-aware repulsion, constrained to a circular boundary.
///
/// Same logic as `lloyd_relax_color_aware`, but projects seeds that drift
/// outside the disk back onto its boundary.
pub fn lloyd_relax_circular_color_aware(
    seeds: &mut [Point],
    width: f64,
    height: f64,
    iterations: usize,
    color_dists: &[f64],
    strength: f64,
) {
    let n = seeds.len();
    if n == 0 || strength == 0.0 {
        lloyd_relax_circular(seeds, width, height, iterations);
        return;
    }
    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = width.min(height) / 2.0 - 2.0;
    let char_spacing = (width * height / n as f64).sqrt();

    for _ in 0..iterations {
        let cells = voronoi_cells(seeds, width, height);
        let mut new_positions: Vec<Point> = Vec::with_capacity(n);

        for (i, cell) in cells.iter().enumerate() {
            let centroid = if cell.vertices.len() >= 3 {
                cell.centroid()
            } else {
                seeds[i]
            };

            // Accumulate repulsive displacement
            let mut rx = 0.0;
            let mut ry = 0.0;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let similarity = 1.0 - color_dists[i * n + j];
                if similarity < 0.01 {
                    continue;
                }
                let dx = seeds[i].x - seeds[j].x;
                let dy = seeds[i].y - seeds[j].y;
                let spatial_dist = (dx * dx + dy * dy).sqrt();
                if spatial_dist < 1e-6 {
                    continue;
                }
                let force_mag = similarity * char_spacing / spatial_dist;
                rx += force_mag * dx / spatial_dist;
                ry += force_mag * dy / spatial_dist;
            }

            let px = centroid.x + strength * rx;
            let py = centroid.y + strength * ry;

            // Project to disk boundary if outside
            let ddx = px - cx;
            let ddy = py - cy;
            let dist = (ddx * ddx + ddy * ddy).sqrt();
            if dist > radius {
                let scale = radius / dist;
                new_positions.push(Point::new(cx + ddx * scale, cy + ddy * scale));
            } else {
                new_positions.push(Point::new(px, py));
            }
        }

        for (i, p) in new_positions.into_iter().enumerate() {
            seeds[i] = p;
        }
    }
}

/// Permute color-to-seed assignment to minimize same-color adjacency.
///
/// After Lloyd relaxation produces well-spaced seeds, this function uses
/// simulated annealing to find a permutation of color assignments that
/// minimizes the number of same-color neighboring cells.
///
/// Returns a permutation vector `perm` where `perm[i]` is the index into
/// the original `colors` array that should be used for seed `i`.
pub fn scatter_colors(
    seeds: &[Point],
    colors: &[&str],
    seed: u64,
    iterations: usize,
) -> Vec<usize> {
    let n = seeds.len();
    if n <= 1 {
        return (0..n).collect();
    }

    // k-nearest neighbors (k=6, brute force -- n is small)
    let k = 6.min(n - 1);
    let mut neighbors: Vec<Vec<usize>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut dists: Vec<(usize, f64)> = (0..n)
            .filter(|&j| j != i)
            .map(|j| (j, seeds[i].dist_sq(&seeds[j])))
            .collect();
        dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        neighbors.push(dists.iter().take(k).map(|&(j, _)| j).collect());
    }

    // Cost: count directed same-color neighbor pairs
    let cost = |perm: &[usize]| -> usize {
        let mut c = 0;
        for i in 0..n {
            for &j in &neighbors[i] {
                if colors[perm[i]] == colors[perm[j]] {
                    c += 1;
                }
            }
        }
        c
    };

    // Simple xorshift64 RNG
    let mut rng_state = seed.wrapping_add(0xdeadbeef);
    if rng_state == 0 {
        rng_state = 1;
    }
    let mut next_u64 = || -> u64 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state
    };

    // Initialize identity permutation
    let mut perm: Vec<usize> = (0..n).collect();
    let mut current_cost = cost(&perm);
    let mut best_perm = perm.clone();
    let mut best_cost = current_cost;

    if current_cost == 0 {
        return best_perm;
    }

    for iter in 0..iterations {
        // Pick two random indices
        let a = (next_u64() % n as u64) as usize;
        let b = (next_u64() % n as u64) as usize;
        if a == b || colors[perm[a]] == colors[perm[b]] {
            continue; // same index or same color -- swap is no-op
        }

        // Swap and recompute cost
        perm.swap(a, b);
        let new_cost = cost(&perm);

        // Simulated annealing acceptance
        let temp = 1.0 - (iter as f64 / iterations as f64); // linear cooling
        if new_cost <= current_cost {
            current_cost = new_cost;
        } else {
            let delta = (new_cost - current_cost) as f64;
            let accept_prob = (-delta / (temp + 1e-10)).exp();
            let r = (next_u64() >> 33) as f64 / (1u64 << 31) as f64;
            if r < accept_prob {
                current_cost = new_cost;
            } else {
                perm.swap(a, b); // revert
            }
        }

        if current_cost < best_cost {
            best_cost = current_cost;
            best_perm = perm.clone();
            if best_cost == 0 {
                break;
            }
        }
    }

    best_perm
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

    // ── Color-aware relaxation tests ──

    #[test]
    fn test_color_distance_matrix_basic() {
        let colors = vec!["#000000", "#ffffff", "#000000"];
        let dists = color_distance_matrix(&colors);
        let n = 3;
        // Diagonal = 0
        for i in 0..n {
            assert_eq!(dists[i * n + i], 0.0, "Diagonal should be 0");
        }
        // Symmetry
        for i in 0..n {
            for j in 0..n {
                assert_eq!(dists[i * n + j], dists[j * n + i], "Matrix should be symmetric");
            }
        }
        // Black-white = 1.0 (max distance)
        assert!((dists[0 * n + 1] - 1.0).abs() < 1e-10,
            "Black-white distance should be 1.0, got {}", dists[0 * n + 1]);
        // Identical colors = 0.0
        assert_eq!(dists[0 * n + 2], 0.0, "Identical colors should have distance 0");
    }

    #[test]
    fn test_lloyd_color_aware_zero_strength() {
        // strength=0 should produce identical results to plain lloyd_relax
        let mut seeds_a = generate_seeds(9, 100.0, 100.0, 42);
        let mut seeds_b = seeds_a.clone();
        let colors = vec!["#ff0000", "#00ff00", "#0000ff",
                          "#ff0000", "#00ff00", "#0000ff",
                          "#ff0000", "#00ff00", "#0000ff"];
        let dists = color_distance_matrix(&colors);

        lloyd_relax(&mut seeds_a, 100.0, 100.0, 5);
        lloyd_relax_color_aware(&mut seeds_b, 100.0, 100.0, 5, &dists, 0.0);

        for (a, b) in seeds_a.iter().zip(seeds_b.iter()) {
            assert!((a.x - b.x).abs() < 1e-10, "x mismatch: {} vs {}", a.x, b.x);
            assert!((a.y - b.y).abs() < 1e-10, "y mismatch: {} vs {}", a.y, b.y);
        }
    }

    #[test]
    fn test_color_aware_separates_similar() {
        // Give some seeds identical colors — after color-aware relaxation,
        // the identical-color seeds should be farther apart on average
        // than under plain Lloyd.
        let n = 8;
        // Colors: 4 red, 4 blue — identical within groups
        let colors = vec!["#ff0000", "#ff0000", "#ff0000", "#ff0000",
                          "#0000ff", "#0000ff", "#0000ff", "#0000ff"];
        let color_refs: Vec<&str> = colors.iter().map(|s| *s).collect();
        let dists = color_distance_matrix(&color_refs);

        let initial = generate_seeds(n, 200.0, 200.0, 123);

        // Plain Lloyd
        let mut seeds_plain = initial.clone();
        lloyd_relax(&mut seeds_plain, 200.0, 200.0, 10);

        // Color-aware Lloyd
        let mut seeds_color = initial.clone();
        lloyd_relax_color_aware(&mut seeds_color, 200.0, 200.0, 10, &dists, 0.5);

        // Compute mean pairwise distance among same-color seeds (indices 0-3)
        let same_color_dist = |seeds: &[Point]| -> f64 {
            let mut total = 0.0;
            let mut count = 0;
            for i in 0..4 {
                for j in (i + 1)..4 {
                    total += seeds[i].dist_sq(&seeds[j]).sqrt();
                    count += 1;
                }
            }
            total / count as f64
        };

        let plain_sep = same_color_dist(&seeds_plain);
        let color_sep = same_color_dist(&seeds_color);
        assert!(color_sep > plain_sep,
            "Color-aware relaxation should push similar colors apart: \
             color-aware={:.2}, plain={:.2}", color_sep, plain_sep);
    }

    #[test]
    fn test_lloyd_color_aware_circular_bounds() {
        let n = 8;
        let w = 200.0;
        let h = 200.0;
        let colors = vec!["#ff0000", "#ff0000", "#00ff00", "#00ff00",
                          "#0000ff", "#0000ff", "#ffff00", "#ffff00"];
        let color_refs: Vec<&str> = colors.iter().map(|s| *s).collect();
        let dists = color_distance_matrix(&color_refs);

        let mut seeds = generate_circular_seeds(n, w, h, 42);
        lloyd_relax_circular_color_aware(&mut seeds, w, h, 10, &dists, 0.3);

        let cx = w / 2.0;
        let cy = h / 2.0;
        let radius = w.min(h) / 2.0 - 2.0;

        for (i, p) in seeds.iter().enumerate() {
            let dx = p.x - cx;
            let dy = p.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(dist <= radius + 0.1,
                "Seed {} at ({:.1},{:.1}) is outside disk (dist={:.1}, radius={:.1})",
                i, p.x, p.y, dist, radius);
        }
    }

    // ── Color scatter tests ──

    #[test]
    fn test_scatter_colors_eliminates_adjacency() {
        // 16 seeds with 8 color pairs (2 each). With k=6 neighbors out of 15,
        // same-color peers are sparse enough for SA to reach 0.
        let n = 16;
        let colors = vec![
            "#ff0000", "#ff0000",
            "#00ff00", "#00ff00",
            "#0000ff", "#0000ff",
            "#ffff00", "#ffff00",
            "#ff00ff", "#ff00ff",
            "#00ffff", "#00ffff",
            "#ff8800", "#ff8800",
            "#8800ff", "#8800ff",
        ];
        let color_refs: Vec<&str> = colors.iter().map(|s| *s).collect();

        let mut seeds = generate_seeds(n, 400.0, 400.0, 123);
        lloyd_relax(&mut seeds, 400.0, 400.0, 10);

        let perm = scatter_colors(&seeds, &color_refs, 42, 8000);

        // Build k=6 neighbor lists and count same-color pairs
        let k = 6.min(n - 1);
        let mut same_color_pairs = 0;
        for i in 0..n {
            let mut dists: Vec<(usize, f64)> = (0..n)
                .filter(|&j| j != i)
                .map(|j| (j, seeds[i].dist_sq(&seeds[j])))
                .collect();
            dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            for &(j, _) in dists.iter().take(k) {
                if color_refs[perm[i]] == color_refs[perm[j]] {
                    same_color_pairs += 1;
                }
            }
        }

        assert_eq!(same_color_pairs, 0,
            "Scatter should eliminate same-color neighbor pairs, got {}", same_color_pairs);
    }

    #[test]
    fn test_scatter_colors_is_permutation() {
        let n = 8;
        let colors = vec!["#ff0000", "#ff0000", "#00ff00", "#00ff00",
                          "#0000ff", "#0000ff", "#ffff00", "#ffff00"];
        let color_refs: Vec<&str> = colors.iter().map(|s| *s).collect();

        let mut seeds = generate_seeds(n, 200.0, 200.0, 42);
        lloyd_relax(&mut seeds, 200.0, 200.0, 10);

        let perm = scatter_colors(&seeds, &color_refs, 42, 4000);

        // Verify it's a valid permutation
        assert_eq!(perm.len(), n);
        let mut sorted = perm.clone();
        sorted.sort();
        assert_eq!(sorted, (0..n).collect::<Vec<_>>(),
            "scatter_colors must return a valid permutation");
    }
}
