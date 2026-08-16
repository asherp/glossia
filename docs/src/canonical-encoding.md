# Canonical Encoding

`canonical_encode` and `canonical_decode` are the stability contract of the
library: one payload has exactly one prose form, and that form stays valid
across library releases. Everything else in the API is free to evolve;
canonical artifacts are not.

```rust
use glossia::{canonical_encode, canonical_decode};

let payload: Vec<u8> = /* any bytes */;
let text = canonical_encode(&payload, "english", "bip39")?;

let d = canonical_decode(&text, "english", "bip39")?;
assert_eq!(d.payload, payload);
assert!(d.verified);   // the wording matches the canonical re-render
```

From WASM the same pair is exposed as `canonical_encode(payload_hex, language,
wordlist)` and `canonical_decode(text, language, wordlist)`, returning JSON.

## How it works

Encoding prefixes the payload with a **version byte**, packs
`[version | payload]` into payload words with the self-describing `bitpack`
codec, and renders cover prose seeded from a checksum of those exact bytes. The
seed makes the *wording* carry the checksum: change any payload bit and the
whole paragraph re-renders differently, which is what makes damage visible to
someone reading the text.

Decoding filters the text against the payload wordlist, unpacks the version
byte, and verifies by re-rendering the payload **under the rules of the version
the artifact carries** — not the current ones — and comparing wording.
Comparison is over normalized alphanumeric tokens, so punctuation spacing,
case, and surrounding markup are formatting, not damage.

## Versioning: how rendering improves without breaking artifacts

Each version freezes a `VersionRules`: the best-of budget, the dialect, and
which languages render with a semantic model. `rules_for(version)` is the
registry; a library at version *n* keeps the rules for every version ≤ *n*.

Version 1: `best_of: 4`, dialect `body`, semantics for English only.

The worked example: today Latin and Czech have no `semantics.yaml`, so v1
renders them with POS-only planning. When a Latin semantics model ships later,
it gets **version 2** with Latin added to `semantics_languages` — new artifacts
are written at v2, and every v1 artifact keeps verifying because its version
byte selects the v1 rules, under which the Latin model does not exist.

A version byte the library does not recognize is refused by name
(`UnsupportedVersion`), not reported as damage: the artifact is from a newer
release and this build cannot verify it.

## Error correction (v3)

v1 and v2 can *detect* damage. v3 **repairs** it, by appending Reed–Solomon
parity to the packed words.

The symbol is a **payload word**, not a byte. Every shipped payload wordlist is
a power of two, so a word's index is a field element — GF(2¹¹) for English,
Czech and German, GF(2¹⁵) for Latin — and one mistranscribed word is exactly one
wrong symbol. Computing parity over the bytes instead would make an 11-bit word
straddle two or three byte boundaries, so a single fault would cost two or three
symbols to repair. This is why parity sits in `VersionRules` beside the
envelope rather than inside it: v3 seals exactly the bytes v2 does, and the
parity is added a layer later, over the words those bytes packed into.

**Parity is pinned to the version**, not passed per call. `V3_PARITY` is 4. A
parameter would have to be declared somewhere the decoder could read before
decoding — costing a symbol, the very thing parity is counted in — and it would
make an artifact's word count depend on a caller's choice rather than on its
payload. Pinning keeps `canonical_encode_fixed`'s word count a constant per
payload size, which is what lets a format state a field's length in its notation
instead of carrying it.

### Located damage costs half

The bound is `2·errors + erasures ≤ parity`. An *error* is a wrong symbol at an
unknown position; an *erasure* is a known-bad position. So v3 repairs **two**
words it has to find, or **four** it is told about.

Being told is what the alignment layer is for. Decoding filters prose against
the wordlist, so damage does not stay put: a payload word mangled *off* the
wordlist never reaches the harvest at all — the sequence is simply one shorter
and every later word has slid up a slot — and a cover word mangled *onto* it
arrives as a symbol nobody sent. No positional code survives that unaided.

`align(received, rendered, markup, is_payload)` compares received prose against
its expected rendering and returns `payload_slots`: one entry per word the
rendering expected, holding what arrived there or `None` where nothing usable
did. That is a codeword of known length with its holes marked, which is exactly
what the decoder wants.

```rust
use glossia::{align, canonical_decode_slots_fixed, canonical_encode_fixed};

let text = canonical_encode_fixed(&payload, "english", "bip39")?;
// ... transcription damages the prose ...
let a = align(&damaged, &candidate_rendering, None, &is_payload);
let d = canonical_decode_slots_fixed(&a.payload_slots, "english", "bip39", payload.len())?;
assert_eq!(d.payload, payload);
assert!(!d.repaired.is_empty());   // which words were corrected
```

Alignment needs a candidate to render from, so it cannot bootstrap: a decode
whose payload damage fails the checksum has nothing to compare against. What it
does is turn a *candidate* into a verdict with positions attached — the position
an error-correcting decoder is in once it has something to propose.

### Corrections are reported, never silent

`CanonicalDecoded::repaired` lists the word positions parity corrected. Non-empty
means the payload is a *correction*: those words did not say what the artifact
was written with. The correction is backed by the envelope's crc32, but a caller
should surface it rather than apply it silently — a reader who mis-copied a word
is better told which one.

Damage past the bound returns an error rather than a best effort, and every
repair is checked by recomputing the syndromes over the corrected word. A
decoder that hands back its best guess is how a burst beyond the bound becomes a
valid-looking wrong answer.

## What a shipped version freezes

Verification-by-re-render makes the whole rendering path part of the format.
Once a canonical version has shipped, for its languages:

- **Grammar, dialect config, and cover wordlists** must not change in place.
  Cover appends change RNG draws and therefore renderings, so even an
  append-only cover change needs a new version if canonical artifacts exist.
- **`semantics.yaml`** for languages in `semantics_languages` is frozen —
  regenerating English's model is a v2, not an edit.
- **The RNG** is pinned to ChaCha12 (`CoverRng`, see `tests/rng_pinning.rs`).
- **`GLOSSIA_DISABLE_SEMANTICS` is ignored** by the canonical path — an
  environment variable must not change what an artifact looks like.
- **The parity, the field, and the generator polynomial** for a version that
  spends parity. RS symbols are payload words like any other, so any of those
  moving moves the prose.

`tests/canonical.rs` pins exact golden renderings per language. A golden test
failing means a change re-rendered a shipped version's artifacts; the fix is a
new version entry, never an updated golden.
`examples/canonical_probe.rs` prints the renderings for pinning a new
version's goldens.

## The address format uses it

The prose Bitcoin address panel (`web/index.html`) is the first consumer: an
address's program bytes go through `canonical_encode`, so a 20-byte hash160 is
17 words (version byte + pad word included) and a 32-byte witness program is
25. The panel's checker builds on `canonical_encode_traced` (grammatical-role
annotations), `canonical_decode_raw` (repair-search candidates without a
verify render), and the `canonical_text` returned by `canonical_decode`
(wording diff without a second generation). The opcode glyphs remain page-side
markup around the canonical prose.

## When not to use it

Callers that need their own seeds, packing, or best-of policy — or prose that
deliberately is not stable across releases — should use the unversioned seams
(`encode_words_into_language`, `ZoneGenerator`, `checksum_seed`). Those are
deliberately flexible; the canonical pair is deliberately rigid.
