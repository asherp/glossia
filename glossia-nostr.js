// glossia-nostr.js — minimal nostr identity + event + relay layer for the
// Glossia bulletin board, built on the vendored @noble crypto in ./vendor/noble.
//
// Design: a single signing key roots a three-tier capability hierarchy.
//   signing key d  --schnorr--> npub                         (publish + identity)
//                  --SHA256(domain‖d)--> read key K          (decrypt)
//   npub = d·G                                               (locate + verify only)
//
//   • nsec (d)      → publish AND decrypt (can compute K)
//   • read key (K)  → decrypt only (one-way hash → can't recover d → can't sign)
//   • npub          → read the prose / verify signatures, but can't decrypt
//
// d can be random (save the nsec) or passphrase-derived (deterministic npub via
// a FIXED domain salt). The content key is then K = deriveContentKey(d), so the
// user never has to choose a separate encryption password.

import { schnorr, secp256k1 } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes, concatBytes } from '@noble/hashes/utils';

const TE = new TextEncoder();

// App-specific regular event kind (1000..9999 => relays store every one,
// append-only, and generic clients ignore it instead of showing it in feeds).
export const GLOSSIA_KIND = 1314;

export const DEFAULT_RELAYS = [
  'wss://relay.damus.io',
  'wss://nos.lol',
  'wss://relay.primal.net',
];

// ─── bech32 (BIP-173) for NIP-19 npub/nsec ────────────────────────────
const BECH32_CHARS = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
const BECH32_GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

function bech32Polymod(values) {
  let chk = 1;
  for (const v of values) {
    const top = chk >> 25;
    chk = ((chk & 0x1ffffff) << 5) ^ v;
    for (let i = 0; i < 5; i++) if ((top >> i) & 1) chk ^= BECH32_GEN[i];
  }
  return chk;
}
function bech32HrpExpand(hrp) {
  const out = [];
  for (let i = 0; i < hrp.length; i++) out.push(hrp.charCodeAt(i) >> 5);
  out.push(0);
  for (let i = 0; i < hrp.length; i++) out.push(hrp.charCodeAt(i) & 31);
  return out;
}
function bech32Checksum(hrp, data) {
  const values = bech32HrpExpand(hrp).concat(data, [0, 0, 0, 0, 0, 0]);
  const mod = bech32Polymod(values) ^ 1;
  const out = [];
  for (let i = 0; i < 6; i++) out.push((mod >> (5 * (5 - i))) & 31);
  return out;
}
// Regroup a byte/5-bit stream between bit widths (8<->5).
function convertBits(data, from, to, pad) {
  let acc = 0, bits = 0;
  const ret = [];
  const maxv = (1 << to) - 1;
  for (const value of data) {
    if (value < 0 || value >> from) throw new Error('bech32: value out of range');
    acc = (acc << from) | value;
    bits += from;
    while (bits >= to) { bits -= to; ret.push((acc >> bits) & maxv); }
  }
  if (pad) { if (bits > 0) ret.push((acc << (to - bits)) & maxv); }
  else if (bits >= from || ((acc << (to - bits)) & maxv)) throw new Error('bech32: bad padding');
  return ret;
}
function bech32Encode(hrp, data5) {
  const combined = data5.concat(bech32Checksum(hrp, data5));
  let s = hrp + '1';
  for (const d of combined) s += BECH32_CHARS[d];
  return s;
}
function bech32Decode(str) {
  const lower = str.toLowerCase();
  if (str !== lower && str !== str.toUpperCase()) throw new Error('bech32: mixed case');
  const s = lower;
  const pos = s.lastIndexOf('1');
  if (pos < 1 || pos + 7 > s.length) throw new Error('bech32: bad separator');
  const hrp = s.slice(0, pos);
  const data = [];
  for (let i = pos + 1; i < s.length; i++) {
    const d = BECH32_CHARS.indexOf(s[i]);
    if (d === -1) throw new Error('bech32: invalid char');
    data.push(d);
  }
  if (bech32Polymod(bech32HrpExpand(hrp).concat(data)) !== 1) throw new Error('bech32: bad checksum');
  return { hrp, data5: data.slice(0, -6) };
}

// 32-byte hex <-> bech32 entity (npub / nsec).
function encodeBech32Entity(hrp, hex) {
  return bech32Encode(hrp, convertBits(hexToBytes(hex), 8, 5, true));
}
function decodeBech32Entity(expectedHrp, str) {
  const { hrp, data5 } = bech32Decode(str);
  if (hrp !== expectedHrp) throw new Error(`expected ${expectedHrp}, got ${hrp}`);
  const bytes = Uint8Array.from(convertBits(data5, 5, 8, false));
  if (bytes.length !== 32) throw new Error('bech32: expected 32 bytes');
  return bytesToHex(bytes);
}

