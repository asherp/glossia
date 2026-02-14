#!/bin/bash
set -e

echo "==> Building Glossia WASM module..."
wasm-pack build --target web --no-default-features --features wasm

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
