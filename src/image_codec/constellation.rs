/// M x M grid geometry in the normal plane.
///
/// Maps sequence positions to displacement vectors for encoding
/// repeated palette colors at distinct locations.

/// CIELAB Just-Noticeable Difference — default constellation grid spacing.
pub const EPSILON: f64 = 2.3;

/// M x M grid of sequence positions in the normal plane.
#[derive(Debug, Clone)]
pub struct Constellation {
    pub m: usize,
    pub epsilon: f64,
    pub capacity: usize,
}

impl Constellation {
    pub fn new(m: usize, epsilon: f64) -> Self {
        Constellation { m, epsilon, capacity: m * m }
    }

    /// Create constellation from tube radius and step size.
    pub fn from_radius(radius: f64, epsilon: f64) -> Self {
        let m = ((2.0 * radius / epsilon) as usize + 1).max(1);
        Self::new(m, epsilon)
    }

    /// Map sequence position to grid coordinates (a, b).
    pub fn position_to_grid(&self, j: usize) -> (usize, usize) {
        (j / self.m, j % self.m)
    }

    /// Map grid coordinates to sequence position.
    pub fn grid_to_position(&self, a: usize, b: usize) -> usize {
        a * self.m + b
    }

    /// Map grid coordinates to (alpha1, alpha2) displacements.
    pub fn grid_to_displacement(&self, a: usize, b: usize) -> (f64, f64) {
        let center = (self.m - 1) as f64 / 2.0;
        let alpha1 = (a as f64 - center) * self.epsilon;
        let alpha2 = (b as f64 - center) * self.epsilon;
        (alpha1, alpha2)
    }

    /// Snap continuous displacements to nearest grid coordinates.
    pub fn displacement_to_grid(&self, alpha1: f64, alpha2: f64) -> (usize, usize) {
        let center = (self.m - 1) as f64 / 2.0;
        let a = (alpha1 / self.epsilon + center).round() as i64;
        let b = (alpha2 / self.epsilon + center).round() as i64;
        let a = a.clamp(0, self.m as i64 - 1) as usize;
        let b = b.clamp(0, self.m as i64 - 1) as usize;
        (a, b)
    }

    /// Map sequence position to displacement vector components.
    pub fn position_to_displacement(&self, j: usize) -> (f64, f64) {
        let (a, b) = self.position_to_grid(j);
        self.grid_to_displacement(a, b)
    }

    /// Snap displacements and recover sequence position.
    pub fn displacement_to_position(&self, alpha1: f64, alpha2: f64) -> usize {
        let (a, b) = self.displacement_to_grid(alpha1, alpha2);
        self.grid_to_position(a, b)
    }
}

/// Per-color constellations keyed by palette index.
///
/// Each palette color gets its own Constellation sized to the local
/// tube radius.
#[derive(Debug, Clone)]
pub struct ConstellationMap {
    pub constellations: Vec<Constellation>,
    pub epsilon: f64,
}

impl ConstellationMap {
    /// Build one Constellation per palette color from local tube radii.
    pub fn new(radii: &[f64], epsilon: f64) -> Self {
        let constellations: Vec<Constellation> = radii.iter()
            .map(|&r| Constellation::from_radius(r, epsilon))
            .collect();
        ConstellationMap { constellations, epsilon }
    }

    pub fn get(&self, palette_index: usize) -> &Constellation {
        &self.constellations[palette_index]
    }

    pub fn len(&self) -> usize {
        self.constellations.len()
    }

    pub fn m_min(&self) -> usize {
        self.constellations.iter().map(|c| c.m).min().unwrap_or(0)
    }

    pub fn m_max(&self) -> usize {
        self.constellations.iter().map(|c| c.m).max().unwrap_or(0)
    }

    pub fn capacity_min(&self) -> usize {
        self.constellations.iter().map(|c| c.capacity).min().unwrap_or(0)
    }

    pub fn capacity_max(&self) -> usize {
        self.constellations.iter().map(|c| c.capacity).max().unwrap_or(0)
    }

    pub fn total_capacity(&self) -> usize {
        self.constellations.iter().map(|c| c.capacity).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constellation_roundtrip() {
        let c = Constellation::new(5, EPSILON);
        for j in 0..c.capacity {
            let (alpha1, alpha2) = c.position_to_displacement(j);
            let recovered = c.displacement_to_position(alpha1, alpha2);
            assert_eq!(j, recovered, "Position {} should round-trip", j);
        }
    }

    #[test]
    fn test_constellation_from_radius() {
        let c = Constellation::from_radius(10.0, EPSILON);
        // M = floor(2*10/2.3) + 1 = floor(8.69) + 1 = 8 + 1 = 9
        assert_eq!(c.m, 9, "M should be 9 for radius=10, epsilon=2.3");
        assert_eq!(c.capacity, 81);
    }

    #[test]
    fn test_constellation_center_displacement_is_zero() {
        let c = Constellation::new(5, EPSILON);
        // Center grid position for M=5 is (2,2), which is position 12
        let (alpha1, alpha2) = c.grid_to_displacement(2, 2);
        assert!(alpha1.abs() < 1e-10 && alpha2.abs() < 1e-10,
            "Center should have zero displacement");
    }

    #[test]
    fn test_constellation_map_basics() {
        let radii = vec![10.0, 15.0, 20.0, 12.0];
        let cmap = ConstellationMap::new(&radii, EPSILON);
        assert_eq!(cmap.len(), 4);
        assert!(cmap.m_min() > 0);
        assert!(cmap.m_max() >= cmap.m_min());
    }
}
