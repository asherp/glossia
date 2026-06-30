#!/usr/bin/env python3
"""
Script to get the top N most common words with 6 or fewer characters
from word frequency data, formatted for BIP39 encode usage.

Output format: word|POS1,POS2 (matching cover_POS.txt format)

Supports multiple data sources:
1. COCA word frequency data from wordfrequency.info (recommended)
2. Google Books Ngram 1-gram files
3. CSV frequency files

Data source: https://www.wordfrequency.info/samples.asp
"""

import sys
import gzip
from collections import defaultdict
from urllib.request import urlopen
import tempfile
import os
import re

def normalize_pos(pos_str):
    """
    Convert POS values to simplified format used in cover_POS.txt
    Returns a set of normalized POS tags.
    """
    if not pos_str:
        return set()
    
    pos_str = pos_str.strip().lower()
    pos_tags = set()
    
    # Map to simplified POS tags
    # Noun
    if re.search(r'\bn\.?\b', pos_str) or 'noun' in pos_str:
        pos_tags.add('N')
    
    # Verb (transitive, intransitive, or general)
    if re.search(r'\bv\.?\s*(t\.?|i\.?)?\b', pos_str) or 'verb' in pos_str:
        pos_tags.add('V')
    
    # Adjective
    if re.search(r'\ba\.?\b', pos_str) or re.search(r'\badj\.?\b', pos_str) or 'adjective' in pos_str:
        pos_tags.add('Adj')
    
    # Adverb
    if re.search(r'\badv\.?\b', pos_str) or 'adverb' in pos_str:
        pos_tags.add('Adv')
    
    # Preposition
    if re.search(r'\bprep\.?\b', pos_str) or 'preposition' in pos_str:
        pos_tags.add('Prep')
    
    # Conjunction
    if re.search(r'\bconj\.?\b', pos_str) or 'conjunction' in pos_str:
        pos_tags.add('Conj')
    
    # Pronoun
    if re.search(r'\bpron\.?\b', pos_str) or 'pronoun' in pos_str:
        pos_tags.add('Pron')
    
    # Determiner (definite article, etc.)
    if 'def. art.' in pos_str or 'definite article' in pos_str or 'det.' in pos_str:
        pos_tags.add('Det')
    
    return pos_tags

def parse_ngram_line(line):
    """
    Parse a line from Google Books Ngram 1-gram file.
    Format: word TAB year TAB match_count TAB page_count TAB volume_count
    Returns: (word, match_count, pos_tags)
    """
    parts = line.strip().split('\t')
    if len(parts) >= 3:
        word = parts[0].lower()
        pos_tags = set()
        
        # Extract POS tags if present (word_POS format)
        if '_' in word:
            word_part, pos_part = word.split('_', 1)
            word = word_part
            pos_tags = normalize_pos(pos_part)
        
        try:
            match_count = int(parts[2])
            return word, match_count, pos_tags
        except (ValueError, IndexError):
            pass
    return None, 0, set()

def process_ngram_file(file_path):
    """
    Process a Google Books Ngram 1-gram file.
    Aggregates frequencies across all years for each word.
    Returns dictionary of word -> (total_frequency, pos_tags_set)
    """
    word_data = defaultdict(lambda: {'freq': 0, 'pos': set()})
    
    # Determine if file is gzipped
    open_func = gzip.open if file_path.endswith('.gz') else open
    mode = 'rt' if file_path.endswith('.gz') else 'r'
    
    try:
        print(f"Processing {file_path}...", file=sys.stderr)
        with open_func(file_path, mode, encoding='utf-8', errors='ignore') as f:
            line_count = 0
            for line in f:
                word, freq, pos_tags = parse_ngram_line(line)
                if word and len(word) <= 6 and word.isalpha():
                    word_data[word]['freq'] += freq
                    word_data[word]['pos'].update(pos_tags)
                
                line_count += 1
                if line_count % 1000000 == 0:
                    print(f"  Processed {line_count:,} lines...", file=sys.stderr)
        
        print(f"Found {len(word_data):,} unique words with 6 or fewer characters", file=sys.stderr)
        return word_data
    except Exception as e:
        print(f"Error processing file {file_path}: {e}", file=sys.stderr)
        return {}

