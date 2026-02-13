# Language Support

Glossia supports multiple languages. Each language provides its own wordlists, grammar rules, and type definitions.

## Supported Languages

| Language | Wordlists | Grammar | Status |
|----------|-----------|---------|--------|
| **English** | BIP39, n-gram, WordNet lemmas | Full YAML grammar with dialects | Fully implemented |
| **Latin** | Custom (65,536 words), Harry Potter | Full YAML grammar with dialects | Fully implemented |
| **Math/Primes** | Prime numbers (payload), composites (cover) | Arithmetic grammar | Experimental |
| **Harry Potter** | HP spell vocabulary | Custom | Experimental |
| **French** | BIP39 (text only) | Not yet | Planned |
| **German** | BIP39 (text only) | Not yet | Planned |

## Using Languages

```bash
# Default: English with BIP39 wordlist
glossia --random 12 --seed 0

# English with n-gram wordlist
glossia --random 12 --wordlist ngram --seed 0

# Specify language explicitly
glossia --random 12 --language english --seed 0
```

## Language File Structure

Each language lives in `languages/{language}/` and requires:

```
languages/my_language/
├── grammar.yaml          # Grammar rules with Montague types
├── payload.yaml          # Payload wordlist with POS weights
├── cover.yaml            # Cover wordlist with POS weights
└── dialect.yaml          # Optional: dialect overlays (subject/body/poetry)
```

### Wordlist Format

Wordlists are YAML files mapping words to POS tag weights:

```yaml
abandon:
  V: 0.6
  N: 0.4

ability:
  N: 1.0

able:
  Adj: 1.0
```

Words can have multiple POS tags with weights that sum to 1.0. Optional refinement tags can constrain cover word selection:

```yaml
the:
  Det: 1.0
  refinement: def

a:
  Det: 1.0
  refinement: indef
```

### Naming Conventions

Multiple wordlists per language are supported:

- `payload.yaml` / `cover.yaml` - default pair
- `payload_bip39.yaml` / `cover.yaml` - BIP39 payload with general cover
- `payload_ngram.yaml` / `cover_ngram.yaml` - n-gram based pair
- `payload_hp.yaml` - Harry Potter themed payload

The build system automatically pairs `payload_X.yaml` with `cover_X.yaml` (or `cover.yaml` as fallback) for disjointness validation.

## Compile-Time Validation

The build system (`build.rs`) performs several checks at compile time:

1. **Scans** all `languages/` directories for YAML files
2. **Pairs** payload and cover wordlists
3. **Validates disjointness** - panics if any word appears in both payload and cover
4. **Embeds** all YAML files as `include_str!()` in the binary

In debug builds, only English is embedded for faster compilation.

## Creating a New Language

See the [Tools](./tools.md) chapter for the complete workflow, including:

1. Creating payload and cover wordlists with POS tagging
2. Writing grammar rules in YAML format
3. Testing and validating your language

### Key Requirements

- **Disjoint wordlists**: Payload and cover words must never overlap (enforced at compile time)
- **Append-only**: Both wordlists are append-only for backward compatibility
- **POS coverage**: Cover wordlist must include words for all function-word POS tags (Aux, Cop, To, Prefix, Dot)
- **Frequency ordering**: Cover words should be ordered by frequency (most frequent first), especially for Merkle mode

## English Wordlists

The English language includes several wordlist options:

| Wordlist | Size | Bits/Word | Use Case |
|----------|------|-----------|----------|
| BIP39 | 2,048 | 11 | Standard cryptocurrency seed phrases |
| N-gram | Large | Variable | General English text encoding |
| WordNet lemmas | Large | Variable | Comprehensive English vocabulary |

## Latin Vocabulary

The Latin language provides 2^16 = 65,536 words (16 bits/word), enabling more compact encodings:

- A 12-word BIP39 seed phrase (132 bits) can be represented with just **9 Latin words** (132 / 16 = 8.25, rounded up to 9)
- This demonstrates how larger wordlists enable more compact encodings
