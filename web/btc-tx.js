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
  if (bytes[r.pos] === 0x00 && bytes[r.pos + 1] === 0x01) { segwit = true; r.pos += 2; }

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

  let witnessBytes = 0;
  if (segwit) {
    const witStart = r.pos;
    for (let i = 0; i < vinCount; i++) {
      const itemCount = r.varint();
      const items = [];
      for (let j = 0; j < itemCount; j++) items.push(bytesToHex(r.bytesN(r.varint())));
      vin[i].witness = items;
    }
    witnessBytes = r.pos - witStart;
  }

  const locktime = r.u32le();
  if (r.pos !== bytes.length) throw new Error(`trailing bytes after transaction (${bytes.length - r.pos} unparsed)`);

  // BIP141 weight: base_size counts once, the segwit marker/flag/witness count 1/4x.
  const size = bytes.length;
  const baseSize = size - (segwit ? 2 + witnessBytes : 0);
  const weight = baseSize * 3 + size;
  const vsize = Math.ceil(weight / 4);

  return { version, locktime, segwit, size, vsize, weight, vin, vout };
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
