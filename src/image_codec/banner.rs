/// Banner encode/decode pipeline: bytes <-> PNG via RSEncoder + Voronoi.
///
/// Ports the Python `generate_banner.py` and `decode_image.py` to Rust.
///
/// Encoding pipeline:
///   1. Select encoding params (N, epsilon) from curve geometry
///   2. RSEncoder: bytes -> RS encode -> mixed-radix -> CIELAB cells
///   3. Encode header cell (self-describing: declares N, epsilon)
///   4. Generate Voronoi seeds + Lloyd relaxation
///   5. Assign cells to seeds in scan order (decodable layout)
///   6. Render as PNG
///
/// Decoding pipeline:
///   1. Load PNG -> sRGB pixels
///   2. Regenerate Voronoi seeds (deterministic from dimensions + seed)
///   3. Segment cells by color quantization + connected components
///   4. Match cells to seeds via nearest neighbor
///   5. Decode header -> (N, epsilon)
///   6. RSEncoder: joint (word, pos) decode -> RS correct -> bytes

use super::color::{Lab, Srgb, lab_to_srgb, srgb_to_lab};
use super::curve::PaletteCurve;
use super::frame::BishopFrame;
use super::capacity::derive_config_table;
use super::codec::{encode_header, decode_header};
use super::rs_encoding::RSEncoder;
use super::voronoi::{Point, generate_seeds, lloyd_relax};

/// Metadata about a banner encoding operation.
#[derive(Debug, Clone)]
pub struct BannerEncodeMeta {
    pub n_palette: usize,
    pub epsilon: f64,
    pub n_payload_cells: usize,
    pub n_total_cells: usize,
    pub bits_per_cell: f64,
    pub rs_parity_bytes: usize,
    pub rs_total_bytes: usize,
    pub max_correctable_bytes: usize,
}

/// Metadata about a banner decoding operation.
#[derive(Debug, Clone)]
pub struct BannerDecodeMeta {
    pub success: bool,
    pub n_palette: usize,
    pub epsilon: f64,
    pub n_cells: usize,
    pub errors_corrected: usize,
    pub error_message: Option<String>,
}

/// Result of encoding a banner (before rendering to image).
#[derive(Debug)]
pub struct BannerEncoded {
    /// All cell colors (header + payload) in sRGB, scan-order.
    pub cells_srgb: Vec<Srgb>,
    /// All cell colors in CIELAB (header + payload).
    pub cells_lab: Vec<Lab>,
    /// Voronoi seed positions after Lloyd relaxation.
    pub seeds: Vec<Point>,
    /// Permutation mapping cell index -> seed index (scan-order assignment).
    pub cell_to_seed: Vec<usize>,
    /// Encoding metadata.
    pub meta: BannerEncodeMeta,
    /// Width of the banner.
    pub width: usize,
    /// Height of the banner.
    pub height: usize,
}