def download_wordfrequency_data():
    """
    Download the free top 5000 word frequency list from wordfrequency.info
    Source: https://www.wordfrequency.info/samples.asp
    """
    txt_url = "https://www.wordfrequency.info/samples/lemmas_60k.txt"
    
    print("Downloading word frequency data from wordfrequency.info...", file=sys.stderr)
    print("Source: https://www.wordfrequency.info/samples.asp", file=sys.stderr)
    
    try:
        with urlopen(txt_url, timeout=30) as response:
            data = response.read().decode('utf-8', errors='ignore')
            
        # Save to temp file
        with tempfile.NamedTemporaryFile(mode='w', delete=False, suffix='.txt') as f:
            f.write(data)
            temp_file = f.name
        
        print(f"Downloaded to {temp_file}", file=sys.stderr)
        return temp_file
    except Exception as e:
        print(f"Error downloading frequency list: {e}", file=sys.stderr)
        return None

def parse_wordfrequency_line(line):
    """
    Parse a line from wordfrequency.info lemmas file.
    Format appears to be tab or pipe separated with: rank, lemma, pos, frequency data, etc.
    Returns: (word, freq, pos_tags)
    """
    # Try different separators
    for sep in ['\t', '|', ',']:
        if sep in line:
            parts = line.strip().split(sep)
            if len(parts) >= 4:
                try:
                    rank = int(parts[0])
                    word = parts[1].strip().lower()
                    pos_str = parts[2].strip() if len(parts) > 2 else ''
                    
                    # Remove POS tags if present in word (word_POS format)
                    if '_' in word:
                        word_part, pos_part = word.split('_', 1)
                        word = word_part
                        if not pos_str:
                            pos_str = pos_part
                    
                    # Parse frequency
                    freq = None
                    for part in parts[3:]:
                        try:
                            freq = float(part.strip())
                            break
                        except ValueError:
                            continue
                    
                    if word and freq is not None:
                        pos_tags = normalize_pos(pos_str)
                        return word, freq, pos_tags
                except (ValueError, IndexError):
                    continue
    return None, None, set()

def get_top_words_from_wordfrequency(file_path, top_n=1000):
    """
    Get top words from wordfrequency.info format file.
    Returns dictionary of word -> {'freq': frequency, 'pos': set_of_pos_tags}
    """
    word_data = {}
    
    try:
        with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
            header_skipped = False
            for line_num, line in enumerate(f):
                # Skip header lines
                if not header_skipped:
                    if 'rank' in line.lower() or 'lemma' in line.lower() or line_num < 2:
                        header_skipped = True
                        continue
                
                word, freq, pos_tags = parse_wordfrequency_line(line)
                if word and freq is not None and len(word) <= 6 and word.isalpha():
                    # Keep highest frequency if word appears multiple times, merge POS tags
                    if word not in word_data:
                        word_data[word] = {'freq': freq, 'pos': pos_tags}
                    else:
                        if freq > word_data[word]['freq']:
                            word_data[word]['freq'] = freq
                        word_data[word]['pos'].update(pos_tags)
    except FileNotFoundError:
        print(f"File not found: {file_path}", file=sys.stderr)
        return {}
    except Exception as e:
        print(f"Error reading {file_path}: {e}", file=sys.stderr)
        return {}
    
    return word_data

def get_top_words_from_csv(csv_file, top_n=1000):
    """
    Get top words from a CSV frequency file.
    Expected format: word,frequency or word,frequency,...
    Returns dictionary of word -> {'freq': frequency, 'pos': set()}
    """
    word_data = {}
    
    try:
        with open(csv_file, 'r', encoding='utf-8') as f:
            header_skipped = False
            for line in f:
                if not header_skipped and ('word' in line.lower() or 'freq' in line.lower()):
                    header_skipped = True
                    continue
                
                parts = line.strip().split(',')
                if len(parts) >= 2:
                    word = parts[0].strip().lower()
                    try:
                        freq = float(parts[1].strip())
                        if word and len(word) <= 6 and word.isalpha():
                            # Keep highest frequency if word appears multiple times
                            if word not in word_data:
                                word_data[word] = {'freq': freq, 'pos': set()}
                            elif freq > word_data[word]['freq']:
                                word_data[word]['freq'] = freq
                    except ValueError:
                        continue
    except FileNotFoundError:
        print(f"File not found: {csv_file}", file=sys.stderr)
        return {}
    except Exception as e:
        print(f"Error reading {csv_file}: {e}", file=sys.stderr)
        return {}
    
    return word_data

