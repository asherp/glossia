/// Reed-Solomon error correction layer for parametric curve image encoding.
///
/// Wraps the parametric encoder with RS codes for byte-level encoding.
///
/// Architecture:
///     payload bytes -> RS encode -> big integer -> mixed-radix cells
///     cells -> big integer -> RS decode -> payload bytes
///
/// Each Voronoi cell carries log2(N * M_min²) bits via mixed-radix encoding.
/// N is not restricted to powers of 2 — each cell encodes one of
/// N * M_min² distinguishable states (word × constellation position).
///
/// Ports the Python `rs_encoding.py` to Rust.

use num_bigint::BigUint;
use num_traits::{Zero, ToPrimitive};
use reed_solomon::{Encoder as RsEncoder, Decoder as RsDecoder};

use super::color::Lab;
use super::curve::PaletteCurve;
use super::frame::BishopFrame;
use super::constellation::{Constellation, ConstellationMap};
use super::capacity::{
    compute_capacity_curve, equal_capacity_positions, interp_radii,
};

/// Metadata about an RS encoding operation.
#[derive(Debug, Clone)]
pub struct RsEncodeMeta {
    pub payload_bytes: usize,
    pub rs_parity_bytes: usize,
    pub rs_total_bytes: usize,
    pub total_bits: usize,
    pub n_cells: usize,
    pub states_per_cell: usize,
    pub bits_per_cell: f64,
    pub max_correctable_bytes: usize,
}

/// Metadata about an RS decoding operation.
#[derive(Debug, Clone)]
pub struct RsDecodeMeta {
    pub success: bool,
    pub errors_corrected: usize,
    pub cells_decoded: usize,
    pub error_message: Option<String>,
}

/// Parametric curve encoder with Reed-Solomon error correction.
///
/// The RS layer works over GF(256) at the byte level:
///   1. Take raw payload bytes
///   2. RS encode: adds `nsym` parity bytes
///   3. Convert to big integer
///   4. Mixed-radix decompose: each cell = (word_index, constellation_pos)
///   5. Encode cells into CIELAB colors via parametric curve
///
/// Decoding reverses the process with RS correcting up to nsym/2 byte errors.
#[derive(Debug, Clone)]
pub struct RSEncoder {
    pub curve: PaletteCurve,
    pub frame: BishopFrame,
    pub n_palette: usize,
    pub s_palette: Vec<f64>,
    pub constellation_map: ConstellationMap,
    /// Uniform M_min constellation used for ALL words.
    /// Using a single M_min for all words ensures pos_idx maps to
    /// the centered M_min×M_min subgrid, avoiding edge positions
    /// that may leave the sRGB gamut.
    uniform_constellation: Constellation,
    #[allow(dead_code)]
    m_min: usize,
    /// Total states per cell = N * M_min²
    pub states_per_cell: usize,
    /// log2(states_per_cell)
    pub bits_per_cell: f64,
    nsym_override: Option<usize>,
    ecc_ratio: f64,
    /// Set during encode, needed for decode.
    last_rs_total_bytes: Option<usize>,
    last_nsym: Option<usize>,
}

impl RSEncoder {
    /// Build an RSEncoder from pre-computed encoding components.
    pub fn new(
        curve: PaletteCurve,
        frame: BishopFrame,
        n_palette: usize,
        s_palette: Vec<f64>,
        constellation_map: ConstellationMap,
        nsym: Option<usize>,
        ecc_ratio: f64,
    ) -> Self {
        let m_min = constellation_map.m_min();
        let states_per_cell = n_palette * m_min * m_min;
        let bits_per_cell = (states_per_cell as f64).log2();
        let uniform_constellation = Constellation::new(m_min, constellation_map.epsilon);

        RSEncoder {
            curve,
            frame,
            n_palette,
            s_palette,
            constellation_map,
            uniform_constellation,
            m_min,
            states_per_cell,
            bits_per_cell,
            nsym_override: nsym,
            ecc_ratio,
            last_rs_total_bytes: None,
            last_nsym: None,
        }
    }

