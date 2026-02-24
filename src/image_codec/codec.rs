/// Encode/decode core: payload word indices <-> CIELAB pixels.
///
/// Also includes self-describing header encoding/decoding.

use std::collections::HashMap;

use super::color::Lab;
use super::curve::PaletteCurve;
use super::frame::BishopFrame;
use super::constellation::{Constellation, ConstellationMap, center_out_order};
use super::capacity::{
    compute_tube_radius, compute_capacity_curve, equal_capacity_positions,
    interp_radii, ConfigEntry, HEADER_S,
};

/// Metadata about an encoding operation.
#[derive(Debug, Clone)]
pub struct EncodeMetadata {
    pub n_palette: usize,
    pub epsilon: f64,
    pub m_min: usize,
    pub m_max: usize,
    pub capacity_min: usize,
    pub capacity_max: usize,
    pub n_payload_words: usize,
    pub max_word_repeats: usize,
}

/// Encode a payload word sequence into CIELAB pixel colors.
pub fn encode(
    payload_words: &[usize],
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_palette: usize,
    constellation_map: &ConstellationMap,
    s_palette: &[f64],
) -> Result<(Vec<Lab>, EncodeMetadata), String> {
    let mut word_counters: HashMap<usize, usize> = HashMap::new();
    let mut pixels = Vec::with_capacity(payload_words.len());

    for (i, &w) in payload_words.iter().enumerate() {
        if w >= n_palette {
            return Err(format!("Payload word {} at index {} exceeds palette size {}", w, i, n_palette));
        }

        let c = constellation_map.get(w);
        let s_w = s_palette[w];
        let base = curve.eval(s_w);
        let (_, u1, u2) = frame.eval_frame(s_w);

        let j = word_counters.entry(w).or_insert(0);
        if *j >= c.capacity {
            return Err(format!(
                "Payload word {} appears more than {} times (constellation capacity exceeded)",
                w, c.capacity
            ));
        }

        let (alpha1, alpha2) = c.position_to_displacement(*j);
        *j += 1;

        let pixel = base.add(&u1.scale(alpha1)).add(&u2.scale(alpha2));
        pixels.push(Lab::from_vec3(&pixel));
    }

    let max_repeats = word_counters.values().cloned().max().unwrap_or(0);

    let metadata = EncodeMetadata {
        n_palette,
        epsilon: constellation_map.epsilon,
        m_min: constellation_map.m_min(),
        m_max: constellation_map.m_max(),
        capacity_min: constellation_map.capacity_min(),
        capacity_max: constellation_map.capacity_max(),
        n_payload_words: payload_words.len(),
        max_word_repeats: max_repeats,
    };

    Ok((pixels, metadata))
}

/// Decode CIELAB pixel colors back to a payload word sequence.
pub fn decode(
    pixels: &[Lab],
    curve: &PaletteCurve,
    _frame: &BishopFrame,
    _n_palette: usize,
    _constellation_map: &ConstellationMap,
    s_palette: &[f64],
) -> Vec<usize> {
    let mut decoded = Vec::with_capacity(pixels.len());

    for px in pixels {
        let point = px.to_vec3();

        // Project onto curve
        let (s_nearest, _dist) = curve.project(&point);

        // Identify nearest palette color (argmin over s_palette)
        let w = s_palette.iter().enumerate()
            .min_by(|(_, a), (_, b)| {
                (s_nearest - **a).abs().partial_cmp(&(s_nearest - **b).abs()).unwrap()
            })
            .map(|(i, _)| i)
            .unwrap_or(0);

        decoded.push(w);
    }

    decoded
}

// ═══════════════════════════════════════════════════════════════════════
// Self-Describing Header
// ═══════════════════════════════════════════════════════════════════════