/// Encode payload bytes into a banner-ready Voronoi layout.
///
/// Returns `BannerEncoded` with cell colors, seed positions, and ordering.
/// The caller can render this to PNG, SVG, or other formats.
pub fn encode_banner(
    payload: &[u8],
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_palette: usize,
    epsilon: f64,
    nsym: usize,
    width: usize,
    height: usize,
    voronoi_seed: u64,
    relax_iters: usize,
) -> Result<BannerEncoded, String> {
    // 1. Build RSEncoder with adaptive spacing
    let mut rse = RSEncoder::from_curve(curve, frame, n_palette, epsilon, Some(nsym), 0.5);

    // 2. Encode payload -> CIELAB cells
    let (payload_pixels, rs_meta) = rse.encode_bytes(payload)?;

    // 3. Build header cell
    let (configs, header_eps) = derive_config_table(curve, frame, 50);
    let header_lab = encode_header(n_palette, epsilon, curve, frame, &configs, header_eps)?;

    // 4. Combine header + payload cells
    let mut all_lab = vec![header_lab];
    all_lab.extend_from_slice(&payload_pixels);
    let all_srgb: Vec<Srgb> = all_lab.iter().map(|lab| lab_to_srgb(lab)).collect();

    let n_total = all_lab.len();

    // 5. Generate Voronoi seeds and relax
    let mut seeds = generate_seeds(n_total, width as f64, height as f64, voronoi_seed);
    lloyd_relax(&mut seeds, width as f64, height as f64, relax_iters);

    // 6. Scan-order assignment: sort seeds by (y, x), cell i -> i-th seed
    let mut scan_idx: Vec<usize> = (0..n_total).collect();
    scan_idx.sort_by(|&a, &b| {
        seeds[a].y.partial_cmp(&seeds[b].y).unwrap()
            .then(seeds[a].x.partial_cmp(&seeds[b].x).unwrap())
    });

    // Build inverse: seed j gets cell inverse_scan[j]
    let mut cell_to_seed = vec![0usize; n_total];
    for (rank, &seed_idx) in scan_idx.iter().enumerate() {
        cell_to_seed[rank] = seed_idx;
    }

    let meta = BannerEncodeMeta {
        n_palette,
        epsilon,
        n_payload_cells: rs_meta.n_cells,
        n_total_cells: n_total,
        bits_per_cell: rs_meta.bits_per_cell,
        rs_parity_bytes: rs_meta.rs_parity_bytes,
        rs_total_bytes: rs_meta.rs_total_bytes,
        max_correctable_bytes: rs_meta.max_correctable_bytes,
    };

    Ok(BannerEncoded {
        cells_srgb: all_srgb,
        cells_lab: all_lab,
        seeds,
        cell_to_seed,
        meta,
        width,
        height,
    })
}

/// Render a `BannerEncoded` to an sRGB pixel buffer (height × width × 3).
///
/// Rasterizes Voronoi cells with optional border. Returns flat RGB buffer.
pub fn render_banner_pixels(
    encoded: &BannerEncoded,
    border_width: f64,
    border_color: Srgb,
) -> Vec<u8> {
    let w = encoded.width;
    let h = encoded.height;
    let n = encoded.seeds.len();

    // Build sRGB color per seed (apply cell_to_seed mapping)
    let mut seed_colors = vec![border_color; n];
    for (cell_idx, &seed_idx) in encoded.cell_to_seed.iter().enumerate() {
        if cell_idx < encoded.cells_srgb.len() && seed_idx < n {
            seed_colors[seed_idx] = encoded.cells_srgb[cell_idx];
        }
    }

    // Rasterize: for each pixel, find nearest seed
    let mut pixels = vec![0u8; w * h * 3];

    for py in 0..h {
        for px in 0..w {
            let p = Point::new(px as f64 + 0.5, py as f64 + 0.5);

            // Find nearest and second nearest seed
            let mut best_idx = 0;
            let mut best_dist = f64::INFINITY;
            let mut second_dist = f64::INFINITY;

            for (i, seed) in encoded.seeds.iter().enumerate() {
                let dx = p.x - seed.x;
                let dy = p.y - seed.y;
                let d = dx * dx + dy * dy;
                if d < best_dist {
                    second_dist = best_dist;
                    best_dist = d;
                    best_idx = i;
                } else if d < second_dist {
                    second_dist = d;
                }
            }

            let offset = (py * w + px) * 3;

            // Check if pixel is on border
            if border_width > 0.0 {
                let d1 = best_dist.sqrt();
                let d2 = second_dist.sqrt();
                if d2 - d1 < border_width {
                    pixels[offset] = border_color.r;
                    pixels[offset + 1] = border_color.g;
                    pixels[offset + 2] = border_color.b;
                    continue;
                }
            }

            let c = &seed_colors[best_idx];
            pixels[offset] = c.r;
            pixels[offset + 1] = c.g;
            pixels[offset + 2] = c.b;
        }
    }

    pixels
}

/// Render a `BannerEncoded` to a PNG file.
#[cfg(feature = "native")]
pub fn render_banner_png(
    encoded: &BannerEncoded,
    output_path: &str,
    border_width: f64,
    border_color: Srgb,
) -> Result<(), String> {
    use image::RgbImage;

    let pixels = render_banner_pixels(encoded, border_width, border_color);
    let w = encoded.width as u32;
    let h = encoded.height as u32;

    let img = RgbImage::from_raw(w, h, pixels)
        .ok_or_else(|| "Failed to create image from pixel buffer".to_string())?;

    img.save(output_path).map_err(|e| format!("Failed to save PNG: {}", e))
}

