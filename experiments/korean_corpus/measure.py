#!/usr/bin/env python3
"""How big a Korean payload wordlist can a frequency corpus support?

Starts from the Shiberu Korean Word Frequency List 2026 (80,000 lemmas,
CC BY 4.0, https://shiberu.app/korean-frequency-list) and walks it through
the filters a Glossia payload list needs, then greedily keeps words in rank
order so that every surface form a word can take in prose (noun + particle,
or a conjugated verb) maps back to exactly one payload index.

Usage:
    pip install kiwipiepy
    python3 experiments/korean_corpus/measure.py ko-frequency-2026.tsv \
        --cover cover.yaml --bip39 payload.yaml [--out report.json]

`--cover` / `--bip39` are the Korean cover.yaml and payload.yaml from the
Korean-language PR (#97); both are optional.

Four tiers are measured, two vocabularies times two strictness levels:

  nouns    payload words are nouns, adverbs and prenouns. This is exactly what
           PR #97's machinery supports today: particles attached by the
           `morphology: korean` pass, decoding by stripping one particle.
  open     nouns plus verbs and adjectives, each verb rendered by a fixed set
           of template conjugations. Needs a conjugation step in the generator
           and a surface->index table on decode (irregulars make suffix
           stripping unsound: 걷다 and 걸다 both give 걸었다).

  loose    any lemma Kiwi tags as one of those categories, including its
           out-of-vocabulary guesses and spaceless phrases (사회변화).
  strict   Kiwi's own lexicon must hold the word as a single morpheme.

Results and discussion: experiments/korean_corpus/README.md.
"""
import argparse
import json
import re
import sys
import unicodedata
from collections import Counter, defaultdict

from kiwipiepy import Kiwi

HANGUL = re.compile(r"^[가-힣]+$")

# PR #97's particle inventory, as (after-consonant, after-vowel) pairs.
PARTICLES = [("이", "가"), ("은", "는"), ("을", "를"), ("에", "에"),
             ("에서", "에서"), ("과", "와"), ("의", "의"), ("으로", "로")]

# Template conjugations for the `open` tier (kiwi morpheme sequences).
VERB_ENDINGS = {
    "past": [("었", "EP"), ("다", "EF")],     # 갔다 / 먹었다
    "and": [("고", "EC")],                    # 가고
    "so": [("어서", "EC")],                   # 가서
    "polite": [("어요", "EF")],               # 가요
}
ADNOMINAL = {"verb": [("는", "ETM")], "adj": [("은", "ETM")]}  # 가는 / 예쁜

SIZES = [2 ** k for k in range(11, 17)]


def nfc(s):
    return unicodedata.normalize("NFC", s)


def jamo(s):
    return unicodedata.normalize("NFD", s)


def has_batchim(word):
    code = ord(word[-1]) - 0xAC00
    return 0 <= code < 11172 and code % 28 != 0


def final_is_rieul(word):
    code = ord(word[-1]) - 0xAC00
    return 0 <= code < 11172 and code % 28 == 8


def noun_forms(word):
    out = {word}
    b = has_batchim(word)
    for cons, vow in PARTICLES:
        if cons == "으로" and final_is_rieul(word):
            out.add(word + "로")
        else:
            out.add(word + (cons if b else vow))
    return out


def classify(kiwi, lemma):
    """Return (category, stem_morphemes) or (reason_rejected, None)."""
    toks = kiwi.analyze(lemma)[0][0]
    tags = [t.tag for t in toks]
    # Kiwi gives out-of-vocabulary guesses morpheme id 2: a string its own
    # lexicon does not hold (히가시, 앵그리버드, 쀍뀹). See `is_oov`.
    if lemma.endswith("다") and len(toks) >= 2 and toks[-1].form == "다" \
            and toks[-1].tag in ("EF", "EC"):
        stem = [(t.form, t.tag) for t in toks[:-1]]
        last = stem[-1][1].split("-")[0]
        if last in ("VV", "XSV"):
            cat = "verb"
        elif last in ("VA", "XSA"):
            cat = "adj"
        else:
            return "other-predicate", None
        if last in ("XSV", "XSA"):
            return cat + "-derived", stem      # 공부하다, 행복하다, 사용되다
        if len(stem) > 1:
            return "compound-predicate", None
        return cat, stem
    if len(toks) == 1:
        t = tags[0]
        return {"NNG": "noun", "MAG": "adverb", "MM": "prenoun",
                "NNP": "proper-noun", "NNB": "bound-noun", "NP": "pronoun",
                "NR": "numeral", "IC": "interjection"}.get(t, "function:" + t), None
    if all(t in ("NNG", "XSN", "XPN") for t in tags) and tags[0] in ("NNG", "XPN"):
        return "noun-compound", None           # 선생님, 김치찌개 split by kiwi
    return "multi:" + "+".join(tags), None


