# Social cards (Open Graph / Twitter)

Source for the link-preview images used by the `og:image` / `twitter:image`
meta tags across the site.

| Template        | Output               | Used by                        |
|-----------------|----------------------|--------------------------------|
| `general.html`  | `../og-glossia.png`  | `index.html`, `compose.html`   |
| `bulletin.html` | `../og-bulletin.png` | `bulletin.html`                |

## Regenerate

Edit the `*.html` templates, then render the 1200×630 PNGs with Chromium:

```sh
node web/og-cards/render.mjs   # requires Playwright (npm i -g playwright)
```

The templates use only system fonts (DejaVu Serif / Sans / Mono) so they render
identically without a network fetch. Commit the regenerated PNGs — they ship to
the site root via the Deploy Web workflow and are referenced by absolute
`https://glossia.io/...` URLs.
