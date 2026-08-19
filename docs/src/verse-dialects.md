# Verse Dialects

Glossia can render a payload as metrical verse — lines of a fixed syllable
count, optionally scanning as iambic pentameter. The lines are real: a reader
can hear when one is wrong.

That is the point. Glossia is a readable encoding, and the properties it sells
are *readable, speakable, transcribable, verifiable*. Meter is the only one of
those a human can check without a machine. A line that does not scan is a line
that lost or gained a word.

## Using one

A verse dialect is an ordinary dialect:

```bash
glossia --dialect verse   ...   # ten-syllable lines
glossia --dialect haiku   ...   # 5 / 7 / 5, cycling
glossia --dialect blank   ...   # iambic pentameter
```

Decoding is unchanged, because nothing about the payload changed: filter the
words against the payload wordlist and read them in order. A verse rendering
decodes with exactly the code that decodes prose.

## What it costs

Measured over 50 payloads at `best_of=4`, against the `body` dialect scored the
same way:

| dialect | form | lines scan | density | vs `body` |
|---|---|---|---|---|
| `body` | — | 34% | 0.581 | — |
| `verse` | 10 syllables | **90%** | 0.570 | −1.9% |
| `haiku` | 5 / 7 / 5 | **68%** | 0.559 | −3.8% |
| `blank` | iambic pentameter | **78%** | 0.502 | −13.6% |

Syllable counting is nearly free. Stress meter costs about an eighth of the
density — the price of a filler that must satisfy the beat as well as the line.

## How it works

Three rules, in order of precedence:

1. **The payload is untouchable.** Payload words and their order are fixed
   before any of this runs. A payload word that will not scan where it lands is
   placed anyway and the line breaks. Meter never removes, reorders, or
   substitutes a payload word, which is why decoding is unaffected.
2. **Cover words carry the meter.** The generator already spends about a third
   of its words on cover. A verse dialect spends the same words differently:
   at each cover slot it asks the wordlist for a word of the slot's POS *and* a
   syllable count that keeps the line completable.
3. **Best-of-N picks the winner.** Density stays primary; among equally dense
   drafts, the one whose lines came out whole wins.

The mechanism that makes this work at all is small: **a monosyllable is
metrically flexible**, so placing one always scans and always flips the parity
of everything after it. Parity is therefore repairable at any slot offering both
a one- and a two-syllable word — and every content-bearing POS in the cover list
offers both.

Before committing to a word, the filler runs a backward pass over the sentence's
slots to find which metrical positions can still finish. Without it, a cover
word that fits locally can leave the line unfinishable; with it, the forward
walk never backtracks.

## Declaring one

A dialect becomes a verse dialect by declaring a `meter:` in `grammar.yaml`.
With no `rules:` of its own it inherits the base grammar, so verse output is
ordinary Glossia prose that has been *filled* to scan — not a different grammar:

```yaml
  dialects:
    blank:
      meter:
        lines: [10]        # syllables per line, cycling
        stress: lenient    # free | lenient | strict
        rise: true         # true = iambic, false = trochaic
```

`stress: free` counts syllables only. `lenient` additionally forbids a primary
stress on a weak beat — the standard reading of "does it scan", and the one
English verse observes; monosyllables float, as they do in practice. `strict`
also forbids an unstressed syllable of a polysyllable on a strong beat, which is
stricter than real verse and fails far more often.

A dialect with no `meter:` never loads the prosody data and generates exactly as
it did before verse dialects existed.

## The data

`languages/<lang>/prosody.yaml` annotates both wordlists — payload and cover —
the way `semantics.yaml` does. English's is built from CMUdict by
`experiments/prosody/build_prosody.py`, which covers 2047 of 2048 BIP39 payload
words without hand-annotation:

```yaml
stress: {"donkey": "10", "record": "010|100", ...}
rhyme:  {"donkey": "AONGKIY", ...}
```

The stress string does double duty: one digit per syllable, so **its length is
the syllable count**. A `|` separates pronunciation variants, which are free
slack for the fitter — *record* is both RE-cord and re-CORD. The filler's index
is `(POS, refinement, syllables, parity) → words`, with parity derived from the
stress string rather than stored.

Like `semantics.yaml`, the file annotates an existing wordlist rather than
defining one, so it is regenerable rather than append-only — **except** that a
canonical version naming the language freezes it, since verification re-renders.

## Why not rejection sampling

The obvious approach — generate normally, keep the drafts that happen to scan —
does not work for stress meter, and the measurements are in
`experiments/prosody/`. Over 10,240 candidate renderings, strict iambic
pentameter appeared **zero** times at 32 and 64 bytes.

The reason is not the vocabulary. Only 3 of 1440 polysyllabic words cannot scan
at *some* position: a trochee is a perfect iamb when it starts on the beat.
The reason is that a word's metrical position is fixed by the cumulative
syllable count of everything before it, so every polysyllable is one more
constraint on a running parity — about three times as many constraints as
syllable counting imposes, each roughly a coin flip.

Those constraints are not unsatisfiable, though. They are unsatisfiable *by
filtering*, because filtering arrives after the cover words have already been
spent on grammar. Building for the meter instead of filtering for it is what
turns 0% into 78%.