/// Decode a banner from an sRGB pixel buffer back to payload bytes.
///
/// Approach: count distinct cell colors (excluding border), regenerate
/// the same deterministic Voronoi seeds, and sample the pixel color at
/// each seed position. This avoids centroid-matching errors.
pub fn decode_banner_from_pixels(
    pixels: &[u8],
    width: usize,
    height: usize,
    curve: &PaletteCurve,
    frame: &BishopFrame,
    nsym: usize,
    voronoi_seed: u64,
    relax_iters: usize,
) -> Result<(Vec<u8>, BannerDecodeMeta), String> {
    if pixels.len() != width * height * 3 {
        return Err(format!(
            "Pixel buffer size mismatch: expected {}, got {}",
            width * height * 3, pixels.len()
        ));
    }

    // 1. Count distinct cell regions to determine n_total.
    //    Exclude border pixels (small connected components or known border color).
    let cell_colors = extract_cell_colors_simple(pixels, width, height, 10.0);

    // Filter out very dark cells (likely border mesh, L < 10)
    let cell_colors: Vec<_> = cell_colors.into_iter()
        .filter(|(_, _, lab)| lab.l > 10.0)
        .collect();
    let n_total = cell_colors.len();

    if n_total < 2 {
        return Err(format!("Too few cells found: {}", n_total));
    }

    // 2. Regenerate same Voronoi seeds (deterministic from n_total + seed)
    let mut seeds = generate_seeds(n_total, width as f64, height as f64, voronoi_seed);
    lloyd_relax(&mut seeds, width as f64, height as f64, relax_iters);

    // 3. Sort seeds by scan order (y, x) — same ordering as encoder
    let mut scan_idx: Vec<usize> = (0..seeds.len()).collect();
    scan_idx.sort_by(|&a, &b| {
        seeds[a].y.partial_cmp(&seeds[b].y).unwrap()
            .then(seeds[a].x.partial_cmp(&seeds[b].x).unwrap())
    });

    // 4. Sample pixel color at each seed position (seed is inside its cell)
    let mut ordered_lab: Vec<Lab> = Vec::with_capacity(n_total);
    for &si in &scan_idx {
        let px = (seeds[si].x.round() as usize).min(width - 1);
        let py = (seeds[si].y.round() as usize).min(height - 1);
        let off = (py * width + px) * 3;
        let srgb = Srgb::new(pixels[off], pixels[off + 1], pixels[off + 2]);
        ordered_lab.push(srgb_to_lab(&srgb));
    }

    // 5. Decode header from first cell -> (N, epsilon)
    let header_lab = &ordered_lab[0];
    let (configs, header_eps) = derive_config_table(curve, frame, 50);
    let config = decode_header(header_lab, curve, frame, &configs, header_eps)?;
    let n_palette = config.n;
    let epsilon = config.epsilon;

    // 6. Payload is the remaining cells
    let payload_lab = &ordered_lab[1..];
    let n_payload_cells = payload_lab.len();

    // 7. Build RSEncoder and decode bytes
    let rse = RSEncoder::from_curve(curve, frame, n_palette, epsilon, Some(nsym), 0.5);

    // The max rs_total from cells may be 1 byte more than actual due to ceiling
    // rounding in the encoder. Try rs_total_max first, then rs_total_max - 1.
    let rs_total_max = (n_payload_cells as f64 * rse.bits_per_cell / 8.0).floor() as usize;
    if rs_total_max <= nsym {
        return Err(format!("Too few cells ({}) for RS decode", n_payload_cells));
    }

    let mut best_recovered = Vec::new();
    let mut best_meta = None;

    for &rs_try in &[rs_total_max, rs_total_max.saturating_sub(1)] {
        if rs_try <= nsym { continue; }
        let result = rse.decode_bytes_with_params(
            payload_lab,
            Some(rs_try),
            Some(nsym),
        );
        if let Ok((recovered, dec_meta)) = result {
            if dec_meta.success {
                // Prefer the shorter rs_total (fewer padding bytes)
                if best_meta.is_none() || recovered.len() < best_recovered.len() {
                    best_recovered = recovered;
                    best_meta = Some(dec_meta);
                }
            }
        }
    }

    let dec_meta = match best_meta {
        Some(m) => m,
        None => {
            return Ok((Vec::new(), BannerDecodeMeta {
                success: false,
                n_palette,
                epsilon,
                n_cells: n_payload_cells,
                errors_corrected: 0,
                error_message: Some("TooManyErrors".to_string()),
            }));
        }
    };

    let meta = BannerDecodeMeta {
        success: dec_meta.success,
        n_palette,
        epsilon,
        n_cells: n_payload_cells,
        errors_corrected: dec_meta.errors_corrected,
        error_message: dec_meta.error_message,
    };

    Ok((best_recovered, meta))
}

