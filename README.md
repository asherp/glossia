# Glossia

A lossless linguistic codec for machine-to-human communication. Glossia generates natural-looking sentences using a context-free grammar (CFG), while embedding BIP39 mnemonic words in-order within the generated text.

## Overview

This tool uses a small, controlled grammar to generate sentences that embed BIP39 words in their natural positions based on part-of-speech (POS) tags. The cover lexicon (non-payload words) is carefully constructed to avoid any BIP39 words, making decoding trivial: simply filter out all words that are not in the BIP39 word list.

## A readable encoding, not steganography

Glossia is **not** a steganographic tool. Its goal is not to hide that data is present, but to make machine data *human-friendly* — something a person can read, speak, transcribe, and verify. This distinction drives the design:

- Steganography pays for concealment by spending nearly all of its capacity on cover text, typically netting **well under 1 bit per symbol**.
- Glossia hides nothing, so every payload word carries its full information content — **~11 bits** with the 2048-word BIP39 list, up to **~15 bits** with larger wordlists.

The payload is meant to be *seen and decoded*. The mission is to make ciphertext and other machine data — keys, signatures, hashes, mnemonics — human-friendly in a way existing formats (hex, Base64, fingerprints, ASCII armor) do not, **without sacrificing too much security**. When iterating on Glossia, optimize for legibility and density, not concealment.

## Features

- **CFG-based sentence generation**: Uses a small context-free grammar to generate grammatically correct sentences
- **POS-tagged payload embedding**: Embeds BIP39 words based on their part-of-speech tags
- **SFW cover lexicon**: Uses a safe-for-work lexicon that excludes all BIP39 words
- **Adaptive sentence length**: Adjusts sentence complexity based on remaining payload tokens
- **Word frequency analysis**: Tool to generate word lists from frequency data with POS tags

## Installation

### Install from Source

```bash
# Clone the repository
git clone https://github.com/asherp/glossia.git
cd glossia

# Install the binary (the CLI lives in the glossia-cli crate)
cargo install --path glossia-cli
```

This will install the `glossia` binary to `~/.cargo/bin/glossia` (or `$CARGO_HOME/bin/glossia` if set). Make sure this directory is in your `PATH`.

**Note:** The English language files (wordlists and grammars) are embedded in the binary, so no additional files need to be copied. For other languages (French, German), you'll need to provide the language files separately.

### Install from Git

```bash
cargo install --git https://github.com/asherp/glossia.git glossia-cli
```

### Verify Installation

```bash
glossia --help
```

## Usage

### Defaults and Use Cases

Glossia defaults to **body grammar with natural length-mode**, which is optimized for email body text and prose. The defaults are designed for different use cases:

- **Body grammar + natural mode (default)**: Best for email body text and prose. Generates longer, more natural sentences by sampling from the grammar's length distribution. This produces readable text that flows naturally.

- **Subject grammar + compact mode**: Best for email subject lines where brevity is critical. Generates short phrases and may include prefixes (Re:, Fwd:, etc.). Uses compact mode to find the shortest possible sentence that fits the payload.

You can override these defaults using `--dialect` and `--length-mode` flags as needed.

### Main Program

#### Basic Usage

**Default: body grammar + natural length-mode**

```bash
$ glossia --random 12 --seed 0
```

```
Snake set robot to the mixed ship. Each program is night to each ten shield.
Bamboo own way to the yellow.
```

The embedded words are: `snake`, `robot`, `mixed`, `ship`, `program`, `night`, `ten`, `shield`, `bamboo`, `own`, `way`, `yellow`. (Use `--highlight bars` to show delimiters like `|snake|`.)

**Subject grammar (shorter sentences with prefixes)**

```bash
$ glossia --random 12 --dialect subject --seed 0
```

```
A snake is out a robot is mixed the ship is pop the program is night the ten is
hot a shield is cut a bamboo is own a way is yellow
```

**Compact body grammar**

```bash
$ glossia --random 12 --dialect body --length-mode compact --seed 0
```

```
Snake see robot. Ban is mixed. Ship program tap. Night is ten.
Shield get bamboo. Pie own way. Yellow is set.
```

**Provide words directly instead of random selection**

```bash
$ glossia abandon ability able about above absent
```

```
The abandon may see the ability to each able ears about above. Guy may absent.
```

**Encode an API key or secret**

```bash
$ glossia --from-ascii "sk_live_51H3ll0W0r1d" --seed 0
```

```
Ban inflict foot to swallow for spray. Green may question. State get cinnamon.
Cricket see the gloom to army. The rid purpose already board the rid mosquito.
```

The API key is encoded into natural-looking prose. To decode, extract all BIP39 words from the output and convert them back to ASCII using the wordlist.

