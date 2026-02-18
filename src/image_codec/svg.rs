/// SVG image generation from color token sequences.
///
/// Takes a list of CSS hex colors (extracted from text notation) and
/// renders them as SVG using different layout strategies (dialects).
/// No external dependencies — SVG is just string building.

use super::voronoi::{generate_seeds, lloyd_relax, voronoi_cells};

/// Layout style for SVG generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layout {
    /// Voronoi cells — Lloyd-relaxed, organic shapes.
    Voronoi,
    /// Rectangular grid — rows and columns of colored squares.
    Grid,
    /// Circular dots on a dark background.
    Constellation,
}

/// Configuration for SVG rendering.
#[derive(Debug, Clone)]
pub struct SvgConfig {
    /// Canvas width in SVG units.
    pub width: f64,
    /// Canvas height in SVG units.
    pub height: f64,
    /// Layout style.
    pub layout: Layout,
    /// Random seed for Voronoi point placement.
    pub seed: u64,
    /// Number of Lloyd relaxation iterations.
    pub relax_iters: usize,
    /// Grid columns (for Grid layout).
    pub cols: usize,
    /// Background color (CSS).
    pub background: String,
    /// Stroke color for cell borders (CSS).
    pub stroke: String,
    /// Stroke width.
    pub stroke_width: f64,
    /// Whether to use circular clipping (Voronoi only).
    pub circular: bool,
}

impl Default for SvgConfig {
    fn default() -> Self {
        SvgConfig {
            width: 400.0,
            height: 400.0,
            layout: Layout::Voronoi,
            seed: 42,
            relax_iters: 5,
            cols: 8,
            background: "#0f0f23".to_string(),
            stroke: "#14142a".to_string(),
            stroke_width: 1.5,
            circular: false,
        }
    }
}

/// Generate an SVG string from a list of CSS hex color strings.
///
/// Each color becomes one cell/element in the chosen layout.
/// Colors should be CSS hex format like "#440255".
pub fn render_svg(colors: &[&str], config: &SvgConfig) -> String {
    match config.layout {
        Layout::Voronoi => render_voronoi(colors, config),
        Layout::Grid => render_grid(colors, config),
        Layout::Constellation => render_constellation(colors, config),
    }
}

/// Voronoi cell layout.
fn render_voronoi(colors: &[&str], config: &SvgConfig) -> String {
    let n = colors.len();
    let mut seeds = generate_seeds(n, config.width, config.height, config.seed);
    lloyd_relax(&mut seeds, config.width, config.height, config.relax_iters);
    let cells = voronoi_cells(&seeds, config.width, config.height);

    let mut svg = String::with_capacity(n * 200 + 500);

    // Header
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n",
        w = config.width,
        h = config.height,
    ));

    // Defs for circular clip
    if config.circular {
        let cr = config.width.min(config.height) / 2.0;
        let cx = config.width / 2.0;
        let cy = config.height / 2.0;
        svg.push_str("<defs>\n");
        svg.push_str(&format!(
            "  <clipPath id=\"circle-clip\">\
             <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\"/>\
             </clipPath>\n",
            cx = cx,
            cy = cy,
            r = cr - 1.0,
        ));
        svg.push_str("</defs>\n");
        svg.push_str("<g clip-path=\"url(#circle-clip)\">\n");
    }

    // Background
    svg.push_str(&format!(
        "<rect width=\"{w}\" height=\"{h}\" fill=\"{bg}\"/>\n",
        w = config.width,
        h = config.height,
        bg = config.background,
    ));

    // Voronoi cells
    for (i, cell) in cells.iter().enumerate() {
        if i >= n || cell.vertices.len() < 3 {
            continue;
        }
        svg.push_str(&format!(
            "<polygon points=\"{pts}\" fill=\"{color}\" \
             stroke=\"{stroke}\" stroke-width=\"{sw}\" \
             stroke-linejoin=\"round\"/>\n",
            pts = cell.svg_points(),
            color = colors[i],
            stroke = config.stroke,
            sw = config.stroke_width,
        ));
    }

    if config.circular {
        svg.push_str("</g>\n");
    }

    svg.push_str("</svg>\n");
    svg
}

