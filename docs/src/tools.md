# Designing Your Own Glossia

To construct a custom language for Glossia, you will need three essential components:

1. **A payload wordlist** with weighted parts of speech
2. **A cover wordlist** with weighted parts of speech
3. **A grammar file** (YAML with CFG productions and Montague types)

The grammar must take any payload sequence and return a sequence of cover words plus grammar words that **preserves the order of the payload**. This ensures payload words appear in the same order in the generated text as in the input. The grammar is deterministic but may accept additional parameters (such as seeds) to guide output variety.

## Important Considerations

- **Decoding requirement**: Others need to decode your messages! They only need the payload wordlist. This is why Glossia includes BIP39 wordlists by default.
- **No overlap**: The cover wordlist must **never** contain any words from the payload wordlist. This is enforced at compile time.
- **POS weights**: Words can have multiple parts of speech with different probabilities.
- **Append-only wordlists**: Both payload and cover wordlists are **append-only**:
  - You can **add** new words to the end
  - You **cannot** replace or reorder existing words
  - Glossia encodes data by mapping words to their index in the wordlist; changing order breaks decoding

## 1. Payload Wordlist

The payload wordlist is a YAML file mapping words to their POS tag weights:

```yaml
abandon:
  V: 0.6
  N: 0.4

ability:
  N: 1.0

able:
  Adj: 1.0

about:
  Prep: 0.6
  Adv: 0.4
```

**Key points:**
- Weights should sum to 1.0 per word (Glossia normalizes if needed)
- Valid POS tags: `N`, `V`, `Adj`, `Adv`, `Prep`, `Det`, `Modal`, `Aux`, `Cop`, `To`, `Conj`
- **Frequency ordering**: Custom payload wordlists should be ordered by frequency (most frequent first). BIP39 is frozen in alphabetical order.

## 2. Cover Wordlist

Same YAML format as the payload wordlist, but for words that fill non-payload slots:

```yaml
the:
  Det: 1.0
  refinement: def

a:
  Det: 1.0
  refinement: indef

is:
  Cop: 1.0

after:
  Prep: 0.6
  Adv: 0.4
```

**Critical requirements:**
- **Must NOT overlap** with the payload wordlist (compile-time enforced)
- Must include function words (determiners, prepositions, conjunctions, copulas, etc.)
- Optional `refinement` tags enable grammar-driven morphology (e.g., `Det[def]` selects "the", `Det[indef]` selects "a")
- **Frequency ordering**: Order by frequency (most frequent first), especially for Merkle mode where the first cover word becomes the Merkle root

## 3. Grammar File

Grammar files are YAML with Montague Grammar types and CFG productions:

```yaml
grammar:
  name: "My Language"

  types:
    - name: "N"
      lambda_type: "e -> t"
    - name: "V"
      lambda_type: "e -> (e -> t)"
    - name: "Adj"
      lambda_type: "e -> e"
    - name: "Det"
      lambda_type: "(e -> t) -> (e -> t)"
    - name: "Cop"
      lambda_type: "(e -> t) -> (e -> t)"
    - name: "Dot"
      lambda_type: "t"

  rules:
    sentence:
      lambda: "N"
      cfg_productions:
        - production: "NP VP Dot"
          weight: 0.85
          lambda: "N"
        - production: "NP V NP Dot"
          weight: 0.15
          lambda: "N"

    noun_phrase:
      lambda: "N"
      cfg_productions:
        - production: "N"
          weight: 0.40
          lambda: "N"
        - production: "Det[def] N"
          weight: 0.35
          lambda: "Det"
        - production: "Det[indef] N"
          weight: 0.25
          lambda: "Det"

    verb_phrase:
      lambda: "N"
      cfg_productions:
        - production: "V NP"
          weight: 0.50
          lambda: "N"
        - production: "Cop Adj"
          weight: 0.30
          lambda: "N"
        - production: "Modal V NP"
          weight: 0.20
          lambda: "N"
```

**Key syntax:**
- Terminal symbols are POS tags: `Det`, `Adj`, `N`, `V`, `Modal`, `Aux`, `Cop`, `To`, `Prep`, `Adv`, `Conj`, `Dot`, `Prefix`
- Refinement tags in brackets: `Det[def]`, `Det[indef]`, `Det[quant]`
- Weights determine probabilistic selection
- Rule names map to abbreviations: `noun_phrase` -> `NP`, `verb_phrase` -> `VP`, `sentence` -> `S`

**Optional dialect overlay** (`dialect.yaml`):
```yaml
dialects:
  subject:
    rules:
      sentence:
        cfg_productions:
          - production: "NP Cop Adj Dot"
            weight: 1.0
```

Test your grammar:
```bash
glossia --show-grammar --dialect body
```

## Complete Workflow

1. **Create language directory:**
   ```bash
   mkdir -p languages/my_language
   ```

2. **Create payload wordlist:**
   - Start with your word list
   - Tag with POS: `cargo run --bin tag_words -- -i words.txt -o words_POS.txt`
   - Generate weights: `cargo run --bin validate_pos_weights -- --file words.yaml --output payload.yaml`

