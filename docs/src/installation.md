# Installation

## From a Release Binary

Each release attaches a prebuilt `glossia` binary, which is the only install
path that does not want a Rust toolchain. Download the archive for your
platform from the
[releases page](https://github.com/asherp/glossia/releases/latest):

| Platform | Asset suffix |
|---|---|
| Linux x86_64 (glibc 2.35+) | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux x86_64 (static, any distro) | `x86_64-unknown-linux-musl.tar.gz` |
| macOS Apple silicon | `aarch64-apple-darwin.tar.gz` |
| macOS Intel | `x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc.zip` |

```bash
# Substitute the release you want for v0.6.0
name=glossia-v0.6.0-x86_64-unknown-linux-gnu
base=https://github.com/asherp/glossia/releases/download/v0.6.0
curl -LO "$base/$name.tar.gz" -LO "$base/$name.tar.gz.sha256"
sha256sum -c "$name.tar.gz.sha256"
tar xzf "$name.tar.gz"
sudo install "$name/glossia" /usr/local/bin/
```

Every archive ships a `.sha256` beside it, holding the same
`sha256sum`/`shasum -a 256` line format on all platforms.

Two platform notes:

- **macOS**: the binaries are unsigned, so Gatekeeper quarantines them on first
  run. `xattr -d com.apple.quarantine glossia` clears it.
- **Linux**: the glibc build is compiled on Ubuntu 22.04, so it runs on glibc
  2.35 and newer. On anything older — or on a musl distro such as Alpine — take
  the static musl build instead. The musl allocator is slower under
  allocation-heavy encoding, which is why both are published rather than only
  the portable one.

Language data (wordlists, grammars, prosody) is embedded in the binary, so a
single file is the whole install.

## From crates.io

```bash
cargo install glossia-cli
```

This installs the most recent published release. It is the shortest path if you
already have a Rust toolchain — no clone, no checkout. Note that the binary is
named `glossia`, while the crate providing it is `glossia-cli`.

## From Source

```bash
# Clone the repository
git clone https://github.com/asherp/glossia.git
cd glossia

# Install the binary (the CLI lives in the glossia-cli crate)
cargo install --path glossia-cli
```

This installs the `glossia` binary to `~/.cargo/bin/glossia` (or `$CARGO_HOME/bin/glossia` if set). Make sure this directory is in your `PATH`.

> **Note:** The repository is a Cargo workspace. The `glossia` crate at the
> root is the library; the `glossia` command-line binary is provided by the
> `glossia-cli` crate, and the developer tooling (POS tagging, weight
> validation, etc.) lives in `glossia-tools`.

## From Git

```bash
cargo install --git https://github.com/asherp/glossia.git glossia-cli
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
