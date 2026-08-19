# Prosody — measuring what meter costs

An offline measurement rig (Python, not part of the Rust build) for the
question behind poetry dialects: **can Glossia's prose be made to scan, and how
much density does that cost?**

Same discipline as `experiments/semantic_planner/`: prove the tradeoff on paper
before touching the generator.

## Why this can only be a filter

Payload words and their order are fixed by the payload — that is the invariant
the decoder rests on, and meter must not touch it. So a poetry dialect cannot
*edit* a candidate into scanning; it can only *choose* candidates that already
do, which is exactly what best-of-N already does. The measurement is therefore:
over N draws, how often does one scan, and how dense is the densest one that
does.

## What's here

| file | what it is |
|------|------------|
| `build_prosody.py` | CMUdict → `data/prosody_<lang>_<profile>.yaml`: syllables, stress, rhyme key per word, with a `heuristic` list naming every word it had to guess |
| `measure.py` | scores a candidate dump against eleven verse forms; prints hit rate vs N, density cost, and which of the two constraints is doing the blocking |
| `anatomy.py` | why stress meter fails where syllable counting succeeds — it is not the vocabulary |
| `show.py` | prints laid-out specimens, because "does it scan" and "is it readable" are different questions |
| `../../examples/prosody_candidates.rs` | the Rust side: dumps K candidates per payload as TSV |

Run:

```
python3 build_prosody.py english bip39
cargo run --release --example prosody_candidates 40 64 > /tmp/candidates.tsv
python3 measure.py /tmp/candidates.tsv
python3 show.py /tmp/candidates.tsv blank-tail 5
python3 anatomy.py /tmp/candidates.tsv
```

Needs `pyyaml` and `cmudict`.

## Result 1 — the wordlist is already prosodically usable

CMUdict covers **2047/2048** BIP39 payload words (the miss is `artefact`, a
British spelling) and 608/610 cover words (the misses are `re:` and `fwd:`,
which are `Prefix` tokens, not words). No hand-annotation is needed, and no
guessing enters the data.

| | mean syllables | 1 syl | 2 syl | 3 syl | >1 stress variant |
|---|---|---|---|---|---|
| payload (bip39) | 1.711 | 40.9% | 48.0% | 10.2% | 4.9% |
| cover | 1.421 | 62.5% | 33.4% | 3.9% | 3.8% |

The stress distribution looks alarming for rising meter — **`10` (falling) is
32.6% of payload words, `01` (rising) only 11.4%** — but see Result 4: that
turns out not to be what blocks iambic verse.

## Result 2 — syllable-counted verse is free; stress meter is not reachable

160 payloads (16/20/32/64 bytes) × 64 draws = 10,240 candidates, English
`body` dialect. "Hit rate" = share of payloads with at least one fitting
candidate in the first N draws.

| form | N=4 | N=16 | N=64 | density vs free best-of-4 |
|---|---|---|---|---|
| `blank-tail` — 10-syllable lines, short last line | 78% | 100% | 100% | **−6.5%** (denser) |
| `trochee-nostr` — 8-syllable lines, short last | 60% | 92% | 98% | **−4.0%** (denser) |
| `renga-tail` — 5/7/5 cycling, short last | 48% | 72% | 95% | +0.4% |
| `blank` — exact multiple of 10 | 25% | 65% | 98% | +6.0% |
| `renga` — exact 5/7/5 | 12% | 40% | 78% | +4.0% |
| `iambic` — decasyllabic *and* scanning | 0% | 0% | **0%** | — |
| `trochaic-8` — 8 syllables *and* scanning | 0% | 5% | 20% | +3.5% |

(16-byte column; the full tables per size are what `measure.py` prints.)

Two readings, and they point opposite ways:

**Syllable counting is essentially free.** Against the best-of-4 baseline that
canonical v1 actually ships, the densest 10-syllable-lined candidate out of 64
is *denser than today's output*, not sparser — the cost of the meter filter is
more than repaid by drawing more candidates. A `lines` or `blank` dialect is a
change to `VersionRules` and a layout pass, not a density sacrifice.

**Stress meter is out of reach by rejection sampling.** Strict iambic
pentameter never once appeared in 10,240 draws at 32 and 64 bytes, and 0–15% of
payloads found one at 16–20 bytes even at N=64. Trochaic — which the wordlist
should favour — only reaches 20%, which is the first sign that the vocabulary
is not the explanation.

## Result 3 — where the failures actually come from

Per candidate, split into the two things that must go right:

| form | bytes | P(total syllables fits) | P(cuts land on word boundaries \| total fits) |
|---|---|---|---|
| `blank` | 16 | 9.9% | 36.2% |
| `blank` | 64 | 10.2% | 2.3% |
| `renga` | 16 | 6.9% | 10.2% |
| `renga` | 64 | 5.5% | 1.4% |