3. **Create cover wordlist:**
   - Generate common words: `cargo run --bin get_top_words -- -n 1000 --download-coca -o cover_words.txt`
   - Tag with POS: `cargo run --bin tag_words -- -i cover_words.txt -o cover_POS.txt`
   - Generate weights: `cargo run --bin validate_pos_weights -- --file cover.yaml --output cover.yaml`
   - Overlap is validated automatically at compile time

4. **Create grammar file:**
   - Write `grammar.yaml` with types and rules
   - Optionally add `dialect.yaml` for subject/body variants
   - Test with `glossia --show-grammar --dialect body`

5. **Test your language:**
   ```bash
   glossia --random 12 --language my_language --seed 0
   ```

6. **Verify decoding:**
   - Extract all words from output
   - Filter to only words in your payload wordlist
   - Verify the original payload is preserved in order

## File Structure

Your language directory should look like:
```
languages/my_language/
├── grammar.yaml          # Grammar rules with Montague types
├── payload.yaml          # Payload wordlist with POS weights
├── cover.yaml            # Cover wordlist with POS weights
└── dialect.yaml          # Optional: dialect overlays (subject/body/poetry)
```

# Tools

Glossia includes several utility tools for creating and validating language files.

## Word Frequency Tool

Generate word lists from frequency data:

```bash
# Download and use COCA word frequency data (recommended)
cargo run --bin get_top_words -- -n 1000 --download-coca -o output.txt

# Re-running reuses the cached download (use --force-download to refresh)
cargo run --bin get_top_words -- -n 1000 --download-coca -o output.txt

# Use a local wordfrequency.info format file
cargo run --bin get_top_words -- -n 1000 --wordfreq lemmas_60k.txt -o output.txt

# Use a CSV frequency file
cargo run --bin get_top_words -- -n 1000 --csv word-freq.csv -o output.txt
```

The `get_top_words` tool:
- Downloads word frequency data from [wordfrequency.info](https://www.wordfrequency.info/samples.asp) (COCA corpus)
- Caches the download locally (reuses for 7 days)
- Filters for words with 6 or fewer characters (no punctuation)
- Extracts POS tags when available
- Sorts by frequency and returns the top N most common words

## POS Tagging Tool

Assign parts of speech to words using nlprule:

```bash
# Tag a word list (one word per line)
cargo run --bin tag_words -- -i input_words.txt -o output_POS.txt

# Use alternative (faster) tagging method
cargo run --bin tag_words -- -i input_words.txt -o output_POS.txt --alternative
```

## POS Weight Generation Tool

Generate POS tag weights from nlprule analysis:

```bash
# Generate weights for cover words
cargo run --bin validate_pos_weights -- \
  --file languages/english/cover.yaml \
  --output cover_nlprule_weights.yaml

# Generate weights for payload words
cargo run --bin validate_pos_weights -- \
  --file languages/english/payload.yaml \
  --output payload_nlprule_weights.yaml

# Test with limited words (for faster testing)
cargo run --bin validate_pos_weights -- \
  --file languages/english/cover.yaml \
  --max-words 10 \
  --output test_weights.yaml

# Adjust minimum weight threshold and decimal places
cargo run --bin validate_pos_weights -- \
  --file languages/english/cover.yaml \
  --threshold 0.05 \
  --round 2 \
  --output cover_weights.yaml
```

The weight generation tool:
- Tests each word in multiple sentence contexts using nlprule
- Calculates observed POS tag frequencies
- Normalizes weights to sum to 1.0
- Filters out weights below a threshold (default: 0.01)
- Rounds weights to specified decimal places (default: 3)

## Weight Cleanup Tool

Remove zero-weighted productions from grammar files:

```bash
cargo run --bin cleanup_weights -- --file grammar.yaml --output grammar_clean.yaml
```

## Weight Comparison Tool

Compare POS weight distributions between two wordlist files:

```bash
cargo run --bin compare_pos_weights -- \
  --file1 payload_old.yaml \
  --file2 payload_new.yaml
```

## Workflow: Generate Short Words and Tag Them

A common workflow for building cover wordlists:

```bash
# Step 1: Generate shortest words (3-4 characters) from frequency data
cargo run --bin get_top_words -- \
  -n 500 \
  --download-coca \
  --min-length 3 \
  --max-length 4 \
  --words-only \
  -o shortest_words.txt

# Step 2: Tag those words with POS using nlprule
cargo run --bin tag_words -- \
  -i shortest_words.txt \
  -o shortest_words_POS.txt
```

## Data Sources

The `get_top_words` tool uses word frequency data from:
- **COCA (Corpus of Contemporary American English)**: Available from [wordfrequency.info](https://www.wordfrequency.info/samples.asp)
- **Google Books Ngram**: Can process downloaded Ngram 1-gram files

Downloaded data is cached locally as `lemmas_60k.txt` and reused for 7 days before automatically refreshing.
