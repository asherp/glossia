#!/usr/bin/env python3
"""
Generate Latin cover words for Glossia from Wiktionary data (Kaikki.org).

Cover words fill grammatical slots in generated sentences and must be
disjoint from the payload wordlist. This script extracts short, common
Latin function words and content words not in the payload.

Usage:
    python generate_latin_cover_wiktionary.py -o languages/latin/cover.yaml

    # Use a previously downloaded JSONL file
    python generate_latin_cover_wiktionary.py --input /tmp/latin_wikt_full.jsonl -o cover.yaml
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

# Map Wiktionary POS to Glossia POS for cover words.
# Cover words can fill function-word slots that payload words cannot.
WIKT_POS_MAP = {
    'noun': 'N',
    'verb': 'V',
    'adj': 'Adj',
    'adv': 'Adv',
    'prep': 'Prep',
    'conj': 'Conj',
    'pron': 'Pron',
    'det': 'Det',
    'intj': 'Intj',
    'particle': 'Adv',  # Latin particles often function as adverbs
}

# POS tags to skip
SKIP_POS = {
    'name',
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
    'contraction',
    'article',
    'postp',
    'num',
}

# Hand-curated function words for slots that Wiktionary may not cover well.
# These are essential Latin grammar words that serve as sentence glue.
FUNCTION_WORDS = {
    # Copula forms (Cop)
    'est': {'Cop': 1.0},
    'sunt': {'Cop': 1.0},
    'erat': {'Cop': 1.0},
    'erit': {'Cop': 1.0},
    'sit': {'Cop': 1.0},
    'fuit': {'Cop': 1.0},
    'esset': {'Cop': 1.0},
    'esse': {'Cop': 1.0},
    # Modal forms (Modal)
    'potest': {'Modal': 1.0},
    'debet': {'Modal': 1.0},
    'solet': {'Modal': 1.0},
    'vult': {'Modal': 1.0},
    'possunt': {'Modal': 1.0},
    'debent': {'Modal': 1.0},
    # Auxiliary (Aux)
    'habet': {'Aux': 1.0},
    'habent': {'Aux': 1.0},
    # To (infinitive marker - Latin doesn't have one, but used for CFG)
    # Dot (sentence punctuation)
    '.': {'Dot': 1.0},
}


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
    for sense in entry.get('senses', []):
        if 'form_of' in sense or 'form-of' in sense:
            return True
        tags = sense.get('tags', [])
        if 'form-of' in tags:
            return True
    for ht in entry.get('head_templates', []):
        arg2 = ht.get('args', {}).get('2', '')
        if arg2.endswith(' form') or arg2.endswith(' forms'):
            return True
    return False


def load_payload_words(payload_file):
    """Load payload words from YAML file."""
    with open(payload_file, 'r') as f:
        data = yaml.safe_load(f)
    return set(data.keys()) if data else set()


def extract_cover_candidates(input_path, payload_words, max_length=8):
    """
    Extract cover word candidates from Wiktionary JSONL.

    Returns dict mapping words to sets of Glossia POS tags.
    Only includes words NOT in the payload set.
    """
    word_pos = defaultdict(set)

    with open(input_path, 'r', encoding='utf-8') as f:
        for line in f:
            obj = json.loads(line)
            pos = obj.get('pos', '')
            word = obj.get('word', '')

            if pos == 'name' or pos in SKIP_POS:
                continue
            if is_form_of(obj):
                continue

            glossia_pos = WIKT_POS_MAP.get(pos)
            if not glossia_pos:
                continue

            normalized = normalize_word(word)
            if not normalized or not normalized.isalpha():
                continue
            if len(normalized) > max_length:
                continue
            if normalized in payload_words:
                continue

            word_pos[normalized].add(glossia_pos)

    return word_pos


def build_cover_list(word_pos, max_per_pos=100):
    """
    Select cover words, limiting to max_per_pos per POS tag.

    Prefers words of length 2-6 (natural cover/glue words),
    then shorter, then longer. Skips single-letter entries for
    content POS (N, V, Adj) since those are usually letter names.
    """
    content_pos = {'N', 'V', 'Adj', 'Adv'}

    # Group words by POS
    by_pos = defaultdict(list)
    for word, pos_tags in word_pos.items():
        for pos in pos_tags:
            # Skip single-letter words for content POS
            if len(word) <= 1 and pos in content_pos:
                continue
            # Skip two-letter words for content POS (usually abbreviations)
            if len(word) <= 2 and pos in content_pos:
                continue
            by_pos[pos].append(word)

    # Sort: prefer 3-6 char words, then by length, then alphabetically
    def sort_key(w):
        l = len(w)
        # Bucket: 3-6 chars first (bucket 0), then 2 (bucket 1),
        # then 7-8 (bucket 2), then 1 (bucket 3)
        if 3 <= l <= 6:
            bucket = 0
        elif l == 2:
            bucket = 1
        elif l >= 7:
            bucket = 2
        else:
            bucket = 3
        return (bucket, l, w)

    selected = {}
    for pos, words in by_pos.items():
        words_sorted = sorted(words, key=sort_key)
        chosen = words_sorted[:max_per_pos]
        for word in chosen:
            if word not in selected:
                selected[word] = {}
            selected[word][pos] = 1.0

    return selected


def main():
    parser = argparse.ArgumentParser(
        description='Generate Latin cover words for Glossia from Wiktionary (Kaikki.org)',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    parser.add_argument('-o', '--output', type=str, default='cover.yaml',
                        help='Output YAML file path')
    parser.add_argument('--input', type=str,
                        help='Path to previously downloaded JSONL file')
    parser.add_argument('--payload-file', type=str,
                        default='languages/latin/payload.yaml',
                        help='Payload file to exclude (default: languages/latin/payload.yaml)')
    parser.add_argument('--max-length', type=int, default=8,
                        help='Maximum word length for cover words (default: 8)')
    parser.add_argument('--max-per-pos', type=int, default=100,
                        help='Maximum words per POS tag (default: 100)')

    args = parser.parse_args()

    # Load payload words
    print(f"Loading payload words from {args.payload_file}...", file=sys.stderr)
    payload_words = load_payload_words(args.payload_file)
    print(f"  {len(payload_words)} payload words to exclude", file=sys.stderr)

    # Get input file
    input_path = args.input
    if not input_path:
        input_path = '/tmp/kaikki-latin-wiktionary.jsonl'
        p = Path(input_path)
        if not p.exists():
            download_wiktionary(input_path)
        else:
            print(f"Using cached {input_path}", file=sys.stderr)

    # Extract candidates
    print(f"Extracting cover candidates (max length {args.max_length})...", file=sys.stderr)
    word_pos = extract_cover_candidates(input_path, payload_words, args.max_length)
    print(f"  {len(word_pos)} candidates found", file=sys.stderr)

    # Select cover words
    cover = build_cover_list(word_pos, args.max_per_pos)

    # Add hand-curated function words (if not in payload)
    for word, pos_map in FUNCTION_WORDS.items():
        if word not in payload_words:
            if word not in cover:
                cover[word] = {}
            cover[word].update(pos_map)

    # POS distribution
    pos_counts = defaultdict(int)
    for word_data in cover.values():
        for pos in word_data:
            pos_counts[pos] += 1

    # Verify disjointness
    overlap = set(cover.keys()) & payload_words
    if overlap:
        print(f"ERROR: {len(overlap)} words overlap with payload: {sorted(overlap)[:10]}", file=sys.stderr)
        sys.exit(1)

    # Write output
    print(f"Writing to {args.output}...", file=sys.stderr)
    with open(args.output, 'w', encoding='utf-8') as f:
        f.write("# Latin cover words for Glossia\n")
        f.write("# Source: Wiktionary via Kaikki.org (Wiktextract)\n")
        f.write("# Generated by generate_latin_cover_wiktionary.py\n")
        f.write("# These words fill grammar slots and must NOT be in the payload wordlist\n")
        f.write("#\n")
        f.write(f"# Excluded payload words from: {args.payload_file}\n")
        f.write(f"# Maximum length: {args.max_length}\n")
        f.write(f"# Maximum words per POS: {args.max_per_pos}\n")
        f.write(f"# Total cover words: {len(cover)}\n")
        f.write("#\n")
        f.write("# POS Tag Distribution:\n")
        for pos, count in sorted(pos_counts.items()):
            f.write(f"#   {pos}: {count}\n")
        f.write("#\n")
        yaml.dump(cover, f, default_flow_style=False, sort_keys=True, allow_unicode=True)

    print(f"\nDone: {len(cover)} cover words", file=sys.stderr)
    for pos, count in sorted(pos_counts.items()):
        print(f"  {pos}: {count}", file=sys.stderr)


if __name__ == '__main__':
    main()
