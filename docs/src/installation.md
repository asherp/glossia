# Installation

## From Source

```bash
# Clone the repository
git clone https://github.com/asherp/glossia.git
cd glossia

# Install the binary
cargo install --path .
```

This installs the `glossia` binary to `~/.cargo/bin/glossia` (or `$CARGO_HOME/bin/glossia` if set). Make sure this directory is in your `PATH`.

## From Git

```bash
cargo install --git https://github.com/asherp/glossia.git
```

## Verify Installation

```bash
glossia --help
```

## What Gets Installed

The English language files (wordlists, grammars, and type definitions) are **embedded in the binary** at compile time, so no additional files need to be copied. Other languages with YAML files in the `languages/` directory are also embedded in release builds.

In debug builds, only English is embedded for faster compile times.

## WebAssembly (Browser)

Glossia can be compiled to WebAssembly and run entirely in the browser.

### Prerequisites

- Rust toolchain (1.70+)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
- (Optional) [binaryen](https://github.com/WebAssembly/binaryen) for `wasm-opt` size optimization

### Build

```bash
./build_web.sh
```

This runs `wasm-pack build --target web --no-default-features --features wasm`, copies the resulting `glossia.js` and `glossia_bg.wasm` into the `web/` directory, and optionally runs `wasm-opt -Os` if available.

### Serve Locally

```bash
python3 -m http.server -d web 8080
```

Then open `http://localhost:8080/index.html`.

### Manual Build

If you prefer to run the steps yourself:

```bash
wasm-pack build --target web --no-default-features --features wasm
mkdir -p web
cp pkg/glossia_bg.wasm web/
cp pkg/glossia.js web/
```

## Requirements

- Rust toolchain (1.70+)
- For POS tagging tools: nlprule binary data files (`en_tokenizer.bin`, `en_rules.bin`)
- For WASM builds: [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
