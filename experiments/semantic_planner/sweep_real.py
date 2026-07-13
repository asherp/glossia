#!/usr/bin/env python3
"""
Validate the semantic-planning tradeoff on the REAL, full-vocabulary BIP39 data
produced by build_data.py (1582 nouns, 1108 verbs) — not the 70-word slice.

Verb frames are expressed in TOP-level classes (animate/agentive/thing/place/
abstract), so the full frame resolution is the `coarse` (5-class) level; that is
the ground truth for coherence. Lower granularities under-enforce:

  none   (1) : no semantics            == today's generator
  binary (2) : animate vs inanimate    (agentive counts as inanimate here)
  quad   (4) : animate/agentive/thing/abstract  (place folds into thing)
  coarse (5) : + place                 == full frame resolution -> coherence 1.0

Run: python3 sweep_real.py [N_random_examples]
"""

import os
import random
import sys
import yaml

HERE = os.path.dirname(os.path.abspath(__file__))
BIP39 = os.path.join(HERE, "..", "..", "languages", "english", "payload_bip39.yaml")
DATA = os.path.join(HERE, "data")


def load():
    pos = yaml.safe_load(open(BIP39))
    nd = yaml.safe_load(open(os.path.join(DATA, "noun_classes_bip39.yaml")))["nouns"]
    vf = yaml.safe_load(open(os.path.join(DATA, "verb_frames_bip39.yaml")))
    noun_top = {w: nd[w]["class"].split("/")[0] for w in nd}          # word -> top
    arch = vf["archetypes"]
    verb_frame = {w: arch[vf["verbs"][w]["archetype"]] for w in vf["verbs"]}
    return pos, noun_top, verb_frame


LEVELS = ["none", "binary", "quad", "coarse"]
NCLASSES = {"none": 1, "binary": 2, "quad": 4, "coarse": 5}


def project(top, level):
    if level == "none":
        return "ANY"
    if level == "binary":
        return "animate" if top == "animate" else "inanimate"
    if level == "quad":
        return "thing" if top == "place" else top
    return top                                   # coarse == full resolution


def visible_ok(top, allowed, level):
    if allowed == "any":
        return True
    return project(top, level) in {project(a, level) for a in allowed}


def true_ok(top, allowed):
    return allowed == "any" or top in set(allowed)   # coarse-level ground truth


def make_payloads(pos, noun_top, verb_frame, n=400, length=12, seed=7):
    rng = random.Random(seed)
    nouns = [w for w in noun_top if "N" in pos.get(w, {})]
    verbs = [w for w in verb_frame if "V" in pos.get(w, {})]
    payloads = [[rng.choice(verbs) if rng.random() < 0.4 else rng.choice(nouns)
                 for _ in range(length)] for _ in range(n)]
    return payloads, set(nouns), set(verbs)


class R:
    def __init__(self): self.p = self.c = self.e = self.bad = self.rep = 0
    def pnp(self): self.c += 1; self.p += 1
    def cnp(self): self.c += 2
    def verb(self): self.p += 1
    def period(self): self.c += 1
    def defer(self): self.c += 2; self.p += 1
    @property
    def density(self): t = self.p + self.c; return self.p / t if t else 0
    @property
    def coherence(self): return 1 - self.bad / self.e if self.e else 1.0
    def merge(self, o):
        self.p += o.p; self.c += o.c; self.e += o.e; self.bad += o.bad; self.rep += o.rep


def plan(payload, noun_top, verb_frame, nouns, verbs, level, repair):
    r = R(); n = len(payload); i = 0

    def arg(word, allowed):
        top = noun_top[word]
        if repair and not visible_ok(top, allowed, level):
            r.cnp(); r.e += 1; r.rep += 1; r.defer()           # cover fills slot, defer payload
        else:
            r.pnp(); r.e += 1
            if not true_ok(top, allowed):
                r.bad += 1

    while i < n:
        w = payload[i]
        if w in nouns and i + 1 < n and payload[i + 1] in verbs:
            fr = verb_frame[payload[i + 1]]
            arg(w, fr["subj"]); r.verb(); i += 2
            if fr["obj"] is not None and i < n and payload[i] in nouns:
                arg(payload[i], fr["obj"]); i += 1
            r.period()
        elif w in verbs:
            fr = verb_frame[w]; r.cnp(); r.verb(); i += 1
            if fr["obj"] is not None and i < n and payload[i] in nouns:
                arg(payload[i], fr["obj"]); i += 1
            r.period()
        else:
            r.pnp(); r.period(); i += 1
    return r