export function npubEncode(pubHex) { return encodeBech32Entity('npub', pubHex); }
export function nsecEncode(secHex) { return encodeBech32Entity('nsec', secHex); }
export function npubToHex(npub) { return decodeBech32Entity('npub', npub.trim()); }
export function nsecToHex(nsec) { return decodeBech32Entity('nsec', nsec.trim()); }

export function isNpub(s) {
  try { npubToHex(s); return true; } catch { return false; }
}

// The board "write key": the 32-byte signing key, shared as nwrite1…. It is the
// SAME secret as the nostr nsec (byte-identical, just a different bech32 HRP), so
// it can still be imported into any nostr client as an nsec. The nwrite badge
// lets the write key sit alongside nread in Glossia's capability naming:
//   nwrite → publish AND decrypt · nread → decrypt only · npub → read prose only.
export function nwriteEncode(secHex) { return encodeBech32Entity('nwrite', secHex); }
export function nwriteToHex(nwrite) { return decodeBech32Entity('nwrite', nwrite.trim()); }
export function isNwrite(s) { try { nwriteToHex(s); return true; } catch { return false; } }

// The board "read key": a 32-byte symmetric content key, shared as nread1….
// Holding it grants decrypt-only access — it cannot publish, since you can't
// recover the signing key from it.
export function nreadEncode(keyHex) { return encodeBech32Entity('nread', keyHex); }
export function nreadToHex(nread) { return decodeBech32Entity('nread', nread.trim()); }
export function isNread(s) { try { nreadToHex(s); return true; } catch { return false; } }
// A 32-byte read key (Uint8Array) -> its nread1… string.
export function nreadFromKey(readKey) { return nreadEncode(bytesToHex(readKey)); }

// ─── identity: passphrase -> deterministic secp256k1 keypair ──────────
const IDENTITY_SALT = TE.encode('glossia/nostr-identity/v1');
const CONTENT_KEY_DOMAIN = TE.encode('glossia/content-key/v1');
const SECP_N = secp256k1.CURVE.n;

// The content (read) key is the symmetric encryption key, derived one-way from
// the signing key with domain separation. Holding the signing key (nsec) lets
// you both sign (publish) and compute this key (decrypt); holding only this key
// (nread) lets you decrypt but not sign; holding only the npub lets you do
// neither — you can't reach the read key without the signing key.
export function deriveContentKey(sk) {
  return sha256(concatBytes(CONTENT_KEY_DOMAIN, sk));   // 32 bytes
}

function bigToBytes32(n) {
  let hex = n.toString(16);
  if (hex.length > 64) throw new Error('scalar too large');
  hex = hex.padStart(64, '0');
  return hexToBytes(hex);
}

// Wrap a raw 32-byte secret key into the identity object the rest of the module
// uses. The secret stays in memory only — never serialized into events.
export function identityFromSk(sk) {
  if (!(sk instanceof Uint8Array) || sk.length !== 32) throw new Error('secret key must be 32 bytes');
  const pubHex = bytesToHex(schnorr.getPublicKey(sk));   // 32-byte x-only (nostr pubkey)
  const readKey = deriveContentKey(sk);
  return {
    sk,
    pubHex,
    secHex: bytesToHex(sk),
    npub: npubEncode(pubHex),
    nsec: nsecEncode(bytesToHex(sk)),
    nwrite: nwriteEncode(bytesToHex(sk)),    // same secret as nsec, Glossia-badged
    readKey,                                 // Uint8Array(32) — symmetric content key
    readKeyHex: bytesToHex(readKey),
    nread: nreadEncode(bytesToHex(readKey)),
  };
}

// ─── seed phrase: a board's keys ⇆ a checksummed hex payload ───────────
// A board can be "saved" as a Glossia seed phrase — its keys rendered as readable
// prose (the prose rendering lives in glossia-msg.js). We append a short checksum
// so a transcription error (a mistyped word) is caught on load instead of silently
// restoring a different board. The PAYLOAD LENGTH selects the layout:
//   36 = [signing key : 32][checksum : 4]                 — signing key only
//   68 = [signing key : 32][read key : 32][checksum : 4]  — + a custom read key
// A custom (passphrase-derived) read key can't be recovered from the signing key,
// so when one is set it rides along in the seed; loading reads the length to tell
// the two apart. The checksum always covers everything before it.
const SEED_CHECK_LEN = 4;
export const SEED_PAYLOAD_LEN = 32 + SEED_CHECK_LEN;              // 36: signing key only
export const SEED_PAYLOAD_LEN_EXT = 32 + 32 + SEED_CHECK_LEN;    // 68: + custom read key

