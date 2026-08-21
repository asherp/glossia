#!/usr/bin/env python3
"""Print specimens: candidates from the dump that fit a given form, laid out.

`measure.py` says how often a form is reachable and what it costs. This says
what it looks like — which is the other half of the go/no-go, since the whole
point of a poetry dialect is that a human reads it.

Usage: python3 show.py candidates.tsv [form] [count]
       forms: renga renga-tail blank blank-tail trochee-nostr iambic-tail ...
"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from measure import Prosody, FORMS, violates, HERE
from functools import lru_cache


def layout(vars_, words, pattern, mode, tail, rise):
    """Return the line split for the first metrical reading found, or None."""
    n = len(vars_)

    @lru_cache(maxsize=None)
    def go(i, line, off):
        if i == n:
            return () if (off == 0 or (tail and off > 0)) else None
        L = pattern[line]
        for pat in vars_[i]:
            s = len(pat)
            if off + s > L or violates(pat, off, mode, rise):
                continue
            nxt = off + s
            rest = go(i + 1, (line + 1) % len(pattern), 0) if nxt == L else go(i + 1, line, nxt)
            if rest is not None:
                return ((i, nxt == L),) + rest
        return None

    r = go(0, 0, 0)
    go.cache_clear()
    if r is None:
        return None
    lines, cur = [], []
    for i, brk in r:
        cur.append(words[i])
        if brk:
            lines.append(" ".join(cur))
            cur = []
    if cur:
        lines.append(" ".join(cur))
    return lines


def main():
    tsv = sys.argv[1]
    form = sys.argv[2] if len(sys.argv) > 2 else "renga"
    want = int(sys.argv[3]) if len(sys.argv) > 3 else 5
    spec = next(f for f in FORMS if f[0] == form)
    _, pattern, mode, tail, rise = spec
    pros = Prosody(os.path.join(HERE, "data", "prosody_english_bip39.yaml"))

    shown = 0
    seen = set()
    with open(tsv) as f:
        next(f)
        for line in f:
            b, s, k, pw, tw, text = line.rstrip("\n").split("\t", 5)
            if (b, s) in seen:
                continue
            words = text.split()
            vars_ = tuple(pros.variants(w) for w in words)
            keep = [(v, w) for v, w in zip(vars_, words) if v]
            out = layout(tuple(v for v, _ in keep), [w for _, w in keep],
                         pattern, mode, tail, rise)
            if out is None:
                continue
            seen.add((b, s))
            density = int(pw) / max(int(tw), 1)
            print(f"\n── {form}  {b} bytes  density {density:.2f}  "
                  f"({pw} payload / {tw} words, draw {k})")
            for l in out:
                print(f"   {l}")
            shown += 1
            if shown >= want:
                break


if __name__ == "__main__":
    main()
