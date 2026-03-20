/// Text notation -> CIELAB pixels -> PNG/SVG output.
///
/// Parses the image grammar's text notation and renders color tokens
/// through the parametric curve pipeline.
///
/// Token formats (in order of preference):
///   - CIELAB: "15.03_40.54_-32.50"  (current — L*_a*_b* coordinates)
///   - Hex:    "#440255"              (CSS hex sRGB)
///   - Index:  "c03"                  (legacy palette index)

#[cfg(feature = "native")]
use image::{RgbImage, Rgb};

use super::color::{Vec3, Lab, Srgb, lab_to_srgb};
use super::curve::PaletteCurve;
use super::frame::BishopFrame;
#[cfg(any(feature = "native", test))]
use super::constellation::EPSILON;
use super::capacity::build_encoder;
use super::codec;

/// Parse color token (e.g., "c03") to palette index (legacy format).
fn parse_color_token(token: &str) -> Option<usize> {
    if token.starts_with('c') && token.len() >= 2 {
        token[1..].parse::<usize>().ok()
    } else {
        None
    }
}

/// Parse a CSS hex color token (e.g., "#440255") to its string value.
fn parse_hex_color_token(token: &str) -> Option<String> {
    let t = token.trim_matches(|c: char| c == '"' || c == '\'');
    if t.starts_with('#') && t.len() == 7 && t[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        Some(t.to_string())
    } else {
        None
    }
}

/// Parse a CIELAB token (e.g., "15.03_40.54_-32.50") to (L*, a*, b*).
///
/// Format: three floats separated by underscores. Recognizable because
/// cover words (cell, bg, |, ||, voronoi:, etc.) never match this pattern.
fn parse_lab_token(token: &str) -> Option<(f64, f64, f64)> {
    let parts: Vec<&str> = token.split('_').collect();
    if parts.len() != 3 {
        return None;
    }
    let l: f64 = parts[0].parse().ok()?;
    let a: f64 = parts[1].parse().ok()?;
    let b: f64 = parts[2].parse().ok()?;
    // Sanity check CIELAB ranges: L in [0, 100], a/b in [-128, 128]
    if l < -1.0 || l > 101.0 || a < -129.0 || a > 129.0 || b < -129.0 || b > 129.0 {
        return None;
    }
    Some((l, a, b))
}

/// Convert a CIELAB token to a CSS hex color string.
///
/// Parses L*_a*_b*, converts to sRGB, returns "#RRGGBB".
fn lab_token_to_hex(token: &str) -> Option<String> {
    let (l, a, b) = parse_lab_token(token)?;
    let lab = Lab { l, a, b };
    let srgb = lab_to_srgb(&lab);
    Some(format!("#{:02x}{:02x}{:02x}", srgb.r, srgb.g, srgb.b))
}

/// Extract CSS hex color strings from text notation.
///
/// Handles both CIELAB tokens ("15.03_40.54_-32.50" -> converted to hex)
/// and direct hex tokens ("#440255" -> passed through).
/// Ignores cover words.
pub fn extract_hex_colors(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|token| {
            // Try CIELAB format first (current standard)
            if let Some(hex) = lab_token_to_hex(token) {
                return Some(hex);
            }
            // Fall back to direct hex format
            parse_hex_color_token(token)
        })
        .collect()
}

/// Extract CIELAB coordinates from text notation.
///
/// Returns (L*, a*, b*) tuples for each CIELAB token found.
pub fn extract_lab_colors(text: &str) -> Vec<(f64, f64, f64)> {
    text.split_whitespace()
        .filter_map(|token| parse_lab_token(token))
        .collect()
}

/// Extract payload color indices from text notation (legacy c## format).
///
/// Filters for tokens matching `c\d+` pattern, ignoring cover words
/// (cell, patch, bg, |, ||, header tokens, etc.).
pub fn extract_payload_indices(text: &str) -> Vec<usize> {
    text.split_whitespace()
        .filter_map(|token| {
            let clean = token.trim_matches(|c: char| !c.is_alphanumeric());
            parse_color_token(clean)
        })
        .collect()
}

