#!/usr/bin/env node
// Unit tests for the pure half of glossia-scan.js: token normalization and
// vocabulary snapping. No OCR engine, no WASM — run with `node --test web/`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { normalizeToken, buildVocab, editDistance, snapBudget, snapToken, snapTokens, surfaceForm, scanText, scanSummary, estimateSkew,
  classifyTokens, annotateScan, classSummary }
  from './glossia-scan.js';

const vocab = buildVocab([
  'abandon', 'ability', 'able', 'about', 'above', 'absent', 'absorb', 'abstract',
  'cat', 'can', 'car', 'cabin', 'cable', 'cactus', 'garden', 'garment',
  'the', 'a', 'of', 'and', 'quickly', 'Café',
]);

test('normalizeToken strips punctuation and maps confusable glyphs', () => {
  assert.equal(normalizeToken('Abandon,'), 'abandon');
  assert.equal(normalizeToken('“about”'), 'about');
  assert.equal(normalizeToken('ab1e'), 'able');       // 1 -> l
  assert.equal(normalizeToken('ab0ut'), 'about');     // 0 -> o
  assert.equal(normalizeToken('ab|e'), 'able');       // | -> l
  assert.equal(normalizeToken('—'), '');
  assert.equal(normalizeToken(''), '');
  assert.equal(normalizeToken('CAFÉ.'), 'café');      // diacritics survive
});

test('editDistance is Damerau with early exit', () => {
  assert.equal(editDistance('abandon', 'abandon'), 0);
  assert.equal(editDistance('abandon', 'abandom'), 1);
  assert.equal(editDistance('abandon', 'abadnon'), 1);   // transposition
  assert.equal(editDistance('abandon', 'abandonment'), 4);
  assert.equal(editDistance('abandon', 'abandonment', 2), 3); // capped: > max
  assert.equal(editDistance('', 'abc'), 3);
});

test('snapBudget never lets short tokens snap', () => {
  assert.equal(snapBudget(3), 0);
  assert.equal(snapBudget(4), 1);
  assert.equal(snapBudget(5), 1);
  assert.equal(snapBudget(6), 2);
  assert.equal(snapBudget(12), 2);
});

test('exact vocabulary words pass through', () => {
  const r = snapToken('Abandon', vocab);
  assert.equal(r.status, 'exact');
  assert.equal(r.word, 'abandon');
  assert.equal(snapToken('A', vocab).status, 'exact');     // one-letter word, not junk
  assert.equal(snapToken('I', vocab).status, 'junk');      // one letter, not a word here
});

test('a unique near miss snaps', () => {
  assert.deepEqual(pick(snapToken('abandom', vocab)), ['snapped', 'abandon', 1]);
  assert.deepEqual(pick(snapToken('abstrct', vocab)), ['snapped', 'abstract', 1]);
  assert.deepEqual(pick(snapToken('gardan', vocab)), ['snapped', 'garden', 1]);
  assert.deepEqual(pick(snapToken('abillty', vocab)), ['snapped', 'ability', 1]);
});

test('a short token never snaps, even one edit away', () => {
  const r = snapToken('cam', vocab);
  assert.equal(r.status, 'unknown');
  assert.equal(r.word, null);
});

test('an ambiguous near miss is refused and the tie is reported', () => {
  // "garmen" is one edit from garment (drop t) AND from garden (m -> d).
  const g = snapToken('garmen', vocab);
  assert.equal(g.status, 'unknown');
  assert.deepEqual(g.candidates, ['garden', 'garment']);
  // "cabi" -> cabin (1). "cabl" -> cable (1). "cabie": cable (1: l->i), cabin (1: e->n) -> tie.
  const r = snapToken('cabie', vocab);
  assert.equal(r.status, 'unknown');
  assert.deepEqual(r.candidates, ['cabin', 'cable']);
});

test('a token beyond the edit budget stays unknown', () => {
  assert.equal(snapToken('xyzzyq', vocab).status, 'unknown');
  assert.equal(snapToken('abandonment', vocab).status, 'unknown');
});

test('low recognizer confidence blocks a snap but not an exact match', () => {
  assert.equal(snapToken('abandom', vocab, { confidence: 10 }).status, 'unknown');
  assert.equal(snapToken('abandon', vocab, { confidence: 10 }).status, 'exact');
});

test('junk tokens are dropped from the text', () => {
  const words = [
    { text: 'The', confidence: 95, line: 0 }, { text: 'abandom', confidence: 80, line: 0 },
    { text: '—', confidence: 20, line: 0 }, { text: 'garden.', confidence: 90, line: 0 },
    { text: 'xyzzyq', confidence: 70, line: 1 }, { text: 'cat', confidence: 90, line: 1 },
  ];
  const toks = snapTokens(words, vocab);
  assert.equal(scanText(toks), 'The abandon garden.\nxyzzyq cat');
  assert.equal(toks[0].bbox, null);                        // none given, none invented
  assert.deepEqual(scanSummary(toks), { exact: 3, snapped: 1, unknown: 1, junk: 1, words: 5 });
});

