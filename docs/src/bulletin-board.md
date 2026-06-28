# Bulletin Board

A **Glossia bulletin** is an encrypted message published as natural-language
prose to a [nostr](https://nostr.com) identity you derive from a passphrase. The
site itself stays static: anyone opens a board by putting its `#npub` in the
URL, and the browser loads that board's bulletins straight from public relays.

It is the project's core idea applied to messaging — *machine data made
human-friendly*. A bulletin reads as prose, transcribes by voice, and verifies
by eye, while every payload word still carries its full entropy. Nothing is
hidden; the encryption is what protects the contents, not the encoding.

Try it at [`/bulletin.html`](https://glossia.io/bulletin.html).

## The model

A board has exactly one public locator and up to two secrets:

| Thing | Role | Who holds it |
|-------|------|--------------|
| **npub** | the board's public **address** (goes in the URL) | everyone — it's how you find and read a board |
| **publish key** | **write** access: signs and posts bulletins as this npub | the author(s) |
| **decrypt passphrase** | **read** access: decrypts the message contents | the intended readers |

With only the npub you can fetch a board and read its *prose*, but not decrypt
it. The secrets are what grant posting and reading.

### Single-key (read + write)

One passphrase is the whole credential. It is stretched two independent ways:

```
passphrase ──PBKDF2(salt = "glossia/nostr-identity/v1")──▶ secp256k1 key ──▶ npub   (identity)
passphrase ──PBKDF2(random per-message salt)─────────────▶ AES-256 key            (content)
```

The fixed identity salt makes the npub **deterministic** — the same passphrase
always lands on the same board, across sessions and devices, with no server or
stored state. The random content salt keeps every message's keystream unique.
Because the two derivations use different salts they are cryptographically
independent, even though they start from the same passphrase.

Anyone you give the passphrase to can both **post and read**. Good for a
personal scratchpad or a fully shared group board.

### Two-key (split roles)

The publish key and the decrypt passphrase are separate, which splits "who can
post" from "who can read":

- **Broadcast / newsletter.** Keep the publish key private; share only the
  decrypt passphrase. Subscribers read every bulletin but cannot post or
  impersonate the board.
- **Dead drop / inbox.** Share the publish key with senders; keep the decrypt
  passphrase. They post blindly, only you read.
- **Verifiable public feed.** Use no decrypt passphrase at all — the bulletins
  are plain readable prose, but every one is signed by the board's stable npub,
  so readers can confirm authorship.

The publish key can be a passphrase (deterministic npub), an existing `nsec`
you bring, or a freshly generated random key (save the `nsec` to post again).

## How a bulletin is built

The content pipeline is the same one behind the demo's
[encrypted-message panel](./usage.md): the message is compressed, optionally
encrypted with AES-256-CTR (PBKDF2-SHA-256, 200k iterations), and Glossia-encoded
into prose. That prose string is the body of a [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md)
nostr event:

```
kind:    1314            (an app-specific regular event — relays store every one,
                          append-only, and generic nostr clients ignore it)
content: "<key>: <prose>"  (encrypted)   or   "<prose>"   (signed but unencrypted)
tags:    [["client","glossia"], ["subject", "..."]]
sig:     schnorr (BIP-340) signature over the event id, by the publish key
```

The event is signed in the browser and pushed to several public relays. Reading
a board is a relay query for `{ authors: [pubkey], kinds: [1314] }`; each event's
signature is verified before its prose is shown, and the message is revealed only
if you supply the decrypt passphrase.

All crypto runs client-side. The schnorr/secp256k1 and SHA-256 primitives are the
audited [`@noble`](https://github.com/paulmillr/noble-curves) ESM builds, vendored
under `web/vendor/noble` so the page stays self-contained and offline-capable;
bech32 (`npub`/`nsec`) is implemented to BIP-173 and cross-checked against
`nostr-tools`.

## Security notes

- **A board is public.** Its npub, ciphertext-prose, post times, and subject
  tags are all visible to anyone. Encryption protects the *message*, not the
  fact that a board exists or how often it is posted to.
- **The passphrase is the only secret.** Because the npub is public, a weak
  passphrase is open to *offline* guessing — an attacker who grinds it recovers
  both the publish key (impersonate) and the decrypt key (read). The 200k-round
  PBKDF2 stretch raises the cost, and generated passphrases target ≥128 bits.
  Use a strong, generated passphrase.
- **AES-CTR is unauthenticated.** It gives confidentiality, not integrity: a
  wrong passphrase yields garbled output rather than a clean error, and a
  tampered ciphertext is not detected. (The nostr signature does authenticate
  the *event* — so you always know which npub posted it.)
- **Relays are untrusted infrastructure.** They can drop or withhold events and
  see all public metadata. Publishing to several relays adds redundancy; it adds
  no confidentiality.

Glossia remains **a readable encoding, not steganography**: the goal is to make
encrypted machine data human-friendly, not to conceal that it exists.
