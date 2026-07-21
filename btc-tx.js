// btc-tx.js — a small, dependency-free Bitcoin transaction wire-format
// parser: raw tx hex -> structured fields (version, vin, vout, locktime,
// witness, size/vsize/weight), plus address derivation from scriptPubKey.
//
// This exists to close the loop on Glossia's round trip: once prose decodes
// back to the exact raw transaction bytes, this is what turns those bytes
// into the same structured view a block explorer shows — no explorer API,
// no external library, just the wire format (BIP144/BIP141) and address
// encodings (base58check, bech32/bech32m per BIP173/BIP350).
//
// What this CANNOT recover from a single transaction's bytes: input values
// (and therefore fee), and confirmation status. A transaction only
// references its inputs by prevout txid:vout, not their amount — that value
// lives in whichever earlier transaction created the output — and
// confirmation/block-height is chain state, not transaction data.

function hexToBytes(hex) {
  const clean = hex.trim();
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.substr(i * 2, 2), 16);
  return out;
}
function bytesToHex(bytes) { return Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join(''); }
function reverseBytes(bytes) { return Uint8Array.from(bytes).reverse(); }

class Reader {
  constructor(bytes) { this.bytes = bytes; this.pos = 0; }
  bytesN(n) {
    if (this.pos + n > this.bytes.length) throw new Error('unexpected end of transaction data');
    const b = this.bytes.subarray(this.pos, this.pos + n);
    this.pos += n;
    return b;
  }
  u8() { return this.bytesN(1)[0]; }
  u32le() { const b = this.bytesN(4); return (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0; }
  u64le() {
    const b = this.bytesN(8);
    const lo = (b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)) >>> 0;
    const hi = (b[4] | (b[5] << 8) | (b[6] << 16) | (b[7] << 24)) >>> 0;
    return hi * 0x100000000 + lo;   // safe for realistic sat amounts (< 2^53)
  }
  varint() {
    const first = this.u8();
    if (first < 0xfd) return first;
    if (first === 0xfd) { const b = this.bytesN(2); return b[0] | (b[1] << 8); }
    if (first === 0xfe) return this.u32le();
    return this.u64le();
  }
}

// Raw tx hex -> structured fields. Throws on malformed/truncated input
// (including trailing bytes — the whole buffer must parse as one transaction).
export function parseTransaction(hex) {
  const bytes = hexToBytes(hex);
  const r = new Reader(bytes);
  const version = r.u32le();

  let segwit = false;
  const markerEnd = (() => {
    if (bytes[r.pos] === 0x00 && bytes[r.pos + 1] === 0x01) { segwit = true; r.pos += 2; }
    return r.pos;
  })();

  const vinCount = r.varint();
  const vin = [];
  for (let i = 0; i < vinCount; i++) {
    const txid = bytesToHex(reverseBytes(r.bytesN(32)));   // wire order is internal (reversed) byte order
    const vout = r.u32le();
    const scriptSig = bytesToHex(r.bytesN(r.varint()));
    const sequence = r.u32le();
    vin.push({ txid, vout, scriptSig, sequence, witness: [] });
  }

  const voutCount = r.varint();
  const vout = [];
  for (let i = 0; i < voutCount; i++) {
    const value = r.u64le();
    const scriptPubKey = bytesToHex(r.bytesN(r.varint()));
    vout.push({ value, scriptPubKey });
  }
  const voutEnd = r.pos;

  let witnessBytes = 0;
  const witStart = voutEnd;
  if (segwit) {
    for (let i = 0; i < vinCount; i++) {
      const itemStart = r.pos;
      const itemCount = r.varint();
      const items = [];
      for (let j = 0; j < itemCount; j++) items.push(bytesToHex(r.bytesN(r.varint())));
      vin[i].witness = items;
      // The raw serialized witness stack for this input (item count + each
      // length-prefixed item), so each input's witness can be encoded on its
      // own -- concatenating them all reproduces the whole witnessHex below.
      vin[i].witnessHex = bytesToHex(bytes.subarray(itemStart, r.pos));
    }
    witnessBytes = r.pos - witStart;
  }

  const locktimeStart = r.pos;
  const locktime = r.u32le();
  if (r.pos !== bytes.length) throw new Error(`trailing bytes after transaction (${bytes.length - r.pos} unparsed)`);

  // BIP141 weight: base_size counts once, the segwit marker/flag/witness count 1/4x.
  const size = bytes.length;
  const baseSize = size - (segwit ? 2 + witnessBytes : 0);
  const weight = baseSize * 3 + size;
  const vsize = Math.ceil(weight / 4);

  // The legacy (pre-segwit) serialization — version + vin/vout + locktime, with
  // the marker/flag and witness stripped out. This is exactly what BIP141 hashes
  // (double-SHA256, byte-reversed) to produce the txid, so it's also what
  // computeTxid expects. witnessHex is the raw witness section verbatim, or
  // empty when the transaction carries no witness data.
  const baseHex = bytesToHex(bytes.subarray(0, 4)) + bytesToHex(bytes.subarray(markerEnd, voutEnd)) + bytesToHex(bytes.subarray(locktimeStart, locktimeStart + 4));
  const witnessHex = segwit ? bytesToHex(bytes.subarray(witStart, witStart + witnessBytes)) : '';

  return { version, locktime, segwit, size, vsize, weight, vin, vout, baseHex, witnessHex };
}

