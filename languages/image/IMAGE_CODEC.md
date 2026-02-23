# Glossia Image Codec

A color-space steganographic codec that encodes arbitrary byte payloads into images. Each visual element (Voronoi cell, pixel, mosaic tile) carries information in its **color**, not its position. The codec operates natively in CIELAB perceptual color space.

## Core idea

A color palette defines a 1D curve $\gamma$ through CIELAB. At each palette point, a 2D constellation grid in the normal plane encodes additional bits. Every pixel color decomposes into:

- **Tangential position** on $\gamma$ &rarr; identifies the payload word (which palette color)
- **Normal-plane displacement** &rarr; encodes the sequence position (which occurrence of that word)

The rendering (Voronoi, grid, brush strokes, mosaic) is purely aesthetic. All information lives in the **multiset of colors**.

## Architecture

```
palette.yaml            Control points in CIELAB (viridis_approx, warm, cool)
        |
        v
  PaletteCurve          Cubic spline, arc-length reparameterized
        |
        v
  BishopFrame           Rotation-minimizing {T, U1, U2} via double-reflection
        |
        v
  compute_capacity_curve   Integrate r(s)^2 along curve
        |
        v
  derive_config_table      Derive (N, eps) configs from tube geometry
        |
        v
  select_encoding_params   Pick optimal config (max bpc)
        |
        v
  equal_capacity_positions  Place N colors at capacity centroids
        |
        v
  ConstellationMap       Per-color M_i x M_i grids sized to local tube radius
        |
        v
  encode_self_describing   Header pixel + payload words -> CIELAB colors
        |
        v
  RSEncoder              Reed-Solomon byte-level error correction (optional)
        |
        v
  render / capture       Voronoi SVG, pixel grid, any visual form
```

## Geometric components

### Palette curve (`PaletteCurve`)

A cubic spline through K control points in CIELAB, reparameterized by arc length so that $|\gamma'(s)| \approx 1$. The total arc length is $L$.

For viridis_approx: L = 137.9, 6 control points from dark purple-blue to warm yellow.

### Capacity curve and adaptive spacing

The **tube radius** $r(s)$ varies along the curve — fat where the gamut is spacious, thin near the sRGB boundary. The **capacity density** $r(s)^2$ measures how much constellation area is available at each point.

The **capacity curve** integrates this density:

$$C(s) = \int_0^s r(u)^2 \, du$$

$C(s)$ is monotonically increasing and $\varepsilon$-independent. N palette colors are placed at **equal-capacity centroids**: the $i$-th color sits at $s_i$ where $C(s_i) = (i + 0.5) \cdot C(L) / N$. This places more colors where the tube is fat (more constellation capacity) and fewer where it is thin.

Centroid mode avoids pinning colors at the thinnest curve endpoints, raising $M_{\min}$ significantly vs. uniform spacing (e.g. N=8: 16→24, +50%).

The optimal (N, $\varepsilon$) configuration is derived from the curve geometry alone — see **Derived config table** below.

### Bishop frame (`BishopFrame`)

An orthonormal frame $\{T(s), U_1(s), U_2(s)\}$ propagated along $\gamma$ via the double-reflection method (Wang et al. 2008). Unlike the Frenet frame, the Bishop frame is smooth through inflection points and has no torsion-induced twist.

$U_1(0)$ is initialized to the Frenet normal at the curve start.

### Tube radius (`compute_tube_radius`)

At each palette point, ray-march outward along $\pm U_1, \pm U_2$ (and intermediate angles) until the sRGB gamut boundary is hit. The minimum over all angles gives $r(s_i)$ -- the radius of the largest inscribed disk in the normal plane that stays within displayable colors.

For viridis_approx: $r$ ranges from 17.8 (bright end) to 59.5 (dark end).

### Constellation (`Constellation`, `ConstellationMap`)

