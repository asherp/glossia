// glossia-msg.js — message → encrypted Glossia artifact, as an ES module.
//
// Shared content pipeline for the demo panel (index.html) and the bulletin
// board (compose.html / bulletin.html):
//
//   encode: message + credential -> reduce -> AES-256-GCM -> "<prose> — <attribution>"
//   decode: "<prose> — <attribution>" + credential -> verify + decrypt -> message
//
// The CREDENTIAL is polymorphic:
//   • a passphrase string  -> key+nonce via PBKDF2-SHA-256 (200k) — the demo's
//     human-typed symmetric password.
//   • a 32-byte Uint8Array -> key+nonce via HKDF-SHA-256 — the board "read key"
//     (derived from the signing key in glossia-nostr.js); already high-entropy,
//     so no slow stretching is needed.
// The on-the-wire format is identical either way; the derivation is chosen by
// credential type, and each flow uses one type consistently, so decoding never
// needs to record which was used.
//
// With NO credential the bytes are compressed and encoded but not encrypted; a
// [flag][len] header rides inside the prose and the artifact is bare prose,
// readable by anyone. AES-256-GCM is authenticated: a wrong credential or any
// tampering fails cleanly.
//
// The glossia WASM (encode_raw_base_n / decode_raw_base_n) is loaded from the
// same ./glossia.js bundle; call init() once first.

import init, {
  encode_raw_base_n as wasmEncodeRawBaseN,
  decode_raw_base_n as wasmDecodeRawBaseN,
  detect_dialect_from_text as wasmDetectDialect,
} from './glossia.js';

export { init };

const SEED = 42n;               // fixed seed -> deterministic prose

// Languages this pipeline can render into / detect from.
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

// ─── per-message AES-256-GCM key + nonce, from a credential + salt ─────
// Both key and nonce are derived (so the nonce is never transmitted); a fresh
// random salt per message keeps every (key, nonce) unique. A passphrase string
// is stretched with PBKDF2; a 32-byte key is expanded with HKDF (no stretching
// needed — it is already high-entropy).
function hasCred(c) { return (typeof c === 'string' && c.length > 0) || (c instanceof Uint8Array && c.length > 0); }

async function deriveKeyNonce(cred, salt) {
  let bits;
  if (cred instanceof Uint8Array) {
    const base = await crypto.subtle.importKey('raw', cred, 'HKDF', false, ['deriveBits']);
    bits = new Uint8Array(await crypto.subtle.deriveBits(
      { name: 'HKDF', hash: 'SHA-256', salt, info: TE.encode('glossia/aead/v1') }, base, (32 + 12) * 8));
  } else {
    const base = await crypto.subtle.importKey('raw', TE.encode(cred), 'PBKDF2', false, ['deriveBits']);
    bits = new Uint8Array(await crypto.subtle.deriveBits(
      { name: 'PBKDF2', salt, iterations: 200000, hash: 'SHA-256' }, base, (32 + 12) * 8));
  }
  const key = await crypto.subtle.importKey('raw', bits.subarray(0, 32), { name: 'AES-GCM' }, false, ['encrypt', 'decrypt']);
  return { key, nonce: bits.subarray(32, 44) };
}

// ─── varint + embedded header (unencrypted path) ──────────────────────
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

// ─── authenticated artifact: "<prose> — <latin attribution>" ──────────
//
// The encrypted artifact reads as a quote with an attribution. The prose IS the
// AES-256-GCM ciphertext; the em-dash trailer is the plumbing — flag + length +
// salt + 96-bit auth tag — rendered as ~11 Latin payload words, so it scans like
// "— Cornelius Vanto Brixia". GCM authenticates: a wrong credential or a tampered
// message fails cleanly instead of yielding garbage.
//
//   trailer bytes: [flag:2b | length:14b : 2 BE][salt : 6][GCM tag : 12] = 20
//
// The top 2 bits of the length field carry the reduction method; the em-dash
// never appears in encoded prose, so it alone signals the format.
const AEAD_SALT_LEN = 6;
const AEAD_TAG_BITS = 96;
const AEAD_TAG_LEN = AEAD_TAG_BITS / 8;     // 12 bytes
const AEAD_MAX_CTLEN = 0x3fff;              // 14-bit length
const AEAD_TRAILER_LEN = 2 + AEAD_SALT_LEN + AEAD_TAG_LEN;   // 20 bytes -> ~11 Latin words
export const EMDASH = ' — ';

