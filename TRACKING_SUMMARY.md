# File Tracking Summary for `cargo build --release`

## Analysis Date
Generated based on successful `cargo build --release` execution.

## ✅ Files That MUST Be Tracked

### Build Files
- ✅ `build.rs` - **UNTRACKED** - Build script that scans languages directory

### Source Files (Rust + Pest)
All of these are **UNTRACKED**:
- ✅ `src/lambda_parser.pest`
- ✅ `src/lambda_parser.rs`
- ✅ `src/lambda_terms.rs`
- ✅ `src/semantic_types.rs`
- ✅ `src/type_driven_grammar.rs`
- ✅ `src/generator/mod.rs`
- ✅ `src/generator/cache.rs`
- ✅ `src/generator/core.rs`
- ✅ `src/generator/data.rs`
- ✅ `src/generator/types.rs`
- ✅ `src/generator/utils.rs`

### Language YAML Files (Required by build.rs)
All of these are **UNTRACKED**:
- ✅ `languages/latin/payload.yaml`
- ✅ `languages/latin/cover.yaml`
- ✅ `languages/latin/grammar.yaml`
- ✅ `languages/latin/dialect.yaml`
- ✅ `languages/latin/pos_mapping.yaml`
- ✅ `languages/hp/hp.yaml`
- ✅ `languages/hp/hp_positions.yaml`
- ✅ `languages/math/primes/payload.yaml`
- ✅ `languages/math/primes/cover.yaml`
- ✅ `languages/math/primes/grammar.yaml`

### Language CFG Files
- ✅ `languages/latin/body.cfg` - **UNTRACKED** (used for Latin language)
- ✅ `languages/latin/subject.cfg` - **UNTRACKED** (used for Latin language)
- ⚠️ `languages/latin/payload_only.cfg` - **UNTRACKED** (may be used)

Note: English CFG files (`languages/english/subject.cfg`, `languages/english/body.cfg`) are already tracked and hardcoded in `src/grammar.rs`.

## 📝 Modified Files Status

### Already Staged (Ready to Commit)
- ✅ `.gitignore` - Modified
- ✅ `Cargo.toml` - Modified (also has unstaged changes)
- ✅ `languages/latin/wordlist.txt` - Modified
- ✅ `languages/latin/wordlist_POS.yaml` - Deleted (staged)

### Modified but Not Staged
These should be committed:
- ✅ `Cargo.toml` - Has additional unstaged changes
- ✅ `README.md` - Modified
- ✅ `src/bin/glossia.rs` - Modified
- ✅ `src/grammar.rs` - Modified
- ✅ `src/lib.rs` - Modified
- ✅ `src/types.rs` - Modified

## ❌ Files NOT Needed for Build

These can remain untracked (or be added to .gitignore):
- Python scripts (*.py) - Data generation tools
- Documentation (*.md) - Except README.md
- Cache directories (`.cache-glossia/`)
- Temporary files (root-level *.txt, *.yaml except language files)
- Environment files (`environment.yml`, `setup_latin_env.sh`)
- Root-level YAML files (`hp_latin_spells.yaml`, `hp_latin_wordlist.yaml`, `latin_wordlist.yaml`, `test_frequency.yaml`)

## 🚀 Quick Commands

### Add all required untracked files:
```bash
# Build script
git add build.rs

# Source files
git add src/lambda_parser.pest src/lambda_parser.rs src/lambda_terms.rs src/semantic_types.rs src/type_driven_grammar.rs
git add src/generator/

# Language files
git add languages/latin/*.yaml languages/latin/*.cfg
git add languages/hp/*.yaml
git add languages/math/primes/*.yaml
```

### Stage all modified files:
```bash
git add Cargo.toml README.md src/bin/glossia.rs src/grammar.rs src/lib.rs src/types.rs
```

### Verify build still works:
```bash
cargo build --release
```

## 📊 Summary

- **26 untracked files** need to be added for the build to work
- **6 modified files** should be staged and committed
- All files listed above are **required** for `cargo build --release` to succeed
