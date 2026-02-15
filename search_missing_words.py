#!/usr/bin/env python3
"""Search for missing words with variations and context."""

import re
from pathlib import Path

def search_word_variations(text, word):
    """Search for a word with various patterns and variations."""
    results = {
        'exact_lower': [],
        'exact_upper': [],
        'exact_title': [],
        'partial': [],
        'with_punctuation': [],
        'context_snippets': []
    }
    
    word_lower = word.lower()
    word_upper = word.upper()
    word_title = word.capitalize()
    
    # Exact lowercase match
    for match in re.finditer(re.escape(word_lower), text, re.IGNORECASE):
        pos = match.start()
        results['exact_lower'].append(pos)
        # Get context (50 chars before and after)
        start = max(0, pos - 50)
        end = min(len(text), pos + len(word) + 50)
        context = text[start:end].replace('\n', ' ')
        results['context_snippets'].append({
            'position': pos,
            'context': context
        })
    
    # Search for partial matches (word appears as part of another word)
    pattern_partial = re.escape(word_lower)
    for match in re.finditer(pattern_partial, text, re.IGNORECASE):
        pos = match.start()
        if pos not in results['exact_lower']:
            results['partial'].append(pos)
            start = max(0, pos - 50)
            end = min(len(text), pos + len(word) + 50)
            context = text[start:end].replace('\n', ' ')
            results['context_snippets'].append({
                'position': pos,
                'context': context,
                'type': 'partial'
            })
    
    return results

def main():
    missing_words = ['depulso', 'evanesca', 'immobulus', 'inflamari', 'lacarnum', 'sanentur', 'vipera']
    
    text_path = Path("/Users/asherp/git/glossia/languages/latin/harrypotter_extracted.txt")
    
    print("Loading text file...")
    with open(text_path, 'r', encoding='utf-8') as f:
        text = f.read()
    
    print(f"Text length: {len(text):,} characters\n")
    print("=" * 80)
    print("SEARCHING FOR MISSING WORDS:")
    print("=" * 80)
    
    for word in missing_words:
        print(f"\n{'='*80}")
        print(f"Searching for: {word}")
        print(f"{'='*80}")
        
        results = search_word_variations(text, word)
        
        total_found = len(results['exact_lower']) + len(results['partial'])
        
        if total_found > 0:
            print(f"✓ FOUND {total_found} occurrence(s)!")
            print(f"  Exact matches: {len(results['exact_lower'])}")
            print(f"  Partial matches: {len(results['partial'])}")
            
            # Show first few context snippets
            print("\n  Context snippets:")
            for i, snippet in enumerate(results['context_snippets'][:5], 1):
                print(f"\n  {i}. Position: {snippet['position']:,}")
                print(f"     Context: ...{snippet['context']}...")
                if 'type' in snippet:
                    print(f"     Type: {snippet['type']}")
        else:
            print(f"✗ NOT FOUND")
            
            # Try searching for similar spellings
            print("\n  Trying similar spellings...")
            variations = [
                word.replace('u', 'v'),  # Latin u/v variation
                word.replace('v', 'u'),
                word + 's',  # plural
                word[:-1] if word.endswith('a') else word + 'a',  # declension variations
            ]
            
            for variant in variations:
                if variant != word:
                    count = len(re.findall(re.escape(variant.lower()), text, re.IGNORECASE))
                    if count > 0:
                        print(f"    '{variant}': found {count} time(s)")

if __name__ == "__main__":
    main()
