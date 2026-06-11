# Deployment & CI

Glossia uses GitHub Actions for continuous integration and for deploying the
WASM web app to GitHub Pages.

## Workflows

| Workflow | File | Trigger | Purpose |
|----------|------|---------|---------|
| CI | `.github/workflows/ci.yml` | every PR, push to `master` | `cargo build` + `cargo test` |
| Deploy Web App | `.github/workflows/deploy-web.yml` | push to `master` (web/src/Cargo/languages paths) + manual | Build WASM, deploy production site |
| PR Preview | `.github/workflows/pr-preview.yml` | PR opened/updated/reopened/closed | Build WASM, deploy a per-PR preview |

## GitHub Pages source

Both the production deploy and PR previews publish to the **`gh-pages`
branch** (production at the root, previews under `pr-preview/pr-<N>/`). Pages
can only serve from a single source, so they must share one branch.

> **One-time setup:** In the repository settings under **Settings → Pages**,
> set the **Source** to **Deploy from a branch** and choose the **`gh-pages`**
> branch (folder `/ (root)`). The `gh-pages` branch is created automatically by
> the first production deploy after this change.
>
> The custom domain (`glossia.io`) is preserved via the `CNAME` file in
> `web/`, which the production deploy publishes to the root of `gh-pages`.

## How previews work

On every push to a (non-fork) pull request, the **PR Preview** workflow builds
the WASM bundle and deploys `web/` into `pr-preview/pr-<N>/` on the `gh-pages`
branch using [`rossjrw/pr-preview-action`](https://github.com/rossjrw/pr-preview-action).
The action posts a comment on the PR with the live URL, e.g.
`https://glossia.io/pr-preview/pr-42/`. When the PR is closed or merged, the
preview directory is removed automatically.

Production deploys use
[`JamesIves/github-pages-deploy-action`](https://github.com/JamesIves/github-pages-deploy-action)
with `clean-exclude: pr-preview/`, so publishing a new production build never
wipes the live previews of open PRs.

The web app loads its WASM via relative paths (`./glossia.js`, with `init()`
resolving the `.wasm` relative to the module URL), so it works correctly when
served from a subpath.

## WASM bundle size

The web app ships as a single `glossia_bg.wasm` module, so keeping it small
matters for first-load time. The bundle is dominated not by code but by the
language data embedded at compile time.

### What dominates the bundle

`build.rs` embeds every `languages/**/*.yaml` file into release builds via
`include_str!`, plus a precomputed sorted word-index (`.txt`) for each payload
wordlist. The English wordlists dwarf everything else:

| Embedded file | Size | Used by web app? |
|---|---|---|
| `english/payload_lemmas.yaml` | ~4.1 MB | No — not referenced by any dialect |
| `english/payload_ngram.yaml` | ~3.5 MB | No |
| `english/cover_ngram.yaml` | ~3.5 MB | No |
| `english/wordnet_lemmas.yaml` | ~2.2 MB | No — WordNet source data |
| All other languages combined | ~1.7 MB | Yes |

Together those four English files are ~13.3 MB of the ~15 MB of embedded YAML —
and each also contributes a multi-megabyte word-index `.txt`. They back the
high-density `lemmas`/`ngram` encodings (2¹⁷-word lists) available in the CLI,
but **no grammar dialect references them**, so the web app never exposes them.

### Levers applied (issue #21)

1. **Trim embedded languages in wasm builds.** `build.rs` detects the wasm
   target (`CARGO_CFG_TARGET_ARCH == "wasm32"`) and excludes the large unused
   English profiles (`is_excluded_in_wasm`) from every generated lookup —
   embedded YAML, wordlist profiles, payload word index, and word counts — so
   the runtime view stays self-consistent. Native (CLI) builds are unchanged
   and keep all wordlists. This is the single largest win: the raw
   `cargo build` wasm lib drops from **~20.7 MB → ~3.8 MB**.
2. **Aggressive size optimisation.** `wasm-opt -Oz --strip-debug
   --strip-producers` is configured under
   `[package.metadata.wasm-pack.profile.release]` in `Cargo.toml`, so wasm-pack
   always runs it using its own bundled wasm-opt. (Installing a *system*
   `wasm-opt` is intentionally avoided: an older system binary gets picked up by
   wasm-pack and rejects the bulk-memory ops modern rustc emits.)
3. **Size-tuned release profile.** `[profile.release]` enables `lto`,
   `codegen-units = 1`, and `strip`, letting the linker drop dead code and
   symbol info from both native and wasm builds.

To re-measure after changes, build the wasm lib and inspect it:

```bash
cargo build --release --target wasm32-unknown-unknown \
  --no-default-features --features wasm
ls -l target/wasm32-unknown-unknown/release/glossia.wasm
# Section-level breakdown (if tooling is installed):
#   wasm-objdump -h glossia_bg.wasm
#   twiggy top    glossia_bg.wasm
```

## Limitations

- **Fork PRs are skipped.** A pull request from a fork uses a restricted
  `GITHUB_TOKEN` that cannot push to `gh-pages`, so no preview is built for it.
- Previews share the production domain under `/pr-preview/...`; they are not
  isolated environments.
