// glossia-scan.js — photograph → Glossia text, as an ES module.
//
// The camera half of the codec: what a QR reader is to a QR code, this is to a
// printed Glossia paragraph. It runs OCR on an image and turns the result into
// text the existing decoders accept — the same text a person would produce by
// transcribing the page — so nothing downstream knows or cares that a camera
// was involved.
//
// Two things make this easier than general OCR, and the module leans on both:
//
//   1. The vocabulary is CLOSED. Every word in a Glossia rendering is either a
//      payload word or a cover word, and both lists are known (get_payload_words
//      / get_cover_words in the WASM). An OCR token that is not in the vocabulary
//      is a misread, and can be SNAPPED to the nearest vocabulary word when that
//      word is unambiguous. BIP39 words are unique in their first four letters,
//      so most misreads resolve.
//   2. The decoders VERIFY. A canonical rendering re-renders from what it decodes
//      to and compares; a seed phrase carries a checksum; a sealed message is
//      authenticated. A wrong snap fails loudly there rather than quietly
//      producing the wrong bytes. So snapping is allowed to be confident, and
//      its mistakes are caught one layer down.
//
// Snapping is deliberately conservative where it could corrupt: a token is only
// replaced when exactly one vocabulary word sits at the smallest edit distance
// and that distance is small for the token's length. Short tokens never snap
// (cat / can / car are all one edit apart); anything the module cannot resolve is
// kept as read, so an aligner can still see a hole where a word should be.
//
// The OCR engine is behind one interface (recognize → [{ text, confidence, bbox,
// line }]) so it can be swapped without touching the snapping or the pages. The
// engine is Tesseract.js, loaded on demand from the jsDelivr CDN the first time
// a scan runs — its worker, core and language data are several megabytes, and
// most visits never scan. The engine caches language data in IndexedDB, so a
// second scan on the same device does not download it again.
//
// Pure functions (normalizeToken, buildVocab, snapToken, snapTokens, surfaceForm, scanText)
// have no DOM or engine dependency and are exercised by web/test_scan.mjs.

// ─── engine ──────────────────────────────────────────────────────────

/// Pinned so the page, the worker and the core agree; bump together.
export const TESSERACT_VERSION = '5.1.1';
const TESSERACT_BASE = `https://cdn.jsdelivr.net/npm/tesseract.js@${TESSERACT_VERSION}/dist`;

/// Tesseract language codes for the languages the pipelines render into.
export const OCR_LANGS = {
  english: 'eng',
  latin: 'lat',
  czech: 'ces',
  german: 'deu',
};

let enginePromise = null;   // the loaded Tesseract module (one per page)
const workers = new Map();  // ocr language -> worker promise (one per language)

/// Load the Tesseract.js module. `loader` is injectable so Node tests and a
/// vendored copy can supply their own import.
export function loadEngine(loader) {
  if (!enginePromise) {
    enginePromise = (loader || (() => import(`${TESSERACT_BASE}/tesseract.esm.min.js`)))()
      // The ESM bundle carries the API on its default export; a namespace with
      // named exports (Node, a vendored build) is used as is.
      .then((m) => (m && m.default && m.default.createWorker ? m.default : m))
      .catch((e) => { enginePromise = null; throw e; });
  }
  return enginePromise;
}

/// One worker per OCR language, initialized once and kept for later scans (the
/// language data is the expensive part). `options` are Tesseract worker options
/// (paths, logger); `onProgress(status, fraction)` reports loading and
/// recognition progress.
export async function getWorker(ocrLang = 'eng', { onProgress, options = {}, loader, timeoutMs = 180000 } = {}) {
  if (!workers.has(ocrLang)) {
    const p = (async () => {
      const T = await loadEngine(loader);
      // In a browser the worker script comes from the same pinned CDN build as
      // the module; under Node the package's own worker path stands.
      const browser = typeof document !== 'undefined';
      // A worker that dies while loading (a blocked download, an unsupported
      // browser) reports through errorHandler rather than the job promise, and
      // a download that stalls reports through nothing at all — so both are
      // turned into a rejection here, or the page would wait forever.
      let failed;
      const failure = new Promise((_, reject) => { failed = reject; });
      const timer = setTimeout(() => failed(new Error(`the reader did not load within ${Math.round(timeoutMs / 1000)} s`)), timeoutMs);
      try {
        const worker = await Promise.race([
          T.createWorker(ocrLang, T.OEM.LSTM_ONLY, {
            ...(browser ? { workerPath: `${TESSERACT_BASE}/worker.min.js` } : {}),
            logger: (m) => { if (onProgress && m && typeof m.progress === 'number') onProgress(m.status, m.progress); },
            errorHandler: (e) => failed(e instanceof Error ? e : new Error(String(e && e.message || e))),
            ...options,
          }),
          failure,
        ]);
        return worker;
      } finally {
        clearTimeout(timer);
      }
    })().then(async (worker) => {
      // AUTO page segmentation finds the paragraph wherever it sits in the frame;
      // a canvas has no DPI metadata, so tell the engine the image is print-sized
      // or it guesses and warns.
      await worker.setParameters({
        tessedit_pageseg_mode: '3',          // PSM.AUTO
        user_defined_dpi: '300',
        preserve_interword_spaces: '1',
      });
      return worker;
    }).catch((e) => { workers.delete(ocrLang); throw e; });
    workers.set(ocrLang, p);
  }
  return workers.get(ocrLang);
}

