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

## Requirements

- Rust toolchain (1.70+)
- For POS tagging tools: nlprule binary data files (`en_tokenizer.bin`, `en_rules.bin`)