    /// Build an RSEncoder from a curve with adaptive spacing.
    pub fn from_curve(
        curve: &PaletteCurve,
        frame: &BishopFrame,
        n_palette: usize,
        epsilon: f64,
        nsym: Option<usize>,
        ecc_ratio: f64,
    ) -> Self {
        let (s_dense, radii_dense, c_curve) = compute_capacity_curve(curve, frame, 200);
        let s_palette = equal_capacity_positions(&s_dense, &c_curve, n_palette);
        let radii = interp_radii(&s_palette, &s_dense, &radii_dense);
        let cmap = ConstellationMap::new(&radii, epsilon);

        Self::new(
            curve.clone(),
            frame.clone(),
            n_palette,
            s_palette,
            cmap,
            nsym,
            ecc_ratio,
        )
    }

    /// Compute the RS codec parameters for a given payload length.
    fn get_rs_params(&self, payload_len: usize) -> usize {
        if let Some(nsym) = self.nsym_override {
            nsym
        } else {
            let nsym = ((payload_len as f64 * self.ecc_ratio).ceil() as usize).max(2);
            // Must be even for symmetric correction
            if nsym % 2 == 1 { nsym + 1 } else { nsym }
        }
    }

    /// Encode raw bytes into CIELAB Voronoi cells with RS protection.
    ///
    /// Returns CIELAB colors for each cell and encoding metadata.
    pub fn encode_bytes(&mut self, payload: &[u8]) -> Result<(Vec<Lab>, RsEncodeMeta), String> {
        let nsym = self.get_rs_params(payload.len());

        // RS encode: payload -> encoded (with parity appended)
        let encoder = RsEncoder::new(nsym);
        let encoded_buf = encoder.encode(payload);
        let encoded_bytes: Vec<u8> = encoded_buf[..].to_vec();
        let rs_total = encoded_bytes.len();

        // Store for decode
        self.last_rs_total_bytes = Some(rs_total);
        self.last_nsym = Some(nsym);

        // Mixed-radix encoding: bytes -> big integer -> cells
        let value_initial = BigUint::from_bytes_be(&encoded_bytes);
        let total_bits = rs_total * 8;

        // Number of cells: ceil(total_bits / log2(states_per_cell))
        let n_cells = (total_bits as f64 / self.bits_per_cell).ceil() as usize;

        // Decompose into mixed-radix digits (least significant first)
        let states = BigUint::from(self.states_per_cell);
        let n_pal = self.n_palette;
        let mut value = value_initial;
        let mut cells: Vec<(usize, usize)> = Vec::with_capacity(n_cells);

        for _ in 0..n_cells {
            let cell_val = (&value % &states).to_usize().unwrap_or(0);
            value /= &states;

            let word_idx = cell_val % n_pal;
            let pos_idx = (cell_val / n_pal).min(self.uniform_constellation.capacity - 1);

            cells.push((word_idx, pos_idx));
        }

        // Encode cells into CIELAB colors using uniform M_min constellation
        let uc = &self.uniform_constellation;
        let mut pixels = Vec::with_capacity(n_cells);

        for &(w, j) in &cells {
            let s_w = self.s_palette[w];
            let base = self.curve.eval(s_w);
            let (_, u1, u2) = self.frame.eval_frame(s_w);
            let (alpha1, alpha2) = uc.position_to_displacement(j);
            let pixel = base.add(&u1.scale(alpha1)).add(&u2.scale(alpha2));
            pixels.push(Lab::from_vec3(&pixel));
        }

        let meta = RsEncodeMeta {
            payload_bytes: payload.len(),
            rs_parity_bytes: nsym,
            rs_total_bytes: rs_total,
            total_bits,
            n_cells,
            states_per_cell: self.states_per_cell,
            bits_per_cell: self.bits_per_cell,
            max_correctable_bytes: nsym / 2,
        };

        Ok((pixels, meta))
    }

