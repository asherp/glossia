#!/bin/bash
set -e

echo "==> Building Glossia WASM module..."
wasm-pack build --target web --no-default-features --features wasm

echo "==> Copying artifacts to web/..."
mkdir -p web
cp pkg/glossia_bg.wasm web/
cp pkg/glossia.js web/

# Size optimization (wasm-opt -Oz + strip) is configured under
# [package.metadata.wasm-pack.profile.release] in Cargo.toml and run by
# wasm-pack itself using its bundled wasm-opt, so it always happens here.
# See issue #21. Report the final size for visibility.
echo "==> glossia_bg.wasm: $(wc -c < web/glossia_bg.wasm) B"

echo "==> Build complete."
echo "    Serve the web/ directory, e.g.:"
echo "    python3 -m http.server -d web 8080"
