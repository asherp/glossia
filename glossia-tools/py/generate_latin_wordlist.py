#!/usr/bin/env python3
"""
Generate a Latin wordlist for Glossia from CLTK lemmata data.

This script downloads or accesses Latin lemmata from the CLTK repository,
extracts words with their parts of speech, and outputs them in Glossia's
YAML format with POS tag weights.

Usage:
    python generate_latin_wordlist.py [options]

Examples:
    # Generate a payload wordlist (default)
    python generate_latin_wordlist.py -o latin_payload.yaml

    # Generate a cover wordlist with common words
    python generate_latin_wordlist.py --cover -n 1000 -o latin_cover.yaml

    # Use a local lemmata file
    python generate_latin_wordlist.py --lemmata-file lemmata.json -o output.yaml

    # Filter by word length
    python generate_latin_wordlist.py --min-length 3 --max-length 6 -o output.yaml
"""

import sys
import json
import argparse
import re
import unicodedata
from collections import defaultdict
from pathlib import Path
from urllib.request import urlopen, urlretrieve
from urllib.error import URLError
import tempfile
import os

try:
    import yaml
except ImportError:
    print("Error: PyYAML is required. Install it with: pip install pyyaml", file=sys.stderr)
    sys.exit(1)

# Mapping from Latin morphological tags to Glossia POS tags
LATIN_POS_MAP = {
    # Nouns
    'n': 'N',
    'noun': 'N',
    'substantive': 'N',
    
    # Verbs
    'v': 'V',
    'verb': 'V',
    'verbum': 'V',
    
    # Adjectives
    'adj': 'Adj',
    'adjective': 'Adj',
    'adjectivum': 'Adj',
    
    # Adverbs
    'adv': 'Adv',
    'adverb': 'Adv',
    'adverbium': 'Adv',
    
    # Prepositions
    'prep': 'Prep',
    'preposition': 'Prep',
    'praepositio': 'Prep',
    
    # Conjunctions
    'conj': 'Conj',
    'conjunction': 'Conj',
    'coniunctio': 'Conj',
    
    # Pronouns
    'pron': 'Pron',
    'pronoun': 'Pron',
    'pronomen': 'Pron',
    
    # Determiners/Articles
    'art': 'Det',
    'article': 'Det',
    'articulus': 'Det',
    'det': 'Det',
    'determiner': 'Det',
}

def parse_lexicon_field(lexicon_str):
    """
    Parse the lexicon field from CLTK Collatinus format.
    
    Examples:
    - 'prep. + abl.|5874' → Prep
    - 'as, are|809' → V (verb forms)
    - 'a, um|2865' → Adj (adjective forms)
    - 'conj. adv.|42726' → Conj, Adv
    - 'ae, f.|561' → N (noun with genitive and gender)
    - 'i, m.|123' → N (noun)
    
    Returns a set of normalized POS tags.
    """
    if not lexicon_str:
        return set()
    
    pos_tags = set()
    lexicon_lower = lexicon_str.lower().strip()
    
    # Remove frequency suffix (everything after |)
    if '|' in lexicon_lower:
        lexicon_lower = lexicon_lower.split('|')[0].strip()
    
    # Check for explicit POS markers
    if 'prep.' in lexicon_lower or 'praep.' in lexicon_lower:
        pos_tags.add('Prep')
    
    if 'conj.' in lexicon_lower or 'coniunctio' in lexicon_lower:
        pos_tags.add('Conj')
    
    if 'adv.' in lexicon_lower or 'adverb' in lexicon_lower:
        pos_tags.add('Adv')
    
    if 'pron.' in lexicon_lower or 'pronomen' in lexicon_lower:
        pos_tags.add('Pron')
    
    # Check for adjective patterns: "a, um" or "us, a, um" (masculine, feminine, neuter endings)
    if re.search(r'\ba[,;]\s*um\b', lexicon_lower) or re.search(r'\bus[,;]\s*a[,;]\s*um\b', lexicon_lower):
        pos_tags.add('Adj')
    
    # Check for verb patterns: forms like "as, are" (2nd person, 3rd person) or "o, as, are"
    if re.search(r'\b(as|are|at|ant|o|is|it|imus|itis|unt)\b', lexicon_lower):
        # Make sure it's not a noun pattern
        if not re.search(r'\b[aeiou],\s*[fm]\.', lexicon_lower):
            pos_tags.add('V')
    
    # Check for noun patterns: genitive + gender like "ae, f." or "i, m." or "is, m."
    # Pattern: genitive ending (ae, i, is, us, ei, etc.) followed by comma, space, and gender (f., m., n., c.)
    if re.search(r'\b(ae|i|is|us|ei|os|es|is),\s*[fmcn]\.', lexicon_lower):
        pos_tags.add('N')
    
    # If we found explicit markers, return them
    if pos_tags:
        return pos_tags
    
    # Fallback: try to infer from word patterns
    # Single letter or very short words are often prepositions/conjunctions
    if len(lexicon_lower.split(',')[0].strip()) <= 3:
        if '+' in lexicon_lower or 'abl.' in lexicon_lower or 'acc.' in lexicon_lower:
            pos_tags.add('Prep')
        else:
            pos_tags.add('Conj')  # Common short words like 'et', 'sed'
    
    return pos_tags if pos_tags else set()

