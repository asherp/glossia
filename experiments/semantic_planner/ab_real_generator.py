#!/usr/bin/env python3
"""
A/B test the semantic planning on the REAL Rust generator (not the prototype).

For each random BIP39 payload, encode it twice with the CLI at the same seed:
  - semantics OFF  (GLOSSIA_DISABLE_SEMANTICS=1)
  - semantics ON   (default)
then score payload-payload verb-argument coherence on the actual output using
the same classes/frames the generator uses. The only difference between the two
runs is the semantic re-weighting, so any coherence delta is attributable to it.

Run (from repo root, after `cargo build -p glossia-cli`):
  python3 experiments/semantic_planner/ab_real_generator.py [N]
"""
import os
import random
import subprocess
import sys

import yaml

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..", "..")
CLI = os.path.join(ROOT, "target", "release", "glossia")
if not os.path.exists(CLI):
    CLI = os.path.join(ROOT, "target", "debug", "glossia")
SEM = os.path.join(ROOT, "languages", "english", "semantics.yaml")
BIP39 = os.path.join(ROOT, "languages", "english", "payload_bip39.yaml")

N = int(sys.argv[1]) if len(sys.argv) > 1 else 200


def load():
    sem = yaml.safe_load(open(SEM))
    classes = sem["classes"]                     # word -> top class
    frames = sem["frames"]                        # verb -> {subj,obj}
    bip = set(yaml.safe_load(open(BIP39)).keys())
    return classes, frames, bip


def accepts(allowed, cls):
    return allowed == "any" or (cls in allowed)


def coherence(text, classes, frames, bip):
    """All verb-argument coherence of one encoding, over BOTH payload and cover
    nouns (classes now includes cover words). For each verb token, the nearest
    classified noun to its left is the subject and to its right the object,
    bounded by the sentence and other verbs."""
    edges = bad = 0
    for sentence in text.replace("\n", " ").split("."):
        toks = [t.strip(",;:!?()[]\"'").lower() for t in sentence.split()]
        for i, w in enumerate(toks):
            if w not in frames:
                continue
            fr = frames[w]
            # subject: nearest classified noun to the left, stop at another verb
            for j in range(i - 1, -1, -1):
                if toks[j] in frames and toks[j] != w:
                    break
                if toks[j] in classes:
                    edges += 1
                    if not accepts(fr["subj"], classes[toks[j]]):
                        bad += 1
                    break
            # object: nearest classified noun to the right, stop at another verb
            for j in range(i + 1, len(toks)):
                if toks[j] in frames and toks[j] != w:
                    break
                if toks[j] in classes:
                    edges += 1
                    if not accepts(fr["obj"], classes[toks[j]]):
                        bad += 1
                    break
    return edges, bad


def encode(words, seed, semantics_on):
    env = dict(os.environ)
    if not semantics_on:
        env["GLOSSIA_DISABLE_SEMANTICS"] = "1"
    else:
        env.pop("GLOSSIA_DISABLE_SEMANTICS", None)
    out = subprocess.run(
        [CLI, "--dialect", "english-body", "--seed", str(seed), *words],
        capture_output=True, text=True, env=env,
    )
    return out.stdout.strip()


def score(text, classes, frames, bip):
    """(coherence, density) for one encoding."""
    e, b = coherence(text, classes, frames, bip)
    coh = 1 - b / e if e else 1.0
    total = max(len(text.split()), 1)
    dens = 11 / total          # 11 payload words per test payload
    return coh, dens


N_CAND = 8          # best-of-N candidates
DENSITY_TOL = 0.02  # matches encode_into_language_best_of


def main():
    classes, frames, bip = load()
    nouns = [w for w in classes if w in bip]
    verbs = [w for w in frames if w in bip]
    rng = random.Random(2024)

    # accumulate (sum_coherence, sum_density) per condition
    agg = {"off": [0.0, 0.0], "greedy": [0.0, 0.0], "best_of_n": [0.0, 0.0]}
    for _ in range(N):
        base = rng.randint(1, 10_000_000)
        words = [rng.choice(verbs) if rng.random() < 0.4 else rng.choice(nouns)
                 for _ in range(11)]

        # semantics OFF (single sample)
        agg["off"] = [a + b for a, b in zip(agg["off"],
                      score(encode(words, base, False), classes, frames, bip))]

        # semantics ON candidates (best-of-N); candidate 0 == greedy single sample
        cands = []
        for k in range(N_CAND):
            t = encode(words, base + k, True)
            cands.append((t, *score(t, classes, frames, bip)))
        greedy = cands[0]
        max_d = max(c[2] for c in cands)
        best = max((c for c in cands if c[2] >= max_d - DENSITY_TOL),
                   key=lambda c: c[1])   # max coherence within density tolerance

        agg["greedy"] = [agg["greedy"][0] + greedy[1], agg["greedy"][1] + greedy[2]]
        agg["best_of_n"] = [agg["best_of_n"][0] + best[1], agg["best_of_n"][1] + best[2]]

    print(f"payloads: {N}   best-of-N candidates: {N_CAND}   density tol: {DENSITY_TOL}\n")
    print(f"{'condition':<12}{'coherence':>11}{'density':>10}")
    print("-" * 33)
    for label in ("off", "greedy", "best_of_n"):
        coh, dens = (x / N for x in agg[label])
        print(f"{label:<12}{coh:>11.3f}{dens:>10.3f}")


if __name__ == "__main__":
    main()