def is_oov(kiwi, lemma):
    return any(t.id == 2 for t in kiwi.analyze(lemma)[0][0])


def conjugate(kiwi, cat, stem):
    base = "verb" if cat.startswith("verb") else "adj"
    forms = {}
    for name, ending in VERB_ENDINGS.items():
        forms[name] = nfc(kiwi.join(stem + ending))
    forms["adnominal"] = nfc(kiwi.join(stem + ADNOMINAL[base]))
    return set(forms.values())


def edit1_fraction(words):
    """Share of words with another word at jamo edit distance 1."""
    js = {w: jamo(w) for w in words}
    sub = defaultdict(set)
    dels = defaultdict(set)
    full = {}
    for w, j in js.items():
        full[j] = w
        for i in range(len(j)):
            sub[(i, j[:i] + j[i + 1:])].add(w)
            dels[j[:i] + j[i + 1:]].add(w)
    close = set()
    for group in sub.values():
        if len(group) > 1:
            close |= group
    for d, ws in dels.items():          # w minus a jamo is another word
        if d in full:
            close |= ws
            close.add(full[d])
    return len(close) / max(1, len(words))


def load_yaml_keys(path):
    keys = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            m = re.match(r"^([^\s#][^:]*):\s*$", line)
            if m:
                keys.append(nfc(m.group(1).strip().strip("'\"")))
    return keys


