# Bits per Syllable

Glossia's density is normally quoted per word: payload words over total words,
`0.581` for English body prose. That is the right unit for the generator, which
spends its budget in grammatical slots. It is the wrong unit for a person.

Speech runs at a roughly constant syllable rate — about 4–6 syllables per second
in English — so time-to-say is time-per-syllable times syllables, and a listener
writing it down gets one chance to slip per syllable. So for an
encoding whose whole pitch is *readable, speakable, transcribable*, the question
is not how many words a payload costs but how many **syllables**. And on that
scale an 11-bit word that happens to be `act` is three times the bargain that
`ability` is.

Measure it yourself:

```bash
cargo run --release --example bits_per_syllable [samples] [best_of]
```

Release matters twice over: debug is ~40× slower for the sequence enumeration,
and debug builds embed English only, so the other languages come back missing.

## Where the syllable counts come from

English has real data. `languages/english/prosody.yaml` carries a CMUdict-derived
stress string per word, one digit per syllable, so the count is measured, not
guessed — the same data the [verse dialects](./verse-dialects.md) use to scan.

No other language ships prosody data, so the rig falls back to counting vowel
nuclei, with each language's diphthongs and digraphs declared so they collapse to
one nucleus (Czech `ou`, German `ie`, Latin `ae`), Czech's syllabic `r`/`l`
handled, and English's silent final `e` subtracted. That fallback is scored
against CMUdict on the one list where truth is known:

```
── vowel-group heuristic vs CMUdict (2048 english payload words)
   exact 94.9%   within 1 100.0%   mean abs error 0.051 syllables
```

Which is the error bar on every Czech, German and Latin figure below. The English
figures carry none — they are measured.

## English, by dialect

13-word payloads, 50 samples, `best_of=4`:

| dialect | words | density | syl/word | syllables | **bits/syl** | scans |
|---|---|---|---|---|---|---|
| *(bare mnemonic)* | 13.0 | 1.000 | 1.711 | 22.2 | **6.43** | — |
| `body` | 22.4 | 0.581 | 1.472 | 32.9 | **4.34** | — |
| `syllabic` | 22.8 | 0.570 | 1.476 | 33.7 | **4.25** | 90% |
| `haiku` | 23.2 | 0.559 | 1.473 | 34.2 | **4.18** | 68% |
| `iambic` | 25.9 | 0.502 | 1.485 | 38.5 | **3.72** | 78% |
| `anapest` | 28.7 | 0.453 | 1.450 | 41.6 | **3.43** | 44% |
| `dactyl` | 27.3 | 0.477 | 1.514 | 41.3 | **3.46** | 84% |

The bare mnemonic row is the ceiling: 11 bits over BIP39's mean 1.711 syllables,
no grammar at all. Prose gives up a third of that to become sentences.

Note that cover words are *shorter* than payload words — 1.472 syllables per word
overall against the payload list's 1.711, because function words are mostly
monosyllables. So prose costs less per syllable than the word-density figure
suggests: it spends 42% of its **words** on cover but only 32% of its
**syllables**.

The verse dialects sort the same way they do on word density, and for the same
reason — meter is bought with cover budget. `syllabic` is nearly free
(−2% against `body`); `iambic` costs 14%.

## Against character encodings

A character encoding's spoken cost needs no sampling: it is fixed by its alphabet
and by how you name a character out loud. Two protocols are worth pricing. Naive
letter names (`b` = "bee") are what people default to; the NATO/ICAO spelling
alphabet (`b` = "bravo") is what anyone uses when the channel is noisy and the
string matters. Mixed-case alphabets get one extra syllable for a case marker —
"cap", the shortest anyone would accept.

| encoding | bits/char | syl/char | **bits/syl** | NATO syl/char | **NATO bits/syl** |
|---|---|---|---|---|---|
| decimal | 3.322 | 1.200 | **2.77** | 1.200 | **2.77** |
| hex | 4.000 | 1.125 | **3.56** | 1.500 | **2.67** |
| bech32 | 5.000 | 1.125 | **4.44** | 1.875 | **2.67** |
| base32 | 5.000 | 1.094 | **4.57** | 1.969 | **2.54** |
| base58 | 5.858 | 1.500 | **3.91** | 2.397 | **2.44** |
| base64 | 6.000 | 1.500 | **4.00** | 2.375 | **2.53** |
| **glossia `body`** | 11/word | — | **4.34** | — | **4.34** |

Read honestly, that is two different results.

Against naive letter names, Glossia prose (4.34) beats hex (3.56), base64 (4.00)
and base58 (3.91), and **loses** to base32 (4.57) and bech32 (4.44). Those two win
by having a 32-character alphabet of almost entirely one-syllable names — there is
not much room above 5 bits in 1.1 syllables.

Against NATO, prose wins everything by 1.6–1.8×, because prose has no NATO column
to pay. A payload word *is* its own phonetic distinguisher: `abandon` and
`ability` do not rhyme the way `b` and `d` do. Glossia gets robust spelling for
free, where a character encoding has to buy it at roughly a syllable per
character.

