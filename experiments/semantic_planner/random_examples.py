#!/usr/bin/env python3
"""
Show random 11-word payloads rendered two ways:

  baseline (none) -- POS-only placement, no semantics == today's generator
  quad            -- 4 classes (animate / agentive / thing / abstract)

Both use the same clause/render logic, so any difference is purely the semantic
rerouting. (The real Rust generator additionally inflects verbs and adds
adjectives / prepositional phrases; this prototype prints bare lemmas so the
semantic change is visible in isolation.)

Payload words are always present and in order in BOTH renderings — cover words
carry no payload, so decoding is identical. Run: python3 random_examples.py [N]
"""

import sys
from planner import (load_data, make_payloads, build_cover_pool, render,
                     leaf_of, visible_ok, true_ok)

N = int(sys.argv[1]) if len(sys.argv) > 1 else 8


def annotate_payload(payload, noun_class, frames, nouns, verbs):
    """Tag each token with its POS/class so the input is readable."""
    parts = []
    for w in payload:
        if w in verbs:
            parts.append(f"{w}(V)")
        else:
            parts.append(f"{w}({leaf_of(noun_class[w])})")
    return " ".join(parts)


def main():
    pos, noun_class, frames = load_data()
    # fresh seed so these differ from the sweep's payloads
    payloads, nouns, verbs = make_payloads(
        pos, noun_class, frames, n_payloads=N, length=11, seed=2024)
    cover_pool = build_cover_pool(noun_class, nouns)

    for k, p in enumerate(payloads, 1):
        print(f"\n[{k}] payload words (in order): {' '.join(p)}")
        print(f"    classes: {annotate_payload(p, noun_class, frames, nouns, verbs)}")
        base = render(p, noun_class, frames, nouns, verbs, "none", cover_pool)
        quad = render(p, noun_class, frames, nouns, verbs, "quad", cover_pool)
        print(f"    default : {base}")
        print(f"    4-class : {quad}")


if __name__ == "__main__":
    main()