/// Release every worker (tests; a page that wants the memory back).
export async function terminateWorkers() {
  const all = [...workers.values()];
  workers.clear();
  await Promise.all(all.map((p) => p.then((w) => w.terminate()).catch(() => {})));
}

/// Run the engine and flatten its output to the one shape the rest of the
/// module reads: words in reading order, each with its confidence (0–100), its
/// box, and the index of the line it sits on.
export async function recognize(worker, image) {
  const r = await worker.recognize(image, {}, { text: true, blocks: true, hocr: false, tsv: false });
  const words = [];
  let line = 0;
  for (const b of r.data.blocks || []) {
    for (const p of b.paragraphs || []) {
      for (const l of p.lines || []) {
        for (const w of l.words || []) {
          words.push({ text: w.text, confidence: w.confidence, bbox: w.bbox, line });
        }
        line++;
      }
    }
  }
  return { text: r.data.text || '', words, confidence: r.data.confidence };
}

// ─── image preparation ───────────────────────────────────────────────

/// Estimate the skew of the text in a greyscale frame, in degrees. Positive
/// means the lines run downhill to the right (the page was rotated clockwise),
/// and rotating the frame by the NEGATIVE of the result straightens it.
///
/// Projection profile: shear the ink by a candidate angle and sum it per row.
/// When the candidate matches the true skew, every line of text lands in a few
/// rows and the row sums are spiky; off by a degree, the lines smear across
/// rows and the sums flatten. The spikiest profile (largest variance) wins.
/// Tesseract's layout analysis gives up past about two degrees of skew, so this
/// is what lets a hand-held photo be read at all.
export function estimateSkew(lum, w, h, { maxDeg = 8, stepDeg = 0.25, sample = 480 } = {}) {
  const s = Math.max(1, Math.round(Math.max(w, h) / sample));
  const dw = Math.floor(w / s), dh = Math.floor(h / s);
  if (dw < 8 || dh < 8) return 0;
  // Otsu threshold on the downsampled histogram separates ink from paper
  // without assuming which is darker than what.
  const hist = new Uint32Array(256);
  const small = new Uint8Array(dw * dh);
  for (let y = 0; y < dh; y++) for (let x = 0; x < dw; x++) {
    const v = lum[(y * s) * w + x * s];
    small[y * dw + x] = v; hist[v]++;
  }
  const total = dw * dh;
  let sumAll = 0;
  for (let v = 0; v < 256; v++) sumAll += v * hist[v];
  let wB = 0, sumB = 0, best = -1, thr = 128;
  for (let v = 0; v < 256; v++) {
    wB += hist[v]; if (!wB) continue;
    const wF = total - wB; if (!wF) break;
    sumB += v * hist[v];
    const mB = sumB / wB, mF = (sumAll - sumB) / wF;
    const between = wB * wF * (mB - mF) * (mB - mF);
    if (between > best) { best = between; thr = v; }
  }
  const ink = [];
  for (let y = 0; y < dh; y++) for (let x = 0; x < dw; x++) if (small[y * dw + x] <= thr) ink.push(x, y);
  if (ink.length < 64) return 0;
  const rows = new Float64Array(dh + 2 * Math.ceil(dw * Math.tan(maxDeg * Math.PI / 180)) + 2);
  const off = Math.ceil(dw * Math.tan(maxDeg * Math.PI / 180)) + 1;
  let bestDeg = 0, bestVar = -1;
  for (let deg = -maxDeg; deg <= maxDeg + 1e-9; deg += stepDeg) {
    const t = Math.tan(deg * Math.PI / 180);
    rows.fill(0);
    for (let i = 0; i < ink.length; i += 2) rows[Math.round(ink[i + 1] - ink[i] * t) + off]++;
    let mean = 0;
    for (let r = 0; r < rows.length; r++) mean += rows[r];
    mean /= rows.length;
    let v = 0;
    for (let r = 0; r < rows.length; r++) { const d = rows[r] - mean; v += d * d; }
    if (v > bestVar) { bestVar = v; bestDeg = deg; }
  }
  return Math.round(bestDeg * 100) / 100;
}

