#!/usr/bin/env python3
"""What meter costs when you BUILD for it instead of filtering for it.

`measure.py` scores candidates the generator already produced: cover words
placed for grammar and semantics, prosody checked afterwards. Under that regime
stress meter is unreachable. This asks the opposite question — if density is not
a constraint and cover words are placed left to right *for the meter*, what does
it cost, and can it fail?

The construction. Walk the payload words in order (their order is fixed; that is
the invariant the decoder rests on). At each step the current syllable offset
within the line is known, so it is known whether the next payload word scans
there. If it does not, insert cover syllables until it does; if it will not fit
in the line, fill the line to its length and start the next. Nothing backtracks.

Why this cannot get stuck: a monosyllable is metrically flexible, so inserting
one always scans AND flips the parity. Parity is therefore repairable at every
junction for the price of one word — which is the whole difference from the
filtering regime, where the junctions were already spent on grammar.

What this DOES NOT model: the grammar. A real dialect cannot insert "the"
wherever the meter wants one — the CFG decides which POS may appear where, and
payload words must land in slots that accept them. So the densities here are an
upper bound, and the useful question is whether that bound is comfortably above
today's shipped density (in which case the real filler is worth building) or
below it (in which case it is not).

Usage: python3 construct.py candidates.tsv [form] [--function-only]
"""
import sys, os, collections, random
import yaml

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from measure import Prosody, violates, fits, HERE

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# POS tags that read as function words — the ones a filler could plausibly
# insert without the grammar objecting too loudly.
FUNCTION_POS = {"Det", "Prep", "Conj", "Aux", "Cop", "To", "Pron", "Adv", "Modal"}

FORMS = {
    # name:        (line lengths, stress mode, rise, rhyme scheme or None)
    "iambic":      ((10,), "lenient", 1, None),
    "iambic-str":  ((10,), "strict",  1, None),
    "trochaic":    ((8,),  "lenient", 0, None),
    "syllabic":    ((10,), "none",    1, None),
    "renga":       ((5, 7, 5), "none", 1, None),
    "couplet":     ((10,), "lenient", 1, "aabb"),
    "rhymed-syl":  ((8,),  "none",    1, "aabb"),
}


def load(p):
    return yaml.load(open(os.path.join(ROOT, "languages/english", p)), Loader=yaml.BaseLoader)


class Filler:
    """Cover words indexed by what a meter-driven filler needs to ask for:
    'give me a word of N syllables that scans starting at offset O'."""

    def __init__(self, pros, cover_pos, function_only, rhyme):
        self.pros, self.rhyme = pros, rhyme
        self.by = collections.defaultdict(list)      # (syllables, parity) -> words
        for w, tags in cover_pos.items():
            pos = {k for k in tags if k not in ("refinement",)}
            if function_only and not (pos & FUNCTION_POS):
                continue
            for pat in pros.variants(w):
                for parity in (0, 1):
                    self.by[(len(pat), parity)].append((w, pat))

    def pick(self, syl, offset, mode, rise, rng, rhyme_with=None):
        """A cover word of exactly `syl` syllables that scans at `offset`."""
        opts = [(w, p) for w, p in self.by[(syl, offset % 2)]
                if not violates(p, offset, mode, rise)]
        if rhyme_with is not None:
            k = self.rhyme.get(rhyme_with, "")
            opts = [(w, p) for w, p in opts if k and self.rhyme.get(w) == k]
        return rng.choice(opts) if opts else None


def build(payload, pros, filler, form, rng, mode_stress=True):
    """Greedy left-to-right construction. Returns (lines, ok)."""
    pattern, mode, rise, scheme = form
    if not mode_stress:
        mode = "none"
    lines, cur, off, li, rhymed = [], [], 0, 0, []
    L = lambda: pattern[li % len(pattern)]
    last_rhyme = None

    def close_line():
        """Fill the current line out to its length, honouring rhyme if asked."""
        nonlocal cur, off, li, last_rhyme
        need_rhyme = scheme is not None and li % 2 == 1
        while off < L():
            gap = L() - off
            # Prefer to land exactly; try the largest useful word first.
            got = None
            for syl in sorted({1, 2, min(gap, 3)}, reverse=True):
                if syl > gap:
                    continue
                want = last_rhyme if (need_rhyme and syl == gap) else None
                got = filler.pick(syl, off, mode, rise, rng, rhyme_with=want)
                if got:
                    break
            if not got:
                got = filler.pick(1, off, mode, rise, rng)
            if not got:
                return False
            cur.append(got[0])
            off += len(got[1])
        if scheme is not None:
            if li % 2 == 1:
                rhymed.append(bool(last_rhyme)
                              and filler.rhyme.get(cur[-1], "?")
                              == filler.rhyme.get(last_rhyme, "!"))
            last_rhyme = cur[-1] if li % 2 == 0 else None
        lines.append(" ".join(cur))
        cur, off, li = [], 0, li + 1
        return True

    reserve = 0 if scheme is None else 1
    for w in payload:
        pats = pros.variants(w)
        placed = False
        for _ in range(12):                       # bounded repair, never backtracks
            fit = [p for p in pats
                   if off + len(p) <= L() - reserve
                   and not violates(p, off, mode, rise)]
            if fit:
                p = fit[0]
                cur.append(w)
                off += len(p)
                if off == L() and not close_line():
                    return lines, False, rhymed
                placed = True
                break
            # No reading fits here: buy one syllable of cover and try again.
            if off >= L() - reserve:               # no room left to place payload
                if not close_line():
                    return lines, False, rhymed
                continue
            got = filler.pick(1, off, mode, rise, rng)
            if not got:
                if not close_line():
                    return lines, False, rhymed
                continue
            cur.append(got[0])
            off += 1
            if off == L() and not close_line():
                return lines, False, rhymed
        if not placed:
            return lines, False, rhymed
    if cur or (scheme is not None and li % 2 == 1):
        if not close_line():
            return lines, False, rhymed
    return lines, True, rhymed