/// Decode a banner from a PNG file.
#[cfg(feature = "native")]
pub fn decode_banner_png(
    input_path: &str,
    curve: &PaletteCurve,
    frame: &BishopFrame,
    nsym: usize,
    voronoi_seed: u64,
    relax_iters: usize,
) -> Result<(Vec<u8>, BannerDecodeMeta), String> {
    use image::ImageReader;

    let img = ImageReader::open(input_path)
        .map_err(|e| format!("Failed to open {}: {}", input_path, e))?
        .decode()
        .map_err(|e| format!("Failed to decode {}: {}", input_path, e))?
        .to_rgb8();

    let width = img.width() as usize;
    let height = img.height() as usize;
    let pixels: Vec<u8> = img.into_raw();

    decode_banner_from_pixels(&pixels, width, height, curve, frame, nsym, voronoi_seed, relax_iters)
}

/// Simple cell extraction: segment image by finding large contiguous
/// regions of similar color.
///
/// Returns Vec<(centroid_x, centroid_y, average_lab)> sorted by scan order.
fn extract_cell_colors_simple(
    pixels: &[u8],
    width: usize,
    height: usize,
    min_cell_area_frac: f64,
) -> Vec<(f64, f64, Lab)> {
    // Quantize pixels to sRGB triples for grouping
    let n_pixels = width * height;
    let min_cell_pixels = (n_pixels as f64 * min_cell_area_frac / 10000.0).max(5.0) as usize;

    // Group pixels by exact sRGB color
    use std::collections::HashMap;
    let mut color_groups: HashMap<(u8, u8, u8), Vec<(usize, usize)>> = HashMap::new();

    for py in 0..height {
        for px in 0..width {
            let off = (py * width + px) * 3;
            let key = (pixels[off], pixels[off + 1], pixels[off + 2]);
            color_groups.entry(key).or_default().push((px, py));
        }
    }

    // For each color group, find connected components
    let mut cells: Vec<(f64, f64, Lab)> = Vec::new();

    for (&(r, g, b), positions) in &color_groups {
        if positions.len() < min_cell_pixels {
            continue;
        }

        // Simple flood-fill via pixel set membership
        let pos_set: std::collections::HashSet<(usize, usize)> = positions.iter().cloned().collect();
        let mut visited = std::collections::HashSet::new();

        for &start in positions {
            if visited.contains(&start) {
                continue;
            }

            // BFS flood fill
            let mut component = Vec::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(start);
            visited.insert(start);

            while let Some((cx, cy)) = queue.pop_front() {
                component.push((cx, cy));
                // 4-connected neighbors
                for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                        let np = (nx as usize, ny as usize);
                        if pos_set.contains(&np) && !visited.contains(&np) {
                            visited.insert(np);
                            queue.push_back(np);
                        }
                    }
                }
            }

            if component.len() < min_cell_pixels {
                continue;
            }

            // Compute centroid and average Lab
            let cx_avg: f64 = component.iter().map(|&(x, _)| x as f64).sum::<f64>() / component.len() as f64;
            let cy_avg: f64 = component.iter().map(|&(_, y)| y as f64).sum::<f64>() / component.len() as f64;
            let lab = srgb_to_lab(&Srgb::new(r, g, b));

            cells.push((cx_avg, cy_avg, lab));
        }
    }

    // Sort by scan order (y, then x)
    cells.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.partial_cmp(&b.0).unwrap()));
    cells
}

