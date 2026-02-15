# Files to Track for `cargo build --release`

## Summary

Based on analyzing the build process, here are the files that **must be tracked** for `cargo build --release` to work:

## ✅ REQUIRED FILES (Must Track)

### 1. Build Script
- `build.rs` - Scans languages directory and generates language index

### 2. Source Files (Rust)
- `src/lib.rs`
- `src/types.rs`
- `src/grammar.rs`
- `src/semantic_types.rs`
- `src/lambda_terms.rs`
- `src/lambda_parser.rs`
- `src/type_driven_grammar.rs`
- `src/bin/glossia.rs`
- `src/bin/compare_pos_weights.rs`
- `src/bin/get_top_words.rs`
- `src/bin/tag_words.rs`
- `src/bin/validate_pos_weights.rs`
- `src/generator/mod.rs`
- `src/generator/cache.rs`
- `src/generator/core.rs`
- `src/generator/data.rs`
- `src/generator/types.rs`
- `src/generator/utils.rs`

### 3. Pest Grammar Files
- `src/grammar_parser.pest` - Referenced in grammar.rs
- `src/lambda_parser.pest` - Referenced in lambda_parser.rs

### 4. Language YAML Files (scanned by build.rs)
- `languages/english/payload.yaml`
- `languages/english/cover.yaml`
- `languages/latin/payload.yaml`
- `languages/latin/cover.yaml`
- `languages/latin/grammar.yaml`
- `languages/latin/dialect.yaml`
- `languages/latin/pos_mapping.yaml`
- `languages/hp/hp.yaml`
- `languages/hp/hp_positions.yaml`
- `languages/math/primes/payload.yaml`
- `languages/math/primes/cover.yaml`
- `languages/math/primes/grammar.yaml`

### 5. Language CFG Files (directly included in source)
- `languages/english/subject.cfg` - Hardcoded in grammar.rs
- `languages/english/body.cfg` - Hardcoded in grammar.rs

### 6. Optional but Recommended (for Latin language support)
- `languages/latin/body.cfg` - Used if Latin language is selected
- `languages/latin/subject.cfg` - Used if Latin language is selected
- `languages/latin/payload_only.cfg` - May be used for Latin

## ❌ FILES NOT NEEDED FOR BUILD (Can ignore)

These files are **not required** for `cargo build --release`:

- Python scripts (*.py) - Used for data generation, not build
- Documentation (*.md) - Not needed for compilation
- Test/example files
- Cache directories (`.cache-glossia/`)
- Temporary files (*.txt files in root, except wordlists)
- Environment files (`environment.yml`, `setup_latin_env.sh`)
- Root-level YAML files (`hp_latin_spells.yaml`, `hp_latin_wordlist.yaml`, `latin_wordlist.yaml`, `test_frequency.yaml`)

## 📋 Quick Add Command

To add all required files:

```bash
# Add build script
git add build.rs

# Add all Rust source files
git add src/**/*.rs src/**/*.pest

# Add language YAML files
git add languages/**/*.yaml

# Add language CFG files (English required, Latin optional)
git add languages/english/*.cfg
git add languages/latin/*.cfg  # Optional but recommended
```

## 🔍 Verification

After adding files, verify the build still works:
```bash
cargo build --release
```