At palette point $c_i$, an $M_i \times M_i$ grid in $\text{span}\{U_1(s_i), U_2(s_i)\}$:

$$\text{point}(a, b) = c_i + \alpha_a \cdot U_1(s_i) + \alpha_b \cdot U_2(s_i)$$

$$\alpha_a = \left(a - \frac{M_i - 1}{2}\right) \cdot \varepsilon, \quad a = 0, \ldots, M_i - 1$$

where $M_i = \lfloor 2 r(s_i) / \varepsilon \rfloor + 1$ and $\varepsilon$ is the grid spacing.

Each grid point encodes a sequence position $j = a \cdot M_i + b$, giving $M_i^2$ positions per palette color. The `ConstellationMap` holds one `Constellation` per palette color, exploiting the variable tube radius so fat-tube regions carry more data.

### Bits per cell (channel capacity)

$$\text{bpc} = \log_2 N + 2 \log_2 M_{\min}$$

For entropy-preserving encoding, each cell should carry the maximum number of bits the palette supports. This is the curve's **intrinsic channel capacity** — a property of the geometry, independent of message length or image resolution. The caller computes the number of cells from the message: $n_{\text{cells}} = \lceil \text{total\_bits} / \text{bpc} \rceil$.

### Cross-palette entropy conservation

For a fixed payload (e.g. a 32-byte Nostr pubkey at 50% RS ECC = 384 bits), the product $\text{bpc} \times n_{\text{cells}} \approx \text{total\_bits}$ is constant across all palettes. This is the **entropy conservation invariant**: different curves encode the same data with different visual densities, but the total information is preserved.

The invariant holds because $n_{\text{cells}} = \lceil \text{total\_bits} / \text{bpc} \rceil$, so fewer bits per cell means more cells (modulo ceiling). What is **not** constant is $\log_2 N \times n_{\text{cells}}$, because each cell carries both word bits and position bits:

$$\text{bpc} = \underbrace{\log_2 N}_{\text{word bits}} + \underbrace{2 \log_2 M_{\min}}_{\text{position bits}}$$

When $N$ decreases, the palette colors are more widely spaced on the curve, leaving more tube radius per color. The optimizer exploits this by allowing a finer constellation grid ($M_{\min}$ increases), partially or fully compensating for the lost word bits. The two terms trade off:

| Palette | $N$ | $M_{\min}$ | word bits | pos bits | bpc | cells (32B) | $\text{bpc} \times \text{cells}$ |
|:--|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| cividis | 4 | 32 | 2.0 | 10.0 | 12.0 | 32 | 384 |
| inferno | 7 | 32 | 2.8 | 10.0 | 12.8 | 30 | 384 |
| magma | 8 | 32 | 3.0 | 10.0 | 13.0 | 30 | 390 |
| mako | 12 | 16 | 3.6 | 8.0 | 11.6 | 34 | 394 |
| plasma | 8 | 16 | 3.0 | 8.0 | 11.0 | 35 | 385 |
| rocket | 7 | 32 | 2.8 | 10.0 | 12.8 | 30 | 384 |
| turbo | 8 | 32 | 3.0 | 10.0 | 13.0 | 30 | 390 |
| viridis | 13 | 16 | 3.7 | 8.0 | 11.7 | 33 | 386 |

The residual variation (384--394) is purely from ceiling rounding.

Note that $N$ (palette size) is **not** the number of Voronoi cells. Multiple cells can share the same palette color — the constellation position distinguishes them. The number of cells is determined solely by $\lceil \text{total\_bits} / \text{bpc} \rceil$.

The tradeoff is not monotone because different curves have different tube-radius profiles: cividis is short (arc length 143) but uniformly fat, so it supports $M_{\min} = 32$ even at $N = 4$. Viridis is longer (arc length 220) with thin spots near the bright end, capping $M_{\min}$ at 16 despite having more room for palette colors.

### Derived config table

