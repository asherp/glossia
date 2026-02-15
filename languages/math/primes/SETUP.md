# Setup Guide for math/primes Language

## Required Files

The following files are required for the `math/primes` language:

1. ✅ `grammar.yaml` - Grammar rules (already created)
2. ✅ `payload.yaml` - Maps primes to POS tags (just created)
3. ✅ `cover.yaml` - Non-prime integers for cover words (just created)
4. ✅ `wordlist.txt` - List of primes (already exists)

## Usage

**Important**: Use the full language path `math/primes`, not just `primes`:

```bash
# Correct - use full path
glossia --language math/primes --random 4

# Wrong - this will look for languages/primes/payload.yaml
glossia --language primes --random 4
```

## File Structure

```
languages/math/primes/
├── grammar.yaml      ✅ Grammar rules with constraints
├── payload.yaml      ✅ Primes mapped to Det POS
├── cover.yaml        ✅ Non-prime integers for cover words
└── wordlist.txt      ✅ List of primes
```

## Generated Files

The `payload.yaml` and `cover.yaml` files were generated from `wordlist.txt`:

- **payload.yaml**: Contains all primes from wordlist.txt, tagged as `Det: 1.0`
- **cover.yaml**: Contains non-prime integers (1-1000), tagged with N, V, Det

## Regenerating Files

If you need to regenerate the YAML files:

```bash
cd languages/math/primes
python3 generate_payload_yaml.py
python3 generate_cover_yaml.py
```

## Testing

Test that everything works:

```bash
# Test grammar loading
glossia --language math/primes --show-grammar

# Generate test sentence
glossia --language math/primes --random 2 --verbose
```
