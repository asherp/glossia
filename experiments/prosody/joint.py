#!/usr/bin/env python3
"""Can grammar and meter be paid for out of ONE cover budget?

`construct.py` showed meter is constructible when cover words may be inserted
freely — but its output scans without being English, because it spends every
cover word on meter and none on syntax. This closes that gap: the cover slots
are the ones the real CFG actually offers, and the filler must satisfy the
slot's POS (and refinement) *and* the meter with the same word.

Inputs are the grammar's own skeleton inventory
(`cargo run --release --example prosody_skeletons`) — POS sequence, refinements,
and grammar probability per sentence shape — so nothing here is invented about
what the grammar allows.

Two regimes are compared:

  today   skeletons drawn by grammar probability, cover words drawn blind,
          prosody checked afterwards. Reproduces the filtering regime.
  joint   skeletons drawn by grammar probability but kept only if the meter can
          still be satisfied through them, cover words chosen for POS + meter.
          Among feasible skeletons the one seating the most payload wins, which
          keeps the project's density-primary rule.

Density is decided by the skeleton, not the filler — the filler only chooses
*which* word fills a slot, never how many. So "what does meter cost" is answered
by comparing the density of the skeletons the joint regime can accept against
the ones today's regime picks freely.

Usage: python3 joint.py skeletons.tsv candidates.tsv [form] [--show]
"""
import sys, os, random, collections, bisect
import yaml

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from measure import Prosody, violates, fits, HERE
from construct import FORMS

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Function-word slots the wordlist rules reserve for cover (CLAUDE.md).
COVER_ONLY = {"Aux", "Cop", "To", "Prefix", "Dot"}
K_MIN, K_MAX = 5, 12          # the generator's defaults for the body dialect
SKELETON_TRIES = 64           # how many shapes the planner may consider per sentence


def load_yaml(p):
    return yaml.load(open(os.path.join(ROOT, "languages/english", p)), Loader=yaml.BaseLoader)


def parse_slot(tok):
    if "[" in tok:
        pos, ref = tok[:-1].split("[", 1)
        return pos, ref
    return tok, None


class Grammar:
    def __init__(self, path):
        self.by_k = collections.defaultdict(list)
        for line in open(path).readlines()[1:]:
            start, k, prob, slots = line.rstrip("\n").split("\t")
            if start != "S":
                continue
            k = int(k)
            if not (K_MIN <= k <= K_MAX):
                continue
            self.by_k[k].append((float(prob), [parse_slot(t) for t in slots.split()]))
        self.ks = sorted(self.by_k)
        # Flatten into one weighted list, so a draw picks a shape the way the
        # planner does: by grammar probability across all admissible lengths.
        self.flat = [(p, s) for k in self.ks for p, s in self.by_k[k]]
        tot = sum(p for p, _ in self.flat)
        acc, self.cum = 0.0, []
        for p, _ in self.flat:
            acc += p / tot
            self.cum.append(acc)

    def draw(self, rng):
        return self.flat[bisect.bisect(self.cum, rng.random())][1]


class Cover:
    """Cover words indexed as the filler must ask for them: by slot, then by the
    syllable count and starting parity the meter needs."""

    def __init__(self, pros, cover_yaml):
        self.by_slot = collections.defaultdict(list)
        for w, tags in cover_yaml.items():
            ref = tags.get("refinement")
            for pos in (t for t in tags if t != "refinement"):
                for pat in pros.variants(w):
                    self.by_slot[(pos, ref)].append((w, pat))
                    if ref is not None:            # an unrefined slot accepts it too
                        self.by_slot[(pos, None)].append((w, pat))

    def options(self, slot, off, mode, rise):
        return [(w, p) for w, p in self.by_slot.get(slot, ())
                if not violates(p, off, mode, rise)]

    def syllables(self, slot, off, mode, rise):
        return {len(p) for _, p in self.options(slot, off, mode, rise)}


