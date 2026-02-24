/// sRGB <-> CIELAB color conversion.
///
/// Pure math, zero crate dependencies. Matches the Python reference
/// implementation in `languages/image/parametric_encoding.py`.

/// 3D vector for color math.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3(pub [f64; 3]);

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3([x, y, z])
    }

    pub fn x(&self) -> f64 { self.0[0] }
    pub fn y(&self) -> f64 { self.0[1] }
    pub fn z(&self) -> f64 { self.0[2] }

    pub fn dot(&self, other: &Vec3) -> f64 {
        self.0[0] * other.0[0] + self.0[1] * other.0[1] + self.0[2] * other.0[2]
    }

    pub fn cross(&self, other: &Vec3) -> Vec3 {
        Vec3([
            self.0[1] * other.0[2] - self.0[2] * other.0[1],
            self.0[2] * other.0[0] - self.0[0] * other.0[2],
            self.0[0] * other.0[1] - self.0[1] * other.0[0],
        ])
    }

    pub fn norm(&self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn normalize(&self) -> Vec3 {
        let n = self.norm().max(1e-12);
        Vec3([self.0[0] / n, self.0[1] / n, self.0[2] / n])
    }

    pub fn scale(&self, s: f64) -> Vec3 {
        Vec3([self.0[0] * s, self.0[1] * s, self.0[2] * s])
    }

    pub fn add(&self, other: &Vec3) -> Vec3 {
        Vec3([
            self.0[0] + other.0[0],
            self.0[1] + other.0[1],
            self.0[2] + other.0[2],
        ])
    }

    pub fn sub(&self, other: &Vec3) -> Vec3 {
        Vec3([
            self.0[0] - other.0[0],
            self.0[1] - other.0[1],
            self.0[2] - other.0[2],
        ])
    }
}

impl std::ops::Index<usize> for Vec3 {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        &self.0[i]
    }
}

/// CIELAB color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

impl Lab {
    pub fn new(l: f64, a: f64, b: f64) -> Self {
        Lab { l, a, b }
    }

    pub fn to_vec3(&self) -> Vec3 {
        Vec3([self.l, self.a, self.b])
    }

    pub fn from_vec3(v: &Vec3) -> Self {
        Lab { l: v.0[0], a: v.0[1], b: v.0[2] }
    }
}

/// sRGB color (0-255).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Srgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Srgb {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Srgb { r, g, b }
    }
}

// D65 illuminant reference white in XYZ
const D65_XN: f64 = 0.95047;
const D65_YN: f64 = 1.00000;
const D65_ZN: f64 = 1.08883;

// sRGB -> XYZ matrix (D65), row-major
const SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.4124564, 0.3575761, 0.1804375],
    [0.2126729, 0.7151522, 0.0721750],
    [0.0193339, 0.1191920, 0.9503041],
];

// XYZ -> sRGB matrix (inverse of above), precomputed
const XYZ_TO_SRGB: [[f64; 3]; 3] = [
    [ 3.2404542, -1.5371385, -0.4985314],
    [-0.9692660,  1.8760108,  0.0415560],
    [ 0.0556434, -0.2040259,  1.0572252],
];