function seedChecksum(material) { return sha256(material).subarray(0, SEED_CHECK_LEN); }
function bytesEq(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

// The checksummed seed payload (hex) for an identity — feed to encodeSeedPhrase.
// Pass a 32-byte custom read key to embed it too (the extended, 68-byte layout).
export function seedPayloadHex(identity, readKey = null) {
  const sk = identity.sk;
  const material = (readKey && readKey.length === 32) ? concatBytes(sk, readKey) : sk;
  return bytesToHex(concatBytes(material, seedChecksum(material)));
}

// Parse a checksummed seed payload (hex) back into a board. Returns
// { identity, readKey } — readKey is a Uint8Array(32) for the extended layout, or
// null for the signing-key-only layout. The decoder may append a byte or two of
// bit-pack padding, so the payload bytes are read as a PREFIX and the layout is
// chosen by length (extended first) and confirmed by the checksum. Throws if
// neither layout's checksum verifies (a mistyped / garbled / wrong-language seed).
export function parseSeedPayloadHex(hex) {
  const bytes = hexToBytes((hex || '').trim().toLowerCase());
  // extended (longer, more specific): [sk:32][readKey:32][sum:4]
  if (bytes.length >= SEED_PAYLOAD_LEN_EXT) {
    const material = bytes.subarray(0, 64);
    if (bytesEq(bytes.subarray(64, SEED_PAYLOAD_LEN_EXT), seedChecksum(material))) {
      return { identity: identityFromSk(bytes.slice(0, 32)), readKey: bytes.slice(32, 64) };
    }
  }
  // base: [sk:32][sum:4]
  if (bytes.length >= SEED_PAYLOAD_LEN) {
    const sk = bytes.slice(0, 32);
    if (bytesEq(bytes.subarray(32, SEED_PAYLOAD_LEN), seedChecksum(sk))) {
      return { identity: identityFromSk(sk), readKey: null };
    }
  }
  throw new Error('seed phrase checksum failed');
}

// Back-compat: just the identity from a seed payload (drops any custom read key).
export function identityFromSeedPayloadHex(hex) {
  return parseSeedPayloadHex(hex).identity;
}

// Bring an existing nostr publishing key (nsec or 64-char hex) as the board's
// identity — used by the TWO-KEY model, where the publish key is independent of
// the decrypt passphrase.
export function identityFromNsec(nsec) { return identityFromSk(hexToBytes(nsecToHex(nsec))); }
export function identityFromNwrite(nwrite) { return identityFromSk(hexToBytes(nwriteToHex(nwrite))); }
export function identityFromSecHex(secHex) {
  const clean = secHex.trim().toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(clean)) throw new Error('secret key must be 64 hex chars');
  return identityFromSk(hexToBytes(clean));
}

// Generate a fresh random publishing identity (no passphrase). Pair it with a
// separate decrypt passphrase for the TWO-KEY model; save the nsec to post again.
export function generateIdentity() {
  const sk = new Uint8Array(32);
  crypto.getRandomValues(sk);
  // Map into [1, n-1] so it is always a valid key.
  const scalar = (BigInt('0x' + bytesToHex(sk)) % (SECP_N - 1n)) + 1n;
  return identityFromSk(bigToBytes32(scalar));
}

// Derive the board identity deterministically from a passphrase (SINGLE-KEY
// model uses the same passphrase here and for content encryption). The 256-bit
// PBKDF2 output is mapped into [1, n-1] so it is always a valid secp256k1 key
// (the reduction only matters with negligible ~2^-128 probability).
export async function deriveIdentity(passphrase) {
  if (!passphrase) throw new Error('passphrase required');
  const baseKey = await crypto.subtle.importKey('raw', TE.encode(passphrase), 'PBKDF2', false, ['deriveBits']);
  const bits = new Uint8Array(await crypto.subtle.deriveBits(
    { name: 'PBKDF2', salt: IDENTITY_SALT, iterations: 200000, hash: 'SHA-256' }, baseKey, 256));
  const scalar = (BigInt('0x' + bytesToHex(bits)) % (SECP_N - 1n)) + 1n;
  return identityFromSk(bigToBytes32(scalar));
}

// Resolve a viewer's decryption credential to the 32-byte content key:
//   • nwrite -> derive the read key from the signing key (full-access holder)
//   • nsec   -> same, for a raw nostr key or an older author link
//   • nread  -> the read key itself (read-only holder)
//   • else   -> treat as a passphrase: derive the board identity, then its read
//               key (works for a passphrase-derived board).
// Returns null for empty input.
export async function resolveContentKey(input) {
  const s = (input || '').trim();
  if (!s) return null;
  if (s.startsWith('nwrite')) return deriveContentKey(hexToBytes(nwriteToHex(s)));
  if (s.startsWith('nsec')) return deriveContentKey(hexToBytes(nsecToHex(s)));
  if (s.startsWith('nread')) return hexToBytes(nreadToHex(s));
  return (await deriveIdentity(s)).readKey;
}