function capWords(s) { return s.replace(/(^|\s)(\p{L})/gu, (_, sp, c) => sp + c.toUpperCase()); }

// Slim a body's prose down to just its payload words, in order — dropping the
// cover words. The decoder filters prose against the wordlist, so the result
// still decodes to the same bytes; payload words are lowercased to their
// canonical wordlist form. Used for the cover-off view (see renderArtifact).
function payloadOnlyProse(prose, words) {
  const set = new Set((words || []).map(w => w.toLowerCase()));
  return prose.split(/\s+/)
    .map(t => t.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ''))
    .filter(t => t && set.has(t.toLowerCase()))
    .map(t => t.toLowerCase())
    .join(' ');
}

// ─── public API ───────────────────────────────────────────────────────

// Phase 1 — encrypt (or just pack, with no credential) into an opaque, language-
// independent cipher state. Render it into any language with renderArtifact, as
// often as you like, without re-encrypting. `cred` is a passphrase string or a
// 32-byte key (Uint8Array); falsy/empty means do not encrypt.
// Returns { encrypted, ctHex, trailerHex }.
export async function sealMessage(message, cred) {
  const { data: reduced, flag } = await maybeReduce(TE.encode(message));
  if (!hasCred(cred)) {
    return { encrypted: false, ctHex: toHex(buildEmbedded(flag, reduced)), trailerHex: null };
  }
  const salt = crypto.getRandomValues(new Uint8Array(AEAD_SALT_LEN));
  const { key, nonce } = await deriveKeyNonce(cred, salt);
  const sealed = new Uint8Array(await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv: nonce, tagLength: AEAD_TAG_BITS }, key, reduced));
  const ct = sealed.subarray(0, sealed.length - AEAD_TAG_LEN);
  const tag = sealed.subarray(sealed.length - AEAD_TAG_LEN);
  if (ct.length > AEAD_MAX_CTLEN) throw new Error('message too long');
  // trailer = [flag:2b | length:14b][salt][tag]
  const tb = new Uint8Array(AEAD_TRAILER_LEN);
  const field0 = ((flag & 0x03) << 14) | ct.length;
  tb[0] = (field0 >> 8) & 0xff;
  tb[1] = field0 & 0xff;
  tb.set(salt, 2);
  tb.set(tag, 2 + AEAD_SALT_LEN);
  return { encrypted: true, ctHex: toHex(ct), trailerHex: toHex(tb) };
}

