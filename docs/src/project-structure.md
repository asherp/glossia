# Project Structure

## Directory Layout

```
glossia/
├── Cargo.toml              # Package configuration
├── build.rs                # Build script: language embedding & validation
├── CLAUDE.md               # Project instructions for Claude Code agents
│
├── src/
│   ├── lib.rs              # Library root: re-exports, GrammarChecker
│   ├── types.rs            # POS enum (13 variants), Sym enum
│   ├── semantic_types.rs   # Montague Grammar types: Entity, Truth, Function, Refined
│   ├── lambda_terms.rs     # Lambda calculus AST: Variable, Constant, Application, Abstraction
│   ├── lambda_parser.rs    # Pest-based parser for lambda expressions
│   ├── type_driven_grammar.rs  # Load grammar.yaml, apply type constraints
│   ├── grammar.rs          # CFG parsing from YAML, production rules, dialect overlay
│   ├── codec.rs            # Binary codec: encode/decode arbitrary data to/from words
│   ├── merkle.rs           # Merkle trees: WordlistTree, merkleize, parse, verify
│   │
│   ├── generator/          # Text generation engine
│   │   ├── mod.rs          # Public API exports
│   │   ├── types.rs        # PayloadTok, Lexicon, GenerationMode, SentenceLengthMode
│   │   ├── core.rs         # Main engine: generate_text, max_subsequence_embedding
│   │   ├── data.rs         # Wordlist loading: load_payload_words, load_cover_words_by_pos
│   │   ├── cache.rs        # SequenceCache: precomputed POS sequences for all k values
│   │   └── utils.rs        # Helpers: normalize_token, wrap_payload, word_wrap
│   │
│   └── bin/
│       ├── glossia.rs          # Main CLI binary
│       ├── get_top_words.rs    # Word frequency analysis tool
│       ├── tag_words.rs        # POS tagging using nlprule
│       ├── cleanup_weights.rs  # Remove zero-weighted productions
│       ├── compare_pos_weights.rs  # Compare weight distributions
│       └── validate_pos_weights.rs # Validate POS weight files
│
├── languages/
│   ├── english/
│   │   ├── grammar.yaml        # English grammar with Montague types
│   │   ├── cover.yaml          # Cover wordlist (BIP39-disjoint)
│   │   ├── payload_bip39.yaml  # BIP39 payload wordlist
│   │   ├── cover_ngram.yaml    # N-gram cover wordlist
│   │   ├── payload_ngram.yaml  # N-gram payload wordlist
│   │   └── payload_lemmas.yaml # WordNet lemma payload
│   ├── latin/
│   │   ├── grammar.yaml        # Latin grammar rules
│   │   ├── cover.yaml          # Latin cover words
│   │   ├── payload.yaml        # Latin payload (65,536 words)
│   │   ├── payload_hp.yaml     # Harry Potter themed payload
│   │   ├── dialect.yaml        # Dialect overlays
│   │   └── pos_mapping.yaml    # POS tag mapping
│   ├── meta/
│   │   ├── grammar.yaml        # Dialect calculus meta-grammar
│   │   ├── payload.yaml        # Dialect identifiers (language/format names)
│   │   └── cover.yaml          # Pipeline function words (verbs, preps, etc.)
│   ├── math/
│   │   └── primes/
│   │       ├── grammar.yaml    # Arithmetic grammar
│   │       ├── payload.yaml    # Prime numbers
│   │       └── cover.yaml      # Composite numbers
│   ├── hp/                     # Harry Potter language
│   ├── french/                 # French BIP39 (text only)
│   └── german/                 # German BIP39 (text only)
│
├── docs/                       # mdBook documentation
│   ├── book.toml
│   ├── src/                    # Markdown source
│   └── book/                   # Generated HTML
│
└── .claude/agents/             # Claude Code agent profiles
    ├── linguist.md
    ├── language-designer.md
    ├── cryptographer.md
    ├── rust-engineer.md
    ├── mathematician.md
    ├── physicist.md
    └── image-artist.md
```

## Core Modules

### Generator (`src/generator/`)

The text generation engine. Takes payload words and produces grammatically correct sentences with those words embedded in-order.

- **`core.rs`**: Main `generate_text()` function, maximum subsequence matching algorithm, sentence planning and slot filling
- **`types.rs`**: Key types including `PayloadTok` (tagged payload word), `Lexicon` (cover word store with POS indexing and refinement tags), `GenerationMode`, `SentenceLengthMode`
- **`data.rs`**: Loads wordlists from embedded YAML, builds POS mappings, tags words
- **`cache.rs`**: `SequenceCache` precomputes all valid POS sequences for lengths k_min to k_max
- **`utils.rs`**: Token normalization, payload highlighting, word wrapping

### Grammar (`src/grammar.rs`)

Loads YAML grammar files and expands CFG productions. Supports weighted alternatives, refinement tags (`Det[def]`, `Det[indef]`), and dialect overlay (base rules + dialect-specific overrides).

### Type System (`src/semantic_types.rs`, `src/lambda_terms.rs`)

Montague Grammar types and lambda calculus terms. Each POS tag maps to a semantic type (e.g., `N` -> `e -> t`), and grammar rules carry lambda terms for type-driven composition.

### Codec (`src/codec.rs`)

Binary encoding/decoding of arbitrary data to wordlist words. Uses bit-packing: each word represents `log2(wordlist_size)` bits. A padding word at the start indicates trailing zero bits.

### Merkle (`src/merkle.rs`)

Merkle tree construction over payload word sequences. `merkleize()` wraps N payload words into a 2N-1 word sequence with cover words as internal nodes. `parse_merkleized()` extracts the original payload. `verify_merkleized()` provides cryptographic verification.

## Build System (`build.rs`)

The build script:

1. Scans `languages/` for all YAML files
2. Validates cover/payload disjointness for each language
3. Generates `language_index.rs` with `include_str!()` for all files
4. In debug mode, only embeds English (faster iteration)
5. Provides `has_embedded_files()` function for runtime language detection

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `rand` | 0.8 | Random selection of cover words and productions |
| `nlprule` | 0.6 | NLP and POS tagging (for tools) |
| `clap` | 4.4 | CLI argument parsing |
| `pest` / `pest_derive` | 2.7 | Parser generator (lambda expressions) |
| `serde` / `serde_yaml` / `serde_json` | 1.0 / 0.9 / 1.0 | Serialization |
| `moniker` | 0.5 | Lambda calculus variable binding |
| `bincode` | 1.3 | Binary encoding |
| `rayon` | 1.11 | Parallel processing |
| `reqwest` | 0.11 | HTTP (for downloading frequency data) |
| `regex` | 1.10 | Pattern matching |
| `flate2` | 1.0 | Gzip compression |
| `csv` | 1.3 | CSV parsing |
| `anyhow` | 1.0 | Error handling |
