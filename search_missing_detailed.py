#!/usr/bin/env python3
"""Detailed search for missing words, including checking spell names."""

import re
from pathlib import Path

def search_with_context(text, word, context_chars=100):
    """Search for word and show detailed context."""
    word_lower = word.lower()
    pattern = re.escape(word_lower)
    
    matches = []
    for match in re.finditer(pattern, text, re.IGNORECASE):
        pos = match.start()
        start = max(0, pos - context_chars)
        end = min(len(text), pos + len(word) + context_chars)
        context = text[start:end]
        
        # Find line breaks around the match
        line_start = context.rfind('\n', 0, context_chars)
        line_end = context.find('\n', context_chars)
        if line_start == -1:
            line_start = 0
        if line_end == -1:
            line_end = len(context)
        
        matches.append({
            'position': pos,
            'full_context': context,
            'line_context': context[line_start:line_end]
        })
    
    return matches

def main():
    missing_words = ['depulso', 'evanesca', 'immobulus', 'inflamari', 'lacarnum', 'sanentur', 'vipera']
    
    text_path = Path("/Users/asherp/git/glossia/languages/latin/harrypotter_extracted.txt")
    
    print("Loading text file...")
    with open(text_path, 'r', encoding='utf-8') as f:
        text = f.read()
    
    print(f"Text length: {len(text):,} characters\n")
    
    # Also search for common spell name patterns
    spell_patterns = {
        'depulso': ['depulso', 'depul', 'repulso'],
        'evanesca': ['evanesca', 'evanesc', 'evanesco'],
        'immobulus': ['immobulus', 'immobil', 'immob'],
        'inflamari': ['inflamari', 'inflam', 'inflamm'],
        'lacarnum': ['lacarnum', 'lacarn', 'lacarn'],
        'sanentur': ['sanentur', 'sanent', 'sana'],
        'vipera': ['vipera', 'viper', 'serpens']
    }
    
    print("=" * 80)
    print("DETAILED SEARCH FOR MISSING WORDS:")
    print("=" * 80)
    
    for word in missing_words:
        print(f"\n{'='*80}")
        print(f"Searching for: {word}")
        print(f"{'='*80}")
        
        # Try exact match
        matches = search_with_context(text, word)
        
        if matches:
            print(f"✓ Found {len(matches)} exact match(es):")
            for i, match in enumerate(matches[:3], 1):
                print(f"\n  Match {i} at position {match['position']:,}:")
                print(f"  {match['line_context']}")
        else:
            print(f"✗ Exact match not found")
            
            # Try variations
            print(f"\n  Trying variations:")
            variations = spell_patterns.get(word, [word])
            found_any = False
            
            for variant in variations:
                if variant != word:
                    variant_matches = search_with_context(text, variant)
                    if variant_matches:
                        found_any = True
                        print(f"\n    '{variant}': Found {len(variant_matches)} occurrence(s)")
                        for match in variant_matches[:2]:
                            print(f"      Position {match['position']:,}: {match['line_context'][:150]}...")
            
            if not found_any:
                print(f"    No variations found either")
            
            # Search for words that might be related (e.g., "Depulso" might be "Repulso")
            print(f"\n  Checking for similar spell names in context...")
            # Look for patterns like "word!" or "word," near spell mentions
            nearby_spells = []
            # Search in areas where other spells appear
            spell_keywords = ['spell', 'incantation', 'charm', 'curse', 'hex']
            for keyword in spell_keywords:
                for match in re.finditer(keyword, text, re.IGNORECASE):
                    start = max(0, match.start() - 200)
                    end = min(len(text), match.end() + 200)
                    context = text[start:end]
                    if word[:4].lower() in context.lower():
                        nearby_spells.append(context)
                        break
            
            if nearby_spells:
                print(f"    Found potential spell contexts:")
                for ctx in nearby_spells[:2]:
                    print(f"      ...{ctx[:200]}...")

if __name__ == "__main__":
    main()
