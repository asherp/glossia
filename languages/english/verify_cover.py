#!/usr/bin/env python3
"""Verify that all weights in cover.yaml sum to 1.0"""

import sys
import yaml
import re
from pathlib import Path

def check_weights_sum_to_one(yaml_file_path):
    """Check that all weights for each word sum to 1.0"""
    words_with_invalid_sums = []
    tolerance = 0.0001
    
    # Read raw file to get all word keys (handles YAML boolean keys)
    with open(yaml_file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Extract all word keys from raw file
    word_keys = []
    pattern = r'^(\s*)([a-z:]+):\s*$'
    for match in re.finditer(pattern, content, re.MULTILINE):
        word = match.group(2)
        word_keys.append(word)
    
    # Load YAML data
    with open(yaml_file_path, 'r', encoding='utf-8') as f:
        data = yaml.safe_load(f)
    
    if data is None:
        return {}, []
    
    word_weights = {}
    for word in word_keys:
        # Handle special keys like "re::" and "fwd::"
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
    yaml_file = script_dir / 'cover.yaml'
    
    print("Checking that weights sum to 1.0 for each word...")
    word_weights, words_with_invalid_sums = check_weights_sum_to_one(yaml_file)
    
    if words_with_invalid_sums:
        print(f"ERROR: Found {len(words_with_invalid_sums)} words where weights don't sum to 1.0:")
        for word, total in sorted(words_with_invalid_sums):
            weights_str = ', '.join([f"{pos}: {weight}" for pos, weight in sorted(word_weights[word].items())])
            print(f"  - '{word}': sum = {total:.6f} (weights: {weights_str})")
        sys.exit(1)
    else:
        print(f"✓ All weights sum to 1.0 for {len(word_weights)} words")
        sys.exit(0)

if __name__ == '__main__':
    main()