/// Encode (N, epsilon) into the header color at s=0.
///
/// Uses center-out ordering so config index 0 maps to the grid center
/// (most noise-robust position).
pub fn encode_header(
    n_palette: usize,
    epsilon: f64,
    curve: &PaletteCurve,
    frame: &BishopFrame,
    configs: &[ConfigEntry],
    header_epsilon: f64,
) -> Result<Lab, String> {
    let idx = configs.iter().position(|c| c.n == n_palette && c.epsilon == epsilon)
        .ok_or_else(|| format!("({}, {}) not in config table", n_palette, epsilon))?;

    let base = curve.eval(HEADER_S);
    let (_, u1, u2) = frame.eval_frame(HEADER_S);

    let radii = compute_tube_radius(curve, frame, &[HEADER_S], 16, 60.0, 0.5);
    let c = Constellation::from_radius(radii[0], header_epsilon);

    if idx >= c.capacity {
        return Err(format!(
            "Header constellation too small ({} positions) for config index {}",
            c.capacity, idx
        ));
    }

    // Center-out mapping: config index -> grid position via spiral order
    let order = center_out_order(c.m);
    let grid_pos = if idx < order.len() { order[idx] } else { idx };
    let (alpha1, alpha2) = c.position_to_displacement(grid_pos);
    let pixel = base.add(&u1.scale(alpha1)).add(&u2.scale(alpha2));
    Ok(Lab::from_vec3(&pixel))
}

/// Decode (N, epsilon) from the header color.
///
/// Reverses center-out ordering to recover the config index.
pub fn decode_header(
    pixel: &Lab,
    curve: &PaletteCurve,
    frame: &BishopFrame,
    configs: &[ConfigEntry],
    header_epsilon: f64,
) -> Result<ConfigEntry, String> {
    let base = curve.eval(HEADER_S);
    let (_, u1, u2) = frame.eval_frame(HEADER_S);

    let residual = pixel.to_vec3().sub(&base);
    let alpha1 = residual.dot(&u1);
    let alpha2 = residual.dot(&u2);

    let radii = compute_tube_radius(curve, frame, &[HEADER_S], 16, 60.0, 0.5);
    let c = Constellation::from_radius(radii[0], header_epsilon);
    let grid_pos = c.displacement_to_position(alpha1, alpha2);

    // Reverse center-out mapping: grid position -> config index
    let order = center_out_order(c.m);
    let idx = order.iter().position(|&p| p == grid_pos)
        .unwrap_or(grid_pos);

    if idx >= configs.len() {
        return Err(format!("Config index {} out of range (max {})", idx, configs.len() - 1));
    }

    Ok(configs[idx])
}

/// Encode payload with a self-describing header color.
///
/// The first color declares the radix (N) and grid spacing (epsilon).
pub fn encode_self_describing(
    payload_words: &[usize],
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_palette: usize,
    epsilon: f64,
    configs: &[ConfigEntry],
    header_epsilon: f64,
) -> Result<(Vec<Lab>, EncodeMetadata), String> {
    let header_pixel = encode_header(n_palette, epsilon, curve, frame, configs, header_epsilon)?;

    // Build encoder components for adaptive spacing
    let (s_dense, radii_dense, c_curve) = compute_capacity_curve(curve, frame, 200);
    let s_palette = equal_capacity_positions(&s_dense, &c_curve, n_palette);
    let radii = interp_radii(&s_palette, &s_dense, &radii_dense);
    let cmap = ConstellationMap::new(&radii, epsilon);

    let (mut payload_pixels, metadata) = encode(
        payload_words, curve, frame, n_palette, &cmap, &s_palette)?;

    let mut all_pixels = vec![header_pixel];
    all_pixels.append(&mut payload_pixels);

    Ok((all_pixels, metadata))
}

/// Decode a self-describing encoded pixel sequence.
pub fn decode_self_describing(
    pixels: &[Lab],
    curve: &PaletteCurve,
    frame: &BishopFrame,
    configs: &[ConfigEntry],
    header_epsilon: f64,
) -> Result<(Vec<usize>, usize, f64), String> {
    if pixels.is_empty() {
        return Err("No pixels to decode".to_string());
    }

    let config = decode_header(&pixels[0], curve, frame, configs, header_epsilon)?;

    // Build encoder components with declared parameters
    let (s_dense, radii_dense, c_curve) = compute_capacity_curve(curve, frame, 200);
    let s_palette = equal_capacity_positions(&s_dense, &c_curve, config.n);
    let radii = interp_radii(&s_palette, &s_dense, &radii_dense);
    let cmap = ConstellationMap::new(&radii, config.epsilon);

    let payload = decode(&pixels[1..], curve, frame, config.n, &cmap, &s_palette);

    Ok((payload, config.n, config.epsilon))
}