Instead of hardcoded (N, $\varepsilon$) pairs, `derive_config_table()` computes all feasible configurations from the curve geometry:

1. For each power-of-2 $N \in \{2, 4, \ldots, 128\}$, place palette colors at equal-capacity centroids and find $r_{\min}$ (minimum tube radius across those positions).
2. For each target $M \in \{2, 4, 8, 16, 32\}$ (powers of 2 for clean bit packing), derive $\varepsilon = 2 r_{\min} / (M - 1)$.
3. Filter: discard configs where $\varepsilon < 2.0$ (sub-threshold) or $\text{bpc} < 2$.

The **optimal config** maximizes bpc, with $\varepsilon$ as tiebreaker for noise robustness. `select_encoding_params(curve, frame)` returns this config — it takes only the curve, not the message.

For viridis_approx: optimal is N=128, $\varepsilon$=2.5, M=16, **bpc=15** (7 word bits + 8 position bits).

### Self-describing header

The first pixel in the output is a **header color** at a fixed position $s=0$ (the fattest tube region). Its constellation displacement encodes an index into the derived config table, declaring the (N, $\varepsilon$) pair used for all subsequent pixels. This makes the encoding self-describing — analogous to declaring the radix in variable-length integer encoding.

The header's constellation spacing is also derived from the curve: $\varepsilon_{\text{header}} = 2 r(0) / (M_{\text{needed}} - 1)$ where $M_{\text{needed}} = \lceil\sqrt{|\text{config table}|}\rceil$. Both encoder and decoder compute the same table from the same curve, so no out-of-band metadata is needed.

## Encoding / decoding

### Encode

For the $i$-th payload word with value $w$:

1. Look up palette position: $s_w$ from the adaptive palette (equal-capacity centroids)
2. Get base color: $c_w = \gamma(s_w)$
3. Assign sequence position $j$ (the $j$-th occurrence of word $w$)
4. Map to displacement: $(a, b) = (j \mathbin{/} M_w,\; j \bmod M_w)$, then $(\alpha_1, \alpha_2)$
5. Pixel color: $c_w + \alpha_1 U_1(s_w) + \alpha_2 U_2(s_w)$

For self-describing mode, the header pixel is prepended (see **Self-describing header** above).

### Decode (geometric)

For each pixel color $x$ in CIELAB:

1. Project onto $\gamma$: find $s^* = \arg\min_s \|x - \gamma(s)\|$
2. Snap to palette: $w = \arg\min_i |s_{\text{palette}}[i] - s^*|$
3. Decompose residual: $\delta = x - c_w$, then $\alpha_1 = \delta \cdot U_1$, $\alpha_2 = \delta \cdot U_2$
4. Snap to grid: $(a, b) = \text{round}(\alpha / \varepsilon + (M-1)/2)$
5. Recover position: $j = a M + b$

For self-describing mode, the header pixel is decoded first to recover (N, $\varepsilon$), then the remaining pixels are decoded with those parameters.

## Noise model and the $\varepsilon / \sigma$ relationship

The grid spacing $\varepsilon$ sets the decision boundary at $\varepsilon / 2$. For a cell to decode correctly, the noise must not push the color past this boundary in either normal-plane axis.

For 2D Gaussian noise with per-axis $\sigma$:

$$P(\text{correct cell}) = \left[\text{erf}\!\left(\frac{\varepsilon}{2\sqrt{2}\,\sigma}\right)\right]^2$$

For 99% cell accuracy: $\varepsilon \geq 5.6 \cdot \sigma$.

### Spatial averaging

When a visual element (Voronoi cell, tile) spans $P$ screen pixels, the decoder can average all pixels in the region to reduce noise:

$$\sigma_{\text{eff}} = \frac{\sigma_{\text{raw}}}{\sqrt{P}}$$

$$\sigma_{95,\text{eff}} = \frac{\varepsilon}{5.6} \cdot \sqrt{P}$$