def normalize_latin_pos(morph_tag):
    """
    Extract POS information from Latin morphological tags.
    
    Latin morphological tags are typically in formats like:
    - "n" for noun
    - "v" for verb
    - "adj" for adjective
    - "adv" for adverb
    - "prep" for preposition
    - "conj" for conjunction
    - "pron" for pronoun
    
    Returns a set of normalized POS tags.
    """
    if not morph_tag:
        return set()
    
    pos_tags = set()
    morph_tag_lower = morph_tag.lower().strip()
    
    # Check for direct matches
    if morph_tag_lower in LATIN_POS_MAP:
        pos_tags.add(LATIN_POS_MAP[morph_tag_lower])
        return pos_tags
    
    # Check for partial matches (e.g., "noun" in "noun.1st")
    for key, pos in LATIN_POS_MAP.items():
        if key in morph_tag_lower:
            pos_tags.add(pos)
    
    # If no match found, try to infer from common patterns
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
    
    return pos_tags if pos_tags else set()  # Return empty set instead of defaulting to N

def find_cltk_lemmata_file():
    """
    Try to find CLTK lemmata file from installed CLTK package.
    
    Returns the path to the lemmata file if found, or None.
    CLTK stores data in ~/cltk_data/ (or ~/.cltk_data/ in older versions).
    """
    # Try common CLTK data locations (check multiple possible paths)
    cltk_data_paths = [
        # Newer CLTK versions use ~/cltk_data/
        os.path.expanduser("~/cltk_data/lat/model/lat_models_cltk/lemmata/backoff/collected.json"),
        os.path.expanduser("~/cltk_data/lat/lat_models_cltk/lemmata/backoff/collected.json"),
        # Older CLTK versions use ~/.cltk_data/
        os.path.expanduser("~/.cltk_data/lat/model/lat_models_cltk/lemmata/backoff/collected.json"),
        os.path.expanduser("~/.cltk_data/lat/lat_models_cltk/lemmata/backoff/collected.json"),
        # Alternative locations
        os.path.expanduser("~/.cltk/data/lat/lat_models_cltk/lemmata/backoff/collected.json"),
        # Also check for other possible filenames
        os.path.expanduser("~/cltk_data/lat/model/lat_models_cltk/lemmata/lat_lemmata.json"),
        os.path.expanduser("~/cltk_data/lat/lat_models_cltk/lemmata/lat_lemmata.json"),
    ]
    
    for path in cltk_data_paths:
        if os.path.exists(path):
            print(f"Found CLTK lemmata file: {path}", file=sys.stderr)
            return path
    
    return None