// Phase 2 — render a sealed state into prose in the chosen language. Encrypted
// states become "<body> — <latin attribution>"; unencrypted ones are bare prose.
// The body and trailer are returned split out (with their payload words) so
// callers can style and underline each independently.
//
// `cover` (default true) fills the body's grammar with cover words for natural
// prose. Set it false to emit only the payload words — a much shorter body that
// still decodes (the decoder filters prose against the wordlist either way), so
// a bulletin can be slimmed to fit tight length limits. The Latin trailer is
// already just its payload words, so it is unaffected.
//
// The body is ALWAYS encoded with the full grammar (lang.dialect): the base-n
// codec is grammar-controlled (payload words differ per dialect, and the decoder
// always decodes with the "body" grammar), so re-encoding with a bare dialect
// would change the payload words. Instead, cover-off just drops the cover words
// from the already-generated prose — the payload words are byte-identical either
// way (mirrors index.html's cover toggle, which re-renders rather than re-encodes).
export function renderArtifact(state, langId = 'english', { cover = true } = {}) {
  const lang = msgLangById(langId);
  const bodyR = JSON.parse(wasmEncodeRawBaseN(state.ctHex, lang.language, lang.wordlist, lang.dialect, SEED));
  if (bodyR.error) throw new Error(bodyR.error);
  const bodyWords = bodyR.payload_words || [];
  const body = cover ? (bodyR.encoded_text || '').trim() : payloadOnlyProse(bodyR.encoded_text || '', bodyWords);
  if (!state.encrypted) {
    return { artifact: body, prose: body, body, trailer: '', bodyWords, trailerWords: [], payloadWords: bodyWords, langId: lang.id, encrypted: false };
  }
  // trailer plumbing -> Latin payload words (capitalized, attribution-like)
  const trR = JSON.parse(wasmEncodeRawBaseN(state.trailerHex, 'latin', 'default', 'body', SEED));
  if (trR.error) throw new Error(trR.error);
  const trailerWords = trR.payload_words || [];
  const trailer = capWords(trailerWords.join(' '));
  const artifact = body + EMDASH + trailer;
  return { artifact, prose: artifact, body, trailer, bodyWords, trailerWords, payloadWords: bodyWords.concat(trailerWords), langId: lang.id, encrypted: true, authenticated: true };
}

// Convenience: seal + render in one step. `opts` is forwarded to renderArtifact
// (e.g. { cover: false } to publish a slimmed, payload-only body).
export async function encodeMessage(message, cred, langId = 'english', opts = {}) {
  return renderArtifact(await sealMessage(message, cred), langId, opts);
}

// Detect the language of some prose, restricted to MSG_LANGS. Falls back to english.
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

// ─── seed-phrase paragraph: a raw key ⇆ readable Glossia prose ────────
// A "seed phrase" is a raw key rendered as natural-language prose whose payload
// words carry the key's bytes — the project's core idea applied to a private
// key. It uses the same word-preserving base-n codec as the demo's BIP39 panel,
// so the bytes round-trip exactly (decoding filters the prose back against the
// wordlist). Callers append a checksum to the key before encoding (see
// glossia-nostr.js) so a mistyped word is caught on load.

// hex string (any byte length) -> { prose, payloadWords, langId }.
export function encodeSeedPhrase(hex, langId = 'english') {
  const lang = msgLangById(langId);
  const r = JSON.parse(wasmEncodeRawBaseN(hex, lang.language, lang.wordlist, lang.dialect, SEED));
  if (r.error) throw new Error(r.error);
  return { prose: (r.encoded_text || '').trim(), payloadWords: r.payload_words || [], langId: lang.id };
}

// prose paragraph + known byte length -> { hex, payloadWords, langId }. Decodes
// in the given language, or the one auto-detected from the prose.
export function decodeSeedPhrase(prose, byteCount, langId) {
  const text = (prose || '').trim();
  if (!text) throw new Error('empty seed phrase');
  const lang = msgLangById(langId || detectLang(text));
  const r = JSON.parse(wasmDecodeRawBaseN(text, lang.language, lang.wordlist, byteCount));
  if (r.error) throw new Error(r.error);
  return { hex: r.decoded_hex || '', payloadWords: r.payload_words || [], langId: lang.id };
}

// artifact string + credential -> { message, prose, payloadWords, langId,
// encrypted, authenticated }. Throws on malformed input; for the authenticated
// form a wrong credential or tampering throws cleanly.
export async function decodeMessage(artifact, cred) {
  const text = (artifact || '').trim();

  // Authenticated form: "<prose> — <latin attribution>".
  const di = text.lastIndexOf(EMDASH);
  if (di > 0) return aeadDecodeMessage(text.slice(0, di).trim(), text.slice(di + EMDASH.length).trim(), cred);

  // Unencrypted bare prose: the [flag][len] header rides inside the payload.
  if (!text) throw new Error('empty artifact');
  const lang = msgLangById(detectLang(text));
  const r = JSON.parse(wasmDecodeRawBaseN(text, lang.language, lang.wordlist, 0));
  if (r.error) throw new Error(r.error);
  const bytes = fromHex(r.decoded_hex || '');
  if (!bytes.length) throw new Error('empty payload');
  const { flag, data } = parseEmbedded(bytes);
  const message = TD.decode(await expand(data, flag));
  return { message, prose: text, header: null, payloadWords: r.payload_words || [], langId: lang.id, encrypted: false };
}

