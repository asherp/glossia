#!/usr/bin/env python3
"""
Generate Latin cover words for Glossia from CLTK lemmata data.

This script extracts common Latin words (excluding payload words) sorted by frequency
and outputs them in Glossia's cover.yaml format.

Usage:
    python generate_latin_cover.py [options]

Examples:
    # Generate cover words excluding payload words
    python generate_latin_cover.py --payload-file languages/latin/payload.yaml -o languages/latin/cover.yaml
    
    # Limit to top N words per POS
    python generate_latin_cover.py --max-per-pos 50 -o cover.yaml
"""

import sys
import json
import argparse
import yaml
from collections import defaultdict
from generate_latin_wordlist import (
    load_lemmata_from_file,
    extract_word_pos_from_lemmata,
    parse_lexicon_field,
    normalize_latin_pos,
    extract_frequency_from_lexicon
)

# Mapping from Latin POS to Glossia POS (excluding Pron which isn't supported)
GLOSSIA_POS_MAP = {
    'N': 'N',
    'V': 'V',
    'Adj': 'Adj',
    'Adv': 'Adv',
    'Prep': 'Prep',
    'Conj': 'Conj',
    'Det': 'Det',
    'Modal': 'Modal',
    'Aux': 'Aux',
    'Cop': 'Cop',
    'To': 'To',
    'Dot': 'Dot',
    'Prefix': 'Prefix',
}

def load_payload_words(payload_file):
    """Load payload words from YAML file."""
    try:
        with open(payload_file, 'r') as f:
            data = yaml.safe_load(f)
        if data:
            return set(word.lower() for word in data.keys())
    except Exception as e:
        print(f"Warning: Could not load payload file {payload_file}: {e}", file=sys.stderr)
    return set()

def filter_glossia_pos(pos_tags):
    """Filter POS tags to only include Glossia-supported tags."""
    filtered = set()
    for pos in pos_tags:
        if pos in GLOSSIA_POS_MAP:
            filtered.add(GLOSSIA_POS_MAP[pos])
    return filtered

def generate_cover_words(lemmata_file, payload_file=None, max_per_pos=None, min_frequency=1, min_length=None, max_length=None, drop_single_letter_nouns=False):
    """Generate cover words from CLTK lemmata data."""
    
    # Load payload words to exclude
    payload_set = set()
    if payload_file:
        payload_set = load_payload_words(payload_file)
        print(f"Loaded {len(payload_set)} payload words to exclude", file=sys.stderr)
    
    # Load lemmata data
    print(f"Loading lemmata from {lemmata_file}...", file=sys.stderr)
    lemmata_data = load_lemmata_from_file(lemmata_file)
    print(f"Loaded {len(lemmata_data)} lemmata entries", file=sys.stderr)
    
    # Extract words, POS tags, and frequencies
    print("Extracting words, POS tags, and frequencies...", file=sys.stderr)
    word_pos, word_freq = extract_word_pos_from_lemmata(lemmata_data)
    print(f"Found {len(word_pos)} unique words", file=sys.stderr)
    
    # Filter out payload words and group by POS
    cover_by_pos = defaultdict(list)  # POS -> list of (word, frequency) tuples
    
    for word, pos_tags in word_pos.items():
        # Skip if in payload
        if word.lower() in payload_set:
            continue
        
        # Length filtering
        if min_length and len(word) < min_length:
            continue
        if max_length and len(word) > max_length:
            continue

        # Drop single-letter words whose only POS is N (bare alphabet letters)
        if drop_single_letter_nouns and len(word) == 1:
            glossia_pos_check = filter_glossia_pos(pos_tags)
            if glossia_pos_check == {'N'}:
                continue

        # Filter to Glossia-supported POS tags
        glossia_pos = filter_glossia_pos(pos_tags)
        if not glossia_pos:
            continue
        
        frequency = word_freq.get(word, 0)
        if frequency < min_frequency:
            continue
        
        # Add word to each POS it can have
        for pos in glossia_pos:
            cover_by_pos[pos].append((word, frequency))
    
    # Sort by frequency (descending) and limit per POS
    result = {}
    for pos, words_freqs in cover_by_pos.items():
        # Sort by frequency (descending), then alphabetically
        sorted_words = sorted(words_freqs, key=lambda x: (-x[1], x[0]))
        
        if max_per_pos:
            sorted_words = sorted_words[:max_per_pos]
        
        # Convert to YAML format: word -> { POS: 1.0 }
        for word, freq in sorted_words:
            if word not in result:
                result[word] = {}
            result[word][pos] = 1.0
        
        print(f"  {pos}: {len(sorted_words)} words (freq range: {sorted_words[-1][1] if sorted_words else 0} - {sorted_words[0][1] if sorted_words else 0})", file=sys.stderr)
    
    return result

def main():
    parser = argparse.ArgumentParser(
        description='Generate Latin cover words for Glossia from CLTK lemmata data',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    
    parser.add_argument('--lemmata-file', type=str,
                        default='/tmp/lat_models_cltk/lemmata/collatinus/collected.json',
                        help='Path to CLTK lemmata JSON file')
    parser.add_argument('--payload-file', type=str,
                        help='Path to payload.yaml file (words to exclude from cover)')
    parser.add_argument('-o', '--output', type=str, default='cover.yaml',
                        help='Output YAML file path (default: cover.yaml)')
    parser.add_argument('--max-per-pos', type=int,
                        help='Maximum number of words per POS tag')
    parser.add_argument('--min-frequency', type=int, default=1,
                        help='Minimum frequency threshold (default: 1)')
    parser.add_argument('--min-length', type=int,
                        help='Minimum word length')
    parser.add_argument('--max-length', type=int,
                        help='Maximum word length')
    parser.add_argument('--drop-single-letter-nouns', action='store_true',
                        help='Drop single-letter words whose only POS is N')

    args = parser.parse_args()
    
    # Generate cover words
    cover_words = generate_cover_words(
        args.lemmata_file,
        args.payload_file,
        args.max_per_pos,
        args.min_frequency,
        args.min_length,
        args.max_length,
        args.drop_single_letter_nouns
    )
    
    # Write YAML output
    print(f"\nWriting output to {args.output}...", file=sys.stderr)
    with open(args.output, 'w', encoding='utf-8') as f:
        f.write("# Latin cover words for Glossia\n")
        f.write("# Generated by generate_latin_cover.py\n")
        f.write("# These words are used to fill sentences and must NOT be in the payload wordlist\n")
        f.write("#\n")
        if args.payload_file:
            f.write(f"# Excluded payload words from: {args.payload_file}\n")
        if args.max_per_pos:
            f.write(f"# Maximum words per POS: {args.max_per_pos}\n")
        if args.min_length:
            f.write(f"# Minimum length: {args.min_length}\n")
        if args.max_length:
            f.write(f"# Maximum length: {args.max_length}\n")
        f.write(f"# Minimum frequency: {args.min_frequency}\n")
        f.write(f"# Total cover words: {len(cover_words)}\n")
        f.write("#\n")
        
        yaml.dump(cover_words, f, default_flow_style=False, sort_keys=True, allow_unicode=True)
    
    print(f"\nSuccessfully generated cover wordlist with {len(cover_words)} words", file=sys.stderr)
    print(f"Output saved to: {args.output}", file=sys.stderr)

if __name__ == '__main__':
    main()