test('surfaceForm keeps the capital and the trailing punctuation that were read', () => {
  assert.equal(surfaceForm(snapToken('Abandom,', vocab)), 'Abandon,');
  assert.equal(surfaceForm(snapToken('“Garden.”', vocab)), 'Garden.');   // closing quote is not carried
  assert.equal(surfaceForm(snapToken('ab0ut', vocab)), 'about');
  assert.equal(surfaceForm(snapToken('Xyzzyq!', vocab)), 'Xyzzyq!');
});

test('buildVocab dedupes and lower-cases', () => {
  const v = buildVocab(['Able', 'able', 'ABLE']);
  assert.equal(v.size, 1);
  assert.ok(v.set.has('able'));
});

function pick(r) { return [r.status, r.word, r.distance]; }

// A synthetic page: dark text lines on light ground, drawn at a known skew.
function skewedPage(deg, w = 600, h = 300) {
  const lum = new Uint8ClampedArray(w * h).fill(235);
  const t = Math.tan(deg * Math.PI / 180);
  for (let y0 = 50; y0 < h - 40; y0 += 40) {
    for (let x = 30; x < w - 30; x++) {
      // words: ink with gaps, six pixels tall
      if (((x / 14) | 0) % 5 === 4) continue;
      const yc = y0 + (x - w / 2) * t;
      for (let dy = -3; dy <= 3; dy++) {
        const y = Math.round(yc + dy);
        if (y >= 0 && y < h) lum[y * w + x] = 30;
      }
    }
  }
  return lum;
}

test('estimateSkew recovers the angle of skewed text lines', () => {
  for (const deg of [0, 1.5, 3, -2.5, -5]) {
    const got = estimateSkew(skewedPage(deg), 600, 300);
    assert.ok(Math.abs(got - deg) <= 0.5, `skew ${deg}: estimated ${got}`);
  }
});

test('estimateSkew reports no skew for an empty frame', () => {
  assert.equal(estimateSkew(new Uint8ClampedArray(600 * 300).fill(240), 600, 300), 0);
});

// ─── verdicts from an alignment ──────────────────────────────────────

const pv = buildVocab(['insect', 'victory', 'ring', 'creek', 'bonus', 'why', 'see', 'set', 'to', 'a'],
                      ['insect', 'victory', 'ring', 'creek', 'bonus']);

function toks(words) {
  return snapTokens(words.map((w, i) => ({ text: w, confidence: 90, line: 0, bbox: { x0: i * 50, y0: 0, x1: i * 50 + 40, y1: 20 } })), pv);
}

// The alignment src/align.rs would produce for "Insect why see victoree. Ring
// set creek to bonus" against "Insect why see victory. Ring set creek to
// bonus": one payload word misread onto nothing ("victoree" is off the list).
const alignment = {
  tokens: [
    { op: 'same', received: 'insect',   received_index: 0, expected: 'insect',  expected_index: 0, payload_index: 0,    received_is_payload: true },
    { op: 'same', received: 'why',      received_index: 1, expected: 'why',     expected_index: 1, payload_index: null, received_is_payload: false },
    { op: 'sub',  received: 'sea',      received_index: 2, expected: 'see',     expected_index: 2, payload_index: null, received_is_payload: false },
    { op: 'sub',  received: 'victoree', received_index: 3, expected: 'victory', expected_index: 3, payload_index: 1,    received_is_payload: false },
    { op: 'same', received: 'ring',     received_index: 4, expected: 'ring',    expected_index: 4, payload_index: 2,    received_is_payload: true },
    { op: 'delete', received: null,     received_index: null, expected: 'set',  expected_index: 5, payload_index: null, received_is_payload: false },
    { op: 'same', received: 'creek',    received_index: 5, expected: 'creek',   expected_index: 6, payload_index: 3,    received_is_payload: true },
    { op: 'same', received: 'to',       received_index: 6, expected: 'to',      expected_index: 7, payload_index: null, received_is_payload: false },
    { op: 'sub',  received: 'ring',     received_index: 7, expected: 'bonus',   expected_index: 8, payload_index: 4,    received_is_payload: true },
  ],
};