fn mat3_mul(m: &[[f64; 3]; 3], v: &[f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// sRGB gamma expansion: [0,1] nonlinear -> [0,1] linear.
fn gamma_expand(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB gamma compression: [0,1] linear -> [0,1] nonlinear.
fn gamma_compress(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// CIELAB forward nonlinearity.
fn lab_f(t: f64) -> f64 {
    let delta = 6.0 / 29.0;
    if t > delta * delta * delta {
        t.cbrt()
    } else {
        t / (3.0 * delta * delta) + 4.0 / 29.0
    }
}

/// CIELAB inverse nonlinearity.
fn lab_f_inv(t: f64) -> f64 {
    let delta = 6.0 / 29.0;
    if t > delta {
        t * t * t
    } else {
        3.0 * delta * delta * (t - 4.0 / 29.0)
    }
}

/// Convert sRGB [0-255] to CIELAB.
pub fn srgb_to_lab(rgb: &Srgb) -> Lab {
    let r = gamma_expand(rgb.r as f64 / 255.0);
    let g = gamma_expand(rgb.g as f64 / 255.0);
    let b = gamma_expand(rgb.b as f64 / 255.0);

    let xyz = mat3_mul(&SRGB_TO_XYZ, &[r, g, b]);
    let xyz_n = [xyz[0] / D65_XN, xyz[1] / D65_YN, xyz[2] / D65_ZN];
    let f = [lab_f(xyz_n[0]), lab_f(xyz_n[1]), lab_f(xyz_n[2])];

    Lab {
        l: 116.0 * f[1] - 16.0,
        a: 500.0 * (f[0] - f[1]),
        b: 200.0 * (f[1] - f[2]),
    }
}

/// Convert CIELAB to sRGB [0-255] (clamped).
pub fn lab_to_srgb(lab: &Lab) -> Srgb {
    let fy = (lab.l + 16.0) / 116.0;
    let fx = lab.a / 500.0 + fy;
    let fz = fy - lab.b / 200.0;

    let xyz_n = [lab_f_inv(fx), lab_f_inv(fy), lab_f_inv(fz)];
    let xyz = [xyz_n[0] * D65_XN, xyz_n[1] * D65_YN, xyz_n[2] * D65_ZN];
    let linear = mat3_mul(&XYZ_TO_SRGB, &xyz);

    let srgb = [
        gamma_compress(linear[0].max(0.0)),
        gamma_compress(linear[1].max(0.0)),
        gamma_compress(linear[2].max(0.0)),
    ];

    Srgb {
        r: (srgb[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        g: (srgb[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        b: (srgb[2] * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

/// Check if a CIELAB color maps to a valid sRGB color.
///
/// Checks linear sRGB values directly (not gamma-compressed). The old
/// implementation clipped negative linear values via `.max(0.0)` before
/// gamma compression, hiding out-of-gamut colors with clamped channels.
pub fn lab_in_srgb_gamut(lab: &Lab, tolerance: f64) -> bool {
    let fy = (lab.l + 16.0) / 116.0;
    let fx = lab.a / 500.0 + fy;
    let fz = fy - lab.b / 200.0;

    let xyz_n = [lab_f_inv(fx), lab_f_inv(fy), lab_f_inv(fz)];
    let xyz = [xyz_n[0] * D65_XN, xyz_n[1] * D65_YN, xyz_n[2] * D65_ZN];
    let linear = mat3_mul(&XYZ_TO_SRGB, &xyz);

    linear.iter().all(|&v| v >= -tolerance && v <= 1.0 + tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srgb_lab_roundtrip() {
        // Test a set of known colors
        let colors = [
            Srgb::new(0, 0, 0),       // black
            Srgb::new(255, 255, 255),  // white
            Srgb::new(255, 0, 0),      // red
            Srgb::new(0, 255, 0),      // green
            Srgb::new(0, 0, 255),      // blue
            Srgb::new(128, 128, 128),  // gray
        ];

        for c in &colors {
            let lab = srgb_to_lab(c);
            let back = lab_to_srgb(&lab);
            assert_eq!(c.r, back.r, "R mismatch for {:?} -> {:?}", c, lab);
            assert_eq!(c.g, back.g, "G mismatch for {:?} -> {:?}", c, lab);
            assert_eq!(c.b, back.b, "B mismatch for {:?} -> {:?}", c, lab);
        }
    }

    #[test]
    fn test_black_is_l_zero() {
        let lab = srgb_to_lab(&Srgb::new(0, 0, 0));
        assert!(lab.l.abs() < 0.01, "Black should have L*~0, got {}", lab.l);
    }

    #[test]
    fn test_white_is_l_hundred() {
        let lab = srgb_to_lab(&Srgb::new(255, 255, 255));
        assert!((lab.l - 100.0).abs() < 0.01, "White should have L*~100, got {}", lab.l);
    }

    #[test]
    fn test_gamut_check() {
        // Mid-gray should be in gamut (tolerance in linear sRGB [0,1] space)
        assert!(lab_in_srgb_gamut(&Lab::new(50.0, 0.0, 0.0), 0.01));
        // Extreme values should be out of gamut with tight tolerance
        assert!(!lab_in_srgb_gamut(&Lab::new(50.0, 120.0, 120.0), 0.01));
        // Negative linear values should be detected (previously hidden by .max(0.0))
        // Lab(50, -80, 80) has a blue channel that goes deeply negative in linear sRGB
        assert!(!lab_in_srgb_gamut(&Lab::new(50.0, -80.0, 80.0), 0.01));
    }

    #[test]
    fn test_vec3_operations() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);

        assert!((a.dot(&b) - 32.0).abs() < 1e-10);
        assert!((a.norm() - (14.0_f64).sqrt()).abs() < 1e-10);

        let c = a.cross(&b);
        assert!((c.dot(&a)).abs() < 1e-10, "Cross product should be perpendicular to a");
        assert!((c.dot(&b)).abs() < 1e-10, "Cross product should be perpendicular to b");
    }
}