def seat_payload(slots, payload, pos_of):
    """Where the generator would put payload words in this shape: greedy,
    in order, first compatible slot wins."""
    out, i = [], 0
    for pos, ref in slots:
        if i < len(payload) and pos not in COVER_ONLY:
            tags = pos_of.get(payload[i], {})
            if pos in tags and (ref is None or tags.get("refinement") == ref):
                out.append(payload[i])
                i += 1
                continue
        out.append(None)
    return out, i


def feasible_states(seating, slots, pros, cover, form):
    """Backward pass: for each slot index, the metrical states from which the rest
    of this shape can still be completed.

    Without this, a filler can paint itself into a corner — pick a cover word that
    fits locally and leaves the line unfinishable. The backward pass is what lets
    the forward walk commit to a word and never backtrack.
    """
    pattern, mode, rise, _ = form
    n = len(slots)
    all_states = {(li, off) for li in range(len(pattern)) for off in range(max(pattern))}
    feas = [None] * (n + 1)
    feas[n] = all_states                      # a shape may end mid-line
    for j in range(n - 1, -1, -1):
        pos, ref = slots[j]
        word = seating[j]
        good = set()
        for li, off in all_states:
            L = pattern[li % len(pattern)]
            if off >= L:
                continue
            if pos == "Dot":
                if (li, off) in feas[j + 1]:
                    good.add((li, off))
                continue
            if word is not None:
                syls = {len(p) for p in pros.variants(word)
                        if not violates(p, off, mode, rise)}
            else:
                syls = cover.syllables((pos, ref), off, mode, rise)
            for sy in syls:
                nxt = (li, off + sy) if off + sy < L else \
                      ((li + 1) % len(pattern), 0) if off + sy == L else None
                if nxt and nxt in feas[j + 1]:
                    good.add((li, off))
                    break
        feas[j] = good
    return feas


def render(seating, slots, pros, cover, state, form, feas, rng, steer=True):
    """Forward walk, committing to words that stay inside the feasible set.

    With `steer=False` the meter is not consulted at all — cover words are drawn
    blind from the slot's POS, which is what the generator does today. That is
    the baseline the joint regime has to beat.
    """
    pattern, mode, rise, _ = form
    li, off = state
    out = []
    if not steer:
        for (pos, ref), word in zip(slots, seating):
            if pos == "Dot":
                if out:
                    out[-1] += "."
                continue
            if word is not None:
                out.append(word)
                continue
            opts = cover.by_slot.get((pos, ref), ())
            if not opts:
                return None, state
            out.append(opts[rng.randrange(len(opts))][0])
        return out, state
    for j, ((pos, ref), word) in enumerate(zip(slots, seating)):
        L = pattern[li % len(pattern)]
        if pos == "Dot":
            if out:
                out[-1] += "."
            continue
        step = lambda sy: (li, off + sy) if off + sy < L else \
                          ((li + 1) % len(pattern), 0) if off + sy == L else None
        if word is not None:
            pats = [p for p in pros.variants(word)
                    if not violates(p, off, mode, rise) and step(len(p)) in feas[j + 1]]
            if not pats:
                return None, (li, off)
            w, pat = word, pats[0]
        else:
            opts = [(w, p) for w, p in cover.options((pos, ref), off, mode, rise)
                    if step(len(p)) in feas[j + 1]]
            if not opts:
                return None, (li, off)
            w, pat = opts[rng.randrange(len(opts))]
        out.append(w)
        li, off = step(len(pat))
    return out, (li, off)


def build_text(payload, gram, pros, cover, pos_of, form, rng, joint):
    """Assemble sentences until the payload is spent.

    Both regimes consider the same number of shapes and pick the one seating the
    most payload — the project's density-primary rule. The joint regime differs
    only in discarding shapes the meter cannot survive, which is exactly the
    change a poetry dialect would make to `plan_sentence`.
    """
    pattern = form[0]
    words, i, state, guard = [], 0, (0, 0), 0
    while i < len(payload) and guard < 400:
        guard += 1
        best = None
        for _ in range(SKELETON_TRIES):
            slots = gram.draw(rng)
            seating, used = seat_payload(slots, payload[i:], pos_of)
            if used == 0:
                continue
            if joint:
                feas = feasible_states(seating, slots, pros, cover, form)
                if state not in feas[0]:
                    continue
            else:
                # No metrical steering: every state is "feasible", so the forward
                # walk picks cover words blind, exactly as the generator does today.
                feas = [{(li, off) for li in range(len(pattern))
                         for off in range(max(pattern) + 1)}] * (len(slots) + 1)
            if best is None or used > best[1]:
                best = (slots, used, seating, feas)
        if best is None:
            return None, i
        slots, used, seating, feas = best
        got, nstate = render(seating, slots, pros, cover, state, form, feas, rng,
                             steer=joint)
        if got is None:
            return None, i
        words += got
        state = nstate
        i += used
    return (words, i) if i >= len(payload) else (None, i)