def render(payload, noun_top, verb_frame, nouns, verbs, level, cover_by_top):
    repair = level != "none"; out = []; i = 0; n = len(payload)
    turn = {}   # rotate cover words per class, as the real generator does

    def cover(allowed):
        tops = ["person"] if allowed == "any" else allowed
        for t in tops:
            pool = cover_by_top.get(t)
            if pool:
                turn[t] = turn.get(t, 0) + 1
                return pool[turn[t] % len(pool)]
        return "thing"

    def arg(word, allowed, defer):
        if repair and not visible_ok(noun_top[word], allowed, level):
            defer.append(word); return f"the {cover(allowed)}"
        return f"the {word}"

    while i < n:
        w = payload[i]
        if w in nouns and i + 1 < n and payload[i + 1] in verbs:
            v = payload[i + 1]; fr = verb_frame[v]; d = []
            clause = f"{arg(w, fr['subj'], d)} {v}"; i += 2
            if fr["obj"] is not None and i < n and payload[i] in nouns:
                clause += " " + arg(payload[i], fr["obj"], d); i += 1
            out.append((clause + "".join(f", and {x}" for x in (f"the {y}" for y in d))).capitalize() + ".")
        elif w in verbs:
            fr = verb_frame[w]; d = []
            clause = f"the {cover(fr['subj'])} {w}"; i += 1
            if fr["obj"] is not None and i < n and payload[i] in nouns:
                clause += " " + arg(payload[i], fr["obj"], d); i += 1
            out.append((clause + "".join(f", and the {y}" for y in d)).capitalize() + ".")
        else:
            out.append(f"The {w}."); i += 1
    return " ".join(out)


def main():
    pos, noun_top, verb_frame = load()
    payloads, nouns, verbs = make_payloads(pos, noun_top, verb_frame)
    cover_by_top = {}
    for w in nouns:
        cover_by_top.setdefault(noun_top[w], []).append(w)

    print(f"Full BIP39 vocab: {len(nouns)} nouns, {len(verbs)} verbs")
    print(f"Payloads: {len(payloads)} x {len(payloads[0])} words\n")
    print(f"{'granularity':<10}{'#classes':>9}{'density':>9}{'coherence':>11}")
    print("-" * 39)
    rows = []
    for level in LEVELS:
        agg = R()
        for p in payloads:
            agg.merge(plan(p, noun_top, verb_frame, nouns, verbs, level, repair=(level != "none")))
        rows.append((level, agg.density, agg.coherence))
        print(f"{level:<10}{NCLASSES[level]:>9}{agg.density:>9.3f}{agg.coherence:>11.3f}")

    print("\nMarginal gain per step:")
    for k in range(1, len(rows)):
        pr, cu = rows[k - 1], rows[k]
        print(f"  {pr[0]:>6} -> {cu[0]:<6}: {(cu[2]-pr[2])*100:+5.1f} coh pts for "
              f"{(cu[1]-pr[1])/pr[1]*100:+5.1f}% density")

    n_ex = int(sys.argv[1]) if len(sys.argv) > 1 else 6
    print(f"\nRandom 11-word payloads (default vs 4-class), real vocab:")
    ep, _, _ = make_payloads(pos, noun_top, verb_frame, n=n_ex, length=11, seed=2024)
    for k, p in enumerate(ep, 1):
        print(f"\n[{k}] {' '.join(p)}")
        print(f"    default : {render(p, noun_top, verb_frame, nouns, verbs, 'none', cover_by_top)}")
        print(f"    4-class : {render(p, noun_top, verb_frame, nouns, verbs, 'quad', cover_by_top)}")


if __name__ == "__main__":
    main()
