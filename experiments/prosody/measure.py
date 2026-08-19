#!/usr/bin/env python3
"""Measure what meter costs: density vs. metrical fit, as a function of best-of-N.

Reads the candidate dump from `cargo run --release --example prosody_candidates`
and asks the go/no-go question for poetry dialects: **how many candidates must
the generator draw before one of them scans, and how much density does picking
that one cost?**

Method. Payload words and their order are fixed by the payload, so a candidate
either admits a metrical reading or it does not — meter is a *filter over
candidates*, never an edit to the text. That is exactly how best-of-N already
works, so the measurement is: over the first N candidates, (a) does any admit a
metrical reading, (b) what is the density of the densest one that does, versus
the densest overall.

Forms measured:

  renga     lines cycling 5/7/5 syllables, exact, split at word boundaries.
            (Pure syllable counting — the classic haiku constraint.)
  blank     every line exactly 10 syllables, no stress constraint.
  iambic    decasyllabic AND every line scans: no polysyllable may put its
            primary stress in a weak position (`lenient`), and additionally no
            unstressed syllable of a polysyllable may sit in a strong position
            (`strict`). Monosyllables are treated as metrically flexible, which
            is standard practice — English monosyllables take their stress from
            context.

  Each form also has a `-tail` variant that lets the FINAL line come up short,
  which separates "the total syllable count is wrong" from "the word boundaries
  fall in the wrong places".

Usage: python3 measure.py candidates.tsv [prosody.yaml]
"""
import sys, os, re, collections
from functools import lru_cache
import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
HERE = os.path.dirname(os.path.abspath(__file__))

NS = [1, 2, 4, 8, 16, 32, 64]


# ── prosody lookup ──────────────────────────────────────────────────────

class Prosody:
    def __init__(self, path):
        d = yaml.safe_load(open(path))
        self.stress = {str(k): str(v) for k, v in d["stress"].items()}
        self._cmu = None
        self.misses = collections.Counter()

    def cmu(self):
        if self._cmu is None:
            import cmudict
            self._cmu = cmudict.dict()
        return self._cmu

    def variants(self, word):
        """All (stress-pattern) readings of a token, as a tuple of strings."""
        w = word.lower().strip(".,;:!?\"'()")
        if not w:
            return ()
        s = self.stress.get(w)
        if s is None:
            # Tokens the generator inflected (plurals, conjugations) are not in
            # the wordlist file; fall back to CMUdict, then to a vowel-group count.
            prons = self.cmu().get(w)
            if prons:
                pats = []
                for p in prons:
                    pat = "".join(ph[-1] for ph in p if ph[-1].isdigit())
                    if pat and pat not in pats:
                        pats.append(pat)
                s = "|".join(pats)
            if not s:
                self.misses[w] += 1
                n = max(1, len(re.findall(r"[aeiouy]+", w)))
                s = "?" * n
            self.stress[w] = s
        return tuple(s.split("|"))


# ── metrical fitting ────────────────────────────────────────────────────

def violates(pat, offset, mode, rise=1):
    """Does a word with stress pattern `pat`, starting at syllable `offset` of a
    line, break the meter? Positions are 0-indexed; `rise` picks which parity
    carries the beat — 1 for rising (iambic: da-DUM), 0 for falling (trochaic:
    DUM-da)."""
    if len(pat) == 1 or mode == "none":
        return False                      # monosyllables float
    for i, d in enumerate(pat):
        strong = (offset + i) % 2 == rise
        if d == "1" and not strong:
            return True                   # primary stress on a weak beat
        if mode == "strict" and d == "0" and strong:
            return True                   # unstressed syllable on a strong beat
    return False


def fits(vars_, pattern, mode="none", allow_short_tail=False, rise=1):
    """Can this word sequence be cut into lines of `pattern` syllables (cycling),
    at word boundaries, honouring the stress constraint `mode`?

    `vars_` is one tuple of stress-pattern strings per word (its readings).
    """
    n = len(vars_)

    @lru_cache(maxsize=None)
    def go(i, line, off):
        if i == n:
            return off == 0 or (allow_short_tail and off > 0)
        L = pattern[line]
        for pat in vars_[i]:
            s = len(pat)
            if off + s > L:
                continue
            if violates(pat, off, mode, rise):
                continue
            nxt = off + s
            if nxt == L:
                if go(i + 1, (line + 1) % len(pattern), 0):
                    return True
            elif go(i + 1, line, nxt):
                return True
        return False

    ok = go(0, 0, 0)
    go.cache_clear()
    return ok


FORMS = [
    # name             line pattern  stress mode  short tail  rise
    ("renga",          (5, 7, 5),    "none",      False,      1),
    ("renga-tail",     (5, 7, 5),    "none",      True,       1),
    ("blank",          (10,),        "none",      False,      1),
    ("blank-tail",     (10,),        "none",      True,       1),
    ("iambic",         (10,),        "lenient",   False,      1),
    ("iambic-tail",    (10,),        "lenient",   True,       1),
    ("iambic-strict",  (10,),        "strict",    True,       1),
    # Falling meter. The payload wordlist is heavily trochaic (33% of words scan
    # "10", only 11% "01"), so rising meter fights the vocabulary and falling
    # meter runs with it. Tetrameter (8) is Hiawatha's line; 10 is kept for a
    # like-for-like comparison against the iambic rows.
    ("trochaic-8",     (8,),         "lenient",   True,       0),
    ("trochaic-8-str", (8,),         "strict",    True,       0),
    ("trochaic-10",    (10,),        "lenient",   True,       0),
    ("trochee-nostr",  (8,),         "none",      True,       0),
]


# ── driver ──────────────────────────────────────────────────────────────