// ─── NIP-01 events ────────────────────────────────────────────────────
function serializeEvent(pubHex, created_at, kind, tags, content) {
  // Exact NIP-01 id preimage: [0, pubkey, created_at, kind, tags, content].
  return JSON.stringify([0, pubHex, created_at, kind, tags, content]);
}

export function eventId(ev) {
  return bytesToHex(sha256(utf8ToBytes(serializeEvent(ev.pubkey, ev.created_at, ev.kind, ev.tags, ev.content))));
}

// Build + schnorr-sign a Glossia bulletin event from an identity and content.
export function buildEvent(identity, content, { created_at, subject, kind = GLOSSIA_KIND } = {}) {
  const tags = [['client', 'glossia']];
  if (subject) tags.push(['subject', String(subject)]);
  const ev = {
    pubkey: identity.pubHex,
    created_at: created_at ?? Math.floor(Date.now() / 1000),
    kind,
    tags,
    content,
  };
  ev.id = eventId(ev);
  ev.sig = bytesToHex(schnorr.sign(hexToBytes(ev.id), identity.sk));
  return ev;
}

export function verifyEvent(ev) {
  try {
    if (!ev || ev.id !== eventId(ev)) return false;
    return schnorr.verify(hexToBytes(ev.sig), hexToBytes(ev.id), hexToBytes(ev.pubkey));
  } catch { return false; }
}

// ─── relay client (browser WebSocket) ─────────────────────────────────
function randSubId() {
  const b = new Uint8Array(8);
  crypto.getRandomValues(b);
  return 'glossia-' + bytesToHex(b);
}

// Publish an event to several relays. Resolves to a per-relay result list once
// every relay has answered with OK or timed out — never rejects.
export function publishEvent(ev, relays = DEFAULT_RELAYS, { timeoutMs = 6000 } = {}) {
  return Promise.all(relays.map(url => new Promise(resolve => {
    let done = false;
    const finish = (ok, message) => {
      if (done) return; done = true;
      try { ws.close(); } catch {}
      resolve({ relay: url, ok, message });
    };
    let ws;
    try { ws = new WebSocket(url); } catch (e) { return finish(false, String(e)); }
    const timer = setTimeout(() => finish(false, 'timeout'), timeoutMs);
    ws.onopen = () => ws.send(JSON.stringify(['EVENT', ev]));
    ws.onmessage = (m) => {
      let msg; try { msg = JSON.parse(m.data); } catch { return; }
      if (msg[0] === 'OK' && msg[1] === ev.id) { clearTimeout(timer); finish(!!msg[2], msg[3] || ''); }
      if (msg[0] === 'NOTICE') { /* keep waiting for OK */ }
    };
    ws.onerror = () => { clearTimeout(timer); finish(false, 'connection error'); };
    ws.onclose = () => { clearTimeout(timer); finish(done ? true : false, 'closed'); };
  })));
}

// Query relays for a board's events. Resolves to a deduped, verified, newest-
// first list once all relays send EOSE or time out.
export function queryEvents(filter, relays = DEFAULT_RELAYS, { timeoutMs = 5000 } = {}) {
  const byId = new Map();
  return Promise.all(relays.map(url => new Promise(resolve => {
    const subId = randSubId();
    let ws, done = false;
    const finish = () => { if (done) return; done = true; try { ws.close(); } catch {} resolve(); };
    try { ws = new WebSocket(url); } catch { return resolve(); }
    const timer = setTimeout(finish, timeoutMs);
    ws.onopen = () => ws.send(JSON.stringify(['REQ', subId, filter]));
    ws.onmessage = (m) => {
      let msg; try { msg = JSON.parse(m.data); } catch { return; }
      if (msg[0] === 'EVENT' && msg[1] === subId) {
        const ev = msg[2];
        if (ev && !byId.has(ev.id) && verifyEvent(ev)) byId.set(ev.id, ev);
      } else if (msg[0] === 'EOSE' && msg[1] === subId) {
        clearTimeout(timer); finish();
      }
    };
    ws.onerror = () => { clearTimeout(timer); finish(); };
    ws.onclose = () => { clearTimeout(timer); finish(); };
  }))).then(() => [...byId.values()].sort((a, b) => b.created_at - a.created_at));
}

// Convenience: every bulletin for a board, by npub or hex pubkey.
export function fetchBoard(npubOrHex, relays = DEFAULT_RELAYS, opts = {}) {
  const authors = [npubOrHex.startsWith('npub') ? npubToHex(npubOrHex) : npubOrHex.trim().toLowerCase()];
  return queryEvents({ authors, kinds: [GLOSSIA_KIND], limit: 200 }, relays, opts);
}
