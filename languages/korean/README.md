# Korean BIP39 Wordlist

## Provenance

The Korean wordlist (`wordlist.txt`) is the **official** BIP-0039 Korean wordlist
of 2048 words, from the canonical Bitcoin BIPs repository, byte for byte.

**Source URL:** https://raw.githubusercontent.com/bitcoin/bips/master/bip-0039/korean.txt

The published file is in NFD: each syllable is stored as its conjoining jamo.
`payload.yaml` holds the same 2048 words in the same order, converted to NFC
(precomposed syllables), which is how Korean text is normally written and what
the prose carries. Decoding composes Hangul before lookup, so input in either
form decodes to the same words.

## Files

| File | Purpose |
|------|---------|
| `wordlist.txt`  | Raw official BIP39 Korean wordlist (NFD, provenance reference) |
| `payload.yaml`  | The 2048 payload words (NFC) with POS tags |
| `cover.yaml`    | Cover words: particles, verbs, adjectives, nouns, pronouns, connectives |
| `grammar.yaml`  | Grammar with `body`, `subject`, `prose`, `bip39` and `payload_only` dialects |
| `generate.py`   | How `payload.yaml` and `cover.yaml` were produced |

## Grammar notes

Korean is verb-final (SOV) and has no articles, so there is no `Det`. The BIP39
list is nearly all nouns, so every sentence ends in a cover verb in the plain
past declarative (보았다, 만들었다, 왔다).

Case and topic are marked by particles written onto the noun: 가격 + 을 is
가격을. The grammar places each particle as a cover `Prep` word with a
refinement naming its function:

| Refinement | Forms | Function |
|---|---|---|
| `subj` | 이 / 가 | subject |
| `top`  | 은 / 는 | topic |
| `obj`  | 을 / 를 | object |
| `loc`  | 에, 에서 | location |
| `com`  | 과 / 와 | "and", "with" |
| `gen`  | 의 | possessive |
| `dir`  | 으로 / 로 | direction, means |

The grammar sets `morphology: korean`. That pass picks the form the noun's last
syllable calls for (the first form after a final consonant, the second after a
vowel; 로 after a final ㄹ) and attaches the particle to the noun.

## Decoding and attached particles

A token that is not a payload word as written is tried once more with one
trailing particle removed. Two properties of the data make this exact, and
`tests/korean.rs` checks both over every word:

- No payload word plus a particle is another payload word.
- No cover noun or pronoun plus a particle is a payload word. Several short
  nouns fail this (길 + 이 = 길이, 강 + 의 = 강의), so they are not cover words.

## POS tagging

Nouns are `N`. Words ending in -히 and a hand-picked set of adverbs (갑자기,
결국, 이렇게) are `Adv`, so no particle is attached to them. Sino-Korean -적
forms and prenouns (온갖) are `Adj`, standing before a noun. Tagging only
affects how the prose reads; decoding is wordlist membership.

## Usage

```bash
glossia --random 12 --language korean --seed 0
glossia --language korean 가격 가방 가을
```