def main():
    tsv = sys.argv[1]
    pfile = sys.argv[2] if len(sys.argv) > 2 else os.path.join(
        HERE, "data", "prosody_english_bip39.yaml")
    pros = Prosody(pfile)

    # cand[(bytes, sample)][k] = (density, fits-per-form)
    cand = collections.defaultdict(dict)
    total_syl = collections.defaultdict(list)
    with open(tsv) as f:
        next(f)
        for line in f:
            b, s, k, pw, tw, text = line.rstrip("\n").split("\t", 5)
            b, s, k, pw, tw = int(b), int(s), int(k), int(pw), int(tw)
            words = text.split()
            vars_ = tuple(pros.variants(w) for w in words)
            vars_ = tuple(v for v in vars_ if v)
            syl = sum(len(v[0]) for v in vars_)
            total_syl[b].append(syl)
            res = {name: fits(vars_, pat, mode, tail, rise)
                   for name, pat, mode, tail, rise in FORMS}
            cand[(b, s)][k] = (pw / max(tw, 1), res, syl)

    sizes = sorted({b for b, _ in cand})
    max_k = max(len(v) for v in cand.values())

    print(f"\ncandidates: {sum(len(v) for v in cand.values())} "
          f"({len(cand)} payloads x {max_k} draws)")
    if pros.misses:
        tot = sum(pros.misses.values())
        print(f"tokens needing the syllable heuristic: {tot} "
              f"({', '.join(w for w, _ in pros.misses.most_common(8))})")

    print("\nSYLLABLES PER TEXT (mean / spread)")
    print(f"  {'bytes':>6} {'mean':>8} {'min':>6} {'max':>6} {'%mod17':>8} {'%mod10':>8}")
    for b in sizes:
        v = total_syl[b]
        print(f"  {b:>6} {sum(v)/len(v):>8.1f} {min(v):>6} {max(v):>6}"
              f" {100*sum(1 for x in v if x % 17 == 0)/len(v):>7.1f}%"
              f" {100*sum(1 for x in v if x % 10 == 0)/len(v):>7.1f}%")

    # ── what actually blocks a fit ──────────────────────────────────────
    #
    # Two independent things must go right: the text's TOTAL syllable count has
    # to land on a multiple of the stanza, and the word boundaries have to fall
    # where the line breaks need them. Only the first is a lottery today — the
    # generator already chooses how many cover words to spend, so it could aim
    # at a total instead of hoping for one. Splitting the two says how much of
    # the cost is fixable by construction rather than by drawing more samples.
    print("\nCONSTRAINT ANATOMY — per candidate (not per payload)")
    print(f"  {'form':>14} {'bytes':>6} {'P(total fits)':>14} {'P(cut | total)':>15} {'P(both)':>9}")
    for name, pat, mode, tail, rise in FORMS:
        if mode != "none" or tail:
            continue                      # only meaningful for exact, stress-free forms
        period = sum(pat)
        for b in sizes:
            cs = [c for key in cand if key[0] == b for c in cand[key].values()]
            tot_ok = [c for c in cs if c[2] % period == 0]
            both = [c for c in tot_ok if c[1][name]]
            if not cs:
                continue
            pt = len(tot_ok) / len(cs)
            pc = len(both) / len(tot_ok) if tot_ok else float("nan")
            print(f"  {name:>14} {b:>6} {100*pt:>13.1f}% {100*pc:>14.1f}%"
                  f" {100*len(both)/len(cs):>8.1f}%")

    # ── P(some candidate in the first N scans) ──────────────────────────
    print("\nHIT RATE — payloads with at least one metrical candidate in N draws")
    for name, *_ in FORMS:
        print(f"\n  {name}")
        print(f"    {'bytes':>6}" + "".join(f"{'N='+str(n):>9}" for n in NS))
        for b in sizes:
            row = [k for k in cand if k[0] == b]
            print(f"    {b:>6}", end="")
            for n in NS:
                hit = sum(1 for key in row
                          if any(cand[key][k][1][name] for k in range(min(n, max_k))
                                 if k in cand[key]))
                print(f"{100*hit/len(row):>8.0f}%", end="")
            print()

    # ── density cost ────────────────────────────────────────────────────
    print("\nDENSITY COST — densest metrical candidate vs densest overall, best-of-N")
    print("  (mean over the payloads that HAVE a metrical candidate; "
          "'cost' is the relative density given up)")
    for name, *_ in FORMS:
        rows = []
        for b in sizes:
            keys = [k for k in cand if k[0] == b]
            free, free4, met, cnt = 0.0, 0.0, 0.0, 0
            n = min(64, max_k)
            for key in keys:
                ks = [k for k in range(n) if k in cand[key]]
                best_free = max(cand[key][k][0] for k in ks)
                # canonical v1 ships best_of=4: that, not the best of 64, is the
                # density a poetry dialect would actually be trading against.
                best4 = max(cand[key][k][0] for k in ks if k < 4)
                mets = [cand[key][k][0] for k in ks if cand[key][k][1][name]]
                if mets:
                    free += best_free
                    free4 += best4
                    met += max(mets)
                    cnt += 1
            if cnt:
                rows.append((b, cnt, len(keys), free4/cnt, free/cnt, met/cnt))
        if not rows:
            continue
        print(f"\n  {name}  (N={min(64, max_k)})")
        print(f"    {'bytes':>6} {'have':>8} {'free@4':>8} {'free@N':>8} {'metrical':>10}"
              f" {'vs free@N':>10} {'vs free@4':>10}")
        for b, cnt, tot, free4, free, met in rows:
            print(f"    {b:>6} {f'{cnt}/{tot}':>8} {free4:>8.3f} {free:>8.3f} {met:>10.3f}"
                  f" {100*(free-met)/free:>9.1f}% {100*(free4-met)/free4:>9.1f}%")


if __name__ == "__main__":
    main()