if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(
        description='Get top N most common words with 6 or fewer characters from word frequency data',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Download and use COCA word frequency data (recommended)
  python get_top_words.py -n 1000 --download-coca -o output.txt
  
  # Use a wordfrequency.info format file
  python get_top_words.py -n 1000 --wordfreq lemmas_60k.txt -o output.txt
  
  # Use a CSV frequency file
  python get_top_words.py -n 1000 --csv word-freq.csv -o output.txt
  
  # Process Google Books Ngram file(s)
  python get_top_words.py -n 1000 --ngram file1.gz -o output.txt

Output format: word|POS1,POS2 (matching cover_POS.txt format)
Words without POS tags will be output as just the word.

Data sources:
  - COCA (Corpus of Contemporary American English): https://www.wordfrequency.info/samples.asp
  - Google Books Ngram: https://storage.googleapis.com/books/ngrams/books/
        """
    )
    parser.add_argument('-n', '--top-n', type=int, default=1000,
                        help='Number of top words to return (default: 1000)')
    parser.add_argument('-o', '--output', type=str,
                        help='Output file path (default: stdout)')
    parser.add_argument('--ngram', type=str, nargs='+',
                        help='Path(s) to Google Books Ngram 1-gram file(s) (.gz or plain text)')
    parser.add_argument('--csv', type=str,
                        help='Path to CSV frequency file (format: word,frequency)')
    parser.add_argument('--wordfreq', type=str,
                        help='Path to wordfrequency.info format file (lemmas_60k.txt format)')
    parser.add_argument('--download-coca', action='store_true',
                        help='Download free COCA word frequency data from wordfrequency.info')
    
    args = parser.parse_args()
    
    word_data = {}
    
    if args.ngram:
        # Process Google Books Ngram file(s)
        all_word_data = {}
        for ngram_file in args.ngram:
            if not os.path.exists(ngram_file):
                print(f"Error: File not found: {ngram_file}", file=sys.stderr)
                continue
            
            file_data = process_ngram_file(ngram_file)
            # Merge data (sum frequencies, merge POS tags)
            for word, data in file_data.items():
                if word not in all_word_data:
                    all_word_data[word] = {'freq': 0, 'pos': set()}
                all_word_data[word]['freq'] += data['freq']
                all_word_data[word]['pos'].update(data['pos'])
        
        word_data = all_word_data
        if not word_data:
            print("No words found in Ngram files.", file=sys.stderr)
            sys.exit(1)
            
    elif args.download_coca:
        # Download COCA word frequency data
        temp_file = download_wordfrequency_data()
        if temp_file:
            word_data = get_top_words_from_wordfrequency(temp_file, args.top_n * 2)
            # Clean up temp file
            try:
                os.unlink(temp_file)
            except:
                pass
        else:
            print("Download failed.", file=sys.stderr)
            sys.exit(1)
            
    elif args.wordfreq:
        # Use wordfrequency.info format file
        print(f"Reading wordfrequency.info file: {args.wordfreq}", file=sys.stderr)
        word_data = get_top_words_from_wordfrequency(args.wordfreq, args.top_n)
        
    elif args.csv:
        # Use CSV frequency file
        print(f"Reading CSV file: {args.csv}", file=sys.stderr)
        word_data = get_top_words_from_csv(args.csv, args.top_n)
    else:
        parser.print_help()
        print("\nError: Must specify one of: --download-coca, --wordfreq, --csv, or --ngram", file=sys.stderr)
        sys.exit(1)
    
    if not word_data:
        print("No words found. Check your input files.", file=sys.stderr)
        sys.exit(1)
    
    # Filter for words with 6 or fewer characters, no punctuation
    filtered = {w: d for w, d in word_data.items() 
                if len(w) <= 6 and w.isalpha()}
    
    # Sort by frequency (descending) and get top N
    sorted_words = sorted(filtered.items(), key=lambda x: x[1]['freq'], reverse=True)[:args.top_n]
    
    # Output results in cover_POS.txt format
    output_file = open(args.output, 'w') if args.output else sys.stdout
    
    for word, data in sorted_words:
        pos_tags = sorted(data['pos'])
        if pos_tags:
            pos_str = ','.join(pos_tags)
            output_file.write(f"{word}|{pos_str}\n")
        else:
            # Output word without POS if no tags available
            output_file.write(f"{word}\n")
    
    if args.output:
        output_file.close()
        print(f"\nTop {len(sorted_words)} words saved to {args.output}", file=sys.stderr)
        words_with_pos = sum(1 for _, d in sorted_words if d['pos'])
        print(f"Words with POS tags: {words_with_pos}/{len(sorted_words)}", file=sys.stderr)
        if sorted_words:
            print(f"Frequency range: {sorted_words[-1][1]['freq']:,.0f} to {sorted_words[0][1]['freq']:,.0f}", file=sys.stderr)
    else:
        print(f"\nTotal words: {len(sorted_words)}", file=sys.stderr)
