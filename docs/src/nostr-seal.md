# Nostr Seal (ASCII Armor for Pubkeys)

The `seal_nostr` dialect wraps a bech32 public key inside ASCII armor so that nostr-mail emails have consistent, searchable markers in their plaintext part.

```
-----BEGIN NOSTR SEAL-----
npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
-----END NOSTR SEAL-----
```

## Why seals exist

Nostr-mail produces dual-format emails:

| Part | Format | Purpose |
|------|--------|---------|
| `text/plain` | ASCII-armored blocks | Machine-readable; searchable by Gmail IMAP |
| `text/html` | Glossia-encoded prose | Human-readable (Latin, English, etc.) |

The plaintext needs every block wrapped in armor so Gmail's `X-GM-RAW` search can find nostr-mail messages by searching for markers like `BEGIN NOSTR SEAL` or `BEGIN NOSTR SIGNATURE`.

Without the seal dialect, the pubkey would appear as a bare `npub1...` string with no searchable framing.

## Encoding

The caller strips the `npub1` prefix and passes only the bech32 data to glossia:

```
npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
      └──────────────── bech32 data (payload) ──────────────────┘
```

The `npub1` prefix is a cover word (`Prefix[npub]`) that the grammar injects automatically. The caller does not include it in the payload.

This follows the same convention as `crypto/nostr`, where `npub1` is always a cover prefix.

## Decoding

To decode the seal:

1. Strip the header (`-----BEGIN NOSTR SEAL-----\n`) and footer (`\n-----END NOSTR SEAL-----`)
2. Strip the `npub1` cover prefix from the remaining line
3. The rest is bech32 payload data
4. Prepend `npub1` to reconstruct the full npub

In pseudocode:

```python
lines = seal_block.strip().split("\n")
# lines[0] = "-----BEGIN NOSTR SEAL-----"
# lines[1] = "npub1<bech32data>"
# lines[2] = "-----END NOSTR SEAL-----"

npub_line = lines[1]
assert npub_line.startswith("npub1")
bech32_data = npub_line[len("npub1"):]

# Reconstruct
full_npub = "npub1" + bech32_data
```

Since the grammar uses `payload_line_width: null`, the npub is always on a single line (no line wrapping).

## Display name (optional)

The email layer may insert a display name line between the header and the npub:

```
-----BEGIN NOSTR SEAL-----
@alice
npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
-----END NOSTR SEAL-----
```

The `@alice` line is added by the nostr-mail client, not by glossia. It is presentation metadata for human readers. The decoder should skip any line that does not start with `npub1`.

## Dual-format email structure

A complete nostr-mail message uses three ASCII-armored blocks in the plaintext part:

```
-----BEGIN NOSTR SEAL-----
npub17umm7nnvf6y2dse2gwyklhq0p9daeqzn6edp523fzfd5utj2upcsm6zk5r
-----END NOSTR SEAL-----

-----BEGIN NOSTR ENCRYPTED MESSAGE-----
<NIP-44 ciphertext in base58, line-wrapped at 76 chars>
-----END NOSTR ENCRYPTED MESSAGE-----

-----BEGIN NOSTR SIGNATURE-----
c8ca1f5e59bc5e915a8044beff139359...
-----END NOSTR SIGNATURE-----
```

Each block maps to a glossia dialect:

| Block | Dialect | Payload alphabet | Line wrap |
|-------|---------|-----------------|-----------|
| Seal (pubkey) | `cs/seal_nostr` | bech32 (32 chars) | No |
| Encrypted message | `cs/nip44` | base58 (58 chars) | 76 chars |
| Signature | `cs/sig_nostr` | base16 / hex (16 chars) | 76 chars |

## Grammar structure

The `seal_nostr` dialect overrides the base CS grammar's sentence production to inject the `npub1` prefix between the header and the payload body:

```
sentence    -> HEADER SEAL_PREFIX BODY FOOTER
HEADER      -> Dot Aux[begin] Prefix[nostr] Modal[seal] Dot Conj
SEAL_PREFIX -> Prefix[npub]
BODY        -> N | N BODY
FOOTER      -> Conj Dot Aux[end] Prefix[nostr] Modal[seal] Dot
```

Token mapping:

| Token | Cover word | POS |
|-------|-----------|-----|
| `Dot` | `-----` | Dot |
| `Aux[begin]` | `BEGIN` | Aux |
| `Aux[end]` | `END` | Aux |
| `Prefix[nostr]` | `NOSTR` | Prefix |
| `Modal[seal]` | `SEAL` | Modal |
| `Prefix[npub]` | `npub1` | Prefix |
| `Conj` | `\n` | Conj |
| `N` | bech32 char | N (payload) |

All tokens except `N` are cover words. The minimum sequence length is 14 (6 header + 1 prefix + 1 body + 6 footer).

## Pipeline usage

The `seal` keyword is a dialect modifier in the meta sentence parser. Combined with the `nostr` language keyword, it selects the `cs/seal_nostr` dialect:

```
transcode(bech32Data, "encode into seal nostr")
```

Where `bech32Data` is the bech32 character data **after** the `npub1` prefix.

## Gmail IMAP search

The ASCII armor markers enable Gmail search via `X-GM-RAW`:

```
# Find all nostr-mail messages
X-GM-RAW "BEGIN NOSTR"

# Find messages with seals specifically
X-GM-RAW "BEGIN NOSTR SEAL"

# Find messages with signatures
X-GM-RAW "BEGIN NOSTR SIGNATURE"
```

This works because the armor lines appear verbatim in the `text/plain` part of the email, which Gmail indexes for full-text search.

## Files

| File | Purpose |
|------|---------|
| `languages/cs/grammar.yaml` | `seal_nostr` dialect definition |
| `languages/cs/cover.yaml` | SEAL, npub1, and other cover words |
| `languages/cs/payload_bech32.yaml` | 32-character bech32 payload alphabet |
| `src/pipeline.rs` | `"seal"` dialect modifier routing |
| `src/wasm.rs` | "Nostr Seal" display name |
