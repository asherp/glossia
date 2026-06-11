#!/bin/bash
set -e

echo "==> Building Glossia WASM module..."
wasm-pack build --target web --no-default-features --features wasm

echo "==> Copying artifacts to web/..."
mkdir -p web
cp pkg/glossia_bg.wasm web/
cp pkg/glossia.js web/

# Aggressively optimize WASM size: -Oz (size first) plus strip the debug and
# name sections, which are large and unneeded in production. wasm-pack already
# runs its own wasm-opt pass, but this makes the size pass explicit and adds
# stripping. See issue #21.
if command -v wasm-opt &> /dev/null; then
    before=$(wc -c < web/glossia_bg.wasm)
    echo "==> Optimizing WASM with wasm-opt -Oz --strip-debug --strip-producers..."
    wasm-opt -Oz --strip-debug --strip-producers \
        web/glossia_bg.wasm -o web/glossia_bg.wasm
    after=$(wc -c < web/glossia_bg.wasm)
    echo "    glossia_bg.wasm: ${before} B -> ${after} B"
else
    echo "==> WARNING: wasm-opt not found on PATH; skipping size optimization."
    echo "    Install it via 'cargo install wasm-opt' or the binaryen package"
    echo "    to significantly shrink the deployed bundle."
    echo "    Current size: $(wc -c < web/glossia_bg.wasm) B"
fi

echo "==> Build complete."
echo "    Serve the web/ directory, e.g.:"
echo "    python3 -m http.server -d web 8080"
