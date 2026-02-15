#!/usr/bin/env python3
"""
Extract Latin words from Harry Potter spells with Latin incantations.

Since web scraping is blocked, this uses a curated list of known HP spells.
Tags words with POS tags and weights using CLTK lemmata data when available.
"""

import re
import sys
import yaml
import json
from collections import defaultdict

# Known Harry Potter spells with Latin incantations
HP_SPELLS = {
    'Accio': 'accio',
    'Aguamenti': 'aguamenti',
    'Alohomora': 'alohomora',
    'Avada Kedavra': 'avada kedavra',
    'Crucio': 'crucio',
    'Depulso': 'depulso',
    'Descendo': 'descendo',
    'Diffindo': 'diffindo',
    'Engorgio': 'engorgio',
    'Evanesco': 'evanesco',
    'Expecto Patronum': 'expecto patronum',
    'Expelliarmus': 'expelliarmus',
    'Finite Incantatem': 'finite incantatem',
    'Flagrate': 'flagrate',
    'Homenum Revelio': 'homenum revelio',
    'Immobulus': 'immobulus',
    'Imperio': 'imperio',
    'Incendio': 'incendio',
    'Lacarnum Inflamari': 'lacarnum inflamari',
    'Legilimens': 'legilimens',
    'Levicorpus': 'levicorpus',
    'Liberacorpus': 'liberacorpus',
    'Lumos': 'lumos',
    'Morsmordre': 'morsmordre',
    'Nox': 'nox',
    'Obliviate': 'obliviate',
    'Petrificus Totalus': 'petrificus totalus',
    'Protego': 'protego',
    'Reducto': 'reducto',
    'Reparo': 'reparo',
    'Revelio': 'revelio',
    'Rictusempra': 'rictusempra',
    'Sectumsempra': 'sectumsempra',
    'Serpensortia': 'serpensortia',
    'Silencio': 'silencio',
    'Sonorus': 'sonorus',
    'Stupefy': 'stupefy',
    'Wingardium Leviosa': 'wingardium leviosa',
    'Vera Verto': 'vera verto',
    'Vipera Evanesca': 'vipera evanesca',
    'Vulnera Sanentur': 'vulnera sanentur',
}

# POS mapping from Latin morphological tags to Glossia POS tags
LATIN_POS_MAP = {
    'n': 'N', 'noun': 'N', 'substantive': 'N',
    'v': 'V', 'verb': 'V', 'verbum': 'V',
    'adj': 'Adj', 'adjective': 'Adj', 'adjectivum': 'Adj',
    'adv': 'Adv', 'adverb': 'Adv', 'adverbium': 'Adv',
    'prep': 'Prep', 'preposition': 'Prep', 'praepositio': 'Prep',
    'conj': 'Conj', 'conjunction': 'Conj', 'coniunctio': 'Conj',
    'pron': 'Pron', 'pronoun': 'Pron', 'pronomen': 'Pron',
    'art': 'Det', 'article': 'Det', 'articulus': 'Det', 'det': 'Det',
}

def normalize_latin_pos(morph_tag):
    """Extract POS information from Latin morphological tags."""
    if not morph_tag:
        return set()
    
    pos_tags = set()
    morph_tag_lower = morph_tag.lower().strip()
    
    if morph_tag_lower in LATIN_POS_MAP:
        pos_tags.add(LATIN_POS_MAP[morph_tag_lower])
        return pos_tags
    
    for key, pos in LATIN_POS_MAP.items():
        if key in morph_tag_lower:
            pos_tags.add(pos)
    
    if not pos_tags:
        if morph_tag_lower.startswith('n'):
            pos_tags.add('N')
        elif morph_tag_lower.startswith('v'):
            pos_tags.add('V')
        elif morph_tag_lower.startswith('adj'):
            pos_tags.add('Adj')
        elif morph_tag_lower.startswith('adv'):
            pos_tags.add('Adv')
        elif morph_tag_lower.startswith('prep'):
            pos_tags.add('Prep')
        elif morph_tag_lower.startswith('conj'):
            pos_tags.add('Conj')
        elif morph_tag_lower.startswith('pron'):
            pos_tags.add('Pron')
    
    return pos_tags if pos_tags else set()

def parse_lexicon_field(lexicon_str):
    """Parse the lexicon field from CLTK Collatinus format."""
    if not lexicon_str:
        return set()
    
    pos_tags = set()
    lexicon_lower = lexicon_str.lower().strip()
    
    if '|' in lexicon_lower:
        lexicon_lower = lexicon_lower.split('|')[0].strip()
    
    if 'prep.' in lexicon_lower or 'praep.' in lexicon_lower:
        pos_tags.add('Prep')
    if 'conj.' in lexicon_lower or 'coniunctio' in lexicon_lower:
        pos_tags.add('Conj')
    if 'adv.' in lexicon_lower or 'adverb' in lexicon_lower:
        pos_tags.add('Adv')
    if 'pron.' in lexicon_lower or 'pronomen' in lexicon_lower:
        pos_tags.add('Pron')
    
    if re.search(r'\ba[,;]\s*um\b', lexicon_lower) or re.search(r'\bus[,;]\s*a[,;]\s*um\b', lexicon_lower):
        pos_tags.add('Adj')
    
    if re.search(r'\b(as|are|at|ant|o|is|it|imus|itis|unt)\b', lexicon_lower):
        if not re.search(r'\b[aeiou],\s*[fm]\.', lexicon_lower):
            pos_tags.add('V')
    
    if re.search(r'\b(ae|i|is|us|ei|os|es|is),\s*[fmcn]\.', lexicon_lower):
        pos_tags.add('N')
    
    if pos_tags:
        return pos_tags
    
    if len(lexicon_lower.split(',')[0].strip()) <= 3:
        if '+' in lexicon_lower or 'abl.' in lexicon_lower or 'acc.' in lexicon_lower:
            pos_tags.add('Prep')
        else:
            pos_tags.add('Conj')
    
    return pos_tags if pos_tags else set()

