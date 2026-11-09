# Language Support

Glossia supports multiple languages. Each language provides its own wordlists, grammar rules, and type definitions.

## Supported Languages

| Language | Wordlists | Grammar | Status |
|----------|-----------|---------|--------|
| **English** | BIP39, n-gram, WordNet lemmas | Full YAML grammar with dialects | Fully implemented |
| **Latin** | Custom (65,536 words), Harry Potter | Full YAML grammar with dialects | Fully implemented |
| **Czech** | BIP39 (official, 2,048 words) | Full YAML grammar (body/subject/prose) | Fully implemented |
| **Korean** | Corpus (16,384 words), BIP39 (official, 2,048 words) | Full YAML grammar (body/subject/prose), attached particles | Fully implemented |
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

# Czech (official BIP39 wordlist)
glossia --random 12 --language czech --seed 0

# Korean (default: 16,384-word corpus list, 14 bits/word)
glossia --random 12 --language korean --seed 0

# Korean with the official BIP39 wordlist
glossia --random 12 --language korean --wordlist bip39 --seed 0
```

### Korean particles

Korean writes case and topic particles onto the noun before them, so a payload
word reaches the reader as 가격을 or 가이드는 rather than as a bare word. The
grammar places each particle as a cover `Prep` word tagged with its function
(`subj`, `top`, `obj`, `loc`, `com`, `gen`, `dir`), and the `korean` morphology
pass (`src/generator/agreement.rs`) picks the allomorph the noun's last syllable
calls for (이/가, 은/는, 을/를, 과/와, 으로/로) and attaches it.

Decoding detaches it again (`codec::payload_form`): a token that is not a payload
word as written is tried with one trailing particle removed. That is only safe
because no payload word plus a particle is another payload word, and no cover
noun plus a particle is a payload word (길 + 이 would be 길이, so 길 is not a cover
word). `tests/korean.rs` pins both properties over the whole wordlist.

Hangul is compared in NFC. The official BIP39 file is NFD; decoding composes
conjoining jamo before lookup (`codec::compose_hangul`), so prose pasted from a
system that decomposes it still decodes.

### Korean corpus wordlist

Korean's default wordlist (`payload.yaml`) is 2¹⁴ = 16,384 words drawn
from the Shiberu Korean Word Frequency List 2026 (CC BY 4.0), the way Latin's
list was drawn from Wiktionary. At 14 bits per word it needs 19 words for 256
bits where BIP39 Korean needs 24, and every word appears in the corpus at least
about once per million words. The official BIP39 Korean list stays available as
`--wordlist bip39` (`payload_bip39.yaml`), since only it reads as a BIP39
mnemonic.

The list keeps, in frequency order, single dictionary words (common nouns,
adverbs, prenouns) that Kiwi's lexicon knows, two to five syllables long, and
only if no particle form of the word collides with another payload word or a
cover word. `languages/korean/generate_corpus.py` regenerates it from the
pinned release; `experiments/korean_corpus/` measures why 2¹⁴ is the largest
size the corpus supports cleanly.

## Language File Structure

Each language lives in `languages/{language}/` and requires:

```
languages/my_language/
├── grammar.yaml          # Grammar rules with Montague types
├── payload.yaml          # Payload wordlist with POS weights
├── cover.yaml            # Cover wordlist with POS weights
└── dialect.yaml          # Optional: dialect overlays (subject/body/poetry)
```

In addition, the language must be registered in the shared meta-grammar layer:

```
languages/meta/
├── payload.yaml          # Append language name here as a dialect identifier
├── cover.yaml            # Append new pipeline function words here (if needed)
└── grammar.yaml          # Dialect calculus rules (usually no changes needed)
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

See the [Tools](./tools.md) chapter for the complete workflow. The process has two layers: creating the language itself, and registering it in the meta-grammar so that pipeline instructions can reference it.

### Language Files

1. Creating payload and cover wordlists with POS tagging
2. Writing grammar rules in YAML format
3. Testing and validating your language

### Meta-Grammar Registration

Every new language must also be registered in the **meta-grammar** (`languages/meta/`), which is Glossia's [Dialect Calculus](./dialect-calculus.md) layer. This enables pipeline instructions like `"translate from english into my_language"`.

1. **Append the language name to `languages/meta/payload.yaml`** with appropriate POS weights (typically `N`, `Adj`, `Pron`). The meta payload must remain power-of-2 sized.
2. **Check `languages/meta/cover.yaml`** for any new pipeline function words your language may need (new verbs, prepositions, etc.). Append them if not already present.
3. **Add a match arm in `src/pipeline.rs`** in `meta_word_to_language()` to map the meta payload word to your language directory and default dialect.

### Key Requirements

- **Disjoint wordlists**: Payload and cover words must never overlap (enforced at compile time)
- **Append-only**: Both wordlists are append-only for backward compatibility
- **POS coverage**: Cover wordlist must include words for all function-word POS tags (Aux, Cop, To, Prefix, Dot)
- **Frequency ordering**: Cover words should be ordered by frequency (most frequent first), especially for Merkle mode
- **Meta-grammar registration**: The language name must appear in `languages/meta/payload.yaml` and `src/pipeline.rs` for pipeline routing to work

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
