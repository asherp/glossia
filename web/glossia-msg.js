// glossia-msg.js — message → encrypted Glossia artifact, as an ES module.
//
// This is the content pipeline shared by the bulletin board (bulletin.html).
// It mirrors the "Encrypted Message" panel in index.html so that an artifact
// produced here decodes with the exact same rules (and vice-versa):
//
//   encode: message + passphrase -> reduce -> AES-CTR -> "<key>: prose"
//   decode: "<key>: prose" + passphrase -> AES-CTR -> expand -> message
//
// The passphrase is optional. With no passphrase the bytes are compressed and
// encoded but not encrypted; the [flag][len] header rides inside the prose and
// the artifact is bare prose (no "<key>: " prefix), decodable by anyone.
//
// NOTE: AES-CTR provides confidentiality only — it is NOT authenticated, so it
// cannot detect tampering. (Same caveat as the demo panel.)
//
// The glossia WASM (encode_raw_base_n / decode_raw_base_n) is loaded from the
// same ./glossia.js bundle the rest of the site uses; call init() once first.

import init, {
  encode_raw_base_n as wasmEncodeRawBaseN,
  decode_raw_base_n as wasmDecodeRawBaseN,
  detect_dialect_from_text as wasmDetectDialect,
} from './glossia.js';

export { init };

const SEED = 42n;               // fixed seed -> deterministic prose

// Languages this pipeline can render into / detect from (matches index.html's
// ENC_LANGS so artifacts are interchangeable with the demo panel).
export const MSG_LANGS = [
  { id: 'english', label: 'English', language: 'english', wordlist: 'bip39',   dialect: 'body' },
  { id: 'latin',   label: 'Latin',   language: 'latin',   wordlist: 'default', dialect: 'body' },
  { id: 'czech',   label: 'Czech',   language: 'czech',   wordlist: 'default', dialect: 'body' },
];
export function msgLangById(id) { return MSG_LANGS.find(l => l.id === id) || MSG_LANGS[0]; }

const TE = new TextEncoder();
const TD = new TextDecoder();

function toHex(bytes) { return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join(''); }
function fromHex(h) {
  const clean = h.trim().toLowerCase().replace(/[^0-9a-f]/g, '');
  const out = new Uint8Array(clean.length >> 1);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.substr(i * 2, 2), 16);
  return out;
}

// ─── compression (CompressionStream) ──────────────────────────────────
async function gzipBytes(bytes) {
  const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}
async function gunzipBytes(bytes) {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}
// Pack pure-ASCII bytes 7-bits-wide: [charCount:u16 BE][7-bit packed stream].
function asciiPack7(bytes) {
  const L = bytes.length;
  if (L > 0xffff) return null;
  for (let i = 0; i < L; i++) if (bytes[i] > 0x7f) return null;
  const out = new Uint8Array(2 + Math.ceil(L * 7 / 8));
  out[0] = (L >> 8) & 0xff; out[1] = L & 0xff;
  let acc = 0, bits = 0, pos = 2;
  for (let i = 0; i < L; i++) {
    acc = (acc << 7) | bytes[i];
    bits += 7;
    while (bits >= 8) { bits -= 8; out[pos++] = (acc >> bits) & 0xff; }
  }
  if (bits > 0) out[pos++] = (acc << (8 - bits)) & 0xff;
  return out;
}
function asciiUnpack7(packed) {
  const L = (packed[0] << 8) | packed[1];
  const out = new Uint8Array(L);
  let acc = 0, bits = 0, pos = 2, n = 0;
  while (n < L) {
    acc = (acc << 8) | (packed[pos++] || 0);
    bits += 8;
    while (bits >= 7 && n < L) { bits -= 7; out[n++] = (acc >> bits) & 0x7f; }
  }
  return out;
}
// Pick the smallest representation; flag records it: 0=raw 1=gzip 2=7-bit pack.
async function maybeReduce(bytes) {
  let best = { data: bytes, flag: 0 };
  const a7 = asciiPack7(bytes);
  if (a7 && a7.length < best.data.length) best = { data: a7, flag: 2 };
  if (typeof CompressionStream !== 'undefined') {
    try {
      const gz = await gzipBytes(bytes);
      if (gz.length < best.data.length) best = { data: gz, flag: 1 };
    } catch (e) { /* keep current best */ }
  }
  return best;
}
async function expand(bytes, flag) {
  if (flag === 1) return gunzipBytes(bytes);
  if (flag === 2) return asciiUnpack7(bytes);
  return bytes;
}