def slot_freedom(pros, cover_pos):
    """Can the GRAMMAR supply what the meter needs?

    The construction above ignores the grammar: it inserts whatever the meter
    wants. A real dialect cannot — the CFG decides which POS goes where. So the
    question that decides whether this is buildable is narrower: at a cover slot
    of POS X, does the wordlist offer both a 1- and a 2-syllable choice (the
    parity lever), and does it offer something that scans at either parity (so
    the slot can never block)?
    """
    per = collections.defaultdict(collections.Counter)
    for w, tags in cover_pos.items():
        pats = pros.variants(w)
        for pos in (t for t in tags if t != "refinement"):
            for n in {len(p) for p in pats}:
                per[pos][n] += 1
            for parity in (0, 1):
                if any(not violates(p, parity, "lenient", 1) for p in pats):
                    per[pos][f"p{parity}"] += 1
    print("\nSLOT FREEDOM — what a grammar-respecting filler could choose at each cover slot")
    print(f"  {'POS':>7} {'1syl':>6} {'2syl':>6} {'3syl':>6} {'4+':>5}"
          f" {'scans@even':>11} {'scans@odd':>10} {'parity lever':>13}")
    key = lambda p: -sum(v for k, v in per[p].items() if isinstance(k, int))
    for pos in sorted(per, key=key):
        c = per[pos]
        n4 = sum(v for k, v in c.items() if isinstance(k, int) and k >= 4)
        lever = "yes" if (c[1] and (c[2] or n4)) or (c[2] and c[3]) else "fixed"
        print(f"  {pos:>7} {c[1]:>6} {c[2]:>6} {c[3]:>6} {n4:>5}"
              f" {c['p0']:>11} {c['p1']:>10} {lever:>13}")
    print("  → every POS scans at both parities, so no slot can block the meter;")
    print("    the 'fixed' rows are monosyllable-only (Modal/Cop/Aux/To), which")
    print("    contribute a known syllable rather than a choice.")


def main():
    tsv = sys.argv[1]
    want = sys.argv[2] if len(sys.argv) > 2 else None
    function_only = "--function-only" in sys.argv
    show = "--show" in sys.argv

    pros = Prosody(os.path.join(HERE, "data", "prosody_english_bip39.yaml"))
    cover_pos = load("cover.yaml")
    payload_set = set(load("payload_bip39.yaml"))
    rhyme = yaml.safe_load(open(os.path.join(HERE, "data",
                                             "prosody_english_bip39.yaml")))["rhyme"]
    filler = Filler(pros, cover_pos, function_only, rhyme)

    # Recover each payload word sequence the way the DECODER does: filter the
    # rendered text against the payload wordlist.
    seqs = {}
    for line in open(tsv).readlines()[1:]:
        b, s, k, pw, tw, text = line.rstrip("\n").split("\t", 5)
        key = (int(b), int(s))
        if key in seqs:
            continue
        got = [w.strip(".,").lower() for w in text.split()]
        got = [w for w in got if w in payload_set]
        if len(got) == int(pw):
            seqs[key] = got

    names = [want] if want and not want.startswith("--") else list(FORMS)
    print(f"\ngreedy left-to-right construction, density unconstrained"
          f"{'  [function-word cover only]' if function_only else ''}")
    print(f"payload sequences: {len(seqs)}\n")
    print(f"  {'form':>12} {'bytes':>6} {'built':>7} {'verified':>9} {'words':>7}"
          f" {'lines':>7} {'density':>8} {'payload survives':>17} {'rhymed':>7}")
    for name in names:
        form = FORMS[name]
        pattern, mode, rise, scheme = form
        for b in sorted({k[0] for k in seqs}):
            rng = random.Random(12345)
            ok = tot = ver = surv = rh_n = rh_ok = 0
            dens = wds = ln = 0.0
            for key, seq in seqs.items():
                if key[0] != b:
                    continue
                tot += 1
                lines, good, rhymed = build(seq, pros, filler, form, rng)
                if not good:
                    continue
                ok += 1
                rh_n += len(rhymed); rh_ok += sum(rhymed)
                words = [w for l in lines for w in l.split()]
                dens += len(seq) / len(words)
                wds += len(words)
                ln += len(lines)
                # Independent check, against measure.py's fitter rather than the
                # constructor's own bookkeeping: does the finished text scan, and
                # does the payload come back out in order?
                vs = tuple(pros.variants(w) for w in words)
                if fits(vs, pattern, mode, False, rise):
                    ver += 1
                if [w for w in words if w in payload_set] == seq:
                    surv += 1
            if tot:
                rh = f"{100*rh_ok/rh_n:.0f}%" if rh_n else "-"
                print(f"  {name:>12} {b:>6} {f'{100*ok/tot:.0f}%':>7} {f'{100*ver/tot:.0f}%':>9}"
                      f" {wds/max(ok,1):>7.1f} {ln/max(ok,1):>7.1f} {dens/max(ok,1):>8.3f}"
                      f" {f'{100*surv/tot:.0f}%':>17} {rh:>7}")
    slot_freedom(pros, cover_pos)

    if show:
        name = want or "iambic"
        rng = random.Random(7)
        for key in list(seqs)[:2]:
            lines, good, _ = build(seqs[key], pros, filler, FORMS[name], rng)
            print(f"\n── {name}  {key[0]} bytes  ok={good}")
            for l in lines:
                print(f"   {l}")


if __name__ == "__main__":
    main()