// The legacy-serialization bytes (parseTransaction's baseHex) hashed with
// double-SHA256 and byte-reversed, per BIP141 — the transaction's txid,
// computed with no explorer API involved. A mismatch against a claimed txid
// means the bytes don't actually belong to that transaction.
export async function computeTxid(baseHex) {
  return bytesToHex(reverseBytes(await hash256(hexToBytes(baseHex))));
}

// ─── block header parsing ───────────────────────────────────────────────
//
// The 80-byte block header -- version, previous block hash, merkle root,
// timestamp, bits (compact difficulty target), nonce. It never appears
// inside a transaction; a transaction only references its confirming block
// indirectly (a merkle proof), so this is a separate wire format, fetched
// and parsed on its own.

// Raw block header hex -> structured fields. Throws unless it's exactly 80
// bytes -- a header has no variable-length parts to account for a short or
// padded read. prevBlockHash/merkleRoot come back in display (byte-reversed)
// order, same convention as parseTransaction's txid.
export function parseBlockHeader(hex) {
  const bytes = hexToBytes(hex);
  if (bytes.length !== 80) throw new Error(`block header must be 80 bytes, got ${bytes.length}`);
  const r = new Reader(bytes);
  const version = r.u32le();
  const prevBlockHash = bytesToHex(reverseBytes(r.bytesN(32)));
  const merkleRoot = bytesToHex(reverseBytes(r.bytesN(32)));
  const timestamp = r.u32le();
  const bits = r.u32le();
  const nonce = r.u32le();
  return { version, prevBlockHash, merkleRoot, timestamp, bits, nonce };
}

// The header bytes hashed with double-SHA256 and byte-reversed -- a block's
// hash, computed with no explorer API involved (mirrors computeTxid).
export async function computeBlockHash(headerHex) {
  return bytesToHex(reverseBytes(await hash256(hexToBytes(headerHex))));
}

// ─── compact difficulty target (nBits) ──────────────────────────────────
//
// nBits packs a 256-bit target into 4 bytes: the top byte is a byte-length
// exponent, the low 3 bytes a mantissa -- arith_uint256::SetCompact in
// Bitcoin Core. Unpacked two ways: the full 32-byte target (so its leading
// zero bytes -- literally the proof-of-work requirement -- are visible in
// full, unlike a mined hash's they're never dropped here) and a difficulty
// ratio against the genesis block's target.

// nBits -> the 256-bit target, as 64 hex chars (32 bytes, display order).
export function bitsToTargetHex(bits) {
  const exponent = bits >>> 24;
  const mantissa = BigInt(bits & 0x007fffff);   // top bit of the 3-byte mantissa is a sign flag, masked off
  const target = exponent <= 3 ? mantissa >> BigInt(8 * (3 - exponent)) : mantissa << BigInt(8 * (exponent - 3));
  return target.toString(16).padStart(64, '0');
}

// nBits -> difficulty relative to the genesis block's target (defined as
// difficulty 1). Mirrors Bitcoin Core's GetDifficulty: shift by whole bytes
// in floating point rather than dividing the raw 256-bit targets, which
// would overflow a double.
export function bitsToDifficulty(bits) {
  let shift = bits >>> 24;
  let diff = 0x0000ffff / (bits & 0x00ffffff);
  while (shift < 29) { diff *= 256; shift++; }
  while (shift > 29) { diff /= 256; shift--; }
  return diff;
}

// ─── address derivation from scriptPubKey (no external library) ──────────

const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

async function sha256(bytes) { return new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)); }
async function hash256(bytes) { return sha256(await sha256(bytes)); }

