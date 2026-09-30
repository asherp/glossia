# Scanning: reading a paragraph with a camera

A QR code is machine data that a camera reads back. A Glossia paragraph is
machine data a *person* can read back, and the web app now reads it with a
camera too: photograph a printed or on-screen paragraph, and the page decodes
it exactly as if the text had been pasted.

Open any decode panel in `index.html` (API key, Bitcoin address, message) and
press **📷 Scan**. Choose a photo, drop or paste a screenshot, or open the
camera and capture a frame. The transcription appears with each word marked
as *read*, *corrected*, or *unsure*; fix anything unsure, then **Use this
text** hands it to the panel, which decodes and verifies it the usual way.

Everything runs in the browser. The image never leaves the device.

## Why this is easier than general OCR

Two properties of the codec do most of the work, and the reader leans on both.

**The vocabulary is closed.** Every word in a rendering is a payload word or a
cover word, and both lists are known (`get_payload_words`, `get_cover_words`).
A recognized token that is not in the vocabulary is a misread, and it can be
*snapped* to the nearest vocabulary word when that word is unambiguous. BIP39
words are unique in their first four letters, so most misreads resolve.

**The decoders verify.** A canonical rendering re-renders from what it decodes
to and compares the wording; a sealed message is authenticated; the address
panel re-renders and diffs. A wrong snap fails loudly one layer down rather
than producing the wrong bytes quietly. So the reader is allowed to be
confident, and its mistakes are caught by the same machinery that catches a
person's.

## The pipeline

```
photo / frame / screenshot
        |
        v
  prepareImage      scale to ~1400–2400 px, greyscale, contrast stretch,
        |           deskew (projection profile, ±8°)
        v
  Tesseract.js      LSTM recognizer in a web worker; words with confidence,
        |           bounding box and line
        v
  snapTokens        exact | snapped | unknown | junk, against the language's
        |           payload + cover vocabulary
        v
  scanText          the transcription, surface form preserved
        |
        v
  the panel         pasted as text; decodes and verifies as usual
```

The engine sits behind one interface, `recognize(worker, image)` returning
`[{ text, confidence, bbox, line }]`, so it can be replaced without touching
the snapping or the pages.

### Image preparation

Tesseract wants roughly 30 px of x-height and a wide luminance histogram, and
its layout analysis gives up past about two degrees of skew. Hand-held photos
are rarely straighter than that, so `prepareImage` straightens the frame
before recognition: it shears the ink by each candidate angle, sums it per
row, and keeps the angle whose row profile is spikiest. Measured on rendered
prose, a frame rotated by 5° that recognized zero words untreated recognizes
every word after this step.

Phone photos carry an EXIF orientation tag; the reader honours it, or portrait
shots would arrive a quarter turn off.

### Snapping rules

A token is replaced only when exactly one vocabulary word sits at the smallest
edit distance (Damerau–Levenshtein, transpositions count as one) and that
distance fits the budget for the token's length:

| letters | edits allowed |
|---|---|
| 1–3 | 0 (never snapped; *cat / can / car* are all one edit apart) |
| 4–5 | 1 |
| 6+ | 2 |

Digits and bars inside a word are mapped to the letters they stand in for
(`0→o`, `1→l`, `5→s`, `8→b`, `|→l`) before lookup, so `ab1e` reaches `able`
without spending an edit. A token the recognizer is unsure of (confidence
below 30) is never snapped, only kept. Ties are refused and the candidates are
shown on hover, so the person decides. Anything unresolved is kept *as read*,
which leaves a hole an aligner can see rather than a wrong word it cannot.

The transcription keeps the capitalization and trailing punctuation the
recognizer saw. Canonical verification compares wording exactly, so
`Insect why see victory.` must come back as that and not as
`insect why see victory`.

### The address panel

A camera does not read the opcode glyphs (`⓪`, `①`, `⧉`, `⌖`, `≡`, `∇`) that
frame a prose address. The scan dialog asks which script the address locks
and puts the glyphs back around the transcription; the panel's own checker
then verifies the result.

## Verdicts on the frame

Every recognized word is boxed over the frame, and the box says what the
decoder concluded about it. The canonical path makes that possible: a
canonical rendering re-renders from what it decodes to and aligns the
received text against that (`src/align.rs`), which is a per-token diff in
the decoder's own coordinates, and under v3 the decoder also reports which
payload positions parity corrected. Mapped onto the recognizer's bounding
boxes, each word falls into one class:

| class | drawn as | meaning |
|---|---|---|
| payload word | green | carries bytes, matched the rendering |
| cover word | faint outline | matched the rendering |
| cover word misread | amber, `→ expected` | wording differs, bytes unaffected |
| payload word corrected | orange, `→ expected` | misread, and Reed–Solomon parity repaired it |
| payload word wrong | red | misread and past repair, or a payload word the rendering never had |
| corrected to a vocabulary word | amber, dashed | snapping fixed it; no alignment to confirm |
| unsure | red, dotted, `?` | not resolved to any word; kept as read |

A word the rendering has and the frame does not is drawn as a bar where it
should have been, with `+ word` above it. The same classes colour the
transcription under the frame, and hovering a word shows the full note.

