# Semantic sentence planning — offline prototype

A throwaway measurement rig (Python, not part of the Rust build) that answers
the question we kept circling: **is it worth teaching Glossia's generator about
noun/verb semantics, and if so, how many semantic classes?**

It follows the "measure before you build" plan: prove the tradeoff on paper
before touching the 1852-line Rust generator in `src/generator/`.

## The idea in one paragraph

Today the generator seats payload words into POS slots (noun goes anywhere a
noun fits), which produces grammatical-but-silly prose like *"clock discovers
mountain."* Semantic classes let a verb declare what it expects of its arguments
(`discover` wants an animate subject) so the planner can **reroute** a forced
payload word into a slot that fits and pick a compatible cover word — without
ever dropping or reordering payload words, so the decoder is untouched.

The catch: every reroute costs cover words, and cover words lower **density**
(`payload_count / total_words`), which is Glossia's core metric. More classes =
finer constraints = more reroutes = lower density. So there's a sweet spot, and
this rig finds it.

## What's here

| file | what it is |
|------|------------|
| `semantics.yaml` | noun → semantic class, for a 70-word slice of real BIP39 nouns. The noun-side feature table; mirrors how POS is already stored, and maps onto the existing `SemanticType::Refined` machinery (`e[animate/person]`). |
| `verb_frames.yaml` | verb → subject/object selectional restrictions. The genuinely new table (POS has no analog). |
| `planner.py` | greedy sentence planner + a granularity sweep + a surface renderer. |

Classes use slash notation so a coarse class subsumes its children
(`animate` ⊇ `animate/person`) — exactly the hierarchical subsumption already
implemented and tested in `src/semantic_types.rs` (`refinement_subsumes`).

## Run it

```
python3 planner.py
```

No dependencies beyond `pyyaml`. It reads real POS data from
`languages/english/payload_bip39.yaml`.

## The result

Holding the payload fixed and sweeping class granularity (400 random 12-word
payloads):

```
granularity #classes  density  coherence
none              1    0.363      0.424     <- today's generator (POS only)
binary            2    0.330      0.709
quad              4    0.310      0.918     <- knee: animate/agentive/thing/abstract
coarse            5    0.307      0.950
fine             10    0.303      1.000

Marginal gain per step:
  none   -> binary: +28.5 coh pts for  -9.0% density
  binary -> quad  : +20.9 coh pts for  -6.2% density
  quad   -> coarse:  +3.2 coh pts for  -0.9% density   <- place split barely helps
  coarse -> fine  :  +5.0 coh pts for  -1.4% density   <- diminishing returns
```

**Coherence saturates at 4 classes.** Binary animacy alone removes most of the
blunders (subject-animacy violations); the 4-class split
(`animate / agentive / thing / abstract`) reaches ~92%. Splitting `place` out of
`thing` (the 5th class) adds only 3 points, and going to 10 fine classes only 5
more — both while still costing density. That is the over-constraint effect made
concrete: past the knee you keep paying density for coherence you already have.

### Recommendation

**Start at 4 classes (`animate / agentive / thing / abstract`), soft-scored,
payload exempt.** Keeping `animate` and `agentive` distinct is what earns the
big jump (it's what lets `engine process evidence` stay valid while blocking
`clock discover mountain`); the remaining physical/abstract split covers the
rest. It captures the coherence humans actually notice for a bounded density
cost, and it's the cheapest table to author and audit. Add the `place` split or
per-verb fine object classes (`drink`→substance, `harvest`→plant) later only if
specific cases prove worth it.

### What the surface sentences look like

```
clock discover mountain   none : The clock discover the mountain.        (silly)
                          coarse: The captain discover the mountain, and the clock.
engine process evidence   coarse: The engine process the evidence.       (machine subj OK — not over-blocked)
captain decide idea       coarse: The captain decide the idea.           (already fine — untouched)
```

Note *clock/discover/mountain* all survive in order in the rerouted version — the
cover word (`captain`) carries no payload, so decoding is unchanged.

## Honest caveats

- **Coherence is scored against the same frames the planner enforces**, so the
  absolute coherence numbers are somewhat self-referential. The robust signal is
  the *shape* (saturation at coarse) and the *density cost*, not the exact values.
- **Vocabulary is the 70-word annotated slice**, so payloads are drawn from it.
  A real run needs the full noun list annotated (I can do BIP39's ~1580 nouns
  by hand; the 131k-lemma list needs a WordNet-assisted first pass).
- **Morphology is out of scope** — the prototype prints bare lemmas
  ("clock discover"); the real generator already handles agreement/inflection in
  `src/generator/agreement.rs`.
- **The repair model (defer to a trailing appositive) is a stand-in.** The real
  generator would reroute via frame/sentence-boundary search, which should be
  *cheaper* than a fixed appositive — so the density costs here are an upper bound.

## Real BIP39 dataset (full vocabulary)

The files above are the hand-authored 70-word teaching slice. The **full**
dataset for all 1582 BIP39 nouns and 1108 verbs is generated by `build_data.py`
using the hybrid from the design thread — **WordNet first pass + LLM override
layer** — and validated on full-vocab payloads by `sweep_real.py`.

```
pip install nltk && python3 -c "import nltk; nltk.download('wordnet')"   # once
python3 build_data.py     # -> data/noun_classes_bip39.yaml, data/verb_frames_bip39.yaml
python3 sweep_real.py 6    # full-vocab sweep + 6 random 11-word examples
```

(The generated `data/*.yaml` are committed, so `sweep_real.py` runs without
`nltk`; you only need WordNet to regenerate them via `build_data.py`.)