def download_cltk_lemmata():
    """
    Download or locate Latin lemmata data from CLTK.
    
    Strategy:
    1. First, try to find locally installed CLTK data
    2. If not found, try to use CLTK package to download it
    3. If CLTK package not available, provide instructions
    
    Returns the path to the lemmata file, or None if not found.
    """
    # First, try to find locally installed CLTK data
    local_file = find_cltk_lemmata_file()
    if local_file:
        return local_file
    
    # Try to use CLTK package if available
    # Note: CLTK 2.x has a different API than 1.x
    try:
        import cltk
        cltk_version = getattr(cltk, '__version__', 'unknown')
        print(f"CLTK package found (version {cltk_version}).", file=sys.stderr)
        
        # CLTK 2.x doesn't have FetchCorpus - models download automatically when using NLP
        # For lemmata data, we need to check if it's already downloaded or guide user
        if cltk_version.startswith('2.'):
            print("CLTK 2.x detected. Lemmata data may need to be downloaded separately.", file=sys.stderr)
            print("The lemmata files are typically in ~/cltk_data/lat/model/lat_models_cltk/lemmata/", file=sys.stderr)
            print("after models are downloaded (they download automatically when using CLTK NLP).", file=sys.stderr)
        else:
            # Try CLTK 1.x API
            try:
                from cltk.data.fetch import FetchCorpus
                print("Attempting to download models using CLTK 1.x API...", file=sys.stderr)
                print("This may take a few minutes on first run...", file=sys.stderr)
                corpus_downloader = FetchCorpus(language="lat")
                corpus_downloader.import_corpus("lat_models_cltk")
                print("CLTK models downloaded successfully.", file=sys.stderr)
                
                # Try to find the file again after download
                local_file = find_cltk_lemmata_file()
                if local_file:
                    return local_file
            except ImportError:
                pass  # CLTK 2.x doesn't have this module
    except ImportError:
        print("CLTK package not installed.", file=sys.stderr)
    
    # If all else fails, provide helpful instructions
    print("\n" + "="*60, file=sys.stderr)
    print("Could not find or download CLTK lemmata data.", file=sys.stderr)
    print("="*60, file=sys.stderr)
    print("\nRecommended: Download lemmata data:", file=sys.stderr)
    print("  Option 1: Use CLTK NLP to trigger model download, then find lemmata file", file=sys.stderr)
    print("    python -c \"from cltk import NLP; nlp = NLP('lati1261'); print('Models downloading...')\"", file=sys.stderr)
    print("  Option 2: Download manually from GitHub:", file=sys.stderr)
    print("    Visit: https://github.com/cltk/lat_models_cltk/tree/master/lemmata", file=sys.stderr)
    print("    Download collected.json and use --lemmata-file", file=sys.stderr)
    print("\nAlternative: Use a local lemmata file:", file=sys.stderr)
    print("  python generate_latin_wordlist.py --lemmata-file path/to/lemmata.json", file=sys.stderr)
    print("\nThe lemmata file should be in JSON format with lemma data.", file=sys.stderr)
    print("You can find it in ~/cltk_data/lat/model/lat_models_cltk/lemmata/", file=sys.stderr)
    print("after downloading via CLTK.", file=sys.stderr)
    print("="*60, file=sys.stderr)
    
    return None

