#!/usr/bin/env python3
"""
Script to verify that:
1. Every word from cover_POS.txt is contained in cover.yaml
2. There are no duplicates in cover.yaml
3. All weights for each word sum to 1.0
"""

import sys
import re
import yaml
from pathlib import Path

def extract_words_from_pos_file(pos_file_path):
    """Extract words from cover_POS.txt"""
    words = set()
    with open(pos_file_path, 'r', encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            if not line:  # Skip empty lines
                continue
            # Format: word|POS1,POS2,...
            parts = line.split('|')
            if parts:
                word = parts[0].strip()
                if word:
                    words.add(word)
    return words

def extract_words_from_yaml(yaml_file_path):
    """Extract words from cover.yaml and check for duplicates"""
    words = []
    word_set = set()
    duplicates = []
    
    # Read raw file to extract keys (handles YAML boolean keys and special chars like "re::")
    with open(yaml_file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Match lines that are word definitions (word: or word::)
    # Pattern matches: word: or word:: at start of line (possibly with leading whitespace)
    pattern = r'^(\s*)([a-z:]+):\s*$'
    
    for match in re.finditer(pattern, content, re.MULTILINE):
        word = match.group(2)
        words.append(word)
        
        if word in word_set:
            duplicates.append(word)
        else:
            word_set.add(word)
    
    return words, word_set, duplicates

def check_weights_sum_to_one(yaml_file_path):
    """Check that all weights for each word sum to 1.0"""
    words_with_invalid_sums = []
    tolerance = 0.0001
    
    # Read raw file to get all word keys
    with open(yaml_file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Extract all word keys from raw file
    word_keys = []
    pattern = r'^(\s*)([a-z:]+):\s*$'
    for match in re.finditer(pattern, content, re.MULTILINE):
        word_keys.append(match.group(2))
    
    # Load YAML data
    with open(yaml_file_path, 'r', encoding='utf-8') as f:
        data = yaml.safe_load(f)
    
    if data is None:
        return {}, []
    
    word_weights = {}
    for word in word_keys:
        pos_weights = data.get(word)
        if pos_weights is None:
            continue
        
        # Convert all values to float
        weights = {}
        for pos, weight in pos_weights.items():
            weights[pos] = float(weight)
        
        word_weights[word] = weights
        
        # Check if weights sum to 1.0
        total = sum(weights.values())
        if abs(total - 1.0) > tolerance:
            words_with_invalid_sums.append((word, total))
    
    return word_weights, words_with_invalid_sums

def main():
    script_dir = Path(__file__).parent
    pos_file = script_dir / 'cover_POS.txt'
    yaml_file = script_dir / 'cover.yaml'
    
    if not pos_file.exists():
        print(f"ERROR: {pos_file} not found", file=sys.stderr)
        sys.exit(1)
    
    if not yaml_file.exists():
        print(f"ERROR: {yaml_file} not found", file=sys.stderr)
        sys.exit(1)
    
    print("Extracting words from cover_POS.txt...")
    reference_words = extract_words_from_pos_file(pos_file)
    print(f"Found {len(reference_words)} words in reference file")
    
    print("\nExtracting words from cover.yaml...")
    yaml_words, yaml_word_set, duplicates = extract_words_from_yaml(yaml_file)
    print(f"Found {len(yaml_words)} word entries in cover.yaml")
    print(f"Found {len(yaml_word_set)} unique words in cover.yaml")
    
    # Check for duplicates
    print("\n" + "="*60)
    if duplicates:
        print(f"ERROR: Found {len(duplicates)} duplicate words in cover.yaml:")
        for dup in sorted(set(duplicates)):
            count = yaml_words.count(dup)
            print(f"  - '{dup}' appears {count} times")
        print("="*60)
        sys.exit(1)
    else:
        print("✓ No duplicates found in cover.yaml")
    
    # Check for missing words
    print("\n" + "="*60)
    missing_words = reference_words - yaml_word_set
    if missing_words:
        print(f"ERROR: Found {len(missing_words)} words missing from cover.yaml:")
        for word in sorted(missing_words):
            print(f"  - '{word}'")
        print("="*60)
        sys.exit(1)
    else:
        print("✓ All words from cover_POS.txt are present in cover.yaml")
    
    # Check for extra words (words in yaml but not in reference)
    extra_words = yaml_word_set - reference_words
    if extra_words:
        print(f"\nWARNING: Found {len(extra_words)} words in cover.yaml not in reference file:")
        for word in sorted(extra_words):
            print(f"  - '{word}'")
    else:
        print("✓ No extra words found (all words in cover.yaml are in reference)")
    
    # Check that weights sum to 1.0
    print("\n" + "="*60)
    print("Checking that weights sum to 1.0 for each word...")
    word_weights, words_with_invalid_sums = check_weights_sum_to_one(yaml_file)
    
    if words_with_invalid_sums:
        print(f"ERROR: Found {len(words_with_invalid_sums)} words where weights don't sum to 1.0:")
        for word, total in sorted(words_with_invalid_sums):
            weights_str = ', '.join([f"{pos}: {weight}" for pos, weight in sorted(word_weights[word].items())])
            print(f"  - '{word}': sum = {total:.6f} (weights: {weights_str})")
        print("="*60)
        sys.exit(1)
    else:
        print("✓ All weights sum to 1.0 for each word")
    
    print("\n" + "="*60)
    print("SUMMARY:")
    print(f"  Reference words: {len(reference_words)}")
    print(f"  YAML entries: {len(yaml_words)}")
    print(f"  Unique YAML words: {len(yaml_word_set)}")
    print(f"  Missing words: {len(missing_words)}")
    print(f"  Duplicates: {len(duplicates)}")
    print(f"  Words with invalid weight sums: {len(words_with_invalid_sums)}")
    print("="*60)
    
    if not missing_words and not duplicates and not words_with_invalid_sums:
        print("\n✓ All checks passed!")
        sys.exit(0)
    else:
        sys.exit(1)

if __name__ == '__main__':
    main()