| Image size | Cells (32B msg) | px/cell | $\sqrt{P}$ | $\sigma_{95,\text{eff}}$ ($\varepsilon=2.5$, bpc=15) |
|:-:|:-:|:-:|:-:|:-:|
| 100x100 | 26 | 385 | 20 | 8.8 |
| 400x400 | 26 | 6154 | 78 | 35.2 |
| 800x800 | 26 | 24615 | 157 | 70.1 |

QR code reference: $\sigma_{95} \approx 18$ (B/W threshold at $\Delta L^* \approx 50$, 4 px/module).

At 400x400 with the optimal config ($\varepsilon = 2.5$, 26 cells), we exceed QR's noise tolerance (35.2 vs 18) while using 24x fewer visual elements (26 cells vs 625 modules).

## Reed-Solomon error correction (`RSEncoder`)

Wraps the parametric encoder with RS codes over GF(256) for fair comparison against QR codes (which use RS internally).

```
payload bytes -> RS encode (+ parity) -> bitstream -> pack into cells
cells -> unpack -> RS decode (correct errors) -> payload bytes
```

Bits are packed into cells using the **minimum** constellation size ($M_{\min}$) for uniform bit packing:

- word_bits = $\log_2 N$
- pos_bits = $2 \log_2 M_{\min}$
- bits/cell = word_bits + pos_bits

The joint decoder searches all $N$ palette colors per cell to find the $(w, j)$ pair with minimum residual. This is necessary because large constellation displacements can push a pixel closer to an adjacent palette color's base than to its own.

### 32-byte Nostr pubkey: Voronoi vs QR

With the optimal derived config (N=128, $\varepsilon$=2.5, bpc=15):

| Config | Elements | bits/cell | $\sigma_{95,\text{eff}}$ (400x400) |
|:--|:-:|:-:|:-:|
| QR-L V2 (25x25) | 625 modules | 1 | 18 |
| Voronoi, optimal (N=128, $\varepsilon$=2.5), 50% ECC | 26 cells | 15 | 22.6 |
| Voronoi, N=16, $\varepsilon$=15, 50% ECC | 64 cells | 7 | 134 |

## Decoder comparison

Three decoders are implemented, forming a Pareto frontier on the (information required, noise tolerance) axes.

### Geometric decoder (`parametric_encoding.decode`)

Per-pixel, independent: project onto $\gamma$, snap to palette, decompose residual via Bishop frame, snap to constellation grid. **O(n S)** where S = 2000 curve search samples.

- Best per-pixel accuracy under noise (uses full 3D + constellation snapping)
- Requires pixel ordering (layout-dependent)
- Recovers both word and sequence position
- Trivially parallelizable

### Rips filtration decoder (`spectral_decoder.rips_decode`)

Project all colors onto $\gamma$, build all-pairs distance matrix, union-find persistence, gap cut, Hungarian matching. **O(n^2 log n)**.

- Rendering-agnostic (only color values, no spatial coordinates)
- Exact on clean data (persistence gap = palette spacing, a geometric invariant)
- Fragile to per-pixel noise (1D projection discards normal-plane information)
- Dominant regime: spatial averaging drives $\sigma_{\text{eff}} \to 0$, restoring clean-data perfection

### Spectral decoder (`spectral_decoder.spectral_decode`)

Gaussian similarity on 1D projections, normalized Laplacian, eigengap, k-means. **O(n^3)** (dense eigendecomposition).

- Rendering-agnostic (same as Rips)
- More noise-robust than Rips (global graph structure smooths local perturbations)
- Only 92.5% on clean data (k-means is approximate)
- Eigengap is the spectral analogue of the persistence gap

### Noise sweep results (per-pixel, no spatial averaging)