test('classifyTokens maps alignment ops onto tokens, skipping junk', () => {
  const t = toks(['Insect', 'why', '—', 'sea', 'victoree.', 'Ring', 'creek', 'to', 'ring']);
  assert.deepEqual(t[1].bbox, { x0: 50, y0: 0, x1: 90, y1: 20 });   // the box rides along for the overlay
  const { classes, missing } = classifyTokens(t, alignment, [1]);
  assert.deepEqual(classes.map(c => c.cls), [
    'payload-ok', 'cover-ok', 'junk', 'cover-error', 'payload-repaired', 'payload-ok', 'payload-ok', 'cover-ok', 'payload-error',
  ]);
  assert.match(classes[3].note, /cover word.*should be “see”/);
  assert.match(classes[4].note, /corrected by parity to “victory”/);
  assert.match(classes[8].note, /should be “bonus”/);
  // "set" is missing before the token "creek" (index 6 in the token list)
  assert.deepEqual(missing, [{ before: 6, expected: 'set', payload: false, repaired: false }]);
  assert.deepEqual(classSummary(classes), { 'payload-ok': 3, 'cover-ok': 2, 'cover-error': 1, 'payload-repaired': 1, 'payload-error': 1, snapped: 0, unsure: 0, junk: 1 });
});

test('classifyTokens without an alignment falls back to what snapping knows', () => {
  const t = toks(['Insect', 'why', 'victoree', 'xqzt', 'creek']);
  const { classes } = classifyTokens(t, null);
  assert.deepEqual(classes.map(c => c.cls), ['payload-ok', 'cover-ok', 'snapped', 'unsure', 'payload-ok']);
});

test('annotateScan takes the plain decode when it succeeds', () => {
  const t = toks(['Insect', 'why', 'sea', 'victoree.', 'Ring', 'creek', 'to', 'ring']);
  const scan = { text: scanText(t), tokens: t };
  const decode = text => JSON.stringify({ version: 3, payload_hex: 'ab', verified: false, repaired: [1], alignment });
  const r = annotateScan(scan, { decode });
  assert.equal(r.ok, true);
  assert.equal(r.attempts, 1);
  assert.equal(r.classes[3].cls, 'payload-repaired');
});

test('annotateScan searches unsure tokens as erasures when the count is short', () => {
  // Five payload words expected; "victoree" and "xqzt" are unsure, and only the
  // placement with a hole at "victoree" (slot 1) decodes.
  const t = toks(['Insect', 'why', 'vqxzkry', 'xqzt', 'Ring', 'creek', 'bonus']);
  const scan = { text: scanText(t), tokens: t };
  const seen = [];
  const decode = () => JSON.stringify({ error: 'crc mismatch' });
  const decodeSlots = (slotsJson) => {
    const slots = JSON.parse(slotsJson);
    seen.push(slots);
    const good = slots.length === 5 && slots[1] === null && slots[0] === 'insect';
    return JSON.stringify(good ? { version: 3, payload_hex: 'cd', verified: true, repaired: [1], alignment: { tokens: [], clean: true }, canonical_text: 'Insect why see victory. Ring creek bonus' } : { error: 'crc mismatch' });
  };
  // The slots result's own alignment is over the slot text, so the module
  // must re-align the transcription against the rendering it returns.
  let aligned = null;
  const align = (text, rendered) => { aligned = [text, rendered]; return JSON.stringify({ tokens: [], clean: false }); };
  const r = annotateScan(scan, { decode, decodeSlots, align, expectedWords: 5 });
  assert.equal(r.ok, true);
  assert.equal(r.verified, false);
  assert.equal(r.payload_hex, 'cd');
  assert.equal(seen.length, 1);                       // first hypothesis was the right one
  assert.deepEqual(seen[0], ['insect', null, 'ring', 'creek', 'bonus']);
  assert.deepEqual(aligned, [scan.text, 'Insect why see victory. Ring creek bonus']);
});

test('annotateScan without an expected count tries one hole, then two', () => {
  const t = toks(['Insect', 'why', 'vqxzkry', 'xqzt', 'Ring', 'creek', 'bonus']);
  const seen = [];
  const decodeSlots = (slotsJson) => {
    const slots = JSON.parse(slotsJson);
    seen.push(slots);
    const good = slots.length === 6 && slots[1] === null && slots[2] === null;   // both unsure tokens were payload
    return JSON.stringify(good ? { version: 3, payload_hex: 'ef', verified: false, repaired: [1, 2], alignment: null } : { error: 'crc mismatch' });
  };
  const r = annotateScan({ text: scanText(t), tokens: t }, { decode: () => JSON.stringify({ error: 'crc' }), decodeSlots });
  assert.equal(r.ok, true);
  assert.deepEqual(seen, [
    ['insect', null, 'ring', 'creek', 'bonus'],
    ['insect', null, 'ring', 'creek', 'bonus'],
    ['insect', null, null, 'ring', 'creek', 'bonus'],
  ]);
});

test('annotateScan reports failure without guessing', () => {
  const t = toks(['Insect', 'why', 'ring']);
  const r = annotateScan({ text: scanText(t), tokens: t }, { decode: () => JSON.stringify({ error: 'no payload' }) });
  assert.equal(r.ok, false);
  assert.equal(r.error, 'no payload');
  assert.deepEqual(r.classes.map(c => c.cls), ['payload-ok', 'cover-ok', 'payload-ok']);
});
