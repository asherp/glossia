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
/// pipeline the browser does. Returns { canvas, skew } — the skew in degrees
/// that was removed.
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
  return { canvas, skew };
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
export function buildVocab(words) {
  const set = new Set();
  const byLength = new Map();
  for (const w of words) {
    const n = w.normalize('NFC').toLowerCase();
    if (!n || set.has(n)) continue;
    set.add(n);
    if (!byLength.has(n.length)) byLength.set(n.length, []);
    byLength.get(n.length).push(n);
  }
  return { set, byLength, size: set.size };
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
  if (vocab.set.has(norm)) return { raw, norm, word: norm, status: 'exact', distance: 0, candidates: [] };
  // A single letter that is not itself a word ("a" is one) is a stray mark.
  if (norm.length < 2) return { raw, norm, word: null, status: 'junk', distance: null, candidates: [] };
  // Below the confidence floor the recognizer is guessing; so would we.
  if (confidence < minConfidence) return { raw, norm, word: null, status: 'unknown', distance: null, candidates: [] };
  const budget = snapBudget(norm.length);
  if (budget === 0) return { raw, norm, word: null, status: 'unknown', distance: null, candidates: [] };
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
  if (best > budget) return { raw, norm, word: null, status: 'unknown', distance: null, candidates: [] };
  if (tied.length === 1) return { raw, norm, word: tied[0], status: 'snapped', distance: best, candidates: [] };
  return { raw, norm, word: null, status: 'unknown', distance: best, candidates: tied.sort() };
}

/// Snap every recognized word. Tokens keep their `line` so the text can be
/// re-flowed the way the page was laid out.
export function snapTokens(words, vocab, opts = {}) {
  return words.map((w) => ({ ...snapToken(w.text, vocab, { ...opts, confidence: w.confidence }), line: w.line, confidence: w.confidence }));
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
/// Returns { text, tokens, summary, raw, canvas, skew }.
export async function scanImage(source, { vocab, ocrLang = 'eng', onProgress, prepare = true, prepareOptions, minConfidence, workerOptions, loader } = {}) {
  const worker = await getWorker(ocrLang, { onProgress, options: workerOptions, loader });
  const prepared = prepare ? await prepareImage(source, prepareOptions) : { canvas: null, skew: 0 };
  if (onProgress) onProgress('recognizing text', 0);
  const raw = await recognize(worker, prepared.canvas || source);
  const tokens = snapTokens(raw.words, vocab, { minConfidence });
  return { text: scanText(tokens), tokens, summary: scanSummary(tokens), raw, canvas: prepared.canvas, skew: prepared.skew };
}