async function base58checkEncode(versionByte, payload) {
  const body = new Uint8Array(1 + payload.length);
  body[0] = versionByte;
  body.set(payload, 1);
  const checksum = (await hash256(body)).subarray(0, 4);
  const full = new Uint8Array(body.length + 4);
  full.set(body, 0);
  full.set(checksum, body.length);

  let n = 0n;
  for (const b of full) n = (n << 8n) | BigInt(b);
  let out = '';
  while (n > 0n) { out = BASE58_ALPHABET[Number(n % 58n)] + out; n /= 58n; }
  for (const b of full) { if (b === 0) out = '1' + out; else break; }
  return out || '1';
}

const BECH32_CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
const BECH32_CONST = 1;
const BECH32M_CONST = 0x2bc830a3;

function bech32Polymod(values) {
  const GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
  let chk = 1;
  for (const v of values) {
    const top = chk >>> 25;
    chk = ((chk & 0x1ffffff) << 5) ^ v;
    for (let i = 0; i < 5; i++) if ((top >> i) & 1) chk ^= GEN[i];
  }
  return chk >>> 0;
}
function bech32HrpExpand(hrp) {
  const out = [];
  for (const c of hrp) out.push(c.charCodeAt(0) >> 5);
  out.push(0);
  for (const c of hrp) out.push(c.charCodeAt(0) & 31);
  return out;
}
function convertBits(data, fromBits, toBits, pad) {
  let acc = 0, bits = 0;
  const out = [];
  const maxv = (1 << toBits) - 1;
  for (const value of data) {
    acc = (acc << fromBits) | value;
    bits += fromBits;
    while (bits >= toBits) { bits -= toBits; out.push((acc >> bits) & maxv); }
  }
  if (pad && bits > 0) out.push((acc << (toBits - bits)) & maxv);
  return out;
}
function segwitAddrEncode(hrp, witver, witprogBytes) {
  const data5 = [witver, ...convertBits(Array.from(witprogBytes), 8, 5, true)];
  const constant = witver === 0 ? BECH32_CONST : BECH32M_CONST;
  const values = bech32HrpExpand(hrp).concat(data5, [0, 0, 0, 0, 0, 0]);
  const mod = bech32Polymod(values) ^ constant;
  const checksum = [];
  for (let i = 0; i < 6; i++) checksum.push((mod >> (5 * (5 - i))) & 31);
  return hrp + '1' + data5.concat(checksum).map((d) => BECH32_CHARSET[d]).join('');
}

// scriptPubKey hex -> { type, address }. address is null for scripts with no
// standard address form (OP_RETURN, unrecognized templates).
export async function scriptToAddress(scriptHex) {
  const s = hexToBytes(scriptHex);

  // P2PKH: OP_DUP OP_HASH160 <20> OP_EQUALVERIFY OP_CHECKSIG
  if (s.length === 25 && s[0] === 0x76 && s[1] === 0xa9 && s[2] === 0x14 && s[23] === 0x88 && s[24] === 0xac) {
    return { type: 'p2pkh', address: await base58checkEncode(0x00, s.subarray(3, 23)) };
  }
  // P2SH: OP_HASH160 <20> OP_EQUAL
  if (s.length === 23 && s[0] === 0xa9 && s[1] === 0x14 && s[22] === 0x87) {
    return { type: 'p2sh', address: await base58checkEncode(0x05, s.subarray(2, 22)) };
  }
  // OP_RETURN: no address, carries arbitrary data
  if (s.length > 0 && s[0] === 0x6a) return { type: 'op_return', address: null };
  // Witness programs: OP_0 / OP_1..OP_16 <push:2-40 bytes> <program>
  if (s.length >= 4 && s.length <= 42) {
    const op = s[0];
    const witver = op === 0x00 ? 0 : (op >= 0x51 && op <= 0x60 ? op - 0x50 : -1);
    if (witver >= 0) {
      const pushLen = s[1];
      if (s.length === 2 + pushLen && pushLen >= 2 && pushLen <= 40) {
        const prog = s.subarray(2, 2 + pushLen);
        const type = witver === 0 ? (pushLen === 20 ? 'p2wpkh' : 'p2wsh') : (witver === 1 ? 'p2tr' : `witness_v${witver}`);
        return { type, address: segwitAddrEncode('bc', witver, prog) };
      }
    }
  }
  return { type: 'unknown', address: null };
}

// Attach a derived { type, address } to every output of a parsed transaction.
export async function withAddresses(tx) {
  const vout = [];
  for (const o of tx.vout) vout.push({ ...o, ...(await scriptToAddress(o.scriptPubKey)) });
  return { ...tx, vout };
}