/// Verify that encode -> decode recovers the original payload.
pub fn verify_roundtrip(
    payload_words: &[usize],
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_palette: usize,
    constellation_map: &ConstellationMap,
    s_palette: &[f64],
) -> bool {
    let (pixels, _) = match encode(payload_words, curve, frame, n_palette, constellation_map, s_palette) {
        Ok(result) => result,
        Err(_) => return false,
    };
    let recovered = decode(&pixels, curve, frame, n_palette, constellation_map, s_palette);
    payload_words == &recovered[..]
}

/// Generate N payload token names for the image codec.
pub fn generate_payload_tokens(n: usize) -> Vec<String> {
    let width = 2.max(format!("{}", n - 1).len());
    (0..n).map(|i| format!("c{:0width$}", i, width = width)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_codec::color::Vec3;
    use crate::image_codec::constellation::EPSILON;
    use crate::image_codec::capacity::{build_encoder, derive_config_table};

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
    fn test_encode_decode_roundtrip_uniform() {
        let (curve, frame) = viridis_curve_and_frame();
        let n_palette = 16;
        let (s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, EPSILON, false);

        // Simple payload: each word appears once
        let payload: Vec<usize> = (0..n_palette).collect();
        assert!(verify_roundtrip(&payload, &curve, &frame, n_palette, &cmap, &s_palette),
            "Uniform spacing roundtrip should succeed");
    }

    #[test]
    fn test_encode_decode_roundtrip_adaptive() {
        let (curve, frame) = viridis_curve_and_frame();
        let n_palette = 16;
        let (s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, EPSILON, true);

        let payload: Vec<usize> = (0..n_palette).collect();
        assert!(verify_roundtrip(&payload, &curve, &frame, n_palette, &cmap, &s_palette),
            "Adaptive spacing roundtrip should succeed");
    }

    #[test]
    fn test_encode_decode_repeated_words() {
        let (curve, frame) = viridis_curve_and_frame();
        let n_palette = 8;
        let (s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, EPSILON, false);

        // Repeat word 0 a few times (within constellation capacity)
        let max_repeats = cmap.get(0).capacity.min(4);
        let payload: Vec<usize> = (0..max_repeats).map(|_| 0).collect();
        assert!(verify_roundtrip(&payload, &curve, &frame, n_palette, &cmap, &s_palette),
            "Repeated word roundtrip should succeed");
    }

    #[test]
    fn test_self_describing_roundtrip() {
        let (curve, frame) = viridis_curve_and_frame();
        let (configs, header_eps) = derive_config_table(&curve, &frame, 50);

        if configs.is_empty() {
            return; // Skip if no valid configs
        }

        let config = &configs[0];
        let payload: Vec<usize> = (0..config.n.min(8)).collect();

        let (pixels, _meta) = encode_self_describing(
            &payload, &curve, &frame, config.n, config.epsilon,
            &configs, header_eps,
        ).expect("Self-describing encode should succeed");

        let (recovered, n_recovered, eps_recovered) = decode_self_describing(
            &pixels, &curve, &frame, &configs, header_eps,
        ).expect("Self-describing decode should succeed");

        assert_eq!(n_recovered, config.n, "Recovered N should match");
        assert_eq!(eps_recovered, config.epsilon, "Recovered epsilon should match");
        assert_eq!(recovered, payload, "Recovered payload should match");
    }

    #[test]
    fn test_generate_payload_tokens() {
        let tokens = generate_payload_tokens(64);
        assert_eq!(tokens.len(), 64);
        assert_eq!(tokens[0], "c00");
        assert_eq!(tokens[63], "c63");

        let tokens16 = generate_payload_tokens(16);
        assert_eq!(tokens16[0], "c00");
        assert_eq!(tokens16[15], "c15");
    }

    #[test]
    fn test_encode_out_of_range_word() {
        let (curve, frame) = viridis_curve_and_frame();
        let n_palette = 8;
        let (s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, EPSILON, false);

        let payload = vec![99]; // Out of range
        assert!(encode(&payload, &curve, &frame, n_palette, &cmap, &s_palette).is_err());
    }
}
