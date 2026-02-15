#!/usr/bin/env python3
"""
Extract English WordNet lemma list for inspection or use as payload candidate source.

Requires: pip install nltk && python -c "import nltk; nltk.download('wordnet')"

Usage:
  python wordnet_lemmas.py [--output wordnet_lemmas.txt] [--by-pos]
  python wordnet_lemmas.py --include-proper-nouns   # include proper nouns

By default, lemmas that appear in WordNet instance synsets (proper nouns) are excluded.
Output: one lemma per line (lowercase, single-word only).
"""

import argparse
import sys
from pathlib import Path

try:
    from nltk.corpus import wordnet as wn
except ImportError:
    print("Error: nltk is required. Run: pip install nltk", file=sys.stderr)
    print("Then: python -c \"import nltk; nltk.download('wordnet')\"", file=sys.stderr)
    sys.exit(1)


# WordNet POS codes -> our simplified names
POS_NAMES = {"n": "noun", "v": "verb", "a": "adj", "r": "adv", "s": "adj_sat"}


def _is_single_word_alpha(name_lower):
    return "_" not in name_lower and name_lower.replace("_", "").isalpha()


def proper_noun_lemmas():
    """
    Lemmas that appear in any WordNet instance-hyponym synset (proper nouns / named entities).
    Single-word, alphabetic only, lowercased.
    """
    proper = set()
    for synset in wn.all_synsets():
        for instance_syn in synset.instance_hyponyms():
            for lemma in instance_syn.lemmas():
                name = lemma.name().lower()
                if _is_single_word_alpha(name):
                    proper.add(name)
    return proper


def all_english_lemma_names(single_word_only=True, alphabetic_only=True, exclude_proper=True):
    """Yield all unique English WordNet lemma names, optionally filtered."""
    proper = proper_noun_lemmas() if exclude_proper else set()
    if exclude_proper:
        print(f"Excluding {len(proper)} lemmas that appear in instance (proper noun) synsets...", file=sys.stderr)
    seen = set()
    for pos in ["n", "v", "a", "r", "s"]:
        for name in wn.all_lemma_names(pos=pos, lang="eng"):
            name_lower = name.lower()
            if single_word_only and "_" in name_lower:
                continue
            if alphabetic_only and not name_lower.replace("_", "").isalpha():
                continue
            if exclude_proper and name_lower in proper:
                continue
            if name_lower in seen:
                continue
            seen.add(name_lower)
            yield name_lower


def main():
    ap = argparse.ArgumentParser(description="Extract WordNet English lemmas")
    ap.add_argument(
        "--output", "-o",
        type=Path,
        default=Path(__file__).parent / "wordnet_lemmas.txt",
        help="Output file (one lemma per line)",
    )
    ap.add_argument(
        "--by-pos",
        action="store_true",
        help="Print counts per POS (noun, verb, adj, adv)",
    )
    ap.add_argument(
        "--include-proper-nouns",
        action="store_true",
        help="Include lemmas that appear in instance (proper noun) synsets (default: exclude)",
    )
    args = ap.parse_args()

    lemmas = sorted(all_english_lemma_names(exclude_proper=not args.include_proper_nouns))
    total = len(lemmas)

    if args.by_pos:
        proper = proper_noun_lemmas() if not args.include_proper_nouns else set()
        pos_count = {"n": set(), "v": set(), "a": set(), "r": set(), "s": set()}
        for pos in pos_count:
            for name in wn.all_lemma_names(pos=pos, lang="eng"):
                name_lower = name.lower()
                if "_" in name_lower or not name_lower.replace("_", "").isalpha():
                    continue
                if not args.include_proper_nouns and name_lower in proper:
                    continue
                pos_count[pos].add(name_lower)
        print("WordNet English lemmas (single-word, alphabetic) by POS:")
        for pos, names in pos_count.items():
            print(f"  {POS_NAMES.get(pos, pos)}: {len(names)}")
        print(f"  (unique total: {total})")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with open(args.output, "w") as f:
        for w in lemmas:
            f.write(w + "\n")
    print(f"Wrote {total} lemmas to {args.output}")

    # Sample first and last
    if lemmas:
        print("  First 20:", " ".join(lemmas[:20]))
        print("  Last 20:", " ".join(lemmas[-20:]))


if __name__ == "__main__":
    main()