/// Grid layout — colored rectangles in rows.
fn render_grid(colors: &[&str], config: &SvgConfig) -> String {
    let n = colors.len();
    let cols = config.cols.max(1);
    let rows = (n + cols - 1) / cols;
    let cell_w = config.width / cols as f64;
    let cell_h = config.height / rows as f64;

    let mut svg = String::with_capacity(n * 150 + 300);

    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n",
        w = config.width,
        h = config.height,
    ));

    svg.push_str(&format!(
        "<rect width=\"{w}\" height=\"{h}\" fill=\"{bg}\"/>\n",
        w = config.width,
        h = config.height,
        bg = config.background,
    ));

    for (i, color) in colors.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = col as f64 * cell_w;
        let y = row as f64 * cell_h;
        // Inset by stroke_width/2 for clean borders
        let inset = config.stroke_width / 2.0;
        svg.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" \
             width=\"{w:.1}\" height=\"{h:.1}\" \
             fill=\"{color}\" \
             stroke=\"{stroke}\" stroke-width=\"{sw}\"/>\n",
            x = x + inset,
            y = y + inset,
            w = cell_w - config.stroke_width,
            h = cell_h - config.stroke_width,
            color = color,
            stroke = config.stroke,
            sw = config.stroke_width,
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

/// Constellation layout — colored dots on dark background.
fn render_constellation(colors: &[&str], config: &SvgConfig) -> String {
    let n = colors.len();
    let mut seeds = generate_seeds(n, config.width, config.height, config.seed);
    lloyd_relax(&mut seeds, config.width, config.height, config.relax_iters);

    // Dot radius scales with canvas size and cell count
    let avg_spacing = (config.width * config.height / n as f64).sqrt();
    let dot_r = avg_spacing * 0.3;

    let mut svg = String::with_capacity(n * 120 + 300);

    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n",
        w = config.width,
        h = config.height,
    ));

    svg.push_str(&format!(
        "<rect width=\"{w}\" height=\"{h}\" fill=\"{bg}\"/>\n",
        w = config.width,
        h = config.height,
        bg = config.background,
    ));

    for (i, color) in colors.iter().enumerate() {
        if i >= seeds.len() {
            break;
        }
        svg.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r:.1}\" fill=\"{color}\"/>\n",
            x = seeds[i].x,
            y = seeds[i].y,
            r = dot_r,
            color = color,
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_voronoi_basic() {
        let colors = vec!["#440255", "#2a788e", "#7ad151", "#fde725"];
        let config = SvgConfig {
            width: 100.0,
            height: 100.0,
            ..Default::default()
        };
        let svg = render_svg(&colors, &config);
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("#440255"));
        assert!(svg.contains("<polygon"));
    }

    #[test]
    fn test_render_grid_basic() {
        let colors = vec!["#ff0000", "#00ff00", "#0000ff", "#ffff00"];
        let config = SvgConfig {
            layout: Layout::Grid,
            cols: 2,
            width: 100.0,
            height: 100.0,
            ..Default::default()
        };
        let svg = render_svg(&colors, &config);
        assert!(svg.contains("<rect"));
        assert!(svg.contains("#ff0000"));
        // 4 color rects + 1 background rect = 5 rects total
        assert_eq!(svg.matches("<rect").count(), 5);
    }

    #[test]
    fn test_render_constellation_basic() {
        let colors = vec!["#440255", "#2a788e", "#7ad151"];
        let config = SvgConfig {
            layout: Layout::Constellation,
            width: 100.0,
            height: 100.0,
            ..Default::default()
        };
        let svg = render_svg(&colors, &config);
        assert!(svg.contains("<circle"));
        assert_eq!(svg.matches("<circle").count(), 3);
    }

    #[test]
    fn test_render_voronoi_circular() {
        let colors = vec!["#440255", "#2a788e"];
        let config = SvgConfig {
            circular: true,
            width: 100.0,
            height: 100.0,
            ..Default::default()
        };
        let svg = render_svg(&colors, &config);
        assert!(svg.contains("clipPath"));
        assert!(svg.contains("circle-clip"));
    }

    #[test]
    fn test_render_deterministic() {
        let colors = vec!["#440255", "#2a788e", "#7ad151", "#fde725"];
        let config = SvgConfig::default();
        let a = render_svg(&colors, &config);
        let b = render_svg(&colors, &config);
        assert_eq!(a, b, "Same inputs should produce identical SVG");
    }
}
