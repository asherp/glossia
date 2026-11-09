# A corpus-based Korean payload wordlist

Question: what do we gain and lose by replacing (or supplementing) the official
BIP39 Korean list with a list drawn from a frequency corpus, the way Latin's
list was drawn from Wiktionary?

`measure.py` answers the measurable part. It starts from the **Shiberu Korean
Word Frequency List 2026** (80,000 dictionary-form lemmas, release 2026.10.28,
sha256 `6ed27cd6…c124`), tags every lemma with Kiwi (kiwipiepy 0.24.1), filters,
and keeps words greedily in rank order so that **every surface form a word can
take in prose maps back to exactly one payload index**: the bare noun and all
eight particle forms PR #97 attaches (이/가, 은/는, 을/를, 에, 에서, 과/와, 의,
으로/로), or, for verbs, five template conjugations (past 었다, -고, -어서,
-어요, adnominal -는/-은). No form may equal a cover token either.

```bash
pip install kiwipiepy
python3 experiments/korean_corpus/measure.py ko-frequency-2026.tsv \
    --cover languages/korean/cover.yaml --bip39 languages/korean/payload.yaml \
    --out report.json --dump kept
```

`report.json` here is the output of that run. The TSV is not committed; it is
CC BY 4.0 and downloads from https://shiberu.app/korean-frequency-list.

## Tiers

| tier | payload POS | lexicon check | survivors |
|---|---|---|---|
| nouns-loose | nouns (incl. compounds), adverbs, prenouns | none | 44,570 |
| open-loose | + verbs, adjectives | none | 48,316 |
| **nouns-strict** | nouns, adverbs, prenouns | Kiwi lexicon, one morpheme | **24,809** |
| open-strict | + verbs, adjectives | Kiwi lexicon, one morpheme | 28,558 |

Funnel: 80,000 → 79,358 all-Hangul → ~46k noun-like / ~56k incl. predicates.
The big cuts are proper nouns (16,146), 하다/되다 twins of a listed noun
(5,898: 공부하다 next to 공부, the confusion BIP39 reviewers flagged), and
in the strict tiers Kiwi out-of-vocabulary guesses and spaceless phrases
(사회변화, 주요경력, 앵그리버드, 히가시).

**Particle and conjugation collisions are not the bottleneck**: only 59–185
words are lost to them across all tiers (e.g. 길 vs 길이, 강 vs 강의; 걷다 vs
걸다 → 걸었다). Supply of real words is.

## Size versus quality

Rank and per-million frequency of the *last* word admitted at each size:

| size | bits | nouns-strict: last rank | per million | mean syllables | bits/syllable | edit-1 neighbours |
|---|---|---|---|---|---|---|
| 2,048 | 11 | 3,941 | 25.5 | 2.22 | 4.95 | 46% |
| 4,096 | 12 | 7,773 | 10.2 | 2.28 | 5.26 | 55% |
| 8,192 | 13 | 16,060 | 3.6 | 2.37 | 5.49 | 58% |
| 16,384 | 14 | 37,536 | 0.94 | 2.50 | 5.60 | 57% |
| 32,768 | 15 | — (loose only: rank 58,534, 0.44/M) | | 2.89 | 5.19 | 45% |
| 65,536 | 16 | — (not reachable in any tier) | | | | |

BIP39 Korean for comparison: 2.23 syllables, 4.93 bits/syllable, 40% edit-1
neighbours; median Shiberu rank 3,790, only 613 of its words in the top 2,048,
5 absent from the list entirely (올가을, 외갓집, 작은딸, 큰어머니, 팔월).

"Edit-1 neighbours" is the share of words with another list word one jamo
edit away. Short Korean words are dense in jamo space at every size, so this
is something Reed–Solomon parity has to absorb, not something filtering can
remove without gutting the list.

## Reading the numbers

- **2¹⁶ per word is out of reach** from this corpus: even the loosest tier tops
  out at ~48k. 256 bits in 16 Korean words would need a much larger source and
  would be full of names, brands and phrases.
- **2¹⁴ is the largest clean size** (strict nouns, 16,384 words, down to about
  one occurrence per million). That is 14 bits/word, 19 words for 256 bits
  instead of 24, and also the best bits-per-syllable of any size: past 2¹⁴ the
  words get longer faster than they get more informative.
- **2¹¹ from the corpus is simply more familiar than BIP39** at the same
  density: every one of its words is at least 25/million.
- **Nouns-only is enough.** Verbs add ~3.7k strict words but need a
  conjugation pass and a surface→index decode table (irregulars break
  suffix stripping). The noun tier works with PR #97's decoder unchanged.
- Strict still lets through slang and loaded words (몰카, 도둑놈, 조지다) and
  occasional mis-tags; a native-speaker review pass is still needed, as it was
  for BIP39 Korean.
