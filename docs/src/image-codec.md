# Glossia Image Codec

A color-space steganographic codec that encodes arbitrary byte payloads into images. Each visual element (Voronoi cell, pixel, mosaic tile) carries information in its **color**, not its position. The codec operates natively in CIELAB perceptual color space.

## Core idea

A color palette defines a 1D curve $\gamma$ through CIELAB. At each palette point, a 2D constellation grid in the normal plane encodes additional bits. Every pixel color decomposes into:

- **Tangential position** on $\gamma$ &rarr; identifies the payload word (which palette color)
- **Normal-plane displacement** &rarr; encodes the sequence position (which occurrence of that word)

The rendering (Voronoi, grid, brush strokes, mosaic) is purely aesthetic. All information lives in the **multiset of colors**.

## Architecture

```
palette.yaml          Control points in CIELAB (viridis_approx, warm, cool)
        |
        v
  PaletteCurve        Cubic spline, arc-length reparameterized
        |
        v
  BishopFrame          Rotation-minimizing {T, U1, U2} via double-reflection
        |
        v
  ConstellationMap     Per-color M_i x M_i grids sized to local tube radius
        |
        v
  encode / decode      Payload words <-> CIELAB colors
        |
        v
  RSEncoder            Reed-Solomon byte-level error correction (optional)
        |
        v
  render / capture     Voronoi SVG, pixel grid, any visual form
```

## Geometric components

### Palette curve (`PaletteCurve`)

A cubic spline through K control points in CIELAB, reparameterized by arc length so that $|\gamma'(s)| \approx 1$. N palette colors are equally spaced along $\gamma$:

$$c_i = \gamma\!\left(\frac{i \cdot L}{N-1}\right), \quad i = 0, \ldots, N-1$$

where $L$ is the total arc length.

For viridis_approx with N=16: L = 137.9, palette spacing = 9.2 CIELAB units.

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

where $M_i = \lfloor 2 r(s_i) / \varepsilon \rfloor + 1$ and $\varepsilon$ is the grid spacing (default: 2.3 CIELAB, the just-noticeable difference).

Each grid point encodes a sequence position $j = a \cdot M_i + b$, giving $M_i^2$ positions per palette color. The `ConstellationMap` holds one `Constellation` per palette color, exploiting the variable tube radius so fat-tube regions carry more data.

### Bits per cell

$$\text{bits/cell} = \log_2 N + 2 \log_2 M_{\min}$$

| $\varepsilon$ | $M_{\min}$ | bits/cell (N=16) | $\sigma_{95}$ single pixel |
|:-:|:-:|:-:|:-:|
| 2.3 | 16 | 12 | 0.4 |
| 5.0 | 7 | 10 | 0.9 |
| 10.0 | 4 | 8 | 1.8 |
| 15.0 | 3 | 7 | 2.7 |

## Encoding / decoding

### Encode

For the $i$-th payload word with value $w$:

1. Look up palette point: $s_w = w \cdot L / (N-1)$
2. Get base color: $c_w = \gamma(s_w)$
3. Assign sequence position $j$ (the $j$-th occurrence of word $w$)
4. Map to displacement: $(a, b) = (j \mathbin{/} M_w,\; j \bmod M_w)$, then $(\alpha_1, \alpha_2)$
5. Pixel color: $c_w + \alpha_1 U_1(s_w) + \alpha_2 U_2(s_w)$

### Decode (geometric)

For each pixel color $x$ in CIELAB:

1. Project onto $\gamma$: find $s^* = \arg\min_s \|x - \gamma(s)\|$
2. Snap to palette: $w = \text{round}(s^* \cdot (N-1) / L)$
3. Decompose residual: $\delta = x - c_w$, then $\alpha_1 = \delta \cdot U_1$, $\alpha_2 = \delta \cdot U_2$
4. Snap to grid: $(a, b) = \text{round}(\alpha / \varepsilon + (M-1)/2)$
5. Recover position: $j = a M + b$

## Noise model and the $\varepsilon / \sigma$ relationship

The grid spacing $\varepsilon$ sets the decision boundary at $\varepsilon / 2$. For a cell to decode correctly, the noise must not push the color past this boundary in either normal-plane axis.

For 2D Gaussian noise with per-axis $\sigma$:

$$P(\text{correct cell}) = \left[\text{erf}\!\left(\frac{\varepsilon}{2\sqrt{2}\,\sigma}\right)\right]^2$$

For 99% cell accuracy: $\varepsilon \geq 5.6 \cdot \sigma$.

### Spatial averaging

When a visual element (Voronoi cell, tile) spans $P$ screen pixels, the decoder can average all pixels in the region to reduce noise:

$$\sigma_{\text{eff}} = \frac{\sigma_{\text{raw}}}{\sqrt{P}}$$

$$\sigma_{95,\text{eff}} = \frac{\varepsilon}{5.6} \cdot \sqrt{P}$$

