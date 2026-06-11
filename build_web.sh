#!/bin/bash
set -e

echo "==> Building Glossia WASM module..."
# Scope the build to the glossia library package only. Without -p, wasm-pack's
# `cargo build` selects every default-member (glossia-cli, glossia-tools), and
# resolver v2 unifies their features onto glossia — pulling in the native-only
# nlprule dependency (and its C lib onig_sys, which cannot compile to wasm32).
wasm-pack build --target web --no-default-features --features wasm -- -p glossia

echo "==> Copying artifacts to web/..."
mkdir -p web
cp pkg/glossia_bg.wasm web/
cp pkg/glossia.js web/

# Optional: optimize WASM size if wasm-opt is available
if command -v wasm-opt &> /dev/null; then
    echo "==> Optimizing WASM with wasm-opt..."
    wasm-opt -Os web/glossia_bg.wasm -o web/glossia_bg.wasm
fi

echo "==> Build complete."
echo "    Serve the web/ directory, e.g.:"
echo "    python3 -m http.server -d web 8080"
