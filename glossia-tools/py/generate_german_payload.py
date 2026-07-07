#!/usr/bin/env python3
"""Generate languages/german/payload.yaml from the dys2p de-2048 wordlist.

POS tags come from two offline sources, combined:
  * HanTa (Hanover Tagger) — statistical German POS tagger (STTS tagset)
  * german-nouns — Wiktionary-derived noun lexicon (disambiguates HanTa's
    FM/XY guesses on loanwords, which are almost always nouns)

The input word ORDER is preserved verbatim: Glossia's codec derives payload
indices from YAML key order, so the canonical BIP39 ordering is authoritative.

Usage:
    python3 glossia-tools/py/generate_german_payload.py \
        languages/german/german_bip39.txt languages/german/payload.yaml
"""
import sys
from collections import Counter
from HanTa import HanoverTagger as ht
from german_nouns.lookup import Nouns

# STTS (Stuttgart-Tübingen TagSet) -> Glossia POS
def map_stts(tag: str) -> str:
    if tag.startswith(("NN", "NE")):
        return "N"
    if tag.startswith(("VV", "VA", "VM")):
        return "V"
    if tag.startswith("ADJ"):
        return "Adj"
    if tag.startswith("ADV"):
        return "Adv"
    if tag.startswith("APP"):        # APPR, APPRART, APPO
        return "Prep"
    if tag.startswith("KO"):         # KON, KOUS, KOUI, KOKOM
        return "Conj"
    if tag in ("PIS", "PIAT", "PPER", "PDS", "PRELS", "PWS"):
        return "Pron"
    return tag                        # FM, XY, PTKVZ, CARD ... -> handled below


_GENUS_TO_CHAR = {
    "m": "m", "männlich": "m",
    "f": "f", "weiblich": "f",
    "n": "n", "neutrum": "n", "sächlich": "n",
}


def lookup_gender(word: str, nouns) -> str | None:
    """Grammatical gender (m/f/n) for a noun, from german-nouns' `genus`.

    Takes the first entry's genus; words tagged with several genera keep the
    most common (first-listed) reading, which is a fine default for a word list.
    """
    for entry in nouns[word.capitalize()]:
        for key in ("genus", "genus 1"):
            g = entry.get(key)
            if isinstance(g, str):
                c = _GENUS_TO_CHAR.get(g.strip().lower())
                if c:
                    return c
    return None


def classify(word: str, tagger, nouns) -> dict:
    """Return a weighted {POS: weight} dict for one payload word."""
    _lemma, tag = tagger.analyze(word, taglevel=1)
    m = map_stts(tag)
    is_noun = bool(nouns[word.capitalize()])

    # Garbage tags (FM foreign, XY non-word, PTKVZ particle, CARD, etc.):
    # trust the noun lexicon first, then fall back to morphology.
    if m not in ("N", "V", "Adj", "Adv", "Prep", "Conj", "Pron"):
        if is_noun:
            return {"N": 1.0}
        if word.endswith(("en", "eln", "ern", "ieren")):
            return {"V": 1.0}
        return {"Adj": 1.0}

    # Clean content tag from HanTa.
    if m == "N":
        return {"N": 1.0}

    # HanTa picked a non-noun reading. If the lexicon also knows it as a noun
    # (kraft/Kraft, laut/Laut, paar/Paar ...), the noun reading dominates in a
    # word-list context; keep the alternate reading as a secondary sense.
    if is_noun:
        return {"N": 0.6, m: 0.4}
    return {m: 1.0}


def main() -> None:
    src, dst = sys.argv[1], sys.argv[2]
    words = [w.strip() for w in open(src, encoding="utf-8") if w.strip()]

    tagger = ht.HanoverTagger("morphmodel_ger.pgz")
    nouns = Nouns()

    dist = Counter()
    gender_count = 0
    lines = [
        "# German payload wordlist — dys2p de-2048-v1 (BIP39-compatible)",
        "#",
        "# POS tags generated offline by glossia-tools/py/generate_german_payload.py",
        "# (HanTa POS tagger + german-nouns Wiktionary lexicon). Nouns also carry a",
        "# `gender` (m/f/n) used by the German agreement pass for article",
        "# gender/case selection. Word order is the canonical dys2p ordering and is",
        "# authoritative for codec indices.",
        "#",
        f"# Total words: {len(words)}",
        "",
    ]
    for w in words:
        tags = classify(w, tagger, nouns)
        for p in tags:
            dist[p] += 1
        lines.append(f"{w}:")
        for pos, weight in tags.items():
            lines.append(f"  {pos}: {weight}")
        # Annotate gender for anything the noun lexicon recognizes (the
        # agreement pass only consults it for words landing in N slots).
        gender = lookup_gender(w, nouns)
        if gender:
            lines.append(f"  gender: {gender}")
            gender_count += 1

    with open(dst, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")

    print(f"wrote {dst}: {len(words)} words")
    print("POS distribution (tag occurrences):", dict(dist))
    print(f"words with gender: {gender_count}")


if __name__ == "__main__":
    main()