### When the plain decode fails

A payload word mangled *off* the wordlist never reaches the decoder's harvest,
so the plain decode comes up a word short and fails. The only trace of that
word is an unsure box. The reader therefore tries the unsure tokens as holes:
for each placement it hands the decoder the payload sequence with a `null`
there (`canonical_decode_slots`, the self-describing packing's erasure entry),
which turns an unlocated fault into an erasure that parity fills at half the
cost. With k words short and n unsure tokens there are C(n, k) placements; the
first whose checksum passes wins, and the search stops after twenty attempts.
When the page knows the rendering's word count (an address does), k is exact;
otherwise it runs one, two, three.

A wrong placement fails the checksum, so the search cannot invent a payload.
What it cannot resolve stays red.

### Live mode

Opening the camera starts a continuous read: each frame is straightened,
recognized, decoded and boxed over the live video, at roughly one to two
frames a second once the engine is warm. When a canonical payload verifies,
the view **locks** on that frame the way a QR reader locks, freezing the
straightened frame with its boxes and filling the transcription. A frame the
decoder had to repair locks once three frames running agree on the payload:
the checksum backs the bytes either way, but a few more reads give the camera
a chance at a clean one first. **Resume
live** starts again; **Capture** takes one frame on demand, which is the path
for prose that carries no checksum (the API key and message panels), where
the boxes show what snapping alone knows and the panel does the decoding.

Boxes are found on the prepared frame, so drawing them over the raw video
means undoing the preparation: rotate back through the skew that was removed,
then divide by the scale factor. `drawOverlay` takes both from `scanImage`.

**Use corrected text** appears when the decoder recovered a payload from a
damaged frame: it hands the panel the verified rendering rather than the
transcription, with every misread word already put right.

## Measured

Rendered canonical prose (English, 20-byte payloads), recognized in Node with
the same module the page uses; six payloads per condition:

| rendering | decoded | words | snapped | unknown | per frame |
|---|---|---|---|---|---|
| serif 26 px | 6/6 | 236 | 0 | 0 | 427 ms |
| sans 18 px | 6/6 | 241 | 0 | 0 | 360 ms |
| serif 22 px + noise | 6/6 | 239 | 0 | 0 | 526 ms |
| serif 24 px, rotated 3° | 6/6 | 237 | 0 | 0 | 420 ms |
| monospace 20 px | 6/6 | 238 | 0 | 0 | 456 ms |
| serif 14 px, narrow | 6/6 | 250 | 0 | 0 | 437 ms |
| serif 22 px, rotated −5° + noise | 5/6 | 242 | 1 | 1 | 500 ms |

The miss in the last row is the honest kind: a word such as `flat` read as
`tlat` is one edit from both `flat` and `that`, so the reader refuses to guess,
keeps it as read, and the decoder reports the damage instead of returning
wrong bytes. In the browser (Chromium, Playwright) a
2.5°-rotated render of a prose address decodes to the original address with
the panel's verdict at **verified**, and an API key round-trips.

Verdicts, in Chromium against renders of the page's own prose address,
rotated 1.5°: a misread cover word is boxed amber with the payload verified;
a dropped cover word is reported missing; a payload word swapped for another
payload word is corrected by parity; a payload word rendered unreadable is
found by the hole search on its second attempt, and two unreadable payload
words plus one unreadable cover word on its third. In every case **Use
corrected text** left the address panel at *verified*. Live mode, fed the
swapped-word render through a fake camera, locked after four seconds.

Recognition takes under a second per frame once the engine is loaded. The
first scan on a device downloads the engine and its language data (a few
megabytes) from the jsDelivr CDN; the language data is cached in IndexedDB
afterwards.

## Files

- `web/glossia-scan.js` — the module: engine loading, image preparation,
  snapping, transcription, verdict classification (`classifyTokens`,
  `annotateScan`) and the overlay (`drawOverlay`). The pure functions have no
  DOM dependency; the decoders are passed in, so the module never imports the
  WASM.
- `src/canonical.rs` — `canonical_decode_slots` / `canonical_decode_repaired`,
  the self-describing packing's erasure entries (WASM: `canonical_decode_slots`).
- `web/test_scan.mjs` — unit tests for normalization, snapping, surface form
  and skew estimation (`node --test web/test_scan.mjs`).
- `web/index.html` — the scan dialog and the hand-off into each panel.
- `web/sw.js` — the module is part of the offline shell; the engine's CDN
  assets are cross-origin and left to the browser cache.

## Choosing the engine

Tesseract.js was chosen over the pure-Rust `ocrs` because it is proven on
camera frames, ships language data for every language the pipelines render
(English, Latin, Czech, German), and can take a custom dictionary should
snapping after recognition ever prove insufficient (it has not so far, so the
recognizer runs with its stock dictionary). The
interface above is the seam: if a Rust engine inside the WASM build ever
earns its place, it replaces Tesseract behind `recognize` and nothing
downstream changes. Cloud OCR services were ruled out; a scanner that posts
keys and mnemonics to a third party is not one.
