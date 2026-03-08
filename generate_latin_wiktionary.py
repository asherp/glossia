#!/usr/bin/env python3
"""
Generate a Latin payload wordlist for Glossia from Wiktionary data (Kaikki.org).

Downloads structured JSONL extracted from English Wiktionary via Wiktextract,
filters to Latin lemmas (excluding proper nouns and inflected forms), and
outputs in Glossia's YAML format with POS tag weights.

Usage:
    python generate_latin_wiktionary.py -o languages/latin/payload_wiktionary.yaml

    # Use a previously downloaded JSONL file
    python generate_latin_wiktionary.py --input /tmp/latin_wikt_full.jsonl -o output.yaml

    # Adjust word count (must be power of 2)
    python generate_latin_wiktionary.py --max-words 16384 -o output.yaml
"""

import sys
import json
import argparse
import unicodedata
from collections import defaultdict
from pathlib import Path

try:
    import yaml
except ImportError:
    print("Error: PyYAML is required. Install it with: pip install pyyaml", file=sys.stderr)
    sys.exit(1)

WIKTIONARY_URL = "https://kaikki.org/dictionary/Latin/kaikki.org-dictionary-Latin.jsonl"

# Map Wiktionary POS tags to Glossia POS tags
WIKT_POS_MAP = {
    'noun': 'N',
    'verb': 'V',
    'adj': 'Adj',
    'adv': 'Adv',
    'prep': 'Prep',
    'conj': 'Conj',
    'pron': 'Pron',
    'det': 'Det',
}

# POS tags to skip entirely
SKIP_POS = {
    'name',          # proper nouns
    'suffix',
    'prefix',
    'infix',
    'interfix',
    'circumfix',
    'character',
    'symbol',
    'punct',
    'phrase',
    'prep_phrase',
    'proverb',
    'particle',
    'contraction',
    'article',
    'postp',
    'num',
    'intj',
}


def is_power_of_two(n):
    return n > 0 and (n & (n - 1)) == 0


def download_wiktionary(dest_path):
    """Download the Latin Wiktionary JSONL from Kaikki.org."""
    from urllib.request import urlretrieve
    print(f"Downloading {WIKTIONARY_URL}...", file=sys.stderr)
    print("This is ~1 GB and may take a few minutes.", file=sys.stderr)
    urlretrieve(WIKTIONARY_URL, dest_path)
    print(f"Downloaded to {dest_path}", file=sys.stderr)


def normalize_word(word):
    """Normalize a Latin word: lowercase, strip diacritics, ASCII only."""
    word = word.lower().strip()
    word = unicodedata.normalize('NFKD', word)
    word = word.encode('ascii', 'ignore').decode('ascii')
    return word


def is_form_of(entry):
    """Check if a Wiktionary entry is a form-of (inflected form, not a lemma)."""
    # Check senses for form_of markers
    for sense in entry.get('senses', []):
        if 'form_of' in sense or 'form-of' in sense:
            return True
        tags = sense.get('tags', [])
        if 'form-of' in tags:
            return True
    # Check head_templates for "X form" markers (catches verb form, noun form, etc.)
    for ht in entry.get('head_templates', []):
        arg2 = ht.get('args', {}).get('2', '')
        if arg2.endswith(' form') or arg2.endswith(' forms'):
            return True
    return False


def extract_lemmas(input_path, min_length=3, max_length=12):
    """
    Extract Latin lemmas from a Wiktionary JSONL file.

    Returns a dict mapping normalized words to sets of Glossia POS tags.
    """
    word_pos = defaultdict(set)
    skipped_proper = 0
    skipped_form = 0
    skipped_pos = 0
    total = 0

    with open(input_path, 'r', encoding='utf-8') as f:
        for line in f:
            obj = json.loads(line)
            total += 1
            pos = obj.get('pos', '')
            word = obj.get('word', '')

            # Skip proper nouns
            if pos == 'name':
                skipped_proper += 1
                continue

            # Skip non-content POS
            if pos in SKIP_POS:
                skipped_pos += 1
                continue

            # Skip inflected forms
            if is_form_of(obj):
                skipped_form += 1
                continue

            # Map to Glossia POS
            glossia_pos = WIKT_POS_MAP.get(pos)
            if not glossia_pos:
                skipped_pos += 1
                continue

            normalized = normalize_word(word)

            # Skip empty, non-alpha, or wrong length
            if not normalized or not normalized.isalpha():
                continue
            if len(normalized) < min_length or len(normalized) > max_length:
                continue

            word_pos[normalized].add(glossia_pos)

    print(f"Total entries scanned: {total:,}", file=sys.stderr)
    print(f"Skipped proper nouns: {skipped_proper:,}", file=sys.stderr)
    print(f"Skipped inflected forms: {skipped_form:,}", file=sys.stderr)
    print(f"Skipped non-content POS: {skipped_pos:,}", file=sys.stderr)
    print(f"Unique lemmas extracted: {len(word_pos):,}", file=sys.stderr)

    return word_pos


