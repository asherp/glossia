# Generating Latin Wordlists for Glossia

This guide explains how to use `glossia-tools/py/generate_latin_wordlist.py` to create Latin wordlists for Glossia from CLTK (Classical Language Toolkit) lemmata data.

## Prerequisites

### Option 1: Using Conda (Recommended)

Create a conda environment with CLTK and dependencies:

```bash
# Create environment from environment.yml
conda env create -f environment.yml

# Activate the environment
conda activate cltk
```

### Option 2: Using pip

Install the required Python packages:

```bash
pip install cltk pyyaml
```

## Getting Latin Lemmata Data

You have several options to obtain Latin lemmata data:

### Option 1: Download from CLTK GitHub Repository

The script will attempt to automatically download lemmata from the CLTK repository:

```bash
python glossia-tools/py/generate_latin_wordlist.py -o latin_wordlist.yaml
```

### Option 2: Use CLTK Python Package (Recommended)

If you've set up the conda environment or installed CLTK via pip, download the models:

```bash
# Activate conda environment if using conda
conda activate cltk

# Download CLTK Latin models
python -c "from cltk.data.fetch import FetchCorpus; FetchCorpus('lat').import_corpus('lat_models_cltk')"
```

The script will automatically find the downloaded data. Alternatively, you can specify the file path:

```bash
python glossia-tools/py/generate_latin_wordlist.py --lemmata-file ~/cltk_data/lat/model/lat_models_cltk/lemmata/backoff/collected.json -o latin_wordlist.yaml
```

### Option 3: Download Manually

Download the lemmata JSON file directly from:
- https://github.com/cltk/lat_models_cltk/tree/master/lemmata

Then use it with `--lemmata-file`:

```bash
python glossia-tools/py/generate_latin_wordlist.py --lemmata-file path/to/lemmata.json -o latin_wordlist.yaml
```

## Usage Examples

### Generate a Payload Wordlist

Generate a complete wordlist (all words):

```bash
python glossia-tools/py/generate_latin_wordlist.py -o latin_payload.yaml
```

### Generate a Cover Wordlist

Generate a cover wordlist with shorter, common words (3-6 characters, top 1000):

```bash
python glossia-tools/py/generate_latin_wordlist.py --cover -o latin_cover.yaml
```

### Custom Filtering

Filter words by length and limit the number:

```bash
# Words between 4-6 characters, maximum 500 words
python glossia-tools/py/generate_latin_wordlist.py --min-length 4 --max-length 6 -n 500 -o latin_short.yaml
```

### Use Local File

If you have a local lemmata file:

```bash
python glossia-tools/py/generate_latin_wordlist.py --lemmata-file my_lemmata.json -o output.yaml
```

## Command-Line Options

- `-o, --output`: Output YAML file path (default: `latin_wordlist.yaml`)
- `--lemmata-file`: Path to local lemmata JSON file
- `--min-length`: Minimum word length (default: no minimum)
- `--max-length`: Maximum word length (default: 6)
- `-n, --max-words`: Maximum number of words to include
- `--cover`: Generate a cover wordlist (sets min-length=3, max-length=6, max-words=1000)
- `--no-download`: Do not attempt to download lemmata (require --lemmata-file)

## Output Format

The script generates a YAML file in Glossia's format:

```yaml
amor:
  N: 1.0

amo:
  V: 1.0

bonus:
  Adj: 1.0

et:
  Conj: 1.0
```

Each word maps to one or more POS tags with weights. Currently, the script uses equal weights for all POS tags of a word. For production use, you should use Glossia's POS weight generation tool to get accurate weights based on frequency data.

## Next Steps

After generating the wordlist:

1. **Generate POS weights** (if needed):
   ```bash
   cargo run --bin validate_pos_weights -- \
     --file latin_wordlist.yaml \
     --output latin_wordlist_weighted.yaml
   ```

2. **Create grammar files**: You'll need to create `subject.cfg` and `body.cfg` grammar files for Latin.

3. **Test the wordlist**: Use Glossia to test encoding/decoding with your new Latin wordlist.

## Notes

- The script maps Latin morphological tags to Glossia's simplified POS tags (N, V, Adj, Adv, Prep, Conj, Pron, Det)
- Words are normalized to lowercase
- Only alphabetic words are included (hyphens are allowed)
- POS weights are initially set to equal values; use Glossia's weight generation tool for accurate weights