def main():
    sk, tsv = sys.argv[1], sys.argv[2]
    want = sys.argv[3] if len(sys.argv) > 3 and not sys.argv[3].startswith("-") else None
    show = "--show" in sys.argv

    pros = Prosody(os.path.join(HERE, "data", "prosody_english_bip39.yaml"))
    cover_yaml, payload_yaml = load_yaml("cover.yaml"), load_yaml("payload_bip39.yaml")
    gram, cover = Grammar(sk), Cover(pros, cover_yaml)
    payload_set = set(payload_yaml)

    seqs = {}
    for line in open(tsv).readlines()[1:]:
        b, s, k, pw, tw, text = line.rstrip("\n").split("\t", 5)
        key = (int(b), int(s))
        if key in seqs:
            continue
        got = [w.strip(".,").lower() for w in text.split() if w.strip(".,").lower() in payload_set]
        if len(got) == int(pw):
            seqs[key] = got

    names = [want] if want else ["syllabic", "renga", "iambic", "trochaic"]
    print(f"\ngrammar-constrained filling: real CFG slots, cover chosen for POS + meter")
    print(f"skeletons: {len(gram.flat)} shapes, k {K_MIN}-{K_MAX}   "
          f"payload sequences: {len(seqs)}\n")
    print(f"  {'form':>10} {'regime':>7} {'bytes':>6} {'built':>7} {'scans':>7}"
          f" {'payload ok':>11} {'words':>7} {'density':>8}")
    for name in names:
        form = FORMS[name]
        for joint in (False, True):
            for b in sorted({k[0] for k in seqs}):
                rng = random.Random(99)
                tot = ok = scan = surv = 0
                wds = dens = 0.0
                for key, seq in seqs.items():
                    if key[0] != b:
                        continue
                    tot += 1
                    words, used = build_text(seq, gram, pros, cover, payload_yaml,
                                             form, rng, joint)
                    if words is None or used < len(seq):
                        continue
                    ok += 1
                    wds += len(words)
                    dens += len(seq) / len(words)
                    bare = [w.strip(".").lower() for w in words]
                    vs = tuple(pros.variants(w) for w in bare)
                    if fits(vs, form[0], form[1], True, form[2]):
                        scan += 1
                    if [w for w in bare if w in payload_set] == seq:
                        surv += 1
                if tot:
                    print(f"  {name:>10} {'joint' if joint else 'today':>7} {b:>6}"
                          f" {f'{100*ok/tot:.0f}%':>7} {f'{100*scan/tot:.0f}%':>7}"
                          f" {f'{100*surv/tot:.0f}%':>11}"
                          f" {wds/max(ok,1):>7.1f} {dens/max(ok,1):>8.3f}")
        if show:
            rng = random.Random(5)
            key = list(seqs)[0]
            words, _ = build_text(seqs[key], gram, pros, cover, payload_yaml,
                                  form, rng, True)
            if words:
                pattern = form[0]
                li = off = 0
                line = []
                print(f"\n── {name}  {key[0]} bytes")
                for w in words:
                    line.append(w)
                    off += len(pros.variants(w.strip("."))[0])
                    if off >= pattern[li % len(pattern)]:
                        print("   " + " ".join(line))
                        line, off, li = [], 0, li + 1
                if line:
                    print("   " + " ".join(line))


if __name__ == "__main__":
    main()
