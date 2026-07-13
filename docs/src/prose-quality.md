# Prose Quality (Semantic Planning)

By default, Glossia embeds payload words into any grammatically valid slot. That
is always correct, but it can read oddly — *"the clock discovers the mountain"* is
grammatical yet nonsensical. **Semantic planning** biases word choice toward
*coherent* pairings (an animate subject for a thinking verb, a place for a motion
verb, and so on), so English output reads more naturally.

## The guarantee: it never changes what you decode

Semantic planning only affects **which cover words appear and how sentences are
shaped**. It never changes which payload words are emitted or their order. So an
encoding decodes to exactly the same data whether or not this layer is active —
decoding is just filtering the text against the payload wordlist, and the payload
is untouched. You can always recover your data.

It's on by default for English. Languages without a semantic dataset are
unaffected and behave exactly as before.

## Prose quality (best-of-N)

The strongest quality lever is **best-of-N**: Glossia generates several
realizations of the *same* payload and keeps the best-reading one. Selection is
**density-first** — it never sacrifices how compactly your data is packed — and
breaks ties on coherence. Because every candidate encodes the identical payload
in order, choosing among them is free: the result always decodes the same.

### In the web composer

The compose page has a **Prose quality** stepper (under the advanced options,
next to the variant controls). Higher = more candidates sampled = better-reading
prose, for the same message. `1` is the fastest (a single realization).

It composes with the **Cover variant** stepper cleanly:

- **Cover variant** picks *which* family of wordings you're looking at.
- **Prose quality** decides *how hard* to search that family for the best one.

Each cover variant is anchored to its own well-separated block of candidate
seeds, so stepping the variant always gives a genuinely different wording, and
changing the quality never makes the variant jump somewhere unrelated — at any
sample size.

### On the command line

```
# Sample 8 candidates, keep the densest / most coherent
glossia --dialect english-body --best-of 8  <your words...>
```

`--best-of 1` (the default) is the classic single-pass behavior.

### From the library / WASM

- Rust: `encode_into_language_best_of(...)`, or `generate_text_best_of(...)`.
- WASM: `encode_best_of(...)` and `encode_raw_base_n_best_of(...)`.

All take an `N` (candidate count); `N = 1` falls back to a single encoding.

## What "coherent" means here

Each noun in the wordlist carries a coarse class — `animate` (people, animals),
`agentive` (machines that can act, like an engine or sensor), `thing`, `place`,
or `abstract`. Each verb declares what it expects of its subject and object. When
Glossia fills a slot next to a payload verb, it prefers a cover word whose class
fits — so you get *"the author discovers the theory"* rather than *"the clock
discovers the mountain."*

This is a **soft** preference, never a hard rule: a forced payload word is always
placed, even if no perfectly coherent frame exists. Coherence improves the
prose; it never blocks your data.

## Turning it off

Set the environment variable `GLOSSIA_DISABLE_SEMANTICS=1` to force the classic
part-of-speech-only planning (no semantic biasing, no coherence scoring). Useful
for reproducing pre-semantic output or benchmarking.

## Performance

Best-of-N is cheap. Generating a sentence is fast once Glossia has built its
grammar tables — and that build happens **once** per process and is reused, so
extra candidates cost only a fraction of a second each. In the web app the tables
are built once when the page loads, so stepping the Prose quality control stays
responsive.

## Scope and accuracy

Semantic planning currently ships for **English** only. The word classifications
are generated with WordNet assistance and hand-tuned overrides; they're good but
not perfect, and because the layer is soft, an occasional mis-classification only
slightly nudges word choice — it never affects correctness or decoding. See
`experiments/semantic_planner/` in the repository for the dataset and the tools
that regenerate it.
