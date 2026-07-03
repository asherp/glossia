# Bulletin Board

A **Glossia bulletin** is an encrypted message published as natural-language
prose to a [nostr](https://nostr.com) identity (a random key, or one derived from
a passphrase). The site itself stays static: anyone opens a board by putting its
`#npub` in the URL, and the browser loads that board's bulletins straight from
public relays.

It is the project's core idea applied to messaging — *machine data made
human-friendly*. A bulletin reads as prose, transcribes by voice, and verifies
by eye, while every payload word still carries its full entropy. Nothing is
hidden; the encryption is what protects the contents, not the encoding.

Read a board at [`/bulletin.html`](https://glossia.io/bulletin.html) (open a
shared `#npub` link and add a read key, nsec, or passphrase to decrypt); post one at
[`/compose.html`](https://glossia.io/compose.html).

## The model

A single signing key roots a three-tier capability hierarchy — so you never set
a separate encryption password:

```
signing key d ──schnorr──────────────▶ npub                  (publish + identity)
              ──SHA256("glossia/content-key/v1" ‖ d)──▶ read key K   (decrypt)
npub = d·G                                                    (locate + verify)
```

| Credential | Can publish? | Can decrypt? | Notes |
|-----------|:---:|:---:|-------|
| **nsec** (the signing key `d`) | ✅ | ✅ | can compute `K = H(d)` — full access |
| **read key** (`K`, shared as `nread1…`) | ❌ | ✅ | one-way hash → can't recover `d` → can't sign |
| **npub** (public address, in the URL) | ❌ | ❌ | reads the *prose* and verifies signatures only |

The content key is derived **one-way** from the signing key, with domain
separation. So:

- **Share the nsec** → co-authors can post *and* read.
- **Share the read key** → subscribers can read but not post or impersonate the
  board (broadcast / newsletter).
- **Share nothing** → anyone with the npub still reads the encrypted *prose* and
  can verify who signed it, but can't decrypt. (Publish unencrypted for a plain
  verifiable public feed.)

This subsumes the old single-key / two-key split into one root. Publishing leaks
neither `d` nor `K`: the schnorr signature and the AES-256-GCM ciphertext reveal
nothing about either.

### Where the signing key comes from

- **Random** — a fresh full-entropy key; save the `nsec` to post again.
- **Passphrase** — `d` is derived deterministically (`PBKDF2`, fixed domain salt)
  so the same passphrase always reaches the same board. Sharing the passphrase
  grants full access; the view page also accepts it to decrypt.
- **Bring your own `nsec`** — keep posting to an existing board.

## Saving and loading a board

The compose page's address bar already *is* the board (the author link carries the
`nwrite`), so bookmarking it is the quickest way to return. For a durable backup
that survives a lost bookmark, **Save board** renders the board's signing key as a
**Glossia seed phrase** — the private key itself, written as a readable
paragraph of prose:

```
Save board  ─▶  [signing key : 32][passLen : 2][passphrase][checksum : 4]
            ─▶  encode_raw_base_n  ─▶  natural-language paragraph  ("write this down")
Load board  ─▶  paste the paragraph  ─▶  decode + verify checksum  ─▶  board (+ passphrase) restored
```

This is the project's core idea applied to a private key: the seed phrase *is* the
key, made human-friendly — readable, speakable, transcribable. If a **custom
passphrase** is set, it is folded into the payload too, so the seed restores the
*whole* board — signing key and passphrase — and **Load board** fills the passphrase
back in. A 4-byte checksum (`sha256(payload)[..4]`) rides along, so a mistyped or
garbled word is caught on load instead of silently restoring a different board (the
same safeguard BIP39 mnemonics use). The phrase is rendered in whichever prose
language the board uses; **Load board** auto-detects the language and the checksum
confirms the decode. Everything is generated and encoded entirely in the browser —
it never leaves the device.

Anyone who holds the seed phrase has the signing key (and any embedded passphrase),
so it grants full access (post *and* decrypt): treat it exactly like the `nwrite`.

## How a bulletin is built

The message is compressed and encrypted with authenticated **AES-256-GCM**.
Per message, the AES key and nonce are derived from the board's content key `K`
and a fresh 6-byte random salt via **HKDF-SHA-256** (the content key is already
high-entropy, so no PBKDF2 stretching is needed — decryption is instant). The
result is Glossia-encoded into prose. An encrypted
bulletin reads as a **quote with an attribution**: the prose *is* the ciphertext,
and the em-dash trailer carries the plumbing — `[flag:2b | length:14b][salt:6][tag:12]`
(20 bytes) — rendered as ~11 Latin payload words, so it scans like a cited source:

```
"Ara belle arbustum. Obatratus emptor perrogatio…" — Gelu Synapium Cenit Tolerantia Parium Remex …
└──────────────── ciphertext ───────────────────┘   └──── salt + 96-bit GCM tag ─────────────────┘
```

The reduction flag rides in the top 2 bits of the length field, so no version
byte is needed — the em-dash alone signals the format, and it never appears in
encoded prose, so the two halves split cleanly. Latin's ~15 bits/word keeps the
trailer at 11 words. That artifact string is the body of a
[NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) event:

```
kind:    1314            (an app-specific regular event — relays store every one,
                          append-only, and generic nostr clients ignore it)
content: "<prose> — <attribution>"  (encrypted)   or   "<prose>"   (signed but unencrypted)
tags:    [["client","glossia"], ["subject", "..."]]
sig:     schnorr (BIP-340) signature over the event id, by the publish key
```

The event is signed in the browser and pushed to several public relays. Reading
a board is a relay query for `{ authors: [pubkey], kinds: [1314] }`; each event's
signature is verified before its prose is shown, and the message is revealed only
if you supply a decryption credential (read key / nsec / passphrase) — the GCM tag
then guarantees it decrypted to exactly what was published.

All crypto runs client-side. The schnorr/secp256k1 and SHA-256 primitives are the
audited [`@noble`](https://github.com/paulmillr/noble-curves) ESM builds, vendored
under `web/vendor/noble` so the page stays self-contained and offline-capable;
bech32 (`npub` / `nsec` / `nread`) is implemented to BIP-173 and cross-checked
against `nostr-tools`.

## Security notes

- **A board is public.** Its npub, ciphertext-prose, post times, and subject
  tags are all visible to anyone. Encryption protects the *message*, not the
  fact that a board exists or how often it is posted to.
- **Random boards have full-entropy keys.** The signing key (and thus the read
  key) is random, so the content key is never guessable — there is no password to
  brute-force. **Passphrase-derived boards** are the exception: the npub is public,
  so a weak passphrase is open to *offline* guessing (recovering it yields both the
  signing key and the read key). The 200k-round PBKDF2 stretch raises the cost; use
  a strong, generated passphrase, or just use a random board.
- **The read key only grants reading.** It is a one-way hash of the signing key,
  so sharing it for read access can never leak the ability to post. Revoking a
  reader, though, means rotating to a new board (the key is static per board).
- **Encryption is authenticated.** AES-256-GCM gives confidentiality *and*
  integrity: a wrong key/passphrase or any tampering with the prose or the
  attribution fails cleanly instead of yielding garbage. The nostr signature
  independently authenticates the *event*, so you also always know which npub
  posted it.
- **Relays are untrusted infrastructure.** They can drop or withhold events and
  see all public metadata. Publishing to several relays adds redundancy; it adds
  no confidentiality.

Glossia remains **a readable encoding, not steganography**: the goal is to make
encrypted machine data human-friendly, not to conceal that it exists.
