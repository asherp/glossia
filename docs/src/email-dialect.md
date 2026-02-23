# Email Dialect

The email dialect (`cs/email`) models the anatomy of an RFC 5322 / MIME email message as a context-free grammar. Every part of the email is classified as either **cover** (structural) or **payload** (content) — nothing is ignored.

This dialect belongs under `languages/cs/` because CS already models structured formats (ASCII armor for PGP, NIP-04, signatures). The email dialect adds email-specific structure following the same pattern: cover words for framing tokens, N slots for content zones.

## Quick Start

```bash
# Encode data into a simple text/plain email
echo "attack at dawn" | glossia --into cs/base64/email

# Encode into multipart/alternative (text + html, like Gmail)
echo "attack at dawn" | glossia --into cs/base64/email_alt

# Encode into multipart/mixed (with attachment)
echo "attack at dawn" | glossia --into cs/base64/email_mime
```

Example output for `cs/email`:

```
From: <sender@glossia.local>
To: <recipient@glossia.local>
Date: Thu, 01 Jan 2026 00:00:00 +0000
Subject: qt2foq
Content-Type: text/plain; charset="UTF-8"

zp8tk5cwh4yrnx
```

The base64 payload characters (`qt2foq...`) carry the encoded data. Everything else — header labels, addresses, dates, MIME declarations — is cover.

## Email Anatomy

Every part of an email maps to exactly one classification:

```
Return-Path: <alice@example.com>               COVER  (server-added envelope)
Received: from mail.example.com ...            COVER  (transport trace)
Date: Sun, 22 Feb 2026 19:40:58 -0600         COVER  (stable envelope header)
From: Alice <alice@example.com>                COVER  (stable envelope header)
To: Bob <bob@gmail.com>                        COVER  (stable envelope header)
Message-ID: <...@mail.example.com>             COVER  (server-generated)
Subject: Test message                          PAYLOAD ZONE 1
MIME-Version: 1.0                              COVER  (MIME infrastructure)
Content-Type: multipart/alternative; ...       COVER  (MIME container)
                                               COVER  (blank line separator)
--boundary                                     COVER  (MIME boundary)
Content-Type: text/plain; charset="UTF-8"      COVER  (part content-type)
                                               COVER  (blank line separator)
Hello Bob, ...                                 PAYLOAD ZONE 2

--boundary                                     COVER  (next MIME part)
Content-Type: text/html; charset="UTF-8"       COVER  (HTML part header)
<html>...</html>                               COVER  (structural duplicate)

--boundary--                                   COVER  (closing boundary)
```

**Classification principle**: Server-mangled parts (Received, Return-Path, Message-ID, HTML duplicates) are cover — they're structural artifacts of the transport layer, not content the user created.

## Dialects

Four dialect variants model increasing email complexity:

| Dialect | Structure | Use Case |
|---------|-----------|----------|
| `email` | Envelope + Subject + text/plain body | Simple messages |
| `email_alt` | + multipart/alternative (text + html) | Gmail-style messages |
| `email_mime` | + multipart/mixed (text + attachment) | Messages with attachments |
| `email_raw` | Body only, no structure | Testing / raw content |

### `email` — Simple text/plain

```
sentence -> ENVELOPE_HEADERS SUBJECT_LINE PLAIN_PREAMBLE BODY
```

```
From: <sender@glossia.local>
To: <recipient@glossia.local>
Date: Thu, 01 Jan 2026 00:00:00 +0000
Subject: <payload zone 1>
Content-Type: text/plain; charset="UTF-8"

<payload zone 2>
```

### `email_alt` — Multipart/alternative

```
sentence -> ENVELOPE_HEADERS SUBJECT_LINE MIME_ALT_PREAMBLE
            MIME_TEXT_PART MIME_HTML_PART MIME_CLOSE
```

```
From: <sender@glossia.local>
To: <recipient@glossia.local>
Date: Thu, 01 Jan 2026 00:00:00 +0000
Subject: <payload>
MIME-Version: 1.0
Content-Type: multipart/alternative; boundary="glossia-alt"

--glossia-alt
Content-Type: text/plain; charset="UTF-8"

<payload>

--glossia-alt
Content-Type: text/html; charset="UTF-8"

<html><body></body></html>

--glossia-alt--
```

The HTML part is entirely cover — it's a structural duplicate of the text/plain payload.

### `email_mime` — Multipart/mixed with attachment

```
sentence -> ENVELOPE_HEADERS SUBJECT_LINE MIME_PREAMBLE
            MIME_TEXT_PART MIME_ATTACH MIME_CLOSE
```

Both the text/plain body and the attachment body carry payload. The attachment is wrapped with Content-Transfer-Encoding and Content-Disposition headers (all cover).

## Grammar Design

The grammar uses the same POS-to-type mapping as all CS dialects, with structural passthrough lambdas (`λn:(e->t). n`). Email-specific non-terminal rules live in the `email` dialect's rules section and are inherited by child dialects via `parent: email`.