def load_lemmata_from_file(file_path):
    """
    Load lemmata data from a JSON file.
    
    Expected format can vary, so we handle multiple structures:
    1. List of lemmata objects
    2. Dictionary mapping lemmas to data
    3. Dictionary with a 'lemmata' key
    4. CLTK Collatinus format (dict with lemma keys containing inflection data)
    """
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        
        # Handle different JSON structures
        if isinstance(data, list):
            return data
        elif isinstance(data, dict):
            # CLTK Collatinus format: dict with 'lemmas', 'pos', 'models', 'maps' keys
            if 'lemmas' in data:
                lemmas_dict = data['lemmas']
                pos_map = data.get('pos', {})
                
                # Convert to list format
                lemmata_list = []
                for lemma, lemma_data in lemmas_dict.items():
                    if isinstance(lemma_data, dict):
                        # Extract POS from lemma_data or pos_map
                        pos = lemma_data.get('pos') or lemma_data.get('part_of_speech')
                        if not pos and 'pos_id' in lemma_data:
                            # Look up POS in pos_map using pos_id
                            pos_id = str(lemma_data.get('pos_id'))
                            pos_tag = pos_map.get(pos_id, '')
                            # Extract POS from morphological tag (e.g., '--s----n-' -> 'n')
                            if pos_tag:
                                if 'n' in pos_tag.lower():
                                    pos = 'n'
                                elif 'v' in pos_tag.lower():
                                    pos = 'v'
                                elif 'a' in pos_tag.lower():
                                    pos = 'adj'
                        
                        # Don't set default pos here - let extract_word_pos_from_lemmata handle it
                        lemmata_list.append({
                            'lemma': lemma,
                            **lemma_data
                        })
                        # Only add pos if we actually found one
                        if pos:
                            lemmata_list[-1]['pos'] = pos
                    else:
                        lemmata_list.append({'lemma': lemma})
                return lemmata_list
            elif 'lemmata' in data:
                return data['lemmata']
            elif 'data' in data:
                return data['data']
            else:
                # Fallback: dict where keys are lemmas
                lemmata_list = []
                for lemma, lemma_data in data.items():
                    if isinstance(lemma_data, dict):
                        pos = lemma_data.get('pos') or lemma_data.get('part_of_speech')
                        if not pos:
                            if 'declension' in lemma_data or 'decl' in lemma_data:
                                pos = 'n'
                            elif 'conjugation' in lemma_data or 'conj' in lemma_data:
                                pos = 'v'
                        
                        lemmata_list.append({
                            'lemma': lemma,
                            'pos': pos or 'n',
                            **lemma_data
                        })
                    else:
                        lemmata_list.append({'lemma': lemma, 'pos': str(lemma_data)})
                return lemmata_list
        else:
            print(f"Unexpected data format in {file_path}", file=sys.stderr)
            return []
    except Exception as e:
        print(f"Error loading lemmata file {file_path}: {e}", file=sys.stderr)
        return []

def extract_frequency_from_lexicon(lexicon_str):
    """
    Extract frequency from lexicon field.
    
    Format: "as, are|809" -> returns 809
    Returns 0 if no frequency found.
    """
    if not lexicon_str or '|' not in lexicon_str:
        return 0
    try:
        freq_part = lexicon_str.split('|')[1].strip()
        return int(freq_part)
    except (ValueError, IndexError):
        return 0