| $\sigma$ | Geometric | Rips | Spectral |
|:-:|:-:|:-:|:-:|
| 0 | 100% | 100% | 92.5% |
| 0.5 | 100% | 99.9% | 89.6% |
| 1.0 | 99.0% | 70.0% | 87.8% |
| 3.0 | 82.6% | 22.8% | 65.0% |

### Decoder selection guide

| | Geometric | Rips | Spectral |
|:--|:-:|:-:|:-:|
| **Complexity** | O(n S) | O(n^2 log n) | O(n^3) |
| **Clean accuracy** | 100% | 100% | 92.5% |
| **Needs pixel order?** | Yes | No | No |
| **Needs exact params?** | curve + frame + constellation | curve only | curve + $\sigma$ kernel |
| **Best regime** | Per-pixel, known layout | Clean data, unknown layout | Noisy data, unknown layout |

With spatial averaging, the topological decoders become dominant: averaging thousands of pixels per cell drives $\sigma_{\text{eff}} \to 0$, restoring the clean-data regime where Rips achieves 100%.

## Topological decoders (`spectral_decoder.py`)

Two rendering-agnostic decoders that use only color values, no spatial coordinates.

### Why project onto $\gamma$ first?

At $\varepsilon = 2.3$, constellation displacements (up to 83 CIELAB) exceed palette spacing (9.2 CIELAB). Colors from adjacent palette words overlap in raw CIELAB. Clustering in 3D fails.

But the displacements are **purely normal** to $\gamma$ (by construction). Projecting onto $\gamma$ collapses the normal component, leaving only the tangential coordinate where palette words are well-separated. The projection is the key insight: it transforms a 3D overlapping problem into a 1D well-separated one.

The persistence gap is the **topological analogue of the eigengap** in spectral clustering. Both detect the natural cluster scale, but persistence does it via filtration (scanning all scales) rather than requiring a kernel bandwidth.

## Screen-to-camera channel

### Target scenario

Display an encoded image on a screen; a phone camera captures it. The captured photo is decoded without any fiducials, markers, or known pixel positions. All calibration happens in **color space**.

### Noise budget

The screen-to-camera channel has two fundamentally different noise classes. Spatial averaging (many pixels per cell) only helps with the first.

**Random noise** (averaging helps: $\sigma_{\text{eff}} = \sigma_{\text{raw}} / \sqrt{P}$):

| Source | Typical $\sigma_{\text{raw}}$ (CIELAB) | After averaging (2500 px/cell) |
|:--|:-:|:-:|
| Camera sensor (well-lit) | 1--3 | 0.02--0.06 |
| Camera sensor (dim) | 5--10 | 0.1--0.2 |
| JPEG quantization (quality 85--95) | 1--2 | 0.02--0.04 |

With 400x400 capture and 64 cells ($\sqrt{P} \approx 50$), random noise is annihilated.

**Systematic errors** (averaging does NOT help -- correlated across all pixels):

| Source | CIELAB effect | Typical magnitude |
|:--|:--|:--|
| Auto white balance | $b^*$ shift + $0.15 \cdot a^*$ | $\Delta E \sim 5$--$20$ |
| Auto exposure | $L^*$ offset | $\Delta L^* \sim 5$--$15$ |
| Display color profile | nonuniform, gamut-dependent | $\Delta E \sim 3$--$10$ |
| HDR tone mapping | nonlinear $L^*$ curve | $\Delta L^* \sim 5$--$20$ |
| Camera saturation boost | chroma scaling ($a^*, b^*$) | $\times 0.7$--$1.3$ |

Systematic errors dominate. They are modeled as a 4-parameter affine camera transform in CIELAB (see below).

### Camera transform model

The camera's systematic errors are approximately affine in CIELAB with 4 degrees of freedom:

$$\begin{pmatrix} L' \\ a' \\ b' \end{pmatrix} = \begin{pmatrix} 1 & 0 & 0 \\ 0 & s & 0 \\ 0 & 0 & s \end{pmatrix} \begin{pmatrix} L \\ a \\ b \end{pmatrix} + \begin{pmatrix} \Delta L \\ \Delta a \\ \Delta b \end{pmatrix}$$

