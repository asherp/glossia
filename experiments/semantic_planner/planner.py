#!/usr/bin/env python3
"""
Offline prototype: semantic sentence planning for Glossia.

Goal of THIS script (per the design discussion): before touching the 1852-line
Rust generator, measure the tradeoff that decides whether semantic classes are
worth it, and how many to use:

    more classes  ->  more coherent prose  BUT  lower payload density.

Density is Glossia's core metric (payload_count / total_words) and is already
instrumented in the real generator. Coherence has no runtime metric and never
will (there's no LLM at decode time), so here we score it against the ground-truth
verb frames as a stand-in for human judgement.

We hold the payload fixed and sweep the class granularity:

    none    -> every noun is one undifferentiated class (== today's generator)
    binary  -> animate vs inanimate
    coarse  -> animate / agentive / thing / place / abstract
    fine    -> the full slash-hierarchy classes

At each granularity the planner enforces exactly the distinctions VISIBLE at that
granularity, then we report density and TRUE coherence. That produces the curve.

Invariants preserved from Glossia's contract:
  * payload words are forced and stay in order (never dropped/reordered);
  * the semantic layer only reroutes payload words and picks cover words;
  * repairs cost cover words -> that is exactly the density price of coherence.

Run:  python3 planner.py
"""

import os
import random
import yaml

HERE = os.path.dirname(os.path.abspath(__file__))
BIP39 = os.path.join(HERE, "..", "..", "languages", "english", "payload_bip39.yaml")


# --------------------------------------------------------------------------- #
# Data
# --------------------------------------------------------------------------- #
def load_data():
    pos = yaml.safe_load(open(BIP39))                       # real POS + weights
    sem = yaml.safe_load(open(os.path.join(HERE, "semantics.yaml")))["nouns"]
    frames = yaml.safe_load(open(os.path.join(HERE, "verb_frames.yaml")))["verbs"]
    noun_class = {w: d["class"] for w, d in sem.items()}    # word -> "animate/person"
    return pos, noun_class, frames


# --------------------------------------------------------------------------- #
# Class granularity: project a fine leaf class onto the group visible at a level.
# A coarser level lumps fine classes together, so the planner literally cannot
# tell them apart -> it blocks fewer placements -> higher density, lower coherence.
# --------------------------------------------------------------------------- #
_LEAF_TO_TOP = {
    "person": "animate", "animal": "animate",
    "machine": "agentive", "creature": "agentive",
    "artifact": "thing", "substance": "thing", "plant": "thing",
    "place": "place",
    "info": "abstract", "concept": "abstract",
}
LEVELS = ["none", "binary", "coarse", "fine"]
NCLASSES = {"none": 1, "binary": 2, "coarse": 5, "fine": 10}

leaf_of = lambda cls: cls.split("/")[-1]      # "animate/person" -> "person"


def project(leaf, level):
    if level == "none":
        return "ANY"
    if level == "fine":
        return leaf
    top = _LEAF_TO_TOP[leaf]
    if level == "binary":
        return "animate" if top == "animate" else "inanimate"
    if level == "coarse":
        return top
    raise ValueError(level)


def visible_ok(word_leaf, allowed, level):
    """Acceptable in this slot AS JUDGED AT `level` (coarser => more lenient)."""
    if allowed in (None, "any", []):
        return True
    wv = project(word_leaf, level)
    return wv in {project(a, level) for a in allowed}


def true_ok(word_leaf, allowed):
    """Ground-truth (fine) compatibility — used only to score coherence."""
    if allowed in (None, "any", []):
        return True
    return word_leaf in set(allowed)


# --------------------------------------------------------------------------- #
# Payloads: random ordered sequences drawn from the annotated slice.
# (Real payloads are arbitrary; restricting to annotated words just guarantees
# every token has a class. That's a prototype limitation, not a design one.)
# --------------------------------------------------------------------------- #
def make_payloads(pos, noun_class, frames, n_payloads=400, length=12, seed=7):
    rng = random.Random(seed)
    nouns = [w for w in noun_class if "N" in pos.get(w, {})]
    verbs = [w for w in frames if "V" in pos.get(w, {})]
    payloads = []
    for _ in range(n_payloads):
        seq = [rng.choice(verbs) if rng.random() < 0.4 else rng.choice(nouns)
               for _ in range(length)]
        payloads.append(seq)
    return payloads, set(nouns), set(verbs)