### POS tag assignments

| Email element | POS | Refinement | Example |
|---------------|-----|-----------|---------|
| Header labels | `Aux` | `from`, `to`, `subject`, etc. | `From:`, `Subject:` |
| Content-Type lines | `Prefix` | `text-plain`, `multipart-alt`, etc. | `Content-Type: text/plain; charset="UTF-8"` |
| CTE / HTML body | `Cop` | `cte-base64`, `html-body`, etc. | `Content-Transfer-Encoding: base64` |
| MIME infrastructure | `Modal` | `mime-version`, `disp-attach` | `MIME-Version: 1.0` |
| Header values | `To` | `from-val`, `to-val`, `date-val` | `<sender@glossia.local>` |
| MIME boundaries | `Dot` | `boundary`, `boundary-end`, etc. | `--glossia`, `--glossia--` |
| Line separator | `Conj` | — | `\n` |
| Payload characters | `N` | — | `a`, `B`, `7`, `/`, `+` |

Header field values use the `To` POS (a forced-cover slot in the generator) because they are always cover — addresses, dates, and trace lines are structural, not payload.

### Envelope headers

The grammar generates envelope headers with two weighted productions:

- **70%**: Minimal headers (From, To, Date) — what a sent message looks like
- **30%**: Full trace (Return-Path, Received, Date, From, To, Message-ID) — what a received message looks like

All header values are placeholders (`<sender@glossia.local>`, etc.) that application-layer tools replace with real values.

## Cover Wordlist

The email cover wordlist (`cover_email.yaml`) contains 48 tokens across 7 POS categories. Every token contains characters outside the base64 alphabet (`:`, `-`, space, `"`, `;`, `.`, `\n`) so the CS decoder can distinguish cover from payload.

Token categories:

- **11 header labels** (Aux): `From:`, `To:`, `Subject:`, `Date:`, `Received:`, `Return-Path:`, `Message-ID:`, etc.
- **20 MIME type declarations** (Prefix): text/plain, text/html, multipart/alternative, application/pdf, image/png, etc.
- **5 CTE + HTML** (Cop): quoted-printable, base64, 7bit, 8bit, HTML body placeholder
- **3 MIME infrastructure** (Modal): MIME-Version, Content-Disposition attachment/inline
- **6 header values** (To): address placeholders, date placeholder, trace placeholder
- **6 MIME boundaries** (Dot): `--glossia`, `--glossia--`, `--glossia-alt`, etc.
- **1 line separator** (Conj): `\n`

## Integration with nostr-mail

The email dialect is designed as a **reference grammar** for tools that generate or parse email structure. A client like nostr-mail would use it at two levels:

### 1. Structure generation (grammar layer)

Use Glossia to encode data into email structure:

```bash
echo "$ENCRYPTED_PAYLOAD" | glossia --into cs/base64/email_alt --seed $SEED
```

This produces a structurally valid email with placeholder header values and the payload distributed across the subject line and body.

### 2. Header replacement (application layer)

Replace placeholder values with real ones via string substitution:

| Placeholder | Replace with |
|-------------|-------------|
| `<sender@glossia.local>` | Sender's email/npub |
| `<recipient@glossia.local>` | Recipient's email/npub |
| `Thu, 01 Jan 2026 00:00:00 +0000` | Actual send timestamp |
| `<msgid@glossia.local>` | Generated Message-ID |
| `from glossia (localhost [127.0.0.1])` | Actual Received trace |

The grammar guarantees these placeholders appear exactly once per header field (via refinement tags like `To[from-val]`, `To[date-val]`), so simple string replacement is safe.

### 3. Content composition (pipeline layer)

For richer content, compose the email container with a content encoding:

1. Encode a mnemonic into English prose: `glossia --into english`
2. Encode a short summary: `glossia --into english/default/subject`
3. Use the email grammar as a template, inserting the prose into the body zone and the summary into the subject zone

This composition happens at the application layer — the email grammar describes *where* content zones go, not *what* goes in them.

## Relationship to English Subject/Body

The email dialect and English subject/body dialects operate at different layers:

| | English `subject` / `body` | CS `email` |
|---|---|---|
| **Layer** | Content (natural language prose) | Container (RFC 5322 structure) |
| **Alphabet** | BIP39 words (2048, space-delimited) | Base64 chars (64, concatenated) |
| **Output** | `"Re: snake robot mixed ship"` | Full email with headers and MIME |
| **Payload slots** | Grammar slots (N, V, Adj...) | Raw N slots in BODY rule |

They compose naturally: English generates the content, email provides the container.

## Files

| File | Purpose |
|------|---------|
| `languages/cs/grammar.yaml` | Grammar rules and dialect definitions (email section) |
| `languages/cs/cover_email.yaml` | Email structural tokens (48 cover words) |
| `languages/cs/payload_base64.yaml` | Base64 character payload alphabet |
| `languages/cs/email_demo.py` | Python demo script |
| `src/pipeline.rs` | Pipeline routing (`"email"` keyword) |