**How it's built**
- **Nouns**: WordNet `lexname` (`noun.animal`, `noun.artifact`, `noun.person`,
  `noun.location`, `noun.cognition`, …) → fine class, with two correction layers:
  - a **concreteness-preferring sense pick** (a payload noun reads as its concrete
    sense in prose, so a concrete synset among the top senses beats WordNet's
    most-frequent, often-metaphorical one — fixes `table→furniture` not `data-table`);
  - an **`agentive` list** (machines aren't a WordNet category — they're
    `artifact`) and a small **override** map for known quirks (`tiger→animal`
    not "fierce person", `forest→place`, `love→concept` not "beloved person").
  - Every entry carries a `source` field (`wordnet` | `agentive` | `override` |
    `default`) so a human can audit exactly what was machine-guessed. Only **10**
    nouns fall to `default` (words BIP39 weakly tags as nouns, e.g. "beyond").
- **Verbs**: WordNet verb `lexname` → one of 13 selectional **archetypes**
  (`mental`, `motion`, `creation`, `perception`, `stative`, …) rather than 1108
  bespoke frames. Each archetype fixes subject/object class expectations in
  top-level classes. This is the tractable, VerbNet-style way to frame 1000+ verbs.

**Class distribution** (nouns): thing 46%, abstract 34%, animate 13%, place 6%,
agentive 1%.

**Validated result** (400 × 12-word payloads, full real vocab). Frames are
top-level, so `coarse` (5) is full frame resolution and defines coherence 1.0:

```
granularity #classes  density  coherence
none              1    0.432      0.450     <- today's generator
binary            2    0.386      0.612
quad              4    0.315      0.957     <- 4 classes captures 96%
coarse            5    0.307      1.000     <- place split adds only +4.3 pts

Marginal: binary -> quad  = +34.5 coh pts   (the split that does the work)
          quad   -> coarse =  +4.3 coh pts   (place — optional)
```

The 70-word slice's conclusion **holds on the full vocabulary**: coherence
saturates at 4 classes (`animate / agentive / thing / abstract`), and keeping
`agentive` distinct is what earns the big jump. Density cost is steeper here
(~27% at quad) because random real payloads clash more often than the curated
slice — an honest upper bound, since the real generator would reroute via
frame/boundary search rather than the prototype's fixed-cost appositive.

**Known limitations of this dataset**
- WordNet-sourced entries still contain sense errors (the `source` field marks
  them for review); the override layer only covers what spot-checks surfaced.
- Verb frames are coarse archetypes — good for subject animacy and coarse object
  class, not fine object restrictions (`drink`→substance would need per-verb work).
- BIP39 tags many nouns weakly as verbs (`husband`, `soap`, `medal`), so some
  generated clauses use odd denominal verbs — a wordlist-POS issue, not semantics.

## Wired into the Rust generator

The design below is now implemented. The soft-scoring approach ("prefer coherent
skeletons, never block a payload word") is live on the native/CLI encode path.

**Data** — `emit_rust_data.py` converts the experiment dataset into the file the
generator loads:
```
python3 emit_rust_data.py    # -> languages/english/semantics.yaml
```
`semantics.yaml` holds top-level classes per payload word and per-verb
subject/object frames. It's embedded at compile time (via `build.rs`, as an
`other` language file) and loaded by `src/generator/data.rs::load_semantics`.

**Module** — `src/generator/semantics.rs` (`SemanticModel`):
- parses `semantics.yaml`;
- `placement_score(slots, placement, payload) -> f64` returns a coherence
  multiplier in (0,1]. Roles are inferred from the flat POS sequence (nearest
  payload noun left of a verb = subject, right = object), exactly like this
  prototype. Each violated verb-argument edge multiplies the score by 0.15,
  floored so it's never zero.

**Integration point** — `src/generator/core.rs::plan_sentence`. When choosing
among candidate POS skeletons at a fixed payload count `j`, each candidate's
grammar probability is multiplied by its semantic score before the weighted
draw. Because scoring happens *within* a fixed `j`, it never reduces how many
payload words are embedded — **density is untouched, coherence is a tiebreaker**.
The model is carried on the `Lexicon` (`with_semantics`); when absent (every
language without a `semantics.yaml`) the score is 1.0 and behavior — including
the RNG draw — is byte-for-byte identical to before.

**Safety** — payload words are never dropped or reordered, so decoding is
unaffected. Covered by `tests/semantic_planning.rs` (dataset loads; payload
order preserved) and unit tests in `semantics.rs`. Escape hatch:
`GLOSSIA_DISABLE_SEMANTICS=1` forces classic POS-only planning.

**Measured on the real generator** (`ab_real_generator.py`, same payloads/seed,
semantics off vs on): _see script output_ — payload-payload coherence rises with
no change to which payload words appear.

**Not yet wired**: the WASM encode paths build their own `Lexicon` and remain a
no-op (safe) until `with_semantics` is added there; and cover-*word* selection
(picking a class-appropriate cover noun next to a payload verb) needs classes
for the 610 cover words, which aren't authored yet.

## If we productionize this

1. Add a `class:` field to the English payload wordlist (offline, authored once).
2. Add a verb-frames table (offline, authored once).
3. Express both as refined entity types and let the existing `accepts()` /
   `refinement_subsumes()` in `src/semantic_types.rs` do the compatibility check.
4. In the generator, gate verb-argument slot filling with a **soft** score
   (penalty, not veto) so it degrades gracefully to today's output and never
   fails to place a forced payload word.
