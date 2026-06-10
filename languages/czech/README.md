# Czech BIP39 Wordlist

## Provenance

The Czech wordlist (`wordlist.txt`) is the **official** BIP-0039 Czech wordlist
of 2048 words, sourced from the canonical Bitcoin BIPs repository.

**Source URL:** https://raw.githubusercontent.com/bitcoin/bips/master/bip-0039/czech.txt

Unlike the community-maintained German list, Czech **is** one of the officially
supported BIP39 languages defined in
[BIP-0039](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki):

- English, Japanese, Korean, Spanish, Chinese (Simplified), Chinese (Traditional),
  French, Italian, **Czech**, Portuguese

The list contains exactly 2048 words (11 bits/word), one word per line, lowercase,
in BIP39 alphabetical order.

## Files

| File | Purpose |
|------|---------|
| `wordlist.txt`  | Raw official BIP39 Czech wordlist (provenance / decoding reference) |
| `payload.yaml`  | The 2048 payload words with POS tags (the default wordlist) |
| `cover.yaml`    | Czech cover words that fill non-payload grammar slots (disjoint from payload) |
| `grammar.yaml`  | Montague grammar with `body`, `subject`, `prose`, and `payload_only` dialects |

## POS tagging

Czech is morphologically rich, so POS tags in `payload.yaml` are assigned by
suffix heuristics:

- `-ovat`, `-nout`, `-ět` → verb infinitive (`V`)
- `-ít`, `-it`, `-at`, `-et` → verb infinitive (`V`) with a noun (`N`) fallback weight
- everything else → noun (`N`)

POS tags only guide how natural the generated cover text reads. **Decoding never
depends on them** — it simply filters the output against this wordlist, so tagging
accuracy cannot affect round-trip correctness.

## Grammar notes

Czech, like Latin, has **no articles**, so the grammar defines no `Det` category.
Word order is flexible (default SVO); case endings carry the grammatical relations.
Because the BIP39 payload is noun- and verb-heavy, the productions favour `N`/`V`
slots and let the cover wordlist supply copulas, prepositions, conjunctions,
pronouns, adjectives, and adverbs.

The BIP39 Czech list contains no diacritic-vowel-final words, so every adjective
(`-ý`/`-á`/`-é`/`-í`) and manner adverb (`-ě`) in `cover.yaml` is guaranteed
disjoint from the payload.

## Usage

```bash
# Generate Czech cover text from 12 random payload words
glossia --random 12 --language czech --seed 0

# Encode specific Czech BIP39 words
glossia --language czech abdikace abeceda adresa
```