    /// Decode CIELAB Voronoi cells back to raw bytes with RS correction.
    ///
    /// Uses joint (word, position) search: for each pixel, tests all palette
    /// words and picks the (word, pos) pair with minimum reconstruction residual.
    pub fn decode_bytes(&self, pixels: &[Lab]) -> Result<(Vec<u8>, RsDecodeMeta), String> {
        self.decode_bytes_with_params(pixels, None, None)
    }

    /// Decode with explicit RS parameters (for when encoder state isn't available).
    pub fn decode_bytes_with_params(
        &self,
        pixels: &[Lab],
        rs_total: Option<usize>,
        nsym: Option<usize>,
    ) -> Result<(Vec<u8>, RsDecodeMeta), String> {
        let rs_total = rs_total.or(self.last_rs_total_bytes)
            .ok_or("Must call encode_bytes() first or provide rs_total")?;
        let nsym = nsym.or(self.last_nsym)
            .ok_or("Must call encode_bytes() first or provide nsym")?;

        let n_cells = pixels.len();
        let uc = &self.uniform_constellation;

        // Joint (word, position) decode for each cell
        let mut cells: Vec<(usize, usize)> = Vec::with_capacity(n_cells);

        for px in pixels {
            let point = px.to_vec3();
            let mut best_w = 0usize;
            let mut best_j = 0usize;
            let mut best_residual = f64::INFINITY;

            for w in 0..self.n_palette {
                let s_w = self.s_palette[w];
                let base = self.curve.eval(s_w);
                let (_, u1, u2) = self.frame.eval_frame(s_w);

                let diff = point.sub(&base);
                let alpha1 = diff.dot(&u1);
                let alpha2 = diff.dot(&u2);

                // Snap to nearest grid point using uniform constellation
                let j = uc.displacement_to_position(alpha1, alpha2);

                // Reconstruct snapped point and measure residual
                let (a1_snap, a2_snap) = uc.position_to_displacement(j);
                let reconstructed = base.add(&u1.scale(a1_snap)).add(&u2.scale(a2_snap));
                let residual = point.sub(&reconstructed).norm();

                if residual < best_residual {
                    best_residual = residual;
                    best_w = w;
                    best_j = j;
                }
            }

            cells.push((best_w, best_j));
        }

        // Mixed-radix decode: cells -> big integer -> bytes
        // Reconstruct big integer from cell values (reverse order since
        // encode decomposes least-significant-first)
        let mut value = BigUint::zero();
        let states = BigUint::from(self.states_per_cell);

        for &(w, j) in cells.iter().rev() {
            let cell_val = j * self.n_palette + w;
            value = value * &states + BigUint::from(cell_val);
        }

        // Convert big integer back to bytes
        let raw_bytes = biguint_to_bytes_be(&value, rs_total);

        // RS decode
        let decoder = RsDecoder::new(nsym);
        match decoder.correct_err_count(&raw_bytes, None) {
            Ok((corrected, error_count)) => {
                let data = corrected.data().to_vec();
                Ok((data, RsDecodeMeta {
                    success: true,
                    errors_corrected: error_count,
                    cells_decoded: n_cells,
                    error_message: None,
                }))
            }
            Err(e) => {
                Ok((Vec::new(), RsDecodeMeta {
                    success: false,
                    errors_corrected: 0,
                    cells_decoded: n_cells,
                    error_message: Some(format!("{:?}", e)),
                }))
            }
        }
    }

    /// Get the last encoding's RS parameters (for use in decode).
    pub fn last_rs_params(&self) -> Option<(usize, usize)> {
        match (self.last_rs_total_bytes, self.last_nsym) {
            (Some(total), Some(nsym)) => Some((total, nsym)),
            _ => None,
        }
    }

    /// Set RS parameters explicitly (e.g. when decoding without prior encode).
    pub fn set_rs_params(&mut self, rs_total: usize, nsym: usize) {
        self.last_rs_total_bytes = Some(rs_total);
        self.last_nsym = Some(nsym);
    }
}