# --------------------------------------------------------------------------- #
# Result accumulator. Cover-word accounting is explicit and uniform:
#   - every noun phrase costs 1 determiner ("the"/"a");
#   - a cover noun phrase costs +1 more (the cover noun itself);
#   - every clause costs 1 period;
#   - a repair defers the forced payload word to "and the <word>" (+2 cover).
# --------------------------------------------------------------------------- #
class Result:
    def __init__(self):
        self.payload = 0
        self.cover = 0
        self.edges = 0
        self.bad_edges = 0
        self.repairs = 0

    def payload_np(self):   # "the <payload noun>"
        self.cover += 1; self.payload += 1

    def cover_np(self):     # "the <cover noun>"
        self.cover += 2

    def verb(self):         # payload verb
        self.payload += 1

    def period(self):
        self.cover += 1

    def defer(self):        # "and the <payload noun>" trailing appositive
        self.cover += 2; self.payload += 1

    @property
    def total(self): return self.payload + self.cover
    @property
    def density(self): return self.payload / self.total if self.total else 0.0
    @property
    def coherence(self): return 1 - self.bad_edges / self.edges if self.edges else 1.0

    def merge(self, o):
        self.payload += o.payload; self.cover += o.cover
        self.edges += o.edges; self.bad_edges += o.bad_edges; self.repairs += o.repairs


# --------------------------------------------------------------------------- #
# The planner. One left-to-right pass, greedily forming clauses:
#
#   NOUN VERB [NOUN]   -> subject + verb (+ object)   : subject & object edges
#   VERB [NOUN]        -> cover-subject verb (+ object): object edge only
#   NOUN               -> bare "the <noun>."           : no edge
#
# In semantic mode, a forced payload noun that fails its slot AT the active
# granularity is repaired: a compatible cover noun fills the slot (frame satisfied)
# and the payload noun is deferred to a trailing appositive. Payload never dropped.
# Baseline == level 'none' + no repairs: seat everything by POS only.
# --------------------------------------------------------------------------- #
def plan(payload, noun_class, frames, nouns, verbs, level, repair, cover_pool):
    r = Result()
    n = len(payload)
    i = 0

    def place_arg(word, allowed):
        """Place a forced payload noun into a verb-arg slot; return (edge_bad?)."""
        leaf = leaf_of(noun_class[word])
        if repair and not visible_ok(leaf, allowed, level):
            r.cover_np()          # compatible cover noun fills the slot
            r.edges += 1          # cover noun satisfies the frame by construction
            r.repairs += 1
            r.defer()             # forced payload word survives as appositive
        else:
            r.payload_np()
            r.edges += 1
            if not true_ok(leaf, allowed):
                r.bad_edges += 1

    while i < n:
        w = payload[i]
        w_noun = w in nouns
        w_verb = w in verbs

        # NOUN VERB [NOUN]  — noun becomes the verb's subject
        if w_noun and i + 1 < n and payload[i + 1] in verbs:
            v = payload[i + 1]
            fr = frames[v]
            place_arg(w, fr.get("subj"))          # subject slot
            r.verb()
            i += 2
            if fr.get("obj", None) is not None and i < n and payload[i] in nouns:
                place_arg(payload[i], fr["obj"])  # object slot
                i += 1
            r.period()
            continue

        # VERB [NOUN]  — cover subject, optional payload object
        if w_verb:
            fr = frames[w]
            r.cover_np()                          # cover subject (always frame-ok)
            r.verb()
            i += 1
            if fr.get("obj", None) is not None and i < n and payload[i] in nouns:
                place_arg(payload[i], fr["obj"])
                i += 1
            r.period()
            continue

        # bare noun (or non-noun leftover) -> simple NP, no edge
        r.payload_np()
        r.period()
        i += 1

    return r


def build_cover_pool(noun_class, nouns):
    pool = {}
    for w in nouns:
        pool.setdefault(leaf_of(noun_class[w]), []).append(w)
    return pool


