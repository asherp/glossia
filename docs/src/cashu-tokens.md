# Cashu Tokens

A Cashu V4 token (`cashuB…`) encoded into a natural language goes through a
compact envelope and [canonical encoding](./canonical-encoding.md), so it gets
the canonical form's checksum and Reed–Solomon parity. A few mistyped or
misread words (from typing, or OCR of a screenshot) are repaired on the way back.

```bash
# token -> prose
echo -n "cashuB..." | glossia --encode --to english

# prose -> token (a greeting or sign-off around the paragraph is ignored)
glossia --decode --to cashu < voucher.txt
```

From Rust: `glossia::cashu_to_words(token, "english", "bip39")` and
`glossia::cashu_from_words(text, "english", "bip39")`. From WASM:
`cashu_to_words(token, language, wordlist)` → `{ encoded_text }` and
`cashu_from_words(text, language, wordlist)` → `{ token, repaired }`.

## The envelope

A V4 token is CBOR, and it spends space on things that can be packed tighter:
each proof's 32-byte secret is stored as 64 hex characters, and every token
spells out its mint URL and unit. The envelope (all lengths one byte):

```text
0x00 cbor..                       a V4 token's CBOR, unchanged (fallback)
0x01 flags [len url] [len unit] [len memo]
     nkeysets { len id  nproofs { exp secret[32] C[33] } }
```

`flags` bit 0: a mint URL follows (else `https://mint.hashu.io`); bit 1: a unit
follows (else `thh`); bit 2: a memo follows; bit 3: the URL follows without its
`https://`. A proof's amount is `2^exp`.

DLEQ proofs are left out; the receiver checks the token with its mint instead.
A token the envelope can't hold exactly (a NUT-10 P2PK/HTLC secret, a witness,
an amount that isn't a power of two) is carried as `0x00` raw CBOR, so nothing
but DLEQ is ever dropped.

The defaults are [Hashu](https://github.com/asherp/hashu)'s, whose THH
vouchers are Cashu tokens. Hashu writes the same envelope (`hashu_cli::words`),
and `tests/cashu_vector_*.txt` pins the two together.

## Sizes

Each proof costs 66 bytes, about 100 English words with parity and cover words.
Measured on `mint.hashu.io` THH tokens:

| Proofs | `cashuB` length | Raw CBOR, canonical | Envelope, canonical |
|---|---|---|---|
| 1 | 217 chars | 229 words | 126 words |
| 2 | 362 chars | 399 words | 207 words |
| 3 | 510 chars | 545 words | 319 words |

Before this, a `cashuB` token went through the character-level `crypto/cashu`
dialect, which spends a byte on every base64 character (464 words for the
2-proof token). That path is still used for V3 `cashuA` tokens and for verse
dialects, and decoding falls back to it for prose an older release wrote.

## Privacy

The prose names no mint and does not match the `cashu[AB]…` pattern that
secret scanners, link previews and clipboard watchers look for. It is still a
bearer token: whoever can read it and knows it is Glossia can redeem it.