/// Convert a BigUint to a big-endian byte vector of exactly `len` bytes,
/// zero-padding on the left if needed.
fn biguint_to_bytes_be(value: &BigUint, len: usize) -> Vec<u8> {
    let bytes = value.to_bytes_be();
    if bytes.len() >= len {
        bytes[bytes.len() - len..].to_vec()
    } else {
        let mut padded = vec![0u8; len - bytes.len()];
        padded.extend_from_slice(&bytes);
        padded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::color::Vec3;
    use super::super::capacity::build_encoder;

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
    fn test_rs_encode_decode_roundtrip() {
        let (curve, frame) = viridis_curve_and_frame();
        let n_palette = 8;
        let epsilon = 4.0; // Larger epsilon for reliable M_min
        let (s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, epsilon, true);

        let m_min = cmap.m_min();
        if m_min < 2 {
            return; // Skip if constellation too small
        }

        let mut rse = RSEncoder::new(
            curve, frame, n_palette, s_palette, cmap,
            Some(4), // 4 parity bytes
            0.5,
        );

        // Encode a small payload
        let payload = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let (pixels, meta) = rse.encode_bytes(&payload).expect("encode should succeed");

        assert_eq!(meta.payload_bytes, 8);
        assert_eq!(meta.rs_parity_bytes, 4);
        assert!(meta.n_cells > 0);

        // Decode (exact CIELAB, no noise)
        let (recovered, dec_meta) = rse.decode_bytes(&pixels).expect("decode should succeed");

        assert!(dec_meta.success, "RS decode should succeed: {:?}", dec_meta.error_message);
        assert_eq!(recovered, payload, "Round-trip should recover original bytes");
        assert_eq!(dec_meta.errors_corrected, 0, "No errors expected with exact colors");
    }

    #[test]
    fn test_rs_mixed_radix_consistency() {
        // Verify that mixed-radix encode/decode is self-consistent
        let (curve, frame) = viridis_curve_and_frame();
        let n_palette = 8;
        let epsilon = 4.0;
        let (s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, epsilon, true);

        let m_min = cmap.m_min();
        if m_min < 2 {
            return;
        }

        let mut rse = RSEncoder::new(
            curve, frame, n_palette, s_palette, cmap,
            Some(2), // minimal parity
            0.5,
        );

        // Test with various payload sizes
        for size in [1, 4, 8, 16, 32] {
            let payload: Vec<u8> = (0..size).map(|i| i as u8).collect();
            let (pixels, _meta) = rse.encode_bytes(&payload).expect("encode should succeed");
            let (recovered, dec_meta) = rse.decode_bytes(&pixels).expect("decode should succeed");

            assert!(dec_meta.success, "Decode should succeed for {} bytes", size);
            assert_eq!(recovered, payload, "Roundtrip should work for {} bytes", size);
        }
    }

    #[test]
    fn test_rs_states_per_cell() {
        let (curve, frame) = viridis_curve_and_frame();
        let n_palette = 8;
        let epsilon = 4.0;
        let (_s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, epsilon, true);

        let m_min = cmap.m_min();
        let expected_states = n_palette * m_min * m_min;

        let rse = RSEncoder::from_curve(&curve, &frame, n_palette, epsilon, None, 0.5);
        assert_eq!(rse.states_per_cell, expected_states);
        assert!(rse.bits_per_cell > 0.0);
    }

    #[test]
    fn test_biguint_to_bytes_be_padding() {
        // Test zero-padding
        let val = BigUint::from(0x1234u32);
        let bytes = biguint_to_bytes_be(&val, 4);
        assert_eq!(bytes, vec![0x00, 0x00, 0x12, 0x34]);

        // Test exact length
        let bytes = biguint_to_bytes_be(&val, 2);
        assert_eq!(bytes, vec![0x12, 0x34]);

        // Test zero
        let bytes = biguint_to_bytes_be(&BigUint::zero(), 4);
        assert_eq!(bytes, vec![0x00, 0x00, 0x00, 0x00]);
    }
}
