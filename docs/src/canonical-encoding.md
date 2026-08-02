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

`tests/canonical.rs` pins exact golden renderings per language. A golden test
failing means a change re-rendered a shipped version's artifacts; the fix is a
new version entry, never an updated golden.
`examples/canonical_probe.rs` prints the renderings for pinning a new
version's goldens.

## When not to use it

Formats that pack their own bits — a fixed-length address with a header in the
bit-packing slack, custom seeds, their own best-of policy — should keep using
the unversioned seams (`encode_words_into_language`, `ZoneGenerator`,
`checksum_seed`). Those are deliberately flexible; the canonical pair is
deliberately rigid.