/// Load the viridis_approx palette curve from the embedded YAML.
pub fn load_default_curve() -> Result<PaletteCurve, String> {
    let yaml_content = crate::generator::get_embedded_yaml("image/palette.yaml")
        .ok_or_else(|| "palette.yaml not embedded (debug build only embeds English)".to_string())?;

    let data: serde_yaml::Value = serde_yaml::from_str(yaml_content)
        .map_err(|e| format!("Failed to parse palette.yaml: {}", e))?;

    let points = data["palettes"]["viridis_approx"]["control_points_lab"]
        .as_sequence()
        .ok_or_else(|| "Missing control_points_lab in palette.yaml".to_string())?;

    let control_points: Vec<Vec3> = points.iter().map(|p| {
        let arr = p.as_sequence().unwrap();
        Vec3::new(
            arr[0].as_f64().unwrap(),
            arr[1].as_f64().unwrap(),
            arr[2].as_f64().unwrap(),
        )
    }).collect();

    Ok(PaletteCurve::new(&control_points, 2000))
}

/// Hardcoded viridis_approx control points (fallback when YAML not embedded).
pub fn viridis_approx_curve() -> PaletteCurve {
    let pts = vec![
        Vec3::new(25.0,   8.0, -25.0),
        Vec3::new(33.0,  -5.0, -30.0),
        Vec3::new(42.0, -25.0, -15.0),
        Vec3::new(55.0, -35.0,  10.0),
        Vec3::new(68.0, -30.0,  40.0),
        Vec3::new(82.0, -15.0,  60.0),
    ];
    PaletteCurve::new(&pts, 2000)
}

/// Convert text notation to CIELAB pixels.
///
/// Handles three token formats in order of preference:
///   1. CIELAB tokens ("15.03_40.54_-32.50") — used directly, no curve needed
///   2. Hex tokens ("#440255") — parsed to sRGB, converted to Lab
///   3. Legacy index tokens ("c03") — requires curve reconstruction
///
/// CIELAB tokens are the most efficient: the color IS the token name,
/// so no parametric curve rebuild is needed.
pub fn text_to_pixels(
    text: &str,
    n_palette: usize,
    epsilon: f64,
    adaptive: bool,
) -> Result<Vec<(Lab, Srgb)>, String> {
    // Try CIELAB tokens first (current standard — most efficient)
    let lab_colors = extract_lab_colors(text);
    if !lab_colors.is_empty() {
        return Ok(lab_colors.iter().map(|&(l, a, b)| {
            let lab = Lab { l, a, b };
            let srgb = lab_to_srgb(&lab);
            (lab, srgb)
        }).collect());
    }

    // Try hex tokens (direct sRGB, convert to Lab for consistency)
    let hex_colors = extract_hex_colors(text);
    if !hex_colors.is_empty() {
        return Ok(hex_colors.iter().map(|hex| {
            let (r, g, b) = parse_hex_to_rgb(hex);
            let lab = super::color::srgb_to_lab(&Srgb { r, g, b });
            let srgb = Srgb { r, g, b };
            (lab, srgb)
        }).collect());
    }

    // Fall back to legacy c## index tokens (requires full curve reconstruction)
    let indices = extract_payload_indices(text);
    if indices.is_empty() {
        return Err("No color tokens found in text notation".to_string());
    }

    for &idx in &indices {
        if idx >= n_palette {
            return Err(format!("Color token c{:02} exceeds palette size {}", idx, n_palette));
        }
    }

    let curve = load_default_curve().unwrap_or_else(|_| viridis_approx_curve());
    let frame = BishopFrame::new(&curve, 500);
    let (s_palette, _radii, cmap) = build_encoder(&curve, &frame, n_palette, epsilon, adaptive);

    let (pixels_lab, _meta) = codec::encode(&indices, &curve, &frame, n_palette, &cmap, &s_palette)
        .map_err(|e| format!("Encode failed: {}", e))?;

    let result: Vec<(Lab, Srgb)> = pixels_lab.iter().map(|lab| {
        let srgb = lab_to_srgb(lab);
        (*lab, srgb)
    }).collect();

    Ok(result)
}

