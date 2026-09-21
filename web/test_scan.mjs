#!/usr/bin/env node
// Unit tests for the pure half of glossia-scan.js: token normalization and
// vocabulary snapping. No OCR engine, no WASM — run with `node --test web/`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { normalizeToken, buildVocab, editDistance, snapBudget, snapToken, snapTokens, surfaceForm, scanText, scanSummary, estimateSkew }
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