| Parameter | Physical source | Typical range |
|:--|:--|:--|
| $\Delta L$ | Auto exposure, ambient light | $\pm 25$ |
| $s$ | Saturation boost / desaturation | $0.65$--$1.3$ |
| $\Delta a$ | Green-magenta white balance coupling | $\pm 5$ |
| $\Delta b$ | Yellow-blue color temperature shift | $\pm 25$ |

The key structural constraint: $a^*$ and $b^*$ share the same scale factor $s$ (camera saturation affects chroma uniformly). This keeps the model at 4 parameters rather than the full 12 of a general affine map.

### Calibration cells

The encoder includes **N calibration cells** (one per palette color) alongside the payload cells. Each calibration cell's color is exactly the palette base point $c_i = \gamma(s_i)$ with zero constellation displacement.

Calibration cells are visually indistinguishable from payload cells -- they are scattered among payload cells in the rendered image. No fiducials, markers, or known pixel positions are needed. The decoder identifies them by their color-space properties (see below).

The overhead is modest: for a 40-cell payload with N=16 palette colors, the total becomes 56 cells (29% overhead), comparable to RS parity overhead.

### Color-space calibration pipeline

Calibration happens entirely in color space by recognizing the palette's geometric signature under affine transformation.

```
Screen display
    |
Phone camera capture (applies unknown affine A*x + b)
    |
Cell segmentation (bottom-up: connected components of similar color)
    |
Spatial averaging (each cell -> one mean CIELAB color)
    |
Grid search: identify palette + camera params (color space only)
    |
Inlier refinement (calibration cells -> precise affine correction)
    |
Decode corrected colors (geometric decoder, ~zero-noise regime)
    |
RS error correction (fix residual calibration errors)
    |
Recovered payload
```

### Grid search palette identification

The palette and camera transform are identified jointly by exhaustive search over the 4 camera parameters for each candidate palette. The scoring function exploits the calibration cells:

1. For each candidate (palette $P$, $\Delta L$, $s$, $\Delta a$, $\Delta b$):
   a. Apply the inverse camera transform to all observed cell colors
   b. For each corrected color, compute distance to the nearest palette base point
   c. Count **inliers**: corrected colors within a threshold (3.0 CIELAB) of a base point
2. The correct (palette, params) produces the most inliers ($\approx N$ calibration cells)
3. Wrong palette or wrong params produce few inliers

**Why this works**: calibration cells have zero constellation displacement, so after correct inverse transform they land exactly on the palette base points (distance $\approx 0$, always inliers). Payload cells have constellation displacements of 5--60 CIELAB, so they rarely trigger the inlier threshold. Wrong palettes or wrong parameters produce no near-zero distances at all.

**Grid resolution**:

| Parameter | Range | Step | Values |
|:--|:--|:--|:-:|
| $\Delta L$ | $[-30, 30]$ | 5 | 13 |
| $s$ | $[0.5, 1.5]$ | 0.05 | 21 |
| $\Delta a$ | $[-10, 10]$ | 5 | 5 |
| $\Delta b$ | $[-30, 30]$ | 5 | 13 |

Total: $13 \times 21 \times 5 \times 13 = 17{,}745$ evaluations per palette, $\times 3$ palettes $\approx 53{,}000$ total. Each evaluation is O(n N) distance computations. At 56 cells and 16 palette colors, the full search completes in $\sim 4$ seconds (Python) or $< 50$ ms (Rust).

### Inlier refinement

After the coarse grid search identifies the palette and approximate camera parameters:

1. **Local grid refinement**: search a $3^4 = 81$-point grid at half the coarse step around the best point
2. **Iterative inlier fit** (3--5 rounds):
   a. Apply inverse transform to all observed colors
   b. Identify inliers (distance to nearest palette base point $< \tau$)
   c. Inliers are the calibration cells -- fit a least-squares 4-parameter model from them
   d. Start with $\tau = 3.0$, widen to $\tau = 5.0$ in later rounds to capture more calibration cells

The iterative fit converges because calibration cells (zero displacement) are separated from payload cells (large displacement) by the constellation grid spacing. There is no ambiguity once the coarse parameters are within half a grid step.

### Calibration accuracy

Tested with viridis_approx, N=16, $\varepsilon=5.0$, 40 payload + 16 calibration cells, $\sigma_{\text{noise}} = 1.5$ CIELAB:

| Camera transform | $\Delta L$ | $s$ | $\Delta b$ | Palette ID | Decode |
|:--|:-:|:-:|:-:|:-:|:-:|
| None | 0 | 1.0 | 0 | correct | 100% |
| Mild AWB | 5 | 1.0 | 8 | correct | 100% |
| Strong AWB | 12 | 0.85 | 18 | correct | 100% |
| Dim + warm | -15 | 0.9 | 12 | correct | 100% |
| Bright + cool | 20 | 1.1 | -10 | correct | 100% |
| Harsh desaturation | -5 | 0.7 | 5 | correct | 97.5% |
| Extreme ($\Delta L$=25, $s$=0.65, $\Delta b$=25) | 25 | 0.65 | 25 | correct | 82.5% |

Parameter recovery is sub-CIELAB: $|\Delta(\Delta L)| < 1$, $|\Delta s| < 0.02$, $|\Delta(\Delta b)| < 0.5$ for all cases up to "harsh desaturation."

The "extreme" case (35% desaturation + 25 CIELAB brightness shift + 25 CIELAB color temp) still identifies the palette correctly and decodes 82.5% of words -- well within RS correction capability at 50% ECC.

### Recommended parameters for screen-to-camera

The optimal config is derived automatically from the curve via `select_encoding_params()`. For viridis_approx:

| Parameter | Value | Rationale |
|:--|:--|:--|
| N (palette size) | 128 | 7 word bits; derived as optimal bpc config |
| $\varepsilon$ | 2.5 | Derived from $r_{\min}$ at centroid positions for M=16 |
| bpc | 15 | 7 word + 8 position bits; curve's intrinsic channel capacity |
| Payload cells | $\lceil\text{total\_bits}/15\rceil$ | 32 bytes @ 50% ECC → 26 cells |
| Calibration cells | 128 (one per palette color) | Zero constellation displacement; hidden among payload cells |
| Image size | 400x400 | 2500+ px/cell for massive spatial averaging |
| ECC | RS 50% | Corrects up to 25% symbol errors; handles residual calibration imprecision |

For screen-to-camera scenarios where systematic error dominates, a lower-bpc config (e.g. N=16, $\varepsilon$=5.0, bpc=8) may be preferable — the derived config table provides all valid options and the encoder can select among them.

## Rendering

The rendering is **decoupled from encoding**. Any spatial layout that assigns one color to each visual element works:

| Rendering | Spatial info needed? | Decoder |
|:--|:-:|:--|
| Voronoi diagram | seed positions | geometric or topological |
| Pixel grid | row-major order | geometric |
| Mosaic tiles | none | topological |
| Brush strokes | none | topological |
| Color-graded photo | none | topological + segmentation |

The Flask app (`app.py`) demonstrates Voronoi rendering with interactive controls for $\varepsilon$, palette, cell count, and environmental simulation (noise, brightness, color temperature, saturation).

## Environment simulation

The app simulates four perturbation channels, all native to CIELAB:

| Channel | CIELAB operation | Physical source |
|:--|:--|:--|
| Noise $\sigma$ | Gaussian on all 3 axes | Sensor noise, quantization |
| Brightness | $L^*$ offset | Exposure, ambient light |
| Color temp | $b^*$ shift + $0.15 \cdot a^*$ | Tungsten / shade / daylight |
| Saturation | Chroma scaling ($a^*, b^*$) | Display gamut, distance |