// ─── key + nonce both derived from password+salt (so the nonce is never
//     transmitted). algo selects the cipher: 'AES-GCM' (authenticated, current
//     scheme) or 'AES-CTR' (legacy artifacts). The 12-byte nonce doubles as the
//     GCM IV. A fresh random salt per message keeps every (key, nonce) unique.
async function deriveKeyNonce(password, salt, algo = 'AES-GCM') {
  const baseKey = await crypto.subtle.importKey('raw', TE.encode(password), 'PBKDF2', false, ['deriveBits']);
  const bits = new Uint8Array(await crypto.subtle.deriveBits(
    { name: 'PBKDF2', salt, iterations: 200000, hash: 'SHA-256' }, baseKey, (32 + 12) * 8));
  const key = await crypto.subtle.importKey('raw', bits.subarray(0, 32), { name: algo }, false, ['encrypt', 'decrypt']);
  return { key, nonce: bits.subarray(32, 44) };
}
function ctrCounter(nonce) { const c = new Uint8Array(16); c.set(nonce, 0); return c; }

// ─── varint + base64url + header framing ──────────────────────────────
function varintEncode(n) {
  const out = [];
  do { let b = n % 128; n = Math.floor(n / 128); if (n > 0) b |= 0x80; out.push(b); } while (n > 0);
  return new Uint8Array(out);
}
function varintDecode(bytes, pos) {
  let value = 0, mult = 1, p = pos, b;
  do { b = bytes[p++]; value += (b & 0x7f) * mult; mult *= 128; } while (b & 0x80);
  return { value, next: p };
}
function b64urlEncode(bytes) {
  let bin = '';
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
function b64urlDecode(str) {
  const s = str.replace(/-/g, '+').replace(/_/g, '/');
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4));
  const bin = atob(s + pad);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

const FLAG_ENCRYPTED = 0x80;
function buildHeader(flag, ctlen, salt) {
  const lp = varintEncode(ctlen);
  const h = new Uint8Array(1 + lp.length + salt.length);
  h[0] = flag | FLAG_ENCRYPTED;
  h.set(lp, 1);
  h.set(salt, 1 + lp.length);
  return b64urlEncode(h);
}
function parseHeader(b64) {
  const h = b64urlDecode(b64);
  if (h.length < 2) throw new Error('bad header');
  const flag = h[0] & 0x7f;
  const { value: ctlen, next } = varintDecode(h, 1);
  const salt = h.subarray(next);
  if (salt.length < 8) throw new Error('bad header');
  return { flag, ctlen, salt };
}
function buildEmbedded(flag, data) {
  const lp = varintEncode(data.length);
  const out = new Uint8Array(1 + lp.length + data.length);
  out[0] = flag & 0x7f;
  out.set(lp, 1);
  out.set(data, 1 + lp.length);
  return out;
}
function parseEmbedded(bytes) {
  if (bytes.length < 2) throw new Error('bad payload');
  const flag = bytes[0] & 0x7f;
  const { value: len, next } = varintDecode(bytes, 1);
  return { flag, data: bytes.subarray(next, next + len) };
}
// "<base64url-key>: prose" when encrypted, else bare prose.
export function parseArtifact(text) {
  text = (text || '').trim();
  const i = text.indexOf(':');
  if (i > 0 && /^[A-Za-z0-9_-]+$/.test(text.slice(0, i))) {
    const prose = text.slice(i + 1).trim();
    if (prose) return { header: text.slice(0, i), prose };
  }
  return { header: null, prose: text };
}

// ─── authenticated artifact: "<prose> — <latin attribution>" ──────────
//
// The encrypted artifact reads as a quote with an attribution. The prose IS the
// AES-256-GCM ciphertext; the em-dash trailer is the plumbing — flag + length +
// salt + 128-bit auth tag — rendered as Latin payload words, so it scans like
// "— Cornelius Vanto Brixia". GCM authenticates: a wrong passphrase or a
// tampered message fails cleanly instead of yielding garbage.
//
//   trailer bytes: [flag:2b | length:14b : 2 BE][salt : 6][GCM tag : 12] = 20
//
// The top 2 bits of the length field carry the reduction method, the low 14
// bits the ciphertext length (<=16 KB). The em-dash never appears in encoded
// prose, so it alone signals the format (no version byte needed). Latin's ~15
// bits/word lands this at 11 words.
//
// Sizing notes: the 96-bit GCM tag is the integrity check (and the nostr event
// signature independently authenticates the content); the 48-bit salt derives a
// fresh key+nonce per message — a (key, nonce) repeat needs a salt collision,
// ~2^24 messages on one board, which is far beyond any real bulletin.
const AEAD_SALT_LEN = 6;
const AEAD_TAG_BITS = 96;
const AEAD_TAG_LEN = AEAD_TAG_BITS / 8;     // 12 bytes
const AEAD_MAX_CTLEN = 0x3fff;              // 14-bit length
const AEAD_TRAILER_LEN = 2 + AEAD_SALT_LEN + AEAD_TAG_LEN;   // 20 bytes -> ~11 Latin words
const EMDASH = ' — ';

