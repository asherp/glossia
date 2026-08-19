#!/usr/bin/env python3
"""Why stress meter fails where syllable counting succeeds.

`measure.py` shows that syllable-counted verse is reachable and stress meter is
not. This says why, because the obvious explanation — "the payload wordlist is
trochaic and iambic meter is rising" — is wrong, and acting on it would send the
work in the wrong direction.

Three things are measured:

1. Whether words are intrinsically unmetrical. Almost none are: a trochee is a
   perfect iamb when it starts on the beat ("the DON-key" = da-DUM da). Stress
   patterns are not the obstacle.
2. How many constraints each form actually imposes on one text. Syllable
   counting constrains a SUM, once per line break. Stress constrains a POSITION,
   once per polysyllabic word — and a word's position is fixed by the cumulative
   syllable count of everything before it, so the constraints form a chain.
3. Whether rhyme is reachable, which is a different question again: rhyme is a
   lexical choice at one position, so it is constructible where the line-final
   slot is cover and hopeless where it is payload.

Usage: python3 anatomy.py candidates.tsv
"""
import sys, os, collections
import yaml

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from measure import Prosody, violates, fits, HERE

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def intrinsic(pat, mode):
    """Can this stress pattern scan at EITHER starting parity, chain ignored?"""
    return not violates(pat, 0, mode, 1) or not violates(pat, 1, mode, 1)


def main():
    tsv = sys.argv[1]
    pros = Prosody(os.path.join(HERE, "data", "prosody_english_bip39.yaml"))

    # ── 1. the vocabulary is not the obstacle ───────────────────────────
    print("\nINTRINSIC METRICALITY — can a word scan at some parity, chain ignored?")
    poly = [w for w, s in pros.stress.items() if len(s.split("|")[0]) > 1]
    for mode in ("lenient", "strict"):
        bad = [w for w in poly
               if all(not intrinsic(p, mode) for p in pros.stress[w].split("|"))]
        print(f"  {mode:>8}: {len(bad):4d} of {len(poly)} polysyllables cannot scan at any "
              f"parity ({100*len(bad)/len(poly):.1f}%)"
              + (f"  e.g. {' '.join(bad[:6])}" if bad else ""))
    print("  → stress patterns are not what blocks the meter.")

    # ── 2. constraint count, and the measured hit rate it predicts ──────
    agg = collections.defaultdict(collections.Counter)
    for line in open(tsv).readlines()[1:]:
        b, s, k, pw, tw, text = line.rstrip("\n").split("\t", 5)
        vs = tuple(v for v in (pros.variants(w) for w in text.split()) if v)
        syl = sum(len(v[0]) for v in vs)
        c = agg[int(b)]
        c["n"] += 1
        c["syl"] += syl
        c["poly"] += sum(1 for v in vs if len(v[0]) > 1)
        c["blank"] += fits(vs, (10,), "none", True)
        c["iambic"] += fits(vs, (10,), "lenient", True)

    print("\nCONSTRAINT COUNT — how many independent things must go right per text")
    print(f"  {'bytes':>6} {'syls':>7} {'breaks':>8} {'polysyl':>9}"
          f" {'P(blank-tail)':>14} {'P(iambic-tail)':>15} {'ratio':>8}")
    for b in sorted(agg):
        c = agg[b]
        n = c["n"]
        breaks = max(c["syl"] / n / 10 - 1, 0)     # interior line breaks
        pb, pi = c["blank"] / n, c["iambic"] / n
        print(f"  {b:>6} {c['syl']/n:>7.1f} {breaks:>8.1f} {c['poly']/n:>9.1f}"
              f" {100*pb:>13.1f}% {100*pi:>14.2f}%"
              f" {(pb/pi if pi else float('inf')):>7.0f}x")
    print("  → each constraint is roughly a coin flip, and stress imposes ~3x as many.")

    # ── 3. rhyme is a different kind of constraint ──────────────────────
    d = yaml.safe_load(open(os.path.join(HERE, "data", "prosody_english_bip39.yaml")))
    rk = d["rhyme"]
    load = lambda p: set(yaml.load(open(os.path.join(ROOT, "languages/english", p)),
                                   Loader=yaml.BaseLoader))
    cover, payload = load("cover.yaml"), load("payload_bip39.yaml")
    cls = collections.Counter(rk[w] for w in cover if rk.get(w))
    partnered = sum(c for c in cls.values() if c > 1)
    hit = sum(1 for w in payload if cls.get(rk.get(w, ""), 0) > 0)
    print(f"\nRHYME REACH — {len(cls)} rhyme classes across {len(cover)} cover words")
    print(f"  cover words with a cover rhyme partner: {partnered} "
          f"({100*partnered/sum(cls.values()):.0f}%)")
    print(f"  payload words a cover word can rhyme with: {hit}/{len(payload)} "
          f"({100*hit/len(payload):.0f}%)")
    print("  → constructible at a cover-final line, impossible at a payload-final one.")


if __name__ == "__main__":
    main()
