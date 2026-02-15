# Build-Time Language Index

## Overview

Glossia now automatically generates a language index at build time by scanning the `languages/` directory. This index maps language names to their YAML configuration files and eliminates the need to manually update code when adding new languages.

## How It Works

### Build Script (`build.rs`)

1. **Scans `languages/` directory** recursively for YAML files
2. **Identifies language directories** and their associated files:
   - `payload.yaml` - Word list with POS tags
   - `cover.yaml` - Cover words with POS tags
   - `grammar.yaml` - Grammar rules (type-driven)
   - `pos_mapping.yaml` - POS tag mappings
   - `dialect.yaml` - Dialect configuration
   - Other YAML files

3. **Generates Rust code** (`target/debug/build/glossia-*/out/language_index.rs`) with:
   - `get_embedded_yaml(path)` - Returns embedded file content
   - `has_embedded_files(language)` - Checks if language has embedded files
   - `get_available_languages()` - Returns list of available languages
   - `get_language_file_paths(language)` - Returns file paths for a language

### Generated Functions

The generated index provides:

```rust
// Get embedded YAML file content
pub fn get_embedded_yaml(path: &str) -> Option<&'static str>

// Check if language has embedded files
pub fn has_embedded_files(language: &str) -> bool

// Get list of available languages
pub fn get_available_languages() -> &'static [&'static str]

// Get file paths for a language
pub fn get_language_file_paths(language: &str) -> Option<LanguageFilePaths>
```

## Benefits

1. **Automatic Discovery**: New languages are automatically detected at build time
2. **No Manual Updates**: No need to modify `get_embedded_yaml()` or `has_embedded_files()` when adding languages
3. **Subdirectory Support**: Handles nested directories like `math/primes`
4. **Type Safety**: All paths are validated at compile time
5. **Better Error Messages**: Can list available languages when user provides invalid language name

## Adding a New Language

Simply create a directory in `languages/` with the required YAML files:

```
languages/
  my_language/
    payload.yaml    # Required
    cover.yaml      # Optional
    grammar.yaml    # Optional
    pos_mapping.yaml # Optional
```

The build script will automatically:
- Detect the new language
- Generate code to embed its files (in release builds)
- Make it available via the language index

## Example Usage

```rust
// Check if language has embedded files
if has_embedded_files("math/primes") {
    // Use embedded file
    let content = get_embedded_yaml("math/primes/payload.yaml");
}

// List available languages
for lang in get_available_languages() {
    println!("Available language: {}", lang);
}

// Get file paths for a language
if let Some(paths) = get_language_file_paths("math/primes") {
    println!("Payload: {:?}", paths.payload);
    println!("Cover: {:?}", paths.cover);
    println!("Grammar: {:?}", paths.grammar);
}
```

## Build Behavior

- **Debug builds**: Only `english` files are embedded (faster rebuilds)
- **Release builds**: All languages with `payload.yaml` are embedded

## File Detection

The build script detects languages by:
1. Scanning `languages/` directory recursively
2. Finding directories containing YAML files
3. Using the directory path as the language identifier:
   - `languages/english/` → `"english"`
   - `languages/math/primes/` → `"math/primes"`

## Generated Code Location

The generated index is written to:
```
target/{profile}/build/glossia-{hash}/out/language_index.rs
```

And included in the binary via:
```rust
mod language_index {
    include!(concat!(env!("OUT_DIR"), "/language_index.rs"));
}
```