function capWords(s) { return s.replace(/(^|\s)(\p{L})/gu, (_, sp, c) => sp + c.toUpperCase()); }

// ─── public API ───────────────────────────────────────────────────────

// message + passphrase -> artifact string. With a passphrase the result is the
// authenticated "<prose> — <attribution>" form; without one it is bare prose
// (unencrypted, the [flag][len] header riding inside the payload). Returns
// { artifact, prose, payloadWords, langId, encrypted } — prose/payloadWords are
// for rendering with payload words underlined.
export async function encodeMessage(message, passphrase, langId = 'english') {
  const lang = msgLangById(langId);
  const { data: reduced, flag } = await maybeReduce(TE.encode(message));

  if (!passphrase) {
    const ctHex = toHex(buildEmbedded(flag, reduced));
    const r = JSON.parse(wasmEncodeRawBaseN(ctHex, lang.language, lang.wordlist, lang.dialect, SEED));
    if (r.error) throw new Error(r.error);
    const prose = (r.encoded_text || '').trim();
    return { artifact: prose, prose, header: null, payloadWords: r.payload_words || [], langId: lang.id, encrypted: false };
  }

  const salt = crypto.getRandomValues(new Uint8Array(AEAD_SALT_LEN));
  const { key, nonce } = await deriveKeyNonce(passphrase, salt, 'AES-GCM');
  const sealed = new Uint8Array(await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv: nonce, tagLength: AEAD_TAG_BITS }, key, reduced));
  const ct = sealed.subarray(0, sealed.length - AEAD_TAG_LEN);
  const tag = sealed.subarray(sealed.length - AEAD_TAG_LEN);
  if (ct.length > AEAD_MAX_CTLEN) throw new Error('message too long');

  // body prose = ciphertext, in the chosen language
  const bodyR = JSON.parse(wasmEncodeRawBaseN(toHex(ct), lang.language, lang.wordlist, lang.dialect, SEED));
  if (bodyR.error) throw new Error(bodyR.error);
  const body = (bodyR.encoded_text || '').trim();

  // trailer = plumbing bytes -> Latin payload words (capitalized, attribution-like)
  const tb = new Uint8Array(AEAD_TRAILER_LEN);
  const field0 = ((flag & 0x03) << 14) | ct.length;
  tb[0] = (field0 >> 8) & 0xff;
  tb[1] = field0 & 0xff;
  tb.set(salt, 2);
  tb.set(tag, 2 + AEAD_SALT_LEN);
  const trR = JSON.parse(wasmEncodeRawBaseN(toHex(tb), 'latin', 'default', 'body', SEED));
  if (trR.error) throw new Error(trR.error);
  const trailerWords = trR.payload_words || [];
  const trailer = capWords(trailerWords.join(' '));

  const artifact = body + EMDASH + trailer;
  return {
    artifact, prose: artifact, header: null,
    payloadWords: (bodyR.payload_words || []).concat(trailerWords),
    langId: lang.id, encrypted: true, authenticated: true,
  };
}

// Detect the language of some prose, restricted to MSG_LANGS. Falls back to
// english. Detects on the prose value, not the base64 key.
export function detectLang(prose) {
  try {
    const matches = JSON.parse(wasmDetectDialect(prose));
    if (Array.isArray(matches)) {
      const best = matches.find(m => MSG_LANGS.some(l => l.language === m.language));
      if (best) return (MSG_LANGS.find(l => l.language === best.language) || {}).id || 'english';
    }
  } catch (e) { /* fall through */ }
  return 'english';
}