// Skim an artifact WITHOUT a credential: recover the prose's payload words (and
// split the body from its attribution trailer) so a locked bulletin can still be
// rendered with its payload words highlighted. The prose→word mapping is
// deterministic from the wordlist and the public trailer, so no key is needed —
// only the final AES-GCM step (in decodeMessage) requires the credential.
// Returns { prose, body, trailer, payloadWords, encrypted }. Never throws.
export function skimArtifact(artifact) {
  const text = (artifact || '').trim();
  if (!text) return { prose: text, body: text, trailer: '', payloadWords: [], encrypted: false };

  // Authenticated form: "<body> — <latin attribution>".
  const di = text.lastIndexOf(EMDASH);
  if (di > 0) {
    const body = text.slice(0, di).trim();
    const trailer = text.slice(di + EMDASH.length).trim();
    try {
      const tR = JSON.parse(wasmDecodeRawBaseN(trailer.toLowerCase(), 'latin', 'default', AEAD_TRAILER_LEN));
      if (tR.error) throw new Error(tR.error);
      const tb = fromHex(tR.decoded_hex || '');
      if (tb.length < AEAD_TRAILER_LEN) throw new Error('bad trailer');
      const ctlen = ((tb[0] << 8) | tb[1]) & AEAD_MAX_CTLEN;
      const lang = msgLangById(detectLang(body));
      const bR = JSON.parse(wasmDecodeRawBaseN(body, lang.language, lang.wordlist, ctlen));
      const words = (bR.payload_words || []).concat(tR.payload_words || []);
      return { prose: text, body, trailer, payloadWords: words, encrypted: true };
    } catch { return { prose: text, body, trailer, payloadWords: [], encrypted: true }; }
  }

  // Bare, unencrypted prose.
  try {
    const lang = msgLangById(detectLang(text));
    const r = JSON.parse(wasmDecodeRawBaseN(text, lang.language, lang.wordlist, 0));
    return { prose: text, body: text, trailer: '', payloadWords: r.payload_words || [], encrypted: false };
  } catch { return { prose: text, body: text, trailer: '', payloadWords: [], encrypted: false }; }
}

// Decode the authenticated "<body> — <attribution>" form. GCM verifies the tag,
// so a wrong credential or any tampering throws rather than returning garbage.
async function aeadDecodeMessage(body, trailer, cred) {
  // attribution (Latin) -> plumbing bytes (lowercased so capitalization is ignored)
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
  if (!hasCred(cred)) { const e = new Error('decryption key required'); e.needsKey = true; e.needsPassphrase = true; throw e; }

  const { key, nonce } = await deriveKeyNonce(cred, salt);
  const sealed = new Uint8Array(ct.length + AEAD_TAG_LEN);
  sealed.set(ct, 0);
  sealed.set(tag, ct.length);
  let message;
  try {
    const plain = new Uint8Array(await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: nonce, tagLength: AEAD_TAG_BITS }, key, sealed));
    message = TD.decode(await expand(plain, flag));
  } catch (e) {
    throw new Error('Could not decrypt — wrong key/passphrase, or the message was tampered with.');
  }
  return {
    message, prose: body + EMDASH + trailer, header: null,
    payloadWords: (bR.payload_words || []).concat(tR.payload_words || []),
    langId: lang.id, encrypted: true, authenticated: true,
  };
}