## File reference

| File | Purpose |
|:--|:--|
| `parametric_encoding.py` | Core library: curve, frame, constellation, encode/decode |
| `rs_encoding.py` | Reed-Solomon wrapper, QR comparison, noise sweep |
| `spectral_decoder.py` | Rips filtration and spectral decoders |
| `app.py` | Flask demo with Voronoi rendering and environment simulation |
| `visualize_encoding.py` | Voronoi seed generation and image rendering |
| `noise_analysis.py` | Noise sweep and capacity frontier plots |
| `sweep_params.py` | Parameter space sweep for optimal (N, $\varepsilon$, ECC) configs |
| `test_parametric.py` | Round-trip, gamut, separation, noise, capacity tests |
| `palette.yaml` | CIELAB control points for palette curves |

## Key invariants

1. **Constellation displacements are normal to $\gamma$**: by construction, $\alpha_1 U_1 + \alpha_2 U_2 \perp T$. This ensures the curve projection recovers word identity exactly (clean data).

2. **Tube radius bounds the constellation**: $M_i = \lfloor 2r(s_i)/\varepsilon \rfloor + 1$ guarantees all constellation points are within the sRGB gamut.

3. **Capacity curve is $\varepsilon$-independent**: $C(s) = \int_0^s r(u)^2\,du$ depends only on the curve geometry and the sRGB gamut, not on the encoding parameters. This separates palette placement from constellation sizing.

4. **Channel capacity is intrinsic**: the optimal (N, $\varepsilon$) config — and therefore bpc — is a property of the palette curve geometry alone. Message length determines the number of cells ($\lceil\text{total\_bits}/\text{bpc}\rceil$), not the encoding density. This is the entropy-preserving constraint: each cell carries the maximum bits the palette supports.

5. **Entropy conservation across palettes**: for a fixed payload, $\text{bpc} \times n_{\text{cells}} \approx \text{total\_bits}$ regardless of palette curve. Different curves produce different (N, $M_{\min}$) splits, but the total information per image is constant. The word bits ($\log_2 N$) and position bits ($2 \log_2 M_{\min}$) trade off: fewer palette colors → wider spacing → finer constellation grids. The $\log_2 N \times n_{\text{cells}}$ product is **not** constant — only the full $\text{bpc} \times n_{\text{cells}}$ product is.

6. **Self-describing header is deterministic**: both encoder and decoder derive the same config table from the same curve, so the header index unambiguously identifies (N, $\varepsilon$) without out-of-band metadata.

7. **Persistence gap = palette spacing**: the Rips filtration's largest gap in death times equals $L/(N-1)$, the arc-length spacing between adjacent palette colors. This is a geometric invariant of the encoding, independent of the specific payload.

8. **Spatial averaging multiplies noise tolerance by $\sqrt{P}$**: a cell with $P$ pixels has effective noise $\sigma/\sqrt{P}$, giving $\sigma_{95,\text{eff}} = (\varepsilon/5.6) \cdot \sqrt{P}$.

9. **Rendering is arbitrary**: information lives in the multiset of colors, not spatial layout. The topological decoders formalize this via the Vietoris-Rips complex on color space.

10. **Calibration cells are steganographic**: calibration cells (bare palette base colors) are visually indistinguishable from payload cells (palette + constellation displacement). No fiducials or markers are needed. The decoder identifies them by distance to palette base points after inverse camera transform -- a color-space-only operation.

11. **Camera transform is low-dimensional**: the 4-parameter affine model ($\Delta L$, $s$, $\Delta a$, $\Delta b$) captures the dominant systematic errors (exposure, white balance, saturation). Grid search over this space is tractable ($\sim 50$K evaluations) and identifies both the palette and the correction simultaneously.