**Encode an IP address**

```bash
$ glossia --from-ascii "127.0.0.1" --dialect subject --length-mode compact --seed 0
```

```
A couple is out a museum is pop each slide is set a gate is ago a toast is bad
the blade is hot the series is big
```

IP addresses and other network identifiers can be encoded into prose. Using compact mode with subject grammar produces shorter, more concise output suitable for subject lines or when brevity is important.

**Generate multiple variations and select the most compact**

```bash
$ glossia --random 12 --variations 3 --seed 0
```

```
Snake set robot to the mixed ship. Each program is night to each ten shield.
Bamboo own way to the yellow.


Snake get robot to a mixed ship to program per the night ten.
Some shield see bamboo. Die may own way. The yellow might get pay.


Snake may see the robot. Each mixed ship program night.
Ten shield the bad bamboo. Each own way why yellow each cut.

=== Statistics ===
Input:
  Total words: 12
  POS breakdown:
    Nouns: 10
    Verbs: 6
    Adjectives: 5
    Adverbs: 1
Output:
  Total words: 32
  Payload words: 12 (37.5%)
  Cover words: 20 (62.5%)
  Sentences: 1
  Avg words per sentence: 32.0
Compactness:
  Score: 0.580 (58 BIP39 chars / 100 total chars)
  Efficiency: 58.0% BIP39 characters
Variations:
  Tested: 3
  Min compactness: 0.523
  Max compactness: 0.580
  Avg compactness: 0.548
  Improvement: 5.8% better than average
```

#### Additional Options

```bash
# Use a specific language (default: english)
glossia --random 12 --language english --seed 0

# Use a seed for reproducible output
glossia --random 12 --seed 0

# Show grammar rules
glossia --show-grammar --dialect body

# Verbose output for debugging
glossia --random 12 --verbose --seed 0
```

#### Command-Line Options

- `--random <N>`: Generate sentences from N random BIP39 words
- `--dialect <dialect>`: Dialect to use: `body` (default) or `subject`. Can include language: `latin-body`, `english-subject`, `cs-nip04`.
  - `body`: Longer sentences, no prefixes. Best for email body text and prose.
  - `subject`: Short sentences, may include prefixes (Re:, Fwd:, etc.). Best for email subject lines where brevity is critical.
- `--highlight <mode>`: Highlight words: `none` (default), `bars` (| |), or color name
- `--seed <N>`: Seed for deterministic random generation
- `--variations <N>`: Generate N variations and select the most compact (default: 1)
- `--language, -l <lang>`: Language for wordlist: `english` (default, BIP39), `french` (BIP39), `german`
- `--k-min <N>`: Minimum sentence length in POS slots including Dot (default: 3)
- `--k-max <N>`: Maximum sentence length in POS slots including Dot (default: 20)
- `--length-mode <mode>`: Sentence length selection: `compact` or `natural`
  - Default: `body` → `natural`, `subject` → `compact`
  - `compact`: Try k from k_min to k_max, shortest first. Produces the shortest possible sentences.
  - `natural`: Sample k from grammar's length distribution. Produces more natural, varied sentence lengths.
- `--show-grammar`: Display the grammar rules (then continue execution)
- `--verbose, -v`: Show detailed debugging information
- `--help, -h`: Show help message

## How It Works

1. **Payload tokens**: BIP39 words are tagged with allowed POS categories (Noun, Verb, Adjective, etc.)
2. **Grammar expansion**: The CFG generates a stream of POS slots
3. **Slot filling**: Payload tokens are embedded when they fit a slot's POS, otherwise cover words are used
4. **Decoding**: Extract BIP39 words by filtering the output against the BIP39 word list

**Important**: Wordlists are **append-only**. You can add new words to wordlists, but you cannot replace or reorder existing words. This preserves backward compatibility with previously encoded messages, since Glossia encodes data by mapping words to their position in the wordlist. See the [How It Works documentation](docs/src/how-it-works.md) for details.

## Compact vs Natural (sentence length strategy)

There are (at least) two reasonable ways to drive sentence generation, depending on the UX you want:

- **Compact mode (minimize length)**:
  - Generate the *shortest possible* sentence that can fit the payload.
  - Practical approach: try \(k = k_{\min}, k_{\min}+1, \dots\) and stop at the first feasible \(k\).
  - Within that fixed \(k\), pick the highest-probability sequence (tie-break by probability for readability).

- **Natural mode (maximize grammatical “naturalness”)**:
  - Prefer sentence lengths and POS sequences that are more probable under the grammar.
  - Practical approach: sample (or choose) \(k\) from the grammar’s length distribution, then sample a POS sequence proportionally to its probability.

If you want a single continuous “compactness ↔ naturalness” knob, a simple scoring function works well:

\[
\text{score} = \log P(\text{sequence}) - \lambda \cdot \text{length}
\]

Higher \(\lambda\) yields shorter, more compact text; lower \(\lambda\) yields more natural (but sometimes longer) text.

**CLI default behavior:** if you don't pass `--length-mode`, the program defaults to:
- `--dialect subject` → `compact`
- `--dialect body` → `natural`

## Grammar Files

Grammars are defined in CFG format files located in `languages/{language}/` directories:
- `subject.cfg`: Grammar for email subject lines (shorter, may include prefixes)
- `body.cfg`: Grammar for email body text (longer, more natural sentences)

The grammar parser uses the Pest parser generator to parse these CFG files. Grammar rules support weighted productions for probabilistic selection.

### Grammar note: unbounded PP chaining via VP

The English grammar (`languages/english/body.cfg`) is set up so that **VP can optionally end with one-or-more PPs** (e.g., “… in the house on the hill …"). This removes the previous hard maximum sentence length (formerly capped at 13 POS terminals) while keeping `PP` itself as a simple unit (`Prep NP`).

## Project Structure

This repository is a Cargo workspace:

- `glossia` (repo root): the library crate (`src/lib.rs`, grammar, generator, codec, merkle, …), built as `cdylib` + `rlib`
- `glossia-cli/`: the `glossia` command-line binary (`glossia-cli/src/main.rs`)
- `glossia-tools/`: developer tooling binaries (`cleanup_weights`, `tag_words`, `compare_pos_weights`, `get_top_words`, `validate_pos_weights`)
- `glossia-tools/py/`: standalone Python helper scripts (Latin wordlist generation, payload dedup, PDF/word extraction). Run from the repo root, e.g. `python glossia-tools/py/generate_latin_wordlist.py`

Key files:

- `glossia-cli/src/main.rs`: CLI entry point with arg parsing and generation/decoding flow
- `src/lib.rs`: Library root; provides `GrammarChecker` for nlprule integration and re-exports the public API
- `src/grammar.rs`: Grammar parser and CFG implementation using pest
- `src/grammar_parser.pest`: Pest grammar definition for parsing CFG files
- `glossia-tools/src/bin/get_top_words.rs`: Word frequency analysis tool for generating word lists
- `glossia-tools/src/bin/tag_words.rs`: POS tagging tool using nlprule
- `languages/english/subject.cfg`: CFG grammar definition for subject lines
- `languages/english/body.cfg`: CFG grammar definition for body text
- `languages/english/cover_POS.txt`: Cover lexicon with POS tags (format: `word|POS1,POS2`)
- `languages/english/english_bip39_POS.txt`: BIP39 word list with POS tags
- `Cargo.toml`: Rust project configuration with dependencies

## Dependencies

- `rand = "0.8"`: For random selection of cover words and grammar productions
- `nlprule = "0.6"`: For natural language processing and POS tagging
- `anyhow = "1.0"`: For error handling
- `pest = "2.7"`: For parsing CFG grammar files
- `pest_derive = "2.7"`: Derive macro for pest parser
- `serde = "1.0"`: Serialization framework
- `serde_json = "1.0"`: JSON support for serde
- `reqwest = "0.11"`: For downloading word frequency data (get_top_words)
- `clap = "4.4"`: For command-line argument parsing (get_top_words)
- `regex = "1.10"`: For POS tag parsing (get_top_words)
- `flate2 = "1.0"`: For reading gzipped Ngram files (get_top_words)
- `csv = "1.3"`: For parsing CSV frequency files (get_top_words)

## Data Sources

For detailed information about the utility tools (word frequency analysis, POS tagging, POS weight generation), see the [Tools documentation](docs/src/tools.md).

## Language Support

The program supports multiple languages via the `--language` flag. Each language requires:
- A grammar file: `languages/{language}/{subject,body}.cfg`
- A POS-tagged BIP39 wordlist: `languages/{language}/{language}_bip39_POS.txt`
- A cover lexicon: `languages/{language}/cover_POS.txt`

Currently supported languages:
- `english` (default)
- `french` (requires POS-tagged wordlist)
- `german` (requires POS-tagged wordlist)

## Notes

- The current lexicon is minimal and should be expanded for production use
- The grammar is intentionally simple and can be extended for more variety
- Payload words that don't fit available slots will trigger a warning
- Word frequency data is cached locally to avoid repeated downloads
- **Wordlists are append-only**: You can add words but cannot replace or reorder existing ones (see [How It Works](docs/src/how-it-works.md) for why)
- See the [Tools documentation](docs/src/tools.md) for information about utility tools
- Grammar files use Pest parser syntax - see `src/grammar_parser.pest` for the grammar definition