def select_words(word_pos, max_words):
    """
    Select top words, preferring words with more POS tags (more useful)
    and alphabetical order as tiebreaker.
    """
    # Sort by: number of POS tags (descending), then alphabetically
    sorted_words = sorted(word_pos.items(), key=lambda x: (-len(x[1]), x[0]))

    if max_words and len(sorted_words) > max_words:
        sorted_words = sorted_words[:max_words]

    return dict(sorted_words)


def build_yaml_entries(word_pos):
    """Build the YAML dict with equal POS weights per word."""
    result = {}
    for word in sorted(word_pos.keys()):
        pos_tags = sorted(word_pos[word])
        weight = round(1.0 / len(pos_tags), 3)
        result[word] = {pos: weight for pos in pos_tags}
    return result


def main():
    parser = argparse.ArgumentParser(
        description='Generate a Latin wordlist for Glossia from Wiktionary (Kaikki.org)',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    parser.add_argument('-o', '--output', type=str, default='latin_payload_wiktionary.yaml',
                        help='Output YAML file path')
    parser.add_argument('--input', type=str,
                        help='Path to previously downloaded JSONL file')
    parser.add_argument('--min-length', type=int, default=3,
                        help='Minimum word length (default: 3)')
    parser.add_argument('--max-length', type=int, default=12,
                        help='Maximum word length (default: 12)')
    parser.add_argument('--max-words', type=int, default=32768,
                        help='Max words to include, must be power of 2 (default: 32768)')

    args = parser.parse_args()

    if not is_power_of_two(args.max_words):
        print(f"Error: --max-words must be a power of 2, got {args.max_words}", file=sys.stderr)
        sys.exit(1)

    # Get input file
    input_path = args.input
    if not input_path:
        input_path = '/tmp/kaikki-latin-wiktionary.jsonl'
        p = Path(input_path)
        if not p.exists():
            download_wiktionary(input_path)
        else:
            print(f"Using cached {input_path}", file=sys.stderr)

    # Extract lemmas
    print(f"Extracting lemmas (length {args.min_length}-{args.max_length})...", file=sys.stderr)
    word_pos = extract_lemmas(input_path, args.min_length, args.max_length)

    available = len(word_pos)
    bits = args.max_words.bit_length() - 1
    print(f"Available lemmas: {available:,}", file=sys.stderr)
    print(f"Target: {args.max_words:,} (2^{bits} = {bits} bits/word)", file=sys.stderr)

    if available < args.max_words:
        print(f"Error: only {available:,} lemmas available, need {args.max_words:,}", file=sys.stderr)
        print(f"Try --max-words {1 << (available.bit_length() - 1)} (2^{available.bit_length() - 1})", file=sys.stderr)
        sys.exit(1)

    # Select words
    word_pos = select_words(word_pos, args.max_words)
    print(f"Selected {len(word_pos):,} words", file=sys.stderr)

    # Build YAML entries
    wordlist = build_yaml_entries(word_pos)

    # POS distribution
    pos_counts = defaultdict(int)
    for word_data in wordlist.values():
        for pos in word_data:
            pos_counts[pos] += 1

    # Write output
    print(f"Writing to {args.output}...", file=sys.stderr)
    with open(args.output, 'w', encoding='utf-8') as f:
        f.write("# Latin Wordlist for Glossia\n")
        f.write("# Source: Wiktionary via Kaikki.org (Wiktextract)\n")
        f.write(f"# Generated by generate_latin_wiktionary.py\n")
        f.write("#\n")
        f.write("# Script Inputs:\n")
        f.write(f"#   --min-length: {args.min_length}\n")
        f.write(f"#   --max-length: {args.max_length}\n")
        f.write(f"#   --max-words: {args.max_words}\n")
        f.write("#\n")
        f.write("# Statistics:\n")
        f.write(f"#   Total words: {len(wordlist):,}\n")
        f.write(f"#   Bits per word: {bits}\n")
        f.write("#\n")
        f.write("# POS Tag Distribution:\n")
        for pos, count in sorted(pos_counts.items()):
            pct = (count / len(wordlist)) * 100
            f.write(f"#   {pos}: {count:,} ({pct:.1f}%)\n")
        f.write("#\n")
        yaml.dump(wordlist, f, default_flow_style=False, sort_keys=True, allow_unicode=True)

    print(f"\nDone: {len(wordlist):,} words, {bits} bits/word", file=sys.stderr)
    print(f"Output: {args.output}", file=sys.stderr)
    for pos, count in sorted(pos_counts.items()):
        pct = (count / len(wordlist)) * 100
        print(f"  {pos}: {count:,} ({pct:.1f}%)", file=sys.stderr)


if __name__ == '__main__':
    main()
