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


def is_infinitive(word: str) -> bool:
    """German infinitives end in -en / -eln / -ern (machen, basteln, wandern)."""
    return word.endswith(("en", "eln", "ern"))


def classify(word: str, tagger, nouns):
    """Return (weighted {POS: weight}, gender_override_or_None) for one word.

    Because a payload word can never change form (it carries bits and decode
    filters on the exact wordlist), verbs are tagged into positions where their
    citation form is grammatical, not conjugated:

      * infinitives  -> nominalized neuter nouns ("das Abwarten"): tagged N/n
      * participles  -> predicative adjectives ("ist erhalten"):    tagged Adj
      * finite forms -> kept as finite verbs (baut, ankommt):       tagged V
    """
    _lemma, tag = tagger.analyze(word, taglevel=1)
    m = map_stts(tag)
    is_noun = bool(nouns[word.capitalize()])

    # A genuine noun in the lexicon: keep it a noun (its citation form is the
    # nominative singular, which is grammatical in subject/object position).
    if is_noun:
        if m == "N":
            return {"N": 1.0}, None
        # HanTa read it as something else but the lexicon knows it as a noun
        # (kraft/Kraft, laut/Laut ...): noun reading dominates in a word list.
        if m in ("V", "Adj", "Adv", "Prep", "Conj", "Pron"):
            return {"N": 0.6, m: 0.4}, None
        return {"N": 1.0}, None

    # Not a lexicon noun. Resolve verbs by form so the citation stays grammatical.
    if tag.startswith("VV(PP)"):
        return {"Adj": 1.0}, None          # past participle -> predicative
    if is_infinitive(word):
        return {"N": 1.0}, "n"             # infinitive -> nominalized (neuter)
    if m == "V":
        return {"V": 1.0}, None            # genuine finite form (baut, ankommt)
    if m in ("Adj", "Adv", "Prep", "Conj", "Pron"):
        return {m: 1.0}, None

    # Garbage tag (FM/XY), not a noun, not an infinitive: treat as adjective.
    return {"Adj": 1.0}, None


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
    nominalized = 0
    for w in words:
        tags, gender_override = classify(w, tagger, nouns)
        for p in tags:
            dist[p] += 1
        lines.append(f"{w}:")
        for pos, weight in tags.items():
            lines.append(f"  {pos}: {weight}")
        # Gender for the agreement pass: an explicit override (nominalized
        # infinitives are always neuter), else the noun lexicon's genus.
        gender = gender_override or lookup_gender(w, nouns)
        if gender and "N" in tags:
            lines.append(f"  gender: {gender}")
            gender_count += 1
        if gender_override:
            nominalized += 1

    with open(dst, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")

    print(f"wrote {dst}: {len(words)} words")
    print("POS distribution (tag occurrences):", dict(dist))
    print(f"words with gender: {gender_count}")
    print(f"nominalized infinitives (verb->neuter noun): {nominalized}")


if __name__ == "__main__":
    main()