// artifact string + passphrase -> { message, prose, header, payloadWords,
// langId, encrypted, authenticated }. Throws on malformed input; for the
// authenticated form a wrong passphrase or tampering throws cleanly.
export async function decodeMessage(artifact, passphrase) {
  const text = (artifact || '').trim();

  // Authenticated form: "<prose> — <latin attribution>".
  const di = text.lastIndexOf(EMDASH);
  if (di > 0) return aeadDecodeMessage(text.slice(0, di).trim(), text.slice(di + EMDASH.length).trim(), passphrase);

  // Legacy / unencrypted forms.
  const { header, prose } = parseArtifact(text);
  if (!prose) throw new Error('empty artifact');
  const lang = msgLangById(detectLang(prose));

  if (!header) {
    // Unencrypted: pass 0 so the codec returns every byte; the embedded length
    // slices off any trailing bit-pad. No passphrase required.
    const r = JSON.parse(wasmDecodeRawBaseN(prose, lang.language, lang.wordlist, 0));
    if (r.error) throw new Error(r.error);
    const bytes = fromHex(r.decoded_hex || '');
    if (!bytes.length) throw new Error('empty payload');
    const { flag, data } = parseEmbedded(bytes);
    const message = TD.decode(await expand(data, flag));
    return { message, prose, header: null, payloadWords: r.payload_words || [], langId: lang.id, encrypted: false };
  }

  // Encrypted: key carries flag + ciphertext length + salt.
  const hdr = parseHeader(header);
  const r = JSON.parse(wasmDecodeRawBaseN(prose, lang.language, lang.wordlist, hdr.ctlen));
  if (r.error) throw new Error(r.error);
  const ctBytes = fromHex(r.decoded_hex || '');
  if (!ctBytes.length) throw new Error('empty ciphertext');
  if (!passphrase) {
    const e = new Error('passphrase required'); e.needsPassphrase = true; throw e;
  }
  const ct = ctBytes.subarray(0, hdr.ctlen);
  const { key, nonce } = await deriveKeyNonce(passphrase, hdr.salt, 'AES-CTR');
  let message;
  try {
    const plain = new Uint8Array(await crypto.subtle.decrypt(
      { name: 'AES-CTR', counter: ctrCounter(nonce), length: 32 }, key, ct));
    message = TD.decode(await expand(plain, hdr.flag));
  } catch (e) {
    throw new Error('Could not decode — check the passphrase.');
  }
  return { message, prose, header, payloadWords: r.payload_words || [], langId: lang.id, encrypted: true };
}

// Decode the authenticated "<body> — <attribution>" form. GCM verifies the tag,
// so a wrong passphrase or any tampering throws rather than returning garbage.
async function aeadDecodeMessage(body, trailer, passphrase) {
  // attribution (Latin) -> 27 plumbing bytes (lowercased so capitalization is ignored)
  const tR = JSON.parse(wasmDecodeRawBaseN(trailer.toLowerCase(), 'latin', 'default', AEAD_TRAILER_LEN));
  if (tR.error) throw new Error(tR.error);
  const tb = fromHex(tR.decoded_hex || '');
  if (tb.length < AEAD_TRAILER_LEN) throw new Error('bad attribution trailer');
  const field0 = (tb[0] << 8) | tb[1];
  const flag = field0 >> 14;
  const ctlen = field0 & AEAD_MAX_CTLEN;
  const salt = tb.subarray(2, 2 + AEAD_SALT_LEN);
  const tag = tb.subarray(2 + AEAD_SALT_LEN, AEAD_TRAILER_LEN);

  // body prose -> ciphertext, in its detected language
  const lang = msgLangById(detectLang(body));
  const bR = JSON.parse(wasmDecodeRawBaseN(body, lang.language, lang.wordlist, ctlen));
  if (bR.error) throw new Error(bR.error);
  const ct = fromHex(bR.decoded_hex || '').subarray(0, ctlen);
  if (!ct.length) throw new Error('empty ciphertext');
  if (!passphrase) { const e = new Error('passphrase required'); e.needsPassphrase = true; throw e; }

  const { key, nonce } = await deriveKeyNonce(passphrase, salt, 'AES-GCM');
  const sealed = new Uint8Array(ct.length + AEAD_TAG_LEN);
  sealed.set(ct, 0);
  sealed.set(tag, ct.length);
  let message;
  try {
    const plain = new Uint8Array(await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: nonce, tagLength: AEAD_TAG_BITS }, key, sealed));
    message = TD.decode(await expand(plain, flag));
  } catch (e) {
    throw new Error('Could not decrypt — wrong passphrase, or the message was tampered with.');
  }
  return {
    message, prose: body + EMDASH + trailer, header: null,
    payloadWords: (bR.payload_words || []).concat(tR.payload_words || []),
    langId: lang.id, encrypted: true, authenticated: true,
  };
}