function domCanvas(w, h) {
  const c = document.createElement('canvas');
  c.width = w; c.height = h;
  return c;
}

/// Draw an image source (an <img>, <video>, canvas, ImageBitmap, or a File/Blob)
/// onto a fresh canvas sized for the recognizer: greyscale, contrast stretched,
/// and straightened. Tesseract wants roughly 30 px of x-height; a phone photo of
/// a paragraph is usually far larger than needed and a screenshot far smaller,
/// so the frame is scaled into [minSide, maxSide] on its longer edge.
///
/// `createCanvas(w, h)` is injectable so Node (node-canvas) can run the same
/// pipeline the browser does. Returns { canvas, skew, scale } — the skew in
/// degrees that was removed and the factor the frame was scaled by, which is
/// what maps a box on the prepared frame back onto the source.
export async function prepareImage(source, { minSide = 1400, maxSide = 2400, contrast = true, deskew = true, createCanvas = domCanvas } = {}) {
  let bitmap = source;
  // A phone stores a portrait photo sideways plus an EXIF orientation tag;
  // honour the tag or the text arrives rotated a quarter turn.
  if (typeof Blob !== 'undefined' && source instanceof Blob) bitmap = await createImageBitmap(source, { imageOrientation: 'from-image' });
  const sw = bitmap.videoWidth || bitmap.naturalWidth || bitmap.width;
  const sh = bitmap.videoHeight || bitmap.naturalHeight || bitmap.height;
  if (!sw || !sh) throw new Error('image has no size yet');
  const longest = Math.max(sw, sh);
  const scale = longest < minSide ? minSide / longest : longest > maxSide ? maxSide / longest : 1;
  const w = Math.round(sw * scale), h = Math.round(sh * scale);
  let canvas = createCanvas(w, h);
  let g = canvas.getContext('2d', { willReadFrequently: true });
  g.imageSmoothingEnabled = true;
  g.imageSmoothingQuality = 'high';
  g.drawImage(bitmap, 0, 0, w, h);

  const img = g.getImageData(0, 0, w, h);
  const d = img.data;
  // Greyscale, then stretch the 1st–99th percentile of luminance to full range:
  // a photo of a page is grey-on-grey, and the recognizer's own binarization
  // does better on a wide histogram.
  const hist = new Uint32Array(256);
  const lum = new Uint8ClampedArray(w * h);
  for (let i = 0, j = 0; i < d.length; i += 4, j++) {
    const y = (d[i] * 299 + d[i + 1] * 587 + d[i + 2] * 114) / 1000;
    lum[j] = y; hist[y | 0]++;
  }
  const total = w * h;
  let lo = 0, hi = 255, acc = 0;
  for (let v = 0; v < 256; v++) { acc += hist[v]; if (acc > total * 0.01) { lo = v; break; } }
  acc = 0;
  for (let v = 255; v >= 0; v--) { acc += hist[v]; if (acc > total * 0.01) { hi = v; break; } }
  const span = Math.max(hi - lo, 1);
  for (let j = 0; j < lum.length; j++) {
    const y = ((lum[j] - lo) * 255) / span;
    lum[j] = y;                                   // clamped by the typed array
  }
  if (contrast) {
    for (let i = 0, j = 0; i < d.length; i += 4, j++) d[i] = d[i + 1] = d[i + 2] = lum[j];
    g.putImageData(img, 0, 0);
  }

  let skew = 0;
  if (deskew) {
    skew = estimateSkew(lum, w, h);
    if (Math.abs(skew) >= 0.3) {
      // Redraw straightened, on paper-coloured ground so the uncovered corners
      // do not read as ink.
      const out = createCanvas(w, h);
      const og = out.getContext('2d', { willReadFrequently: true });
      og.fillStyle = contrast ? '#fff' : `rgb(${hi},${hi},${hi})`;
      og.fillRect(0, 0, w, h);
      og.translate(w / 2, h / 2);
      og.rotate(-skew * Math.PI / 180);
      og.translate(-w / 2, -h / 2);
      og.drawImage(canvas, 0, 0);
      canvas = out;
    }
  }
  return { canvas, skew, scale };
}

