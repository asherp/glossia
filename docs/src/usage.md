# Usage

## Input Modes

Glossia accepts payload words in three ways:

| Mode | Flag | Description |
|------|------|-------------|
| Direct words | `glossia word1 word2 ...` | Provide BIP39 words as positional arguments |
| Random | `--random <N>` | Generate N random BIP39 words |
| ASCII encoding | `--from-ascii <text>` | Encode arbitrary text to wordlist words |

For `--from-ascii`, use `-` to read from stdin:

```bash
echo "my secret" | glossia --from-ascii -
```

## Decoding

Use `--decode` to convert payload words back to bytes (inverse of `--from-ascii`):

```bash
glossia --decode word1 word2 word3
```

Words can be provided as positional arguments or piped via stdin.

## Grammar Modes

The `--grammar` flag selects the sentence generation style:

| Grammar | Default Length Mode | Description |
|---------|-------------------|-------------|
| `body` (default) | `natural` | Longer sentences, natural prose. Best for email body text. |
| `subject` | `compact` | Short sentences, may include prefixes (Re:, Fwd:). Best for subject lines. |
| `payload_only` | N/A | Output only payload words, no cover words. |

### Length Modes

The `--length-mode` flag controls how sentence length is chosen:

- **`compact`**: Try lengths from `k_min` to `k_max`, shortest first. Produces the most concise output.
- **`natural`**: Sample length from the grammar's probability distribution. Produces more varied, natural-sounding text.

If not specified, the default depends on the grammar: `body` uses `natural`, `subject` uses `compact`.

## Highlighting

### Payload Highlighting

`--highlight-payload <mode>` marks payload words in the output:

| Mode | Effect |
|------|--------|
| `none` (default) | No highlighting |
| `bars` | Wraps payload words: `\|word\|` |
| Color name | ANSI color: `red`, `green`, `blue`, `yellow`, `magenta`, `cyan`, `white`, `black` |
| ANSI code | Numeric code (30-37) |

### Merkle Highlighting

`--highlight-merkle <mode>` highlights Merkle proof words separately (only with `--merkle`). Same options as `--highlight-payload`.

### Mad-lib Mode

`--madlib` replaces payload words with `[POS]` placeholders:

```bash
$ glossia --random 6 --seed 0 --madlib
```

This shows the grammatical structure without revealing the payload.

## Merkle Trees

`--merkle` wraps N payload words in a Merkle proof structure, expanding them to 2N-1 words:

```bash
$ glossia --random 6 --merkle --seed 0
```

`--demerkle` extracts the original payload from a merkleized sequence:

```bash
$ glossia --demerkle word1 word2 word3 ...
```

## Variations

`--variations <N>` generates N different embeddings and selects the most compact one. Prints statistics showing compactness scores:

```bash
$ glossia --random 12 --variations 3 --seed 0
```

## Language and Wordlist

- `--language, -l <lang>`: Language for grammar and wordlists. Default: `english`.
- `--wordlist <name>`: Wordlist to use. Options: `bip39` (default), `ngram`, `hp`.

## Tuning Parameters

| Flag | Default | Description |
|------|---------|-------------|
| `--k-min <N>` | 3 | Minimum sentence length in POS slots (including Dot) |
| `--k-max <N>` | 20 | Maximum sentence length in POS slots (including Dot) |
| `--width <N>` | 80 | Line wrapping width |
| `--delimiter <str>` | `" "` | Delimiter between words in `payload_only` mode |
| `--seed <N>` | random | Seed for deterministic generation |

## Other Flags

| Flag | Description |
|------|-------------|
| `--show-grammar` | Display grammar rules, then continue execution |
| `--verbose, -v` | Show detailed debugging information |
| `--help` | Show help message |

## Complete Command Reference

```
glossia [OPTIONS] [<word1> <word2> ... <wordN>]

Options:
  --random <N>              Generate N random payload words
  --from-ascii <text>       Encode ASCII text (use '-' for stdin)
  --decode                  Decode words back to bytes
  --grammar <grammar>       body (default), subject, or payload_only
  --length-mode <mode>      compact or natural
  --highlight-payload <mode>  none, bars, or color name/code
  --highlight-merkle <mode>   Merkle word highlighting
  --merkle                  Wrap payload in Merkle proof
  --demerkle                Extract payload from merkleized sequence
  --madlib                  Replace payload with [POS] placeholders
  --seed <N>                Deterministic seed
  --variations <N>          Generate N variations, pick most compact
  --language, -l <lang>     Language (default: english)
  --wordlist <name>         Wordlist: bip39, ngram, hp
  --k-min <N>               Min sentence length (default: 3)
  --k-max <N>               Max sentence length (default: 20)
  --width <N>               Line wrap width (default: 80)
  --delimiter <str>         Delimiter for payload_only mode
  --show-grammar            Display grammar rules
  --verbose, -v             Debug output
  --help                    Show help
```
