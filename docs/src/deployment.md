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

## Limitations

- **Fork PRs are skipped.** A pull request from a fork uses a restricted
  `GITHUB_TOKEN` that cannot push to `gh-pages`, so no preview is built for it.
- Previews share the production domain under `/pr-preview/...`; they are not
  isolated environments.