def extract_word_pos_from_lemmata(lemmata_data):
    """
    Extract words, their POS tags, and frequencies from lemmata data.
    
    Returns a tuple of:
    - Dictionary mapping words to sets of POS tags
    - Dictionary mapping words to frequencies
    """
    word_pos = defaultdict(set)
    word_freq = {}
    
    def normalize_lemma(raw_lemma):
        if not raw_lemma:
            return ''
        # Lowercase and strip, then remove diacritics and non-ASCII characters.
        normalized = raw_lemma.lower().strip()
        normalized = unicodedata.normalize('NFKD', normalized)
        normalized = normalized.encode('ascii', 'ignore').decode('ascii')
        return normalized

    def is_proper_noun(entry, raw_lemma):
        """Detect proper nouns via capitalization or npr. marker."""
        if not raw_lemma:
            return False
        # Collatinus capitalizes proper nouns in the lemma field
        if raw_lemma[0].isupper():
            return True
        # Also check for explicit npr. (nomen proprium) marker in lexicon
        if isinstance(entry, dict):
            lexicon = entry.get('lexicon', '')
            if lexicon and 'npr' in lexicon.lower():
                return True
        return False

    for entry in lemmata_data:
        # Handle different entry formats
        raw_lemma = None
        pos_info = None
        frequency = 0

        if isinstance(entry, str):
            # Simple string entry
            raw_lemma = entry
        elif isinstance(entry, dict):
            raw_lemma = entry.get('lemma') or entry.get('word') or entry.get('form')
            pos_info = entry.get('pos') or entry.get('part_of_speech') or entry.get('morphology')

            # Extract frequency from lexicon field (CLTK Collatinus format)
            lexicon = entry.get('lexicon', '')
            if lexicon:
                frequency = extract_frequency_from_lexicon(lexicon)

        if not raw_lemma:
            continue

        # Skip proper nouns (capitalized lemmas in Collatinus data)
        if is_proper_noun(entry, raw_lemma):
            continue

        lemma = normalize_lemma(raw_lemma)

        # Skip if empty or contains non-alphabetic characters
        if not lemma or not lemma.isalpha():
            continue
        
        pos_tags = set()
        
        # First, try explicit POS field
        if pos_info:
            if isinstance(pos_info, str):
                pos_tags.update(normalize_latin_pos(pos_info))
            elif isinstance(pos_info, list):
                for pos in pos_info:
                    pos_tags.update(normalize_latin_pos(str(pos)))
            else:
                pos_tags.update(normalize_latin_pos(str(pos_info)))
        
        # If no POS found, try lexicon field (CLTK Collatinus format)
        if not pos_tags and isinstance(entry, dict):
            lexicon = entry.get('lexicon')
            if lexicon:
                pos_tags.update(parse_lexicon_field(lexicon))
        
        # If still no POS, try model field to infer POS
        if not pos_tags and isinstance(entry, dict):
            model = entry.get('model', '').lower()
            if model:
                # Model names often indicate POS: verbs start with verb roots, nouns with noun patterns
                # Common verb models: amo, doceo, lego, etc.
                # Common noun models: uita, puella, etc.
                # Invariable words: inv
                if model == 'inv':
                    # Invariable - could be prep, conj, adv, etc.
                    # Check lexicon if available
                    lexicon = entry.get('lexicon', '')
                    if lexicon:
                        pos_tags.update(parse_lexicon_field(lexicon))
                    else:
                        # Default to Conj for very short invariable words
                        if len(lemma) <= 3:
                            pos_tags.add('Conj')
                elif any(v in model for v in ['amo', 'doceo', 'lego', 'audio', 'capio', 'sum', 'eo']):
                    pos_tags.add('V')
                # For other models, try to infer from common patterns
                elif not pos_tags:
                    # Check if it looks like a verb model (ends with o, or has verb-like patterns)
                    if model.endswith('o') and len(model) > 2:
                        pos_tags.add('V')
        
        # Try morphology field as fallback
        if not pos_tags and isinstance(entry, dict):
            morph = entry.get('morphology') or entry.get('morph') or entry.get('tag')
            if morph:
                pos_tags.update(normalize_latin_pos(str(morph)))
        
        # If still no POS tags found, default to noun
        if not pos_tags:
            pos_tags.add('N')
        
        if pos_tags:
            word_pos[lemma].update(pos_tags)
            # Keep maximum frequency if word appears multiple times
            if lemma not in word_freq or frequency > word_freq[lemma]:
                word_freq[lemma] = frequency
    
    return word_pos, word_freq

def generate_pos_weights(word_pos_dict, word_freq_dict=None, sort_by_frequency=False):
    """
    Generate POS weights for words.
    
    For now, we use equal weights for all POS tags of a word.
    In a production system, you'd want to use frequency data or
    the POS weight generation tool to get accurate weights.
    
    If sort_by_frequency is True, words are sorted by frequency (descending).
    Otherwise, words are sorted alphabetically.
    """
    result = {}
    
    # Determine sort order
    if sort_by_frequency and word_freq_dict:
        # Sort by frequency (descending), then alphabetically
        sorted_words = sorted(
            word_pos_dict.items(),
            key=lambda x: (-word_freq_dict.get(x[0], 0), x[0])
        )
    else:
        # Sort alphabetically
        sorted_words = sorted(word_pos_dict.items())
    
    for word, pos_tags in sorted_words:
        if not pos_tags:
            continue
        
        pos_list = sorted(pos_tags)
        # Equal weights (will be normalized)
        weight = 1.0 / len(pos_list)
        result[word] = {pos: round(weight, 3) for pos in pos_list}
    
    return result

