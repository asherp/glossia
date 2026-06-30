#!/usr/bin/env python3
"""Find character positions for each word in hp.yaml within the Harry Potter PDF text."""

import yaml
import fitz
import re
from pathlib import Path

def extract_words_from_yaml(yaml_path):
    """Extract word keys from the YAML file."""
    with open(yaml_path, 'r', encoding='utf-8') as f:
        data = yaml.safe_load(f)
    return list(data.keys()) if data else []

def extract_text_from_pdf(pdf_path):
    """Extract all text from PDF."""
    doc = fitz.open(pdf_path)
    full_text = []
    for page_num in range(len(doc)):
        page = doc[page_num]
        text = page.get_text()
        full_text.append(text)
    doc.close()
    return "\n".join(full_text)

def find_word_positions(text, words):
    """Find character positions for each word in the text."""
    results = {}
    text_lower = text.lower()
    
    for word in words:
        word_lower = word.lower()
        positions = []
        
        # Find all occurrences (case-insensitive)
        # Use word boundaries to avoid partial matches
        pattern = r'\b' + re.escape(word_lower) + r'\b'
        
        for match in re.finditer(pattern, text_lower):
            char_pos = match.start()
            positions.append(char_pos)
        
        # Also try without word boundaries for compound words/spells
        if not positions:
            pattern_no_boundary = re.escape(word_lower)
            for match in re.finditer(pattern_no_boundary, text_lower):
                char_pos = match.start()
                positions.append(char_pos)
        
        results[word] = positions
    
    return results

def main():
    # Paths
    yaml_path = Path("/Users/asherp/git/glossia/languages/latin/hp.yaml")
    pdf_path = Path("/Users/asherp/Documents/harrypotter.pdf")
    output_path = Path("/Users/asherp/git/glossia/languages/latin/harrypotter_extracted.txt")
    
    # Extract words from YAML
    print("Reading words from hp.yaml...")
    words = extract_words_from_yaml(yaml_path)
    print(f"Found {len(words)} words to search for.\n")
    
    # Extract text from PDF
    print("Extracting text from PDF...")
    if output_path.exists():
        print(f"Reading from existing extracted text file...")
        with open(output_path, 'r', encoding='utf-8') as f:
            text = f.read()
    else:
        print(f"Extracting from PDF: {pdf_path}")
        text = extract_text_from_pdf(str(pdf_path))
        # Save extracted text
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(text)
        print(f"Saved extracted text to: {output_path}")
    
    print(f"Text length: {len(text):,} characters\n")
    
    # Find positions
    print("Searching for word positions...")
    results = find_word_positions(text, words)
    
    # Output results
    print("\n" + "=" * 80)
    print("CHARACTER POSITIONS FOR EACH WORD:")
    print("=" * 80)
    
    found_count = 0
    not_found = []
    
    for word in sorted(words):
        positions = results[word]
        if positions:
            found_count += 1
            # Show first occurrence and total count
            print(f"{word:20s} -> First: {positions[0]:8,} | Total occurrences: {len(positions)}")
            if len(positions) > 1:
                print(f"{'':20s}   All positions: {positions[:10]}{'...' if len(positions) > 10 else ''}")
        else:
            not_found.append(word)
            print(f"{word:20s} -> NOT FOUND")
    
    print("\n" + "=" * 80)
    print(f"Summary: {found_count}/{len(words)} words found")
    if not_found:
        print(f"Words not found: {', '.join(not_found)}")
    
    # Save results to YAML format
    output_yaml = Path("/Users/asherp/git/glossia/languages/latin/hp_positions.yaml")
    output_data = {}
    for word in words:
        positions = results[word]
        if positions:
            output_data[word] = {
                'first_position': positions[0],
                'total_occurrences': len(positions),
                'positions': positions[:100] if len(positions) <= 100 else positions[:100] + ['...']
            }
        else:
            output_data[word] = {'found': False}
    
    with open(output_yaml, 'w', encoding='utf-8') as f:
        yaml.dump(output_data, f, default_flow_style=False, allow_unicode=True)
    
    print(f"\nResults saved to: {output_yaml}")

if __name__ == "__main__":
    main()