// ─── vocabulary snapping ─────────────────────────────────────────────

/// Substitutions the recognizer makes inside a word that a human would not:
/// digits and bars where letters belong. Applied before lookup, so "abl3" and
/// "ab1e" both reach "able" without spending an edit.
const OCR_CONFUSIONS = { 0: 'o', 1: 'l', 5: 's', 8: 'b', '|': 'l', '!': 'l' };

/// Lower-case letters only. Leading and trailing punctuation is a word
/// boundary, not part of the word; inner confusable glyphs are mapped to the
/// letters they stand in for; anything else non-alphabetic is dropped.
export function normalizeToken(raw) {
  if (!raw) return '';
  let s = raw.normalize('NFC').toLowerCase();
  s = s.replace(/^[^\p{L}\p{N}|!]+|[^\p{L}\p{N}]+$/gu, '');
  let out = '';
  for (const ch of s) {
    if (/\p{L}/u.test(ch)) out += ch;
    else if (ch in OCR_CONFUSIONS) out += OCR_CONFUSIONS[ch];
  }
  return out;
}

/// Index a word list for snapping: the set for exact lookup, and words bucketed
/// by length so a candidate scan only touches words within the edit budget.
/// `payloadWords`, when given, lets a token say whether it carries bytes.
export function buildVocab(words, payloadWords = null) {
  const set = new Set();
  const byLength = new Map();
  for (const w of words) {
    const n = w.normalize('NFC').toLowerCase();
    if (!n || set.has(n)) continue;
    set.add(n);
    if (!byLength.has(n.length)) byLength.set(n.length, []);
    byLength.get(n.length).push(n);
  }
  const payload = payloadWords ? new Set(payloadWords.map((w) => w.normalize('NFC').toLowerCase())) : null;
  return { set, byLength, payload, size: set.size };
}

function withPayload(vocab, r) {
  r.isPayload = r.word && vocab.payload ? vocab.payload.has(r.word) : null;
  return r;
}

/// Restricted Damerau–Levenshtein (adjacent transposition counts as one edit —
/// a common recognizer slip), with early exit once the distance exceeds `max`.
export function editDistance(a, b, max = Infinity) {
  if (a === b) return 0;
  const n = a.length, m = b.length;
  if (Math.abs(n - m) > max) return max + 1;
  if (!n) return m;
  if (!m) return n;
  let prev2 = null;
  let prev = new Uint16Array(m + 1);
  let cur = new Uint16Array(m + 1);
  for (let j = 0; j <= m; j++) prev[j] = j;
  for (let i = 1; i <= n; i++) {
    cur[0] = i;
    let rowMin = i;
    for (let j = 1; j <= m; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      let v = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost);
      if (prev2 && i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1])
        v = Math.min(v, prev2[j - 2] + 1);
      cur[j] = v;
      if (v < rowMin) rowMin = v;
    }
    if (rowMin > max) return max + 1;
    [prev2, prev, cur] = [prev, cur, prev2 || new Uint16Array(m + 1)];
  }
  return prev[m];
}

/// Edit budget for a token of this length. Three letters or fewer never snap —
/// the vocabulary is dense there and one edit reaches several words. Four and
/// five letters allow one edit; longer words allow two.
export function snapBudget(len) {
  if (len <= 3) return 0;
  if (len <= 5) return 1;
  return 2;
}

export const DEFAULT_MIN_CONFIDENCE = 30;

/// Resolve one recognized token against the vocabulary.
///
/// Returns { raw, norm, word, status, distance, candidates } where status is
///   'exact'   — in the vocabulary as read
///   'snapped' — replaced by the unique nearest vocabulary word
///   'unknown' — a word the module will not guess at; kept as read
///   'junk'    — not a word at all (a stray mark, a lone punctuation glyph)
/// `candidates` lists the words tied at the best distance when the snap was
/// refused for ambiguity, so a page can offer them.
export function snapToken(raw, vocab, { confidence = 100, minConfidence = DEFAULT_MIN_CONFIDENCE } = {}) {
  const norm = normalizeToken(raw);
  if (vocab.set.has(norm)) return withPayload(vocab, { raw, norm, word: norm, status: 'exact', distance: 0, candidates: [] });
  // A single letter that is not itself a word ("a" is one) is a stray mark.
  if (norm.length < 2) return { raw, norm, word: null, status: 'junk', distance: null, candidates: [], isPayload: null };
  // Below the confidence floor the recognizer is guessing; so would we.
  if (confidence < minConfidence) return { raw, norm, word: null, status: 'unknown', distance: null, candidates: [], isPayload: null };
  const budget = snapBudget(norm.length);
  if (budget === 0) return { raw, norm, word: null, status: 'unknown', distance: null, candidates: [], isPayload: null };
  let best = budget + 1;
  let tied = [];
  for (let len = norm.length - budget; len <= norm.length + budget; len++) {
    const bucket = vocab.byLength.get(len);
    if (!bucket) continue;
    for (const w of bucket) {
      const d = editDistance(norm, w, best);
      if (d < best) { best = d; tied = [w]; }
      else if (d === best && d <= budget) tied.push(w);
    }
  }
  if (best > budget) return { raw, norm, word: null, status: 'unknown', distance: null, candidates: [], isPayload: null };
  if (tied.length === 1) return withPayload(vocab, { raw, norm, word: tied[0], status: 'snapped', distance: best, candidates: [] });
  return { raw, norm, word: null, status: 'unknown', distance: best, candidates: tied.sort(), isPayload: null };
}