/// Parse "#RRGGBB" hex string to (r, g, b) bytes.
fn parse_hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    (r, g, b)
}

/// Render color pixels as a PNG image with a simple grid layout.
#[cfg(feature = "native")]
pub fn render_png(
    pixels: &[(Lab, Srgb)],
    output_path: &str,
    cell_size: u32,
    cols: u32,
) -> Result<(), String> {
    let n = pixels.len() as u32;
    let rows = (n + cols - 1) / cols;
    let width = cols * cell_size;
    let height = rows * cell_size;

    let mut img = RgbImage::new(width, height);

    // Fill background with dark gray
    for pixel in img.pixels_mut() {
        *pixel = Rgb([32, 32, 32]);
    }

    // Fill each cell with its color
    for (i, (_lab, srgb)) in pixels.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        let x0 = col * cell_size;
        let y0 = row * cell_size;

        // Fill cell with a 1px border
        for dy in 1..cell_size - 1 {
            for dx in 1..cell_size - 1 {
                img.put_pixel(x0 + dx, y0 + dy, Rgb([srgb.r, srgb.g, srgb.b]));
            }
        }
    }

    img.save(output_path).map_err(|e| format!("Failed to save PNG: {}", e))
}

/// Full pipeline: text notation string -> PNG file.
#[cfg(feature = "native")]
pub fn render_text_to_png(
    text: &str,
    output_path: &str,
    n_palette: usize,
    cell_size: u32,
    cols: u32,
) -> Result<(), String> {
    let pixels = text_to_pixels(text, n_palette, EPSILON, true)?;
    render_png(&pixels, output_path, cell_size, cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_token() {
        assert_eq!(parse_color_token("c00"), Some(0));
        assert_eq!(parse_color_token("c63"), Some(63));
        assert_eq!(parse_color_token("c03"), Some(3));
        assert_eq!(parse_color_token("cell"), None);
        assert_eq!(parse_color_token("bg"), None);
        assert_eq!(parse_color_token("|"), None);
    }

    #[test]
    fn test_parse_hex_color_token() {
        assert_eq!(parse_hex_color_token("#440255"), Some("#440255".to_string()));
        assert_eq!(parse_hex_color_token("#fde725"), Some("#fde725".to_string()));
        assert_eq!(parse_hex_color_token("#FDE725"), Some("#FDE725".to_string()));
        assert_eq!(parse_hex_color_token("cell"), None);
        assert_eq!(parse_hex_color_token("#zz0000"), None);
        assert_eq!(parse_hex_color_token("#fff"), None); // too short
        assert_eq!(parse_hex_color_token("|"), None);
    }

    #[test]
    fn test_parse_lab_token() {
        // Valid CIELAB tokens
        let (l, a, b) = parse_lab_token("15.03_40.54_-32.50").unwrap();
        assert!((l - 15.03).abs() < 1e-6);
        assert!((a - 40.54).abs() < 1e-6);
        assert!((b - (-32.50)).abs() < 1e-6);

        // Negative a and b
        let (l, a, b) = parse_lab_token("50.00_-25.00_-10.00").unwrap();
        assert!((l - 50.0).abs() < 1e-6);
        assert!((a - (-25.0)).abs() < 1e-6);
        assert!((b - (-10.0)).abs() < 1e-6);

        // Zero values
        assert!(parse_lab_token("0.00_0.00_0.00").is_some());

        // Cover words should not match
        assert!(parse_lab_token("cell").is_none());
        assert!(parse_lab_token("bg").is_none());
        assert!(parse_lab_token("|").is_none());
        assert!(parse_lab_token("||").is_none());
        assert!(parse_lab_token("voronoi:").is_none());
        assert!(parse_lab_token("size=400").is_none());
        assert!(parse_lab_token("filled").is_none());

        // Wrong number of components
        assert!(parse_lab_token("15.03_40.54").is_none());
        assert!(parse_lab_token("15.03_40.54_-32.50_1.0").is_none());

        // Out of range
        assert!(parse_lab_token("150.00_0.00_0.00").is_none()); // L > 101
    }

    #[test]
    fn test_lab_token_to_hex() {
        // Pure black in CIELAB should be near #000000
        let hex = lab_token_to_hex("0.00_0.00_0.00").unwrap();
        assert!(hex.starts_with('#'));
        assert_eq!(hex.len(), 7);

        // Should produce valid hex for a viridis-like color
        let hex = lab_token_to_hex("15.03_40.54_-32.50").unwrap();
        assert!(hex.starts_with('#'));
        assert_eq!(hex.len(), 7);
    }

    #[test]
    fn test_extract_hex_colors_from_lab_tokens() {
        // CIELAB tokens should be extracted and converted to hex
        let text = "voronoi: size=400 palette=viridis\n15.03_40.54_-32.50 cell 50.00_-25.00_10.00 cell ||";
        let colors = extract_hex_colors(text);
        assert_eq!(colors.len(), 2);
        assert!(colors[0].starts_with('#'));
        assert!(colors[1].starts_with('#'));
    }

    #[test]
    fn test_extract_hex_colors_direct_hex() {
        // Direct hex tokens should still work
        let text = "voronoi: size=400 palette=viridis\n#440255 cell #2a788e cell #7ad151 cell ||";
        let colors = extract_hex_colors(text);
        assert_eq!(colors, vec!["#440255", "#2a788e", "#7ad151"]);
    }

    #[test]
    fn test_extract_hex_colors_ignores_cover() {
        let text = "filled 15.03_40.54_-32.50 cell bg cell 90.00_-10.00_85.00 patch ||";
        let colors = extract_hex_colors(text);
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn test_extract_lab_colors() {
        let text = "voronoi: size=400\n15.03_40.54_-32.50 cell 50.00_-25.00_10.00 cell ||";
        let labs = extract_lab_colors(text);
        assert_eq!(labs.len(), 2);
        assert!((labs[0].0 - 15.03).abs() < 1e-6);
        assert!((labs[1].0 - 50.00).abs() < 1e-6);
    }

    #[test]
    fn test_extract_payload_indices_legacy() {
        let text = "voronoi: size=400 palette=viridis\nc03 cell c15 cell c42 cell c07 cell ||";
        let indices = extract_payload_indices(text);
        assert_eq!(indices, vec![3, 15, 42, 7]);
    }

    #[test]
    fn test_viridis_approx_curve_loads() {
        let curve = viridis_approx_curve();
        assert!(curve.arc_length > 0.0);
    }

    #[test]
    fn test_text_to_pixels_basic() {
        // Use small palette for faster test
        let text = "c00 cell c01 cell c02 cell c03 cell";
        let result = text_to_pixels(text, 8, EPSILON, false);
        assert!(result.is_ok(), "text_to_pixels should succeed: {:?}", result.err());
        let pixels = result.unwrap();
        assert_eq!(pixels.len(), 4);
    }

    #[test]
    fn test_text_to_pixels_lab_tokens() {
        // CIELAB tokens should work directly — no curve reconstruction needed
        let text = "voronoi: size=400\n15.03_40.54_-32.50 cell 50.00_-25.00_10.00 cell ||";
        let result = text_to_pixels(text, 128, EPSILON, true);
        assert!(result.is_ok(), "CIELAB text_to_pixels should succeed: {:?}", result.err());
        let pixels = result.unwrap();
        assert_eq!(pixels.len(), 2);

        // Verify the CIELAB values came through correctly
        assert!((pixels[0].0.l - 15.03).abs() < 0.01);
        assert!((pixels[0].0.a - 40.54).abs() < 0.01);
        assert!((pixels[0].0.b - (-32.50)).abs() < 0.01);

        // Verify sRGB was computed
        assert!(pixels[0].1.r > 0 || pixels[0].1.g > 0 || pixels[0].1.b > 0);
    }

    #[test]
    fn test_text_to_pixels_hex_tokens() {
        // Hex tokens should also work
        let text = "#440255 cell #2a788e cell";
        let result = text_to_pixels(text, 128, EPSILON, true);
        assert!(result.is_ok(), "Hex text_to_pixels should succeed: {:?}", result.err());
        let pixels = result.unwrap();
        assert_eq!(pixels.len(), 2);
    }
}