// ─── embedded human-readable text ──────────────────────────────────────
//
// scriptSig and scriptPubKey bytes are usually cryptographic material --
// signatures, pubkeys, hashes -- effectively random. But two spots
// routinely carry deliberate human-readable text: a coinbase input's
// scriptSig (mining pool tags like "/ViaBTC/") and OP_RETURN outputs
// (arbitrary embedded messages, often UTF-8). This scans any byte string for
// runs of readable text, so those can be surfaced instead of run through the
// wordlist as if they were opaque.
//
// "Readable" means valid UTF-8 decoding to printable characters plus tab,
// newline and carriage return -- so non-English text and emoji come through --
// while any other C0/C1 control, a DEL, or an invalid UTF-8 byte ends a run
// (real cryptographic material and image bytes are riddled with those).
//
// A minimum run length matters: a single random byte has roughly a 37%
// chance of falling in the printable ASCII range (0x20-0x7E is 95 of 256
// values), so short "printable-looking" runs turn up by pure chance in
// genuine cryptographic material. Requiring several consecutive readable
// characters keeps that noise out.
const TEXT_MIN_RUN = 5;

// A code point we'll surface as text: any printable character, plus tab, newline
// and carriage return. Excludes the other C0 controls, DEL, and the C1 controls
// (0x80-0x9F) -- all common in binary, rare in genuine text.
const isReadableCp = (cp) => cp === 0x09 || cp === 0x0a || cp === 0x0d || (cp >= 0x20 && cp !== 0x7f && !(cp >= 0x80 && cp <= 0x9f));