def load_cover(path):
    cover, cover_nouns = set(), set()
    cur = None
    with open(path, encoding="utf-8") as f:
        for line in f:
            m = re.match(r"^([^\s#][^:]*):\s*$", line)
            if m:
                cur = nfc(m.group(1).strip().strip("'\""))
                cover.add(cur)
            elif cur and re.match(r"^\s+(N|Pron):", line):
                cover_nouns.add(cur)
    return cover, cover_nouns


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tsv")
    ap.add_argument("--cover")
    ap.add_argument("--bip39")
    ap.add_argument("--out")
    ap.add_argument("--dump", help="write each tier's kept words (rank, word, category) to DUMP.<tier>.tsv")
    ap.add_argument("--min-syll", type=int, default=2)
    ap.add_argument("--max-syll", type=int, default=5)
    args = ap.parse_args()

    rows = []
    with open(args.tsv, encoding="utf-8") as f:
        next(f)
        for line in f:
            r, lemma, pm = line.rstrip("\n").split("\t")
            rows.append((int(r), nfc(lemma), float(pm)))
    rank_of = {w: r for r, w, _ in rows}
    pm_of = {w: p for _, w, p in rows}

    kiwi = Kiwi()
    funnel = Counter()
    funnel["0 all lemmas"] = len(rows)
    cats = Counter()
    entries = []      # (rank, lemma, category, stem)
    for r, w, _ in rows:
        if not HANGUL.match(w):
            cats["non-hangul"] += 1
            continue
        cat, stem = classify(kiwi, w)
        cats[cat] += 1
        entries.append((r, w, cat, stem, is_oov(kiwi, w)))
    funnel["1 hangul only"] = len(entries)

    cover, cover_nouns = (load_cover(args.cover) if args.cover else (set(), set()))
    reserved = set(cover)
    for c in cover_nouns:
        reserved |= noun_forms(c)

    nounish = ("noun", "noun-compound", "adverb", "prenoun")
    predicate = ("verb", "adj", "verb-derived", "adj-derived")
    nouns_present = {w for _, w, c, _, _ in entries if c in ("noun", "noun-compound")}

    def syll_ok(w, cat):
        n = len(w) - (1 if cat in predicate else 0)   # don't count the 다
        return args.min_syll <= n <= args.max_syll

    results = {}
    tiers = {
        # name: (allowed categories)
        # name: (allowed categories, strict)
        "nouns-loose": (nounish, False),
        "open-loose": (nounish + predicate, False),
        # strict: Kiwi's lexicon must know the word as one morpheme, so no
        # out-of-vocabulary guesses and no spaceless phrases (사회변화).
        "nouns-strict": (("noun", "adverb", "prenoun"), True),
        "open-strict": (("noun", "adverb", "prenoun") + predicate, True),
    }
    for tier, (allowed, strict) in tiers.items():
        stage = Counter()
        taken = {}
        chosen = []
        drop = Counter()
        for r, w, cat, stem, oov in entries:
            if cat not in allowed:
                continue
            stage["pos"] += 1
            if strict and oov:
                drop["not in Kiwi's lexicon"] += 1
                continue
            if not syll_ok(w, cat):
                drop["length"] += 1
                continue
            stage["length"] += 1
            if w in cover:
                drop["is a cover word"] += 1
                continue
            if cat in ("verb-derived", "adj-derived"):
                # X하다 / X되다 next to the noun X: the BIP39 review's complaint.
                root = "".join(f for f, t in stem[:-1])
                if root in nouns_present:
                    drop["하다/되다 twin of a listed noun"] += 1
                    continue
            if cat in ("noun", "noun-compound"):
                forms = noun_forms(w)
            elif cat in predicate:
                forms = conjugate(kiwi, cat, stem)
            else:
                forms = {w}
            if forms & reserved:
                drop["a form equals a cover token"] += 1
                continue
            clash = [taken[f] for f in forms if f in taken]
            if clash:
                drop["a form collides with a kept word"] += 1
                continue
            for f in forms:
                taken[f] = w
            chosen.append((r, w, cat))
        stage["kept"] = len(chosen)

        sizes = {}
        for n in SIZES:
            if len(chosen) < n:
                sizes[n] = None
                continue
            sub = chosen[:n]
            last_rank = sub[-1][0]
            words = [w for _, w, _ in sub]
            mix = Counter(c.split("-")[0] for _, _, c in sub)
            sizes[n] = {
                "last_rank": last_rank,
                "last_per_million": pm_of[sub[-1][1]],
                "mean_syllables": round(sum(len(w) for w in words) / n, 2),
                "edit1_fraction": round(edit1_fraction(words), 3),
                "pos_mix": dict(mix),
                "last_words": words[-5:],
            }
        if args.dump:
            with open(f"{args.dump}.{tier}.tsv", "w", encoding="utf-8") as f:
                for r, w, c in chosen:
                    f.write(f"{r}\t{w}\t{c}\n")
        results[tier] = {"stages": dict(stage), "dropped": dict(drop),
                         "kept": len(chosen), "sizes": sizes}

    bip = None
    if args.bip39:
        words = load_yaml_keys(args.bip39)[:2048]
        ranks = sorted(rank_of[w] for w in words if w in rank_of)
        bip = {
            "words": len(words),
            "in_shiberu": len(ranks),
            "median_rank": ranks[len(ranks) // 2] if ranks else None,
            "p90_rank": ranks[int(len(ranks) * 0.9)] if ranks else None,
            "max_rank": ranks[-1] if ranks else None,
            "in_top_2048": sum(1 for r in ranks if r <= 2048),
            "in_top_10000": sum(1 for r in ranks if r <= 10000),
            "mean_syllables": round(sum(len(w) for w in words) / len(words), 2),
            "edit1_fraction": round(edit1_fraction(words), 3),
            "not_in_shiberu": [w for w in words if w not in rank_of][:20],
        }

    report = {"funnel": dict(funnel), "categories": dict(cats.most_common()),
              "tiers": results, "bip39": bip}
    js = json.dumps(report, ensure_ascii=False, indent=1)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(js)
    print(js)


if __name__ == "__main__":
    sys.exit(main())