Which column is fair depends on whether the string has to survive being said out
loud. It usually does — that is the entire premise. And bech32's alphabet was
chosen to drop the worst confusions (`1`, `b`, `i`, `o`), but it still contains
`p z t v d c e g 3`, nine symbols that rhyme with each other, so its naive column
is optimistic for anything dictated rather than pasted.

One robustness difference that bits per syllable does not capture, in Glossia's
favour: a mistranscribed payload word usually falls *off* the wordlist and is
therefore detectable, which is what the v3 error correction builds on (see
[Canonical Encoding](./canonical-encoding.md)). A mistyped hex character is still
a perfectly valid hex character.

## The wordlist frontier

A wordlist of 2^m words carries m bits per word, so bits/word is free to grow.
But the words you add to reach 2^m are, on average, longer than the ones already
there. Per syllable the two effects fight — and the fight has a winner. Take the
k shortest words of a list and ask what m / mean-syllables comes to:

| language | m | words | syl/word | bits/syl |
|---|---|---|---|---|
| english | 8 | 256 | 1.000 | 8.00 |
| english | **9** | **512** | **1.000** | **9.00** |
| english | 10 | 1024 | 1.183 | 8.46 |
| english | 11 | 2048 | 1.711 | 6.43 |
| latin | 8 | 256 | 1.027 | 7.79 |
| latin | 9 | 512 | 1.514 | 5.95 |
| latin | 10 | 1024 | 1.757 | 5.69 |
| latin | 11 | 2048 | 1.878 | 5.86 |
| latin | 12 | 4096 | 1.939 | 6.19 |
| latin | 13 | 8192 | 2.368 | 5.49 |
| latin | 14 | 16384 | 2.696 | 5.19 |
| latin | 15 | 32768 | 3.538 | 4.24 |

837 of BIP39's 2048 words are monosyllables. A 512-word all-monosyllabic list would carry
**9.00 bits per syllable against BIP39's 6.43** — 40% denser to say, at the cost
of 2 bits per word. Going the other way is worse than it looks: Latin's 32768-word
list carries 15 bits per word, 36% more than English's 11, and is *less* dense per
syllable (4.24 vs 6.43) because its words run 3.54 syllables. A bigger wordlist
only pays if word length grows slower than log₂(N), and past a few thousand words
it does not. The Latin curve is not even monotone — 2^12 (6.19) beats 2^11 (5.86),
because that particular doubling happens to add barely any length — so the right
size for a new list is something to measure, not to reason about from word count.

None of this is a proposal to change BIP39, which is fixed by a standard outside
this project and [append-only](./language-support.md) inside it. It is the number
to look at when sizing a *new* payload wordlist for a speech-first dialect.

## Across languages

Body prose, 50 samples, `best_of=4`. "Bare bits/syl" is the wordlist read out on
its own (bits/word over the wordlist's own mean syllables); "prose bits/syl" is
the rendered text, where the shorter cover words pull syllables-per-word down but
cost density:

| language | bits/word | density | prose syl/word | bare bits/syl | prose bits/syl |
|---|---|---|---|---|---|
| english | 11 | 0.581 | 1.472 | 6.43 | **4.34** |
| german | 11 | 0.511 | 1.562 | 5.56 | **3.60** |
| latin | 15 | 0.617 | 2.689 | 4.24 | **3.44** |
| czech | 11 | 0.571 | 1.934 | 4.55 | **3.25** |

English leads on both. Latin's larger wordlist buys back some of what its word
length costs — it has the highest word density of the four — but not enough to
change the order. German and Czech figures rest on the vowel-group heuristic.

## What an artifact actually costs

Everything above prices the *encoding*. This prices the *product*. A canonical
artifact also carries a version byte, a crc32, and (from v3) Reed–Solomon parity,
so the payload bits a user cares about are fewer than the words carry. Averaged
over 20 payloads per size:

| language | bytes | words | syllables | bits/syl | vs prose |
|---|---|---|---|---|---|
| english | 16 | 35.8 | 52.8 | 2.42 | 56% |
| english | 20 | 40.8 | 59.5 | 2.69 | 62% |
| english | 32 | 56.6 | 86.3 | 2.96 | 68% |
| czech | 32 | 55.9 | 108.5 | 2.36 | 73% |
| latin | 32 | 38.2 | 101.8 | 2.52 | 73% |

The overhead amortizes, as it should: the envelope is a fixed 40 bits and the
parity floor a fixed four words, so a 32-byte payload keeps 68% of the raw prose
rate where a 16-byte one keeps 56%. Read this table when comparing Glossia to a
raw hex string of the same *payload*, and the tables above when comparing the
encodings themselves.

## Caveats

- **Syllables are a proxy for time, not a measure of it.** Real speech rate varies
  with word familiarity and phrase structure; a syllable of `strengths` is not a
  syllable of `a`.
- **Non-English syllable counts are heuristic**, with the error bar quoted above.
  Adding a `prosody.yaml` for a language replaces the guess with data.
- **The character-encoding rows are computed, not measured** — there is no grammar
  in between to sample. The one judgement call is the case marker at one syllable.
- **The `scans` column is not part of the density story.** A low scan rate means
  lines come out broken, not that bits were lost.