/// Verify end-to-end banner encode -> render -> decode round-trip.
///
/// Encodes payload, renders to pixel buffer, decodes, and checks equality.
pub fn verify_banner_roundtrip(
    payload: &[u8],
    curve: &PaletteCurve,
    frame: &BishopFrame,
    n_palette: usize,
    epsilon: f64,
    nsym: usize,
    width: usize,
    height: usize,
) -> Result<bool, String> {
    let encoded = encode_banner(
        payload, curve, frame, n_palette, epsilon, nsym,
        width, height, 42, 10,
    )?;

    let pixels = render_banner_pixels(&encoded, 2.0, Srgb::new(10, 10, 25));

    let (recovered, meta) = decode_banner_from_pixels(
        &pixels, width, height, curve, frame, nsym, 42, 10,
    )?;

    Ok(meta.success && recovered == payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::color::Vec3;

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
    fn test_encode_banner_basic() {
        let (curve, frame) = viridis_curve_and_frame();

        // Find valid config
        let (configs, _) = derive_config_table(&curve, &frame, 50);
        if configs.is_empty() {
            return;
        }
        let config = &configs[0];

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let result = encode_banner(
            &payload, &curve, &frame, config.n, config.epsilon, 4,
            300, 100, 42, 5,
        );
        assert!(result.is_ok(), "encode_banner should succeed: {:?}", result.err());

        let encoded = result.unwrap();
        assert!(encoded.cells_srgb.len() > 1, "Should have header + payload cells");
        assert!(encoded.seeds.len() == encoded.cells_srgb.len());
    }

    #[test]
    fn test_render_banner_pixels() {
        let (curve, frame) = viridis_curve_and_frame();
        let (configs, _) = derive_config_table(&curve, &frame, 50);
        if configs.is_empty() {
            return;
        }
        let config = &configs[0];

        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let encoded = encode_banner(
            &payload, &curve, &frame, config.n, config.epsilon, 4,
            200, 80, 42, 3,
        ).unwrap();

        let pixels = render_banner_pixels(&encoded, 1.0, Srgb::new(10, 10, 25));
        assert_eq!(pixels.len(), 200 * 80 * 3);

        // Verify pixels are non-zero (not all black)
        let non_zero = pixels.iter().filter(|&&p| p > 0).count();
        assert!(non_zero > 0, "Rendered pixels should not be all black");
    }

    #[test]
    fn test_banner_cielab_roundtrip() {
        // Test that encode -> decode works at the CIELAB level
        // (without PNG render, so no quantization noise)
        let (curve, frame) = viridis_curve_and_frame();
        let (configs, header_eps) = derive_config_table(&curve, &frame, 50);
        if configs.is_empty() {
            return;
        }
        let config = &configs[0];
        let nsym = 4;

        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF];

        // Encode
        let mut rse = RSEncoder::from_curve(&curve, &frame, config.n, config.epsilon, Some(nsym), 0.5);
        let (payload_pixels, _rs_meta) = rse.encode_bytes(&payload).unwrap();

        // Header
        let header_lab = encode_header(config.n, config.epsilon, &curve, &frame, &configs, header_eps).unwrap();

        // Decode header
        let decoded_config = decode_header(&header_lab, &curve, &frame, &configs, header_eps).unwrap();
        assert_eq!(decoded_config.n, config.n);
        assert_eq!(decoded_config.epsilon, config.epsilon);

        // Decode payload (exact CIELAB, no noise)
        let (recovered, dec_meta) = rse.decode_bytes(&payload_pixels).unwrap();
        assert!(dec_meta.success, "CIELAB decode should succeed");
        assert_eq!(recovered, payload, "CIELAB roundtrip should recover payload");
    }
}