// Decode one UTF-8 scalar at offset i -> { cp, len }, or null if the bytes there
// aren't a valid, minimally-encoded UTF-8 sequence. Overlong forms, surrogate
// halves and out-of-range code points are rejected, so this never accepts binary
// that a lenient decoder would paper over with U+FFFD.
function utf8At(bytes, i) {
  const b0 = bytes[i];
  if (b0 < 0x80) return { cp: b0, len: 1 };
  const cont = (j) => i + j < bytes.length && (bytes[i + j] & 0xc0) === 0x80;
  if (b0 >= 0xc2 && b0 <= 0xdf && cont(1)) {
    return { cp: ((b0 & 0x1f) << 6) | (bytes[i + 1] & 0x3f), len: 2 };
  }
  if (b0 >= 0xe0 && b0 <= 0xef && cont(1) && cont(2)) {
    const cp = ((b0 & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f);
    return (cp < 0x800 || (cp >= 0xd800 && cp <= 0xdfff)) ? null : { cp, len: 3 };
  }
  if (b0 >= 0xf0 && b0 <= 0xf4 && cont(1) && cont(2) && cont(3)) {
    const cp = ((b0 & 0x07) << 18) | ((bytes[i + 1] & 0x3f) << 12) | ((bytes[i + 2] & 0x3f) << 6) | (bytes[i + 3] & 0x3f);
    return (cp < 0x10000 || cp > 0x10ffff) ? null : { cp, len: 4 };
  }
  return null;
}

// Parse a script into its push-data segments, ignoring non-push opcodes
// (OP_DUP, OP_HASH160, OP_CHECKSIG, the OP_RETURN marker itself, etc.).
// Scanning each push independently -- rather than the whole script blob --
// matters: a pushdata length-prefix byte is just as likely to fall in the
// printable-ASCII range as any other, and scanning the raw blob glues it
// onto the front of the real text (0x45 = 'E' preceding a 69-byte push,
// for example). Returns null if the bytes don't parse as a clean sequence
// of opcodes (some coinbase scriptSigs are arbitrary, not necessarily
// well-formed script) so the caller can fall back to a raw scan.
function scriptPushes(bytes) {
  const pushes = [];
  let i = 0;
  while (i < bytes.length) {
    const op = bytes[i];
    let len, dataStart;
    if (op >= 0x01 && op <= 0x4b) { len = op; dataStart = i + 1; }
    else if (op === 0x4c) { if (i + 2 > bytes.length) return null; len = bytes[i + 1]; dataStart = i + 2; }
    else if (op === 0x4d) { if (i + 3 > bytes.length) return null; len = bytes[i + 1] | (bytes[i + 2] << 8); dataStart = i + 3; }
    else if (op === 0x4e) {
      if (i + 5 > bytes.length) return null;
      len = (bytes[i + 1] | (bytes[i + 2] << 8) | (bytes[i + 3] << 16) | (bytes[i + 4] << 24)) >>> 0;
      dataStart = i + 5;
    } else { i += 1; continue; }   // non-push opcode -- no associated data to extract
    if (dataStart + len > bytes.length) return null;
    pushes.push(bytes.subarray(dataStart, dataStart + len));
    i = dataStart + len;
  }
  return pushes;
}

// The maximal runs of readable UTF-8 text in a byte string, each at least
// `minRun` characters. An unreadable character or an invalid UTF-8 byte ends the
// current run. When `segment` is set (a whole scriptSig, e.g. a coinbase), each
// script push is scanned independently (see scriptPushes) so a push's
// length-prefix byte never glues onto real text; when it's off (the bytes are
// already a single push's data, e.g. an OP_RETURN payload), the blob is scanned
// as-is -- re-parsing raw data as script would mis-split it, since a byte like
// 0x0a (newline) doubles as a push opcode.
export function findTextRuns(hex, { minRun = TEXT_MIN_RUN, segment = true } = {}) {
  const bytes = hexToBytes(hex);
  const pushes = segment ? scriptPushes(bytes) : null;
  // Scan each script push, or the raw blob when segmentation is off, the bytes
  // aren't clean script (null), or a push-less blob yields no segments.
  const segments = pushes && pushes.length ? pushes : [bytes];
  const found = [];
  for (const seg of segments) {
    let run = '', count = 0;
    const flush = () => { if (count >= minRun) found.push(run); run = ''; count = 0; };
    let i = 0;
    while (i < seg.length) {
      const d = utf8At(seg, i);
      if (d && isReadableCp(d.cp)) { run += String.fromCodePoint(d.cp); count++; i += d.len; }
      else { flush(); i += d ? d.len : 1; }   // skip a readable-but-control char, or one bad byte
    }
    flush();
  }
  return found;
}

// A whole byte string that is valid UTF-8 and entirely readable -> its decoded
// text, else null. Requiring the WHOLE input to pass -- not merely a run within
// it -- keeps keys, hashes and signatures (dense binary) from being mistaken for
// text, so it is safe to apply to any push, not just an OP_RETURN payload.
export function readableUtf8Text(hex) {
  const bytes = hexToBytes(hex);
  let str;
  try { str = new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
  catch { return null; }
  for (const ch of str) if (!isReadableCp(ch.codePointAt(0))) return null;
  return str;
}

// ─── script tokenizer ────────────────────────────────────────────────────
//
// A scriptSig / scriptPubKey (hex) -> an ordered list of tokens, so a caller
// can render it as opcode notation. Each token is one of:
//   { op }            a non-push opcode (its byte)
//   { push, pushForm } a data push (the pushed bytes, hex). pushForm records
//                     which push opcode carried it -- 0 for a direct push
//                     (OP_PUSHBYTES_1..75), or 1/2/4 for OP_PUSHDATA1/2/4 --
//                     so a caller can render the push opcode itself, not just
//                     its data
//   { trunc }         a malformed tail (a push claiming more bytes than remain),
//                     carried verbatim so a caller never crashes on odd bytes
// Byte-exact and lossless: the tokens carry everything needed to reproduce
// the script (a direct push's length prefix is its data's length; a PUSHDATA's
// form is in pushForm).
export function tokenizeScript(hex) {
  const bytes = hexToBytes(hex);
  const toks = [];
  const tail = (i) => toks.push({ trunc: bytesToHex(bytes.subarray(i)) });
  let i = 0;
  while (i < bytes.length) {
    const op = bytes[i];
    if (op >= 0x01 && op <= 0x4b) {                    // direct push of `op` bytes
      const start = i + 1, end = start + op;
      if (end > bytes.length) { tail(i); break; }
      toks.push({ push: bytesToHex(bytes.subarray(start, end)), pushForm: 0 });
      i = end;
    } else if (op === 0x4c || op === 0x4d || op === 0x4e) {   // OP_PUSHDATA1/2/4
      const nlen = op === 0x4c ? 1 : op === 0x4d ? 2 : 4;
      if (i + 1 + nlen > bytes.length) { tail(i); break; }
      let len = 0;
      for (let k = 0; k < nlen; k++) len += bytes[i + 1 + k] * 2 ** (8 * k);
      const start = i + 1 + nlen, end = start + len;
      if (end > bytes.length) { tail(i); break; }
      toks.push({ push: bytesToHex(bytes.subarray(start, end)), pushForm: nlen });
      i = end;
    } else {                                           // a non-push opcode
      toks.push({ op });
      i += 1;
    }
  }
  return toks;
}