def filter_words(word_pos_dict, word_freq_dict=None, min_length=None, max_length=None, max_words=None, sort_by_frequency=False):
    """
    Filter words by length and limit the number of words.
    
    If max_words is specified and sort_by_frequency is True, selects the most frequent words.
    Otherwise, sorts by word length (shorter first).
    """
    filtered = {}
    filtered_freq = {}
    
    for word, pos_tags in word_pos_dict.items():
        # Length filtering
        if min_length and len(word) < min_length:
            continue
        if max_length and len(word) > max_length:
            continue
        
        filtered[word] = pos_tags
        if word_freq_dict:
            filtered_freq[word] = word_freq_dict.get(word, 0)
    
    # Limit number of words if specified
    if max_words and len(filtered) > max_words:
        if sort_by_frequency and word_freq_dict:
            # Sort by frequency (descending), then alphabetically for ties
            sorted_items = sorted(
                filtered.items(), 
                key=lambda x: (-filtered_freq.get(x[0], 0), x[0])
            )[:max_words]
        else:
            # Sort by word length (shorter first), then alphabetically
            sorted_items = sorted(filtered.items(), key=lambda x: (len(x[0]), x[0]))[:max_words]
        filtered = dict(sorted_items)
    
    return filtered

def main():
    parser = argparse.ArgumentParser(
        description='Generate a Latin wordlist for Glossia from CLTK lemmata data',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    
    parser.add_argument('-o', '--output', type=str, default='latin_wordlist.yaml',
                        help='Output YAML file path (default: latin_wordlist.yaml)')
    parser.add_argument('--lemmata-file', type=str,
                        help='Path to local lemmata JSON file (if not provided, will try to download)')
    parser.add_argument('--min-length', type=int,
                        help='Minimum word length (default: no minimum)')
    parser.add_argument('--max-length', type=int, default=6,
                        help='Maximum word length (default: 6)')
    parser.add_argument('-n', '--max-words', type=int,
                        help='Maximum number of words to include')
    parser.add_argument('--sort-by-frequency', action='store_true',
                        help='Sort output by frequency (most frequent first). Default: True when --max-words is used, False otherwise')
    parser.add_argument('--cover', action='store_true',
                        help='Generate a cover wordlist (filters for shorter, common words)')
    parser.add_argument('--no-download', action='store_true',
                        help='Do not attempt to download lemmata (require --lemmata-file)')
    
    args = parser.parse_args()
    
    # Adjust defaults for cover wordlist
    if args.cover:
        if args.min_length is None:
            args.min_length = 3
        if args.max_length is None:
            args.max_length = 6
        if args.max_words is None:
            args.max_words = 1000
    
    # Load lemmata data
    lemmata_data = []
    
    if args.lemmata_file:
        print(f"Loading lemmata from {args.lemmata_file}...", file=sys.stderr)
        lemmata_data = load_lemmata_from_file(args.lemmata_file)
    elif not args.no_download:
        temp_file = download_cltk_lemmata()
        if temp_file:
            lemmata_data = load_lemmata_from_file(temp_file)
            # Clean up temp file
            try:
                os.unlink(temp_file)
            except:
                pass
        else:
            print("Warning: Could not download lemmata. Please provide --lemmata-file", file=sys.stderr)
            print("You can download lemmata manually from:", file=sys.stderr)
            print("  https://github.com/cltk/lat_models_cltk/tree/master/lemmata", file=sys.stderr)
            sys.exit(1)
    else:
        print("Error: Must provide --lemmata-file or allow download", file=sys.stderr)
        sys.exit(1)
    
    if not lemmata_data:
        print("Error: No lemmata data loaded", file=sys.stderr)
        sys.exit(1)
    
    print(f"Loaded {len(lemmata_data)} lemmata entries", file=sys.stderr)
    
    # Extract words, POS tags, and frequencies
    print("Extracting words, POS tags, and frequencies...", file=sys.stderr)
    word_pos, word_freq = extract_word_pos_from_lemmata(lemmata_data)
    print(f"Found {len(word_pos)} unique words", file=sys.stderr)
    
    # Determine if we should sort by frequency
    # Default: sort by frequency if max_words is specified, or if explicitly requested
    sort_by_freq = args.sort_by_frequency or (args.max_words is not None)
    
    # Filter words
    if args.min_length or args.max_length or args.max_words:
        if sort_by_freq:
            print(f"Filtering words (min_length={args.min_length}, max_length={args.max_length}, max_words={args.max_words}, sorted by frequency)...", file=sys.stderr)
        else:
            print(f"Filtering words (min_length={args.min_length}, max_length={args.max_length}, max_words={args.max_words})...", file=sys.stderr)
        word_pos = filter_words(word_pos, word_freq, args.min_length, args.max_length, args.max_words, sort_by_frequency=sort_by_freq)
        print(f"After filtering: {len(word_pos)} words", file=sys.stderr)
    elif sort_by_freq:
        # Even without max_words, apply frequency sorting if requested
        print(f"Sorting all words by frequency...", file=sys.stderr)
        word_pos = filter_words(word_pos, word_freq, args.min_length, args.max_length, None, sort_by_frequency=True)
    
    # Show frequency statistics if using frequency sorting
    if sort_by_freq and word_freq:
        freqs = [word_freq.get(w, 0) for w in word_pos.keys()]
        if freqs:
            print(f"Frequency range: {min(freqs)} - {max(freqs)}", file=sys.stderr)
            print(f"Average frequency: {sum(freqs) / len(freqs):.1f}", file=sys.stderr)
    
    # Generate POS weights (preserving frequency sort order if requested)
    print("Generating POS weights...", file=sys.stderr)
    wordlist = generate_pos_weights(word_pos, word_freq, sort_by_frequency=sort_by_freq)
    
    # Calculate POS distribution for header comment
    pos_counts = defaultdict(int)
    for word_data in wordlist.values():
        for pos in word_data.keys():
            pos_counts[pos] += 1
    
    # Write YAML output with header comment
    print(f"Writing output to {args.output}...", file=sys.stderr)
    with open(args.output, 'w', encoding='utf-8') as f:
        # Write header comment with script inputs and statistics
        f.write("# Latin Wordlist for Glossia\n")
        f.write("# Generated by generate_latin_wordlist.py\n")
        f.write("#\n")
        f.write("# Script Inputs:\n")
        if args.lemmata_file:
            f.write(f"#   --lemmata-file: {args.lemmata_file}\n")
        if args.min_length:
            f.write(f"#   --min-length: {args.min_length}\n")
        if args.max_length and args.max_length != 6:  # Only show if not default
            f.write(f"#   --max-length: {args.max_length}\n")
        elif args.max_length == 6:
            f.write(f"#   --max-length: {args.max_length} (default)\n")
        if args.max_words:
            f.write(f"#   --max-words: {args.max_words}\n")
        if args.sort_by_frequency:
            f.write(f"#   --sort-by-frequency: True\n")
        if args.cover:
            f.write(f"#   --cover: True\n")
        f.write("#\n")
        f.write("# Statistics:\n")
        f.write(f"#   Total words: {len(wordlist):,}\n")
        if sort_by_freq and word_freq:
            freqs = [word_freq.get(w, 0) for w in word_pos.keys()]
            if freqs:
                f.write(f"#   Frequency range: {min(freqs)} - {max(freqs)}\n")
                f.write(f"#   Average frequency: {sum(freqs) / len(freqs):.1f}\n")
        f.write("#\n")
        f.write("# POS Tag Distribution:\n")
        for pos, count in sorted(pos_counts.items()):
            percentage = (count / len(wordlist)) * 100 if wordlist else 0
            f.write(f"#   {pos}: {count:,} ({percentage:.1f}%)\n")
        f.write("#\n")
        f.write("# Wordlist (sorted by frequency, most frequent first):\n")
        f.write("#\n")
        
        # Write the actual wordlist
        # If sorted by frequency, preserve order (don't sort keys)
        # Otherwise, sort alphabetically
        yaml.dump(wordlist, f, default_flow_style=False, sort_keys=not sort_by_freq, allow_unicode=True)
    
    print(f"\nSuccessfully generated wordlist with {len(wordlist)} words", file=sys.stderr)
    print(f"Output saved to: {args.output}", file=sys.stderr)
    
    # Print some statistics
    print("\nPOS tag distribution:", file=sys.stderr)
    for pos, count in sorted(pos_counts.items()):
        percentage = (count / len(wordlist)) * 100 if wordlist else 0
        print(f"  {pos}: {count:,} ({percentage:.1f}%)", file=sys.stderr)

if __name__ == '__main__':
    main()