def lookup_word_pos(word, cltk_lemmata=None):
    """
    Look up POS tags for a word in CLTK lemmata data.
    Returns a set of POS tags.
    """
    pos_tags = set()
    
    if cltk_lemmata and word in cltk_lemmata:
        entry = cltk_lemmata[word]
        
        # Try lexicon field first
        lexicon = entry.get('lexicon', '')
        if lexicon:
            pos_tags.update(parse_lexicon_field(lexicon))
        
        # Try model field
        if not pos_tags:
            model = entry.get('model', '').lower()
            if model == 'inv':
                lexicon = entry.get('lexicon', '')
                if lexicon:
                    pos_tags.update(parse_lexicon_field(lexicon))
                elif len(word) <= 3:
                    pos_tags.add('Conj')
            elif any(v in model for v in ['amo', 'doceo', 'lego', 'audio', 'capio', 'sum', 'eo']):
                pos_tags.add('V')
            elif model.endswith('o') and len(model) > 2:
                pos_tags.add('V')
    
    # If still no POS, try to infer from word patterns
    if not pos_tags:
        word_lower = word.lower()
        # Common verb endings
        if word_lower.endswith(('o', 'are', 'ere', 'ire', 'io')):
            pos_tags.add('V')
        # Common noun endings
        elif word_lower.endswith(('us', 'um', 'a', 'is', 'es')):
            pos_tags.add('N')
        # Common adjective endings
        elif word_lower.endswith(('us', 'um', 'a', 'is')):
            pos_tags.add('Adj')
        # Default to verb for spell words (most spells are commands/verbs)
        else:
            pos_tags.add('V')
    
    return pos_tags if pos_tags else {'V'}  # Default to verb for spells

def extract_latin_words_from_spells(cltk_lemmata=None):
    """Extract individual Latin words from spell incantations with POS tagging."""
    all_words = defaultdict(lambda: {'count': 0, 'pos': set()})
    spell_words = {}
    
    for spell_name, incantation in HP_SPELLS.items():
        # Split incantation into words
        words = re.findall(r'\b[a-z]+\b', incantation.lower())
        
        # Filter out very short words and common English words
        filtered_words = []
        skip_words = {'the', 'and', 'for', 'are', 'but', 'not', 'you', 'all', 'can', 'her', 'was', 'one', 'our', 'out', 'day', 'get', 'has', 'him', 'his', 'how', 'its', 'may', 'new', 'now', 'old', 'see', 'two', 'way', 'who', 'boy', 'did', 'let', 'put', 'say', 'she', 'too', 'use', 'via', 'est'}
        
        for word in words:
            if len(word) >= 3 and word not in skip_words:
                all_words[word]['count'] += 1
                # Look up POS tags
                pos_tags = lookup_word_pos(word, cltk_lemmata)
                all_words[word]['pos'].update(pos_tags)
                filtered_words.append(word)
        
        if filtered_words:
            spell_words[spell_name] = filtered_words
    
    return all_words, spell_words

def main():
    print("Extracting Latin words from Harry Potter spells...", file=sys.stderr)
    
    all_words, spell_words = extract_latin_words_from_spells()
    
    print(f"\nFound {len(all_words)} unique Latin words", file=sys.stderr)
    print("\nWords found:", file=sys.stderr)
    for word in sorted(all_words.keys()):
        print(f"  {word} (appears in {all_words[word]} spell(s))", file=sys.stderr)
    
    print(f"\nProcessed {len(spell_words)} spells", file=sys.stderr)
    
    # Generate output
    output = {
        'spells': spell_words,
        'words': dict(sorted(all_words.items(), key=lambda x: (-x[1], x[0])))
    }
    
    # Write to YAML
    with open('hp_latin_spells.yaml', 'w', encoding='utf-8') as f:
        f.write("# Harry Potter Spells with Latin Incantations\n")
        f.write("# Extracted Latin words from spell incantations\n")
        f.write("#\n")
        f.write(f"# Total spells: {len(spell_words)}\n")
        f.write(f"# Total unique Latin words: {len(all_words)}\n")
        f.write("#\n")
        yaml.dump(output, f, default_flow_style=False, sort_keys=False, allow_unicode=True)
    
    print(f"\nOutput saved to hp_latin_spells.yaml", file=sys.stderr)
    
    # Also write a simple word list
    with open('hp_latin_words.txt', 'w', encoding='utf-8') as f:
        f.write("# Latin words from Harry Potter spell incantations\n")
        for word in sorted(all_words.keys()):
            f.write(f"{word}\n")
    
    print(f"Word list saved to hp_latin_words.txt", file=sys.stderr)
    
    # Write a YAML wordlist compatible with Glossia format
    wordlist = {}
    for word in sorted(all_words.keys()):
        # Default to N (noun) - could be improved with POS tagging
        wordlist[word] = {'N': 1.0}
    
    with open('hp_latin_wordlist.yaml', 'w', encoding='utf-8') as f:
        f.write("# Latin Wordlist from Harry Potter Spells\n")
        f.write("# Generated from spell incantations\n")
        f.write("#\n")
        f.write(f"# Total words: {len(wordlist)}\n")
        f.write("#\n")
        yaml.dump(wordlist, f, default_flow_style=False, sort_keys=True, allow_unicode=True)
    
    print(f"Glossia-compatible wordlist saved to hp_latin_wordlist.yaml", file=sys.stderr)

if __name__ == '__main__':
    main()