| Image size | Cells | px/cell | $\sqrt{P}$ | $\sigma_{95,\text{eff}}$ ($\varepsilon=2.3$) | $\sigma_{95,\text{eff}}$ ($\varepsilon=15$) |
|:-:|:-:|:-:|:-:|:-:|:-:|
| 100x100 | 64 | 156 | 12 | 5.1 | 33.5 |
| 400x400 | 64 | 2500 | 50 | 20.5 | 133.9 |
| 800x800 | 64 | 10000 | 100 | 41.1 | 267.9 |

QR code reference: $\sigma_{95} \approx 18$ (B/W threshold at $\Delta L^* \approx 50$, 4 px/module).

At 400x400 with $\varepsilon = 2.3$, we exceed QR's noise tolerance while using 10-20x fewer visual elements.

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

| Config | Elements | bits/cell | $\sigma_{95,\text{eff}}$ (400x400) |
|:--|:-:|:-:|:-:|
| QR-L V2 (25x25) | 625 modules | 1 | 18 |
| Voronoi, N=16, $\varepsilon$=2.3, 50% ECC | 32 cells | 12 | 110 |
| Voronoi, N=16, $\varepsilon$=15, 50% ECC | 64 cells | 7 | 134 |
| Voronoi, N=16, $\varepsilon$=30, 50% ECC | 64 cells | 6 | 268 |

## Topological decoders (`spectral_decoder.py`)

Two rendering-agnostic decoders that use only color values, no spatial coordinates.

### Rips filtration decoder

Implements 0-dimensional persistent homology on colors projected onto $\gamma$:

1. Project each color onto $\gamma$ &rarr; arc-length value $s_i$
2. Build Vietoris-Rips filtration: grow connection radius, track $\beta_0$ (connected components)
3. **Persistence gap** separates within-word merges ($\varepsilon \approx 0$) from between-word merges ($\varepsilon \approx$ palette spacing)
4. Cut at the gap &rarr; connected components = palette word groups
5. Hungarian matching maps components to word indices

For clean data: persistence gap = 9.194 (exactly the palette spacing), 16 components detected, perfect round-trip.

The persistence gap is the **topological analogue of the eigengap** in spectral clustering. Both detect the natural cluster scale, but persistence does it via filtration (scanning all scales) rather than requiring a kernel bandwidth.

### Spectral decoder

Normalized graph Laplacian on the 1D arc-length similarity graph:

1. Project colors onto $\gamma$ &rarr; $s_i$ values
2. Gaussian similarity: $W_{ij} = \exp(-|s_i - s_j|^2 / \sigma^2)$, with $\sigma = 0.4 \times$ palette spacing
3. Normalized Laplacian: $L_{\text{sym}} = I - D^{-1/2} W D^{-1/2}$
4. Eigengap determines $k$ (number of distinct words)
5. K-means in spectral embedding &rarr; cluster assignments

In the continuous limit ($n \to \infty$), $L_{\text{sym}}$ converges to the weighted Laplace-Beltrami operator $\Delta_\rho$, whose eigenfunctions partition color space along low-density separators.

### Why project onto $\gamma$ first?

At $\varepsilon = 2.3$, constellation displacements (up to 83 CIELAB) exceed palette spacing (9.2 CIELAB). Colors from adjacent palette words overlap in raw CIELAB. Clustering in 3D fails.

But the displacements are **purely normal** to $\gamma$ (by construction). Projecting onto $\gamma$ collapses the normal component, leaving only the tangential coordinate where palette words are well-separated. The projection is the key insight: it transforms a 3D overlapping problem into a 1D well-separated one.

### Noise sweep results (per-pixel, no spatial averaging)

| $\sigma$ | Geometric | Rips | Spectral |
|:-:|:-:|:-:|:-:|
| 0 | 100% | 100% | 92.5% |
| 0.5 | 100% | 99.9% | 89.6% |
| 1.0 | 99.0% | 70.0% | 87.8% |
| 3.0 | 82.6% | 22.8% | 65.0% |

The geometric decoder wins per-pixel because it uses full 3D + constellation snapping. The Rips decoder is exact for clean data but fragile to noise (the 1D projection loses information). The spectral decoder is more noise-robust than Rips due to its global graph structure.

With spatial averaging, the topological decoders become dominant: averaging thousands of pixels per cell drives $\sigma_{\text{eff}} \to 0$, restoring the clean-data regime where Rips achieves 100%.

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

3. **Persistence gap = palette spacing**: the Rips filtration's largest gap in death times equals $L/(N-1)$, the arc-length spacing between adjacent palette colors. This is a geometric invariant of the encoding, independent of the specific payload.

4. **Spatial averaging multiplies noise tolerance by $\sqrt{P}$**: a cell with $P$ pixels has effective noise $\sigma/\sqrt{P}$, giving $\sigma_{95,\text{eff}} = (\varepsilon/5.6) \cdot \sqrt{P}$.

5. **Rendering is arbitrary**: information lives in the multiset of colors, not spatial layout. The topological decoders formalize this via the Vietoris-Rips complex on color space.