# --------------------------------------------------------------------------- #
# Illustrative renderer: produce a readable surface string for a short payload
# under a given granularity, so the density/coherence numbers have a human face.
# Mirrors plan()'s clause logic but emits words instead of counting them.
# --------------------------------------------------------------------------- #
def render(payload, noun_class, frames, nouns, verbs, level, cover_pool):
    repair = level != "none"
    out, i, n = [], 0, len(payload)

    def cover_for(allowed):
        if allowed in (None, "any", []):
            allowed = ["person"]
        for leaf in allowed:
            if cover_pool.get(leaf):
                return cover_pool[leaf][0]
        return "thing"

    def arg(word, allowed, defer_sink):
        leaf = leaf_of(noun_class[word])
        if repair and not visible_ok(leaf, allowed, level):
            defer_sink.append(word)                       # reroute to appositive
            return f"the {cover_for(allowed)}"
        return f"the {word}"                              # seat forced word here

    while i < n:
        w = payload[i]
        if w in nouns and i + 1 < n and payload[i + 1] in verbs:
            v = payload[i + 1]; fr = frames[v]; defer = []
            subj = arg(w, fr.get("subj"), defer)
            clause = f"{subj} {v}"
            i += 2
            if fr.get("obj", None) is not None and i < n and payload[i] in nouns:
                clause += " " + arg(payload[i], fr["obj"], defer); i += 1
            for d in defer:
                clause += f", and the {d}"
            out.append(clause.capitalize() + ".")
        elif w in verbs:
            fr = frames[w]; defer = []
            clause = f"the {cover_for(fr.get('subj'))} {w}"
            i += 1
            if fr.get("obj", None) is not None and i < n and payload[i] in nouns:
                clause += " " + arg(payload[i], fr["obj"], defer); i += 1
            for d in defer:
                clause += f", and the {d}"
            out.append(clause.capitalize() + ".")
        else:
            out.append(f"The {w}."); i += 1
    return " ".join(out)


def examples(pos, noun_class, frames, nouns, verbs, cover_pool):
    # Hand-picked payloads that isolate the interesting cases.
    cases = [
        (["clock", "discover", "mountain"],
         "artifact as subject of a mental verb — the classic blunder"),
        (["engine", "process", "evidence"],
         "MACHINE subject of an agentive verb — must NOT be blocked (why 'agentive' pays off)"),
        (["doctor", "harvest", "laptop"],
         "narrow object (harvest wants a plant) — only finer classes catch it"),
        (["captain", "decide", "idea"],
         "already coherent — should pass untouched at every level (no wasted cover words)"),
    ]
    for payload, why in cases:
        print(f"\n  payload: {payload}   [{why}]")
        for level in LEVELS:
            s = render(payload, noun_class, frames, nouns, verbs, level, cover_pool)
            print(f"    {level:<7}: {s}")


# --------------------------------------------------------------------------- #
# Sweep
# --------------------------------------------------------------------------- #
def main():
    pos, noun_class, frames = load_data()
    payloads, nouns, verbs = make_payloads(pos, noun_class, frames)
    cover_pool = build_cover_pool(noun_class, nouns)

    print(f"Annotated vocab: {len(noun_class)} nouns, {len(frames)} verbs")
    print(f"Payloads: {len(payloads)} x {len(payloads[0])} words, "
          f"drawn from the annotated slice\n")
    print(f"{'granularity':<10}{'#classes':>9}{'density':>9}{'coherence':>11}"
          f"{'repairs/word':>14}")
    print("-" * 53)

    rows = []
    for level in LEVELS:
        agg = Result()
        for p in payloads:
            agg.merge(plan(p, noun_class, frames, nouns, verbs,
                           level, repair=(level != "none"), cover_pool=cover_pool))
        rows.append((level, agg.density, agg.coherence))
        print(f"{level:<10}{NCLASSES[level]:>9}{agg.density:>9.3f}"
              f"{agg.coherence:>11.3f}{agg.repairs / max(agg.payload,1):>14.4f}")

    print("\nvs. baseline (no semantics):")
    b_dens, b_coh = rows[0][1], rows[0][2]
    for level, dens, coh in rows[1:]:
        print(f"  {level:<7}: coherence {(coh - b_coh) * 100:+5.1f} pts   "
              f"density {(dens - b_dens) / b_dens * 100:+5.1f}%")

    print("\nMarginal gain per step (is the next split worth it?):")
    for k in range(1, len(rows)):
        prev, cur = rows[k - 1], rows[k]
        dcoh = (cur[2] - prev[2]) * 100
        ddens = (cur[1] - prev[1]) / prev[1] * 100
        ratio = dcoh / abs(ddens) if ddens else float("inf")
        print(f"  {prev[0]:>6} -> {cur[0]:<6}: {dcoh:+5.1f} coh pts for "
              f"{ddens:+5.1f}% density  ({ratio:4.1f} coh-pts per %density)")

    print("\nWhat the planner actually produces (surface sentences):")
    examples(pos, noun_class, frames, nouns, verbs, cover_pool)


if __name__ == "__main__":
    main()