/// Snap every recognized word. Tokens keep their `line` so the text can be
/// re-flowed the way the page was laid out, and their `bbox` so verdicts can
/// be drawn where the word sits.
export function snapTokens(words, vocab, opts = {}) {
  return words.map((w) => ({ ...snapToken(w.text, vocab, { ...opts, confidence: w.confidence }), line: w.line, confidence: w.confidence, bbox: w.bbox || null }));
}

/// A token as it should be written down: the resolved word wearing the
/// capitalization and trailing punctuation the recognizer saw. A canonical
/// rendering verifies by comparing wording exactly, so "Insect why see
/// victory." must come back as that and not as "insect why see victory".
export function surfaceForm(t) {
  const base = t.word || t.norm || t.raw;
  if (!base) return '';
  const raw = t.raw || '';
  const firstLetter = raw.match(/\p{L}/u);
  const capital = firstLetter && firstLetter[0] !== firstLetter[0].toLowerCase();
  const trail = raw.replace(/[”"'’)\]]+$/u, '').match(/[.,;:!?…]+$/);
  return (capital ? base[0].toUpperCase() + base.slice(1) : base) + (trail ? trail[0] : '');
}

/// The transcription: what a careful person would have typed from the page.
/// Vocabulary words as resolved; unknown words as read (a hole an aligner can
/// see); junk dropped; one line per recognized line.
export function scanText(tokens) {
  const lines = [];
  for (const t of tokens) {
    if (t.status === 'junk') continue;
    const li = t.line || 0;
    while (lines.length <= li) lines.push([]);
    lines[li].push(surfaceForm(t));
  }
  return lines.filter((l) => l.length).map((l) => l.join(' ')).join('\n');
}

/// Counts a page can show beside the text.
export function scanSummary(tokens) {
  const n = { exact: 0, snapped: 0, unknown: 0, junk: 0 };
  for (const t of tokens) n[t.status]++;
  return { ...n, words: n.exact + n.snapped + n.unknown };
}

// ─── the whole thing ─────────────────────────────────────────────────

/// Photograph (or any image source) → transcription.
///
///   vocab      — buildVocab(payload words + cover words) for the language
///   ocrLang    — Tesseract language ('eng' | 'lat' | 'ces' | 'deu'), see OCR_LANGS
///   onProgress — (status, fraction) while the engine loads and recognizes
///   prepare    — false to hand the source to the engine untouched
///
/// Returns { text, tokens, summary, raw, canvas, skew, scale }.
export async function scanImage(source, { vocab, ocrLang = 'eng', onProgress, prepare = true, prepareOptions, minConfidence, workerOptions, loader } = {}) {
  const worker = await getWorker(ocrLang, { onProgress, options: workerOptions, loader });
  const prepared = prepare ? await prepareImage(source, prepareOptions) : { canvas: null, skew: 0, scale: 1 };
  if (onProgress) onProgress('recognizing text', 0);
  const raw = await recognize(worker, prepared.canvas || source);
  const tokens = snapTokens(raw.words, vocab, { minConfidence });
  return { text: scanText(tokens), tokens, summary: scanSummary(tokens), raw, canvas: prepared.canvas, skew: prepared.skew, scale: prepared.scale };
}

// ─── verdicts: what the canonical decoder says about each box ─────────
//
// A canonical rendering verifies by re-rendering from what it decodes to and
// aligning the received text against that. The alignment is a per-token diff
// in the decoder's own coordinates (see src/align.rs), so every recognized
// word can be told apart as a cover word or a payload word, right or wrong,
// and — under v3 — whether Reed–Solomon parity corrected it. That is what the
// overlay draws. Without an alignment (a rendering that carries no checksum,
// or damage past what parity can fix) the boxes fall back to what snapping
// alone knows.

/// Class of a recognized token, from strongest evidence to weakest:
///   payload-ok        a payload word, matched the rendering
///   cover-ok          a cover word, matched the rendering
///   cover-error       a cover word misread or added — bytes unaffected
///   payload-repaired  a payload word misread, corrected by parity
///   payload-error     a payload word wrong, spurious, or past repair
///   snapped           (no alignment) corrected to a vocabulary word
///   unsure            (no alignment) not resolved to a vocabulary word
///   junk              not a word
export const TOKEN_CLASSES = ['payload-ok', 'cover-ok', 'cover-error', 'payload-repaired', 'payload-error', 'snapped', 'unsure', 'junk'];

function fallbackClass(t) {
  if (t.status === 'junk') return 'junk';
  if (t.status === 'unknown') return 'unsure';
  if (t.status === 'snapped') return 'snapped';
  return t.isPayload ? 'payload-ok' : 'cover-ok';
}

const q = (w) => '“' + w + '”';

/// Map an alignment (the `alignment` a canonical decode entry returns, or
/// `align_prose`'s result) onto the recognized tokens. `repaired` is the list
/// of payload slots parity corrected. Returns { classes, missing }: one
/// { cls, note } per token, and the words the rendering had that the page did
/// not, each anchored before the token that followed it (`before` is a token
/// index, or null for the end of the text).
export function classifyTokens(tokens, alignment, repaired = []) {
  const classes = tokens.map((t) => ({ cls: fallbackClass(t), note: t.status === 'snapped' ? 'read ' + q(t.raw) + ', corrected to ' + q(t.word) : '' }));
  const missing = [];
  if (!alignment || !Array.isArray(alignment.tokens)) return { classes, missing };
  // The aligner tokenizes the transcription on whitespace, and the
  // transcription is the non-junk tokens in order — so received_index counts
  // non-junk tokens.
  const recv = [];
  tokens.forEach((t, i) => { if (t.status !== 'junk') recv.push(i); });
  const rep = new Set(repaired);
  const al = alignment.tokens;
  for (let k = 0; k < al.length; k++) {
    const a = al[k];
    if (a.op === 'delete') {
      let next = null;
      for (let j = k + 1; j < al.length; j++) if (al[j].received_index != null) { next = recv[al[j].received_index]; break; }
      const isPayload = a.payload_index != null;
      missing.push({
        before: next == null ? null : next,
        expected: a.expected,
        payload: isPayload,
        repaired: isPayload && rep.has(a.payload_index),
      });
      continue;
    }
    const ti = recv[a.received_index];
    if (ti == null) continue;
    const c = classes[ti];
    const shown = surfaceForm(tokens[ti]);
    const slotPayload = a.payload_index != null;
    if (a.op === 'same') {
      c.cls = slotPayload ? 'payload-ok' : 'cover-ok';
      c.note = '';
    } else if (a.op === 'sub') {
      if (slotPayload) {
        const fixed = rep.has(a.payload_index);
        c.cls = fixed ? 'payload-repaired' : 'payload-error';
        c.note = 'payload word: read ' + q(shown) + (fixed ? ', corrected by parity to ' : ', should be ') + q(a.expected);
      } else {
        c.cls = 'cover-error';
        c.note = 'cover word: read ' + q(shown) + ', should be ' + q(a.expected) + ' — bytes unaffected';
      }
    } else if (a.op === 'insert') {
      if (a.received_is_payload) {
        c.cls = 'payload-error';
        c.note = 'payload word ' + q(shown) + ' that the rendering does not have';
      } else {
        c.cls = 'cover-error';
        c.note = 'extra word ' + q(shown) + ' — cover, bytes unaffected';
      }
    }
  }
  return { classes, missing };
}

/// Counts per class, for a status line.
export function classSummary(classes) {
  const n = {};
  for (const k of TOKEN_CLASSES) n[k] = 0;
  for (const c of classes) n[c.cls]++;
  return n;
}

/// k-subsets of [0, n), in lexicographic order.
function* combinations(n, k) {
  const idx = Array.from({ length: k }, (_, i) => i);
  if (k > n) return;
  while (true) {
    yield idx.slice();
    let i = k - 1;
    while (i >= 0 && idx[i] === n - k + i) i--;
    if (i < 0) return;
    idx[i]++;
    for (let j = i + 1; j < k; j++) idx[j] = idx[j - 1] + 1;
  }
}

/// Most attempts a hypothesis search will spend on one frame. Each is a
/// decode plus a render, so this bounds the worst frame at a couple of seconds.
export const MAX_HYPOTHESES = 20;

/// Decode a scan through the canonical path and classify every token.
///
/// `decode(text)` is the plain canonical decode (JSON string in, JSON string
/// out, as the WASM exports are); it succeeds on an intact transcription and,
/// under v3, on one with a payload word or two misread onto the wordlist. When
/// it fails, the search below tries the tokens snapping could not resolve as
/// holes: a payload word mangled OFF the wordlist never reaches the harvest,
/// so the only trace of it is an unsure box, and handing the decoder the
/// payload sequence with a `null` there (`decodeSlots(slotsJson)`) makes it an
/// erasure, which parity fills at half the cost of an unlocated error. With k
/// words short and n unsure tokens there are C(n, k) placements; the first
/// whose checksum passes wins. A caller that knows the rendering's word count
/// passes `expectedWords` and k is exact; otherwise k runs 1, 2, 3. Either way
/// the search stops at MAX_HYPOTHESES.
///
/// `align(text, renderedText)` is `align_prose`: a result from `decodeSlots`
/// aligns the slot sequence it was given, not the transcription, so its
/// alignment is re-taken against the transcription before it is mapped onto
/// tokens. Without `align`, a slots result is reported but the tokens keep
/// their snapping classes.
///
/// Returns { ok, verified, version, payload_hex, repaired, alignment,
/// canonical_text, classes, missing, counts, attempts, error }.
export function annotateScan(scan, { decode, decodeSlots, align, expectedWords } = {}) {
  const tokens = scan.tokens;
  let result = null, attempts = 0, error = null, fromSlots = false;
  if (decode) {
    attempts++;
    const r = safeJson(decode(scan.text));
    if (r && !r.error) result = r; else error = r ? r.error : 'decode returned nothing';
  }
  if (!result && decodeSlots) {
    const live = tokens.filter((t) => t.status !== 'junk');
    const harvested = live.filter((t) => t.isPayload).length;
    const unsure = live.map((t, i) => (t.status === 'unknown' ? i : -1)).filter((i) => i >= 0);
    const ks = expectedWords ? [expectedWords - harvested] : [1, 2, 3];
    search: for (const k of ks) {
      if (k <= 0 || k > unsure.length) continue;
      for (const pick of combinations(unsure.length, k)) {
        if (attempts >= MAX_HYPOTHESES) break search;
        const holes = new Set(pick.map((i) => unsure[i]));
        const slots = [];
        live.forEach((t, i) => { if (holes.has(i)) slots.push(null); else if (t.isPayload) slots.push(t.word); });
        attempts++;
        const r = safeJson(decodeSlots(JSON.stringify(slots)));
        if (r && !r.error) { result = r; fromSlots = true; break search; }
        if (r && r.error) error = r.error;
      }
    }
  }
  if (!result) {
    const { classes, missing } = classifyTokens(tokens, null);
    return { ok: false, verified: false, classes, missing, counts: classSummary(classes), attempts, error };
  }
  let alignment = result.alignment;
  if (fromSlots) {
    alignment = null;
    if (align && result.canonical_text) {
      const a = safeJson(align(scan.text, result.canonical_text));
      if (a && !a.error) alignment = a;
    }
    // A slots result is verified only if the transcription itself is clean,
    // which it is not — a hole was declared in it.
    result = { ...result, verified: alignment ? !!alignment.clean : false };
  }
  const { classes, missing } = classifyTokens(tokens, alignment, result.repaired || []);
  return {
    ok: true,
    verified: !!result.verified,
    version: result.version,
    payload_hex: result.payload_hex,
    repaired: result.repaired || [],
    alignment,
    canonical_text: result.canonical_text,
    classes, missing, counts: classSummary(classes), attempts, error: null,
  };
}

function safeJson(s) {
  if (s == null) return null;
  if (typeof s !== 'string') return s;
  try { return JSON.parse(s); } catch (e) { return { error: 'bad JSON from decoder' }; }
}

// ─── overlay ─────────────────────────────────────────────────────────

/// Stroke colours per class. cover-ok is drawn faintly so the eye lands on
/// what matters; junk is not drawn.
export const CLASS_COLORS = {
  'payload-ok': '#22c55e',
  'cover-ok': 'rgba(120,120,140,0.45)',
  'cover-error': '#f59e0b',
  'payload-repaired': '#f97316',
  'payload-error': '#ef4444',
  'snapped': '#f59e0b',
  'unsure': '#ef4444',
};

/// Draw the boxes over a frame. Boxes are in the prepared frame's coordinates
/// (`width` × `height`, the size the recognizer saw); `scale` and `skew` map
/// them back onto a source the prepared frame was scaled and straightened
/// from — pass the values scanImage returned to draw over the live video, or
/// leave the defaults to draw over the prepared frame itself. Labels show the
/// correction for every box that has one.
export function drawOverlay(ctx, tokens, classes, missing, { width, height, scale = 1, skew = 0, labels = true } = {}) {
  ctx.save();
  // prepared -> source: undo the straightening about the frame's centre, then
  // undo the scaling.
  ctx.scale(1 / scale, 1 / scale);
  ctx.translate(width / 2, height / 2);
  ctx.rotate(skew * Math.PI / 180);
  ctx.translate(-width / 2, -height / 2);
  const unit = Math.max(1, Math.round(Math.max(width, height) / 700));   // line weight that reads at any size
  ctx.lineJoin = 'round';
  const boxes = [];
  tokens.forEach((t, i) => {
    const c = classes[i];
    const b = t.bbox;
    if (!b || !c || c.cls === 'junk') return;
    boxes.push({ t, c, b });
  });
  for (const { t, c, b } of boxes) {
    const color = CLASS_COLORS[c.cls];
    if (!color) continue;
    const pad = unit * 2;
    const x = b.x0 - pad, y = b.y0 - pad, w = b.x1 - b.x0 + 2 * pad, h = b.y1 - b.y0 + 2 * pad;
    ctx.lineWidth = c.cls === 'cover-ok' ? unit : unit * 2;
    ctx.setLineDash(c.cls === 'unsure' || c.cls === 'snapped' ? [unit * 4, unit * 3] : []);
    ctx.strokeStyle = color;
    ctx.strokeRect(x, y, w, h);
    if (c.cls !== 'cover-ok') {
      ctx.fillStyle = color.startsWith('#') ? color + '22' : color;
      ctx.fillRect(x, y, w, h);
    }
  }
  ctx.setLineDash([]);
  // Missing words: a bar where the word should have been.
  for (const m of missing) {
    const color = m.payload ? (m.repaired ? CLASS_COLORS['payload-repaired'] : CLASS_COLORS['payload-error']) : CLASS_COLORS['cover-error'];
    let x, y0, y1;
    if (m.before != null && tokens[m.before] && tokens[m.before].bbox) {
      const b = tokens[m.before].bbox; x = b.x0 - unit * 4; y0 = b.y0; y1 = b.y1;
    } else if (boxes.length) {
      const b = boxes[boxes.length - 1].b; x = b.x1 + unit * 4; y0 = b.y0; y1 = b.y1;
    } else continue;
    ctx.lineWidth = unit * 2;
    ctx.strokeStyle = color;
    ctx.beginPath(); ctx.moveTo(x, y0 - unit * 2); ctx.lineTo(x, y1 + unit * 2); ctx.stroke();
    if (labels) label(ctx, '+ ' + m.expected, x, y0, y1 - y0, color, unit);
  }
  if (labels) {
    for (const { t, c, b } of boxes) {
      let text = null;
      if (c.cls === 'payload-repaired' || c.cls === 'payload-error' || c.cls === 'cover-error') {
        const m = c.note.match(/(?:corrected by parity to|should be) “([^”]+)”/);
        text = m ? '→ ' + m[1] : (c.cls === 'payload-error' ? '✗' : '+');
      } else if (c.cls === 'snapped') {
        text = '→ ' + t.word;
      } else if (c.cls === 'unsure') {
        text = '?';
      }
      if (text) label(ctx, text, b.x0, b.y0, b.y1 - b.y0, CLASS_COLORS[c.cls], unit);
    }
  }
  ctx.restore();
}

function label(ctx, text, x, y, boxH, color, unit) {
  const size = Math.max(10, Math.round(boxH * 0.7));
  ctx.font = `600 ${size}px sans-serif`;
  const w = ctx.measureText(text).width + unit * 4;
  const h = size + unit * 3;
  const ly = y - h - unit;
  ctx.fillStyle = color;
  ctx.fillRect(x, ly < 0 ? y + boxH + unit : ly, w, h);
  ctx.fillStyle = '#fff';
  ctx.textBaseline = 'middle';
  ctx.fillText(text, x + unit * 2, (ly < 0 ? y + boxH + unit : ly) + h / 2);
}