`P(total fits)` sits at exactly chance (1/10, 1/17) — the generator is not
aiming at a syllable count, it is hoping for one. And `P(cuts land)` decays
sharply with length, because a longer text has more line breaks and each one is
another opportunity to fall mid-word.

Both of these are *lotteries the generator could stop playing*. It already
decides how many cover words to spend and which ones; a filler that knows
syllable counts can fill **to the line boundary** — needing two more syllables
and picking a two-syllable cover word of the right POS — instead of filling
blind and checking afterwards. That converts both probabilities toward 1 and is
where the real engineering should go.

## Result 4 — why stress meter is different in kind

`anatomy.py`. The tempting explanation is the one above: the wordlist falls,
iambic meter rises. It is wrong, and it matters, because acting on it would
send the work toward trochaic dialects that fail just as hard.

**Words are not the obstacle.** Only **3 of 1440** polysyllables (0.2%) cannot
scan at *some* starting parity. A trochee is a perfect iamb when it starts on
the beat — *the DON-key* is da-DUM da. Individual words fit fine.

**Position is the obstacle.** A word's metrical role is not a property of the
word; it is fixed by the cumulative syllable count of everything before it. So
every polysyllable is one more constraint on a running parity, and the
constraints form a chain. Syllable counting constrains a *sum*, once per line
break; stress constrains a *position*, once per polysyllabic word:

| bytes | syllables | line breaks (syllabic constraints) | polysyllables (metrical constraints) | P(blank-tail) | P(iambic-tail) |
|---|---|---|---|---|---|
| 16 | 38.1 | 2.8 | 9.2 | 40.1% | 0.31% |
| 20 | 40.3 | 3.0 | 9.7 | 49.9% | 0.31% |
| 32 | 66.0 | 5.6 | 16.6 | 24.5% | **0.00%** |
| 64 | 123.3 | 11.3 | 31.0 | 10.3% | **0.00%** |

Each constraint is roughly a coin flip, and stress imposes about three times as
many — 2⁻⁹ against 2⁻³ at 16 bytes, which is the measured 0.31% against 40%.

**And the repairs behave differently.** A syllable miscount is fixable *locally*
at the break: spend a two-syllable cover word instead of a one-syllable one and
nothing upstream changes. A parity error is not local — fixing it flips the beat
for every word after it, so repairs interact, and where two payload
polysyllables sit adjacent their relative parity is fixed by the payload itself
and no cover choice can touch it. Adjacent payload words are exactly what high
density produces, so stress meter is in *direct tension with density* in a way
syllable counting is not. That is visible in the cost columns: 12–32% for the
iambic rows against 3–6% for `blank-tail`.

**Rhyme is a third thing again**, and more tractable than its reputation: it is
a lexical choice at a single position, constructible whenever the line-final
slot is cover. 62% of cover words have a cover rhyme partner. But only **40% of
payload words have any cover word that rhymes with them**, so a line ending on
payload — which density wants — is unrhymable three times in five. Couplets fail
mostly because their *meter* half is already at zero, not their rhyme half.

## Specimens

Real output, unedited, from `show.py`:

```
── blank-tail  16 bytes  density 0.54
   Donkey may set advice to mosquito.
   Abandon abandon ability
   for affair out lens via army. Craft
   may divorce cannon. Ability why
   set a guy.

── renga  16 bytes  density 0.54
   Moment frequent the
   mobile abuse to cactus.
   April may guide to
   receive out red. Yes
   enlist our canvas to
   opera. Talent how
   get ability.
```

The blank verse reads well. The renga shows the next problem after syllable
counting: nothing stops a line break landing between a determiner and its noun
(`A / sorry rose`). Line breaks want to prefer clause boundaries — and unlike
stress, that *is* something the planner can steer, since the grammar knows
where its constituents end.

## Recommendation

1. Ship syllable-counted dialects (`lines`, `blank`, `haiku`/`renga`). They are
   reachable now, at no density cost against what v1 ships, using machinery
   that already exists.
2. Do not ship a stress-meter dialect (iambic, couplets). Rejection sampling
   cannot reach it, and a filler cannot rescue it either — the parity chain is
   global, and adjacent payload polysyllables are unfixable at any N.
3. Build the syllable-aware filler before the dialects if either is to scale
   past ~32 bytes. At 64 bytes the boundary lottery alone is 2.3%.
4. Worth measuring next, and not in the original plan: **rhymed syllabic verse**
   — equal-syllable lines that rhyme, with no stress requirement. Both halves
   are constructible rather than lucky, so unlike the couplet it may actually be
   reachable. It needs the filler to prefer cover words at line-final position,
   which is a scoring preference, not a new mechanism.
