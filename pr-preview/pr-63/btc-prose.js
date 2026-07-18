// btc-prose.js — compose a Bitcoin transaction's Glossia prose, field by
// field, in wire order: small structural integers (version, counts, an
// input's referenced output index and sequence, an output's value,
// locktime) are spliced in as literal numerals -- they're small fixed-width
// integers, not entropy, and routing them through the wordlist is exactly
// what produces long zero-byte word runs (a version of 1 is stored as
// 01 00 00 00; locktime is 0 in the overwhelming majority of transactions;
// output values sit nowhere near the 8-byte field's ceiling). Only the
// genuinely opaque bytes -- prevout txid, scriptSig, scriptPubKey, the
// witness stack -- are still Glossia-encoded.
//
// The timelock fields -- an input's sequence and the transaction's locktime --
// are rendered as a small symbol grammar rather than raw numerals; see the
// helpers below.
//
// Consumed by bitcoin-book.html, which renders each field into its manuscript
// margin layout.

import { encodeSeedPhrase } from './glossia-msg.js';
import { findAsciiStrings, tokenizeScript, bitsToTargetHex, bitsToDifficulty } from './btc-tx.js';

// The timelock fields get symbols rather than digit strings, on a small grammar
// that separates the whole transaction's status (nLockTime) from each input's
// (nSequence), sharing ■/⊥ for the block/time distinction:
//   Transaction (nLockTime):  □ none · ■n absolute block height · ⊥n absolute unix time
//   Input (nSequence):        ○ final · † replaceable (opt-in RBF) ·
//                             ■n relative block delay · ⊥n relative time delay
// The square reads as the whole document, the circle as one input. A coinbase's
// null prevout is flagged with isNullPrevout so the renderer can mark it (∅);
// other prevouts are carried as references and resolved to a citation
// downstream, not encoded here.
const LOCKTIME_THRESHOLD = 500000000;   // nLockTime below this is a block height, at/above a unix timestamp

// nLockTime -> { mark, title }: □ (none), ■n (absolute block), ⊥n (absolute time).
function locktimeInfo(locktime) {
  if (locktime === 0) return { mark: '□', title: 'no locktime — final with respect to time' };
  if (locktime < LOCKTIME_THRESHOLD) return { mark: `■${locktime}`, title: `locktime: not before block ${locktime}` };
  const date = new Date(locktime * 1000).toISOString().slice(0, 16).replace('T', ' ');
  return { mark: `⊥${locktime}`, title: `locktime: not before ${date} UTC (unix ${locktime})` };
}

// nSequence -> { rbf, mark, kind, title }. BIP68 relative locktime is enabled
// when bit 31 is clear; bit 22 then selects time (512 s units) over blocks, with
// the value in the low 16 bits -- and since such a value is always < 0xfffffffe
// it ALSO signals opt-in RBF, so it's shown as two marks, † then the delay
// (e.g. "† ■144"). Otherwise: ● final (0xffffffff, disables the transaction
// locktime for this input), ○ non-replaceable but respecting the locktime
// (0xfffffffe), or a bare † replaceable (< 0xfffffffe, opt-in RBF).
function sequenceInfo(seq) {
  if ((seq & 0x80000000) === 0) {
    const n = seq & 0x0000ffff;
    return (seq & 0x00400000)
      ? { rbf: true, mark: `⊥${n}`, kind: 'time', title: `replaceable; relative locktime ${n} × 512 s after the input's confirmation` }
      : { rbf: true, mark: `■${n}`, kind: 'block', title: `replaceable; relative locktime ${n} block${n === 1 ? '' : 's'} after the input's confirmation` };
  }
  if (seq === 0xffffffff) return { rbf: false, mark: '●', kind: 'final', title: 'final — disables the transaction locktime for this input' };
  if (seq === 0xfffffffe) return { rbf: false, mark: '○', kind: 'locktime', title: 'not replaceable, but respects the transaction locktime' };
  return { rbf: true, mark: '', kind: 'rbf', title: 'replaceable — signals opt-in RBF' };
}

const SUBSCRIPT_DIGITS = '₀₁₂₃₄₅₆₇₈₉';
const toSubscript = (n) => String(n).split('').map((d) => SUBSCRIPT_DIGITS[+d]).join('');

// nBits (a compact difficulty target) -> { sym, num, title }. The target is
// rendered as the thing it is -- the ceiling a mined hash must dip under --
// via its leading zero run: β's subscript counts the zero hex digits beyond
// the eight the genesis target opens with, so β₀ is difficulty 1 and the
// subscript climbs as difficulty rises. `num` carries the target's remaining
// significant digits; empty for the baseline mantissa ffff, so every
// difficulty-1 block reads as a bare β₀. Exact both ways: zeros + digits
// rebuild the full 256-bit target, which re-packs to the compact form. A
// target looser than the genesis baseline (never on mainnet) falls back to
// the raw compact hex. The title keeps the raw nBits, the full target and
// the difficulty ratio for hover.
function bitsInfo(bits) {
  const targetHex = bitsToTargetHex(bits);
  const difficulty = bitsToDifficulty(bits);
  const diffStr = difficulty.toLocaleString(undefined, { maximumFractionDigits: difficulty < 1000 ? 2 : 0 });
  const compact = bits.toString(16).padStart(8, '0');
  const title = `nBits ${compact} — a valid block hash must read below ${targetHex} — difficulty ${diffStr} (relative to the genesis block)`;
  const zeros = targetHex.length - targetHex.replace(/^0+/, '').length;
  if (zeros < 8) return { sym: compact, num: '', title };
  const digits = targetHex.slice(zeros).replace(/0+$/, '') || '0';
  return { sym: `β${toSubscript(zeros - 8)}`, num: digits === 'ffff' ? '' : digits, title };
}

// A block header's nTime -> { mark, title }: the mark is the human date --
// the interpreted, legible form -- since unlike nonce there's nothing more
// "raw" a reader would want at a glance; the title carries the literal unix
// value for verification against the wire bytes.
function timestampInfo(timestamp) {
  const date = new Date(timestamp * 1000).toISOString().slice(0, 16).replace('T', ' ');
  return { mark: `${date} UTC`, title: `unix ${timestamp}` };
}

// A parsed block header (btc-tx.js's parseBlockHeader) -> its rendered
// fields. version, timestamp, bits and nonce are small structural numbers --
// never entropy -- so they're rendered literally/decoded rather than
// Glossia-encoded, mirroring how composeTransactionFields treats a
// transaction's version and locktime. The nonce in particular gets no
// further decoding: it's already exactly what it looks like, the number a
// miner incremented in the search for a hash below the bits target. The
// previous-block hash and merkle root are genuinely opaque 32-byte hashes --
// callers Glossia-encode those themselves (as bitcoin-book.html already does
// for the block/txid hashes), not here.
export function composeBlockHeaderFields(header) {
  const time = timestampInfo(header.timestamp);
  const bits = bitsInfo(header.bits);
  return {
    version: String(header.version),
    timestamp: time.mark, timestampTitle: time.title,
    bits: bits.sym + bits.num, bitsTitle: bits.title,
    nonce: String(header.nonce),
  };
}

// A decimal integer string with a middle-dot every three digits (an output
// amount, in satoshis): "407621551" -> "407·621·551". Operates on the string to
// avoid any precision loss on large values.
export function groupDigits(s) {
  return s.replace(/\B(?=(\d{3})+(?!\d))/g, '·');
}

// Quoted script text comes directly from raw blockchain data -- a miner's
// coinbase tag, an OP_RETURN message -- not our own wordlist, so unlike the
// Glossia-generated prose it's untrusted content and must be escaped before
// it's spliced into a string callers render via innerHTML.
function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ─── script → opcode notation ──────────────────────────────────────────
//
// A scriptSig / scriptPubKey is rendered as its sequence of opcodes and data
// pushes. An opcode we've given a Glossia glyph renders as that glyph; every
// other opcode falls back to Bitcoin Core's OP_* name. Data pushes stay
// Glossia prose (or, for an OP_RETURN payload, inline-quoted when they're
// legible ASCII) -- exactly what carried the whole script before opcodes had
// their own marks.

// Opcode byte -> Glossia glyph. Every defined opcode has one; families share
// a base glyph, with the house subscript convention distinguishing variants
// (⧉₂ = 2DUP, °₄ = NOP4, ∇₊ = CHECKSIGADD). Disabled opcodes keep their
// natural symbol like any other -- a script is notation whether or not the
// network would still execute it.
const OPCODE_SYMBOLS = {
  // constants
  0x00: '⓪', 0x4f: '⊖',
  0x51: '①', 0x52: '②', 0x53: '③', 0x54: '④', 0x55: '⑤',
  0x56: '⑥', 0x57: '⑦', 0x58: '⑧', 0x59: '⑨', 0x5a: '⑩',
  0x5b: '⑪', 0x5c: '⑫', 0x5d: '⑬', 0x5e: '⑭', 0x5f: '⑮', 0x60: '⑯',
  // flow control
  0x61: '°', 0x63: '⟨', 0x64: '¬⟨', 0x67: '│', 0x68: '⟩', 0x69: '✓', 0x6a: '¶',
  // stack choreography (arrows), the alt-stack shelf pair, and depth/size
  0x6b: '⇥', 0x6c: '⇤',
  0x6d: '⌄₂', 0x6e: '⧉₂', 0x6f: '⧉₃', 0x70: '⇗₂', 0x71: '↻₂', 0x72: '⇄₂',
  0x73: '⧉?', 0x74: '↕', 0x75: '⌄', 0x76: '⧉', 0x77: '⌦', 0x78: '⇗',
  0x79: '⇡', 0x7a: '⥀', 0x7b: '↻', 0x7c: '⇄', 0x7d: '⇘',
  // splice
  0x7e: '⧺', 0x7f: '⊂', 0x80: '↤', 0x81: '↦', 0x82: 'ℓ',
  // bitwise, and byte equality
  0x83: '∼', 0x84: '∩', 0x85: '∪', 0x86: '⊻', 0x87: '=', 0x88: '≡',
  // arithmetic and comparison
  0x8b: '+₁', 0x8c: '−₁', 0x8d: '×₂', 0x8e: '÷₂', 0x8f: '∓', 0x90: '|·|',
  0x91: '¬', 0x92: '≠₀',
  0x93: '+', 0x94: '−', 0x95: '×', 0x96: '÷', 0x97: '%', 0x98: '«', 0x99: '»',
  0x9a: '∧', 0x9b: '∨', 0x9c: '≐', 0x9d: '≑', 0x9e: '≠',
  0x9f: '<', 0xa0: '>', 0xa1: '≤', 0xa2: '≥', 0xa3: '⊓', 0xa4: '⊔', 0xa5: '∈',
  // crypto
  0xa6: 'ρ', 0xa7: 'σ', 0xa8: 'Σ', 0xa9: '⌖', 0xaa: '⌘', 0xab: '‖',
  0xac: '∇', 0xad: '▼', 0xae: '◇', 0xaf: '◆', 0xba: '∇₊',
  // timelocks
  0xb1: 'τ', 0xb2: 'Δ',
  // no-ops
  0xb0: '°₁', 0xb3: '°₄', 0xb4: '°₅', 0xb5: '°₆', 0xb6: '°₇',
  0xb7: '°₈', 0xb8: '°₉', 0xb9: '°₁₀',
  // reserved / invalid
  0x50: '⊘', 0x62: '⊘ᵛ', 0x65: '⊘⟨', 0x66: '⊘¬⟨', 0x89: '⊘₁', 0x8a: '⊘₂',
  0xff: '☒',
};

// Opcode byte -> Bitcoin Core name: the hover title carried on every glyph,
// and the display fallback for undefined bytes (shown as OP_UNKNOWN).
const OPCODE_NAMES = {
  0x00: 'OP_0', 0x4f: 'OP_1NEGATE', 0x50: 'OP_RESERVED',
  0x51: 'OP_1', 0x52: 'OP_2', 0x53: 'OP_3', 0x54: 'OP_4', 0x55: 'OP_5',
  0x56: 'OP_6', 0x57: 'OP_7', 0x58: 'OP_8', 0x59: 'OP_9', 0x5a: 'OP_10',
  0x5b: 'OP_11', 0x5c: 'OP_12', 0x5d: 'OP_13', 0x5e: 'OP_14', 0x5f: 'OP_15', 0x60: 'OP_16',
  0x61: 'OP_NOP', 0x62: 'OP_VER', 0x63: 'OP_IF', 0x64: 'OP_NOTIF', 0x65: 'OP_VERIF', 0x66: 'OP_VERNOTIF',
  0x67: 'OP_ELSE', 0x68: 'OP_ENDIF', 0x69: 'OP_VERIFY', 0x6a: 'OP_RETURN',
  0x6b: 'OP_TOALTSTACK', 0x6c: 'OP_FROMALTSTACK', 0x6d: 'OP_2DROP', 0x6e: 'OP_2DUP', 0x6f: 'OP_3DUP',
  0x70: 'OP_2OVER', 0x71: 'OP_2ROT', 0x72: 'OP_2SWAP', 0x73: 'OP_IFDUP', 0x74: 'OP_DEPTH',
  0x75: 'OP_DROP', 0x76: 'OP_DUP',
  0x77: 'OP_NIP', 0x78: 'OP_OVER', 0x79: 'OP_PICK', 0x7a: 'OP_ROLL', 0x7b: 'OP_ROT', 0x7c: 'OP_SWAP', 0x7d: 'OP_TUCK',
  0x7e: 'OP_CAT', 0x7f: 'OP_SUBSTR', 0x80: 'OP_LEFT', 0x81: 'OP_RIGHT', 0x82: 'OP_SIZE',
  0x83: 'OP_INVERT', 0x84: 'OP_AND', 0x85: 'OP_OR', 0x86: 'OP_XOR',
  0x87: 'OP_EQUAL', 0x88: 'OP_EQUALVERIFY',
  0x89: 'OP_RESERVED1', 0x8a: 'OP_RESERVED2',
  0x8b: 'OP_1ADD', 0x8c: 'OP_1SUB', 0x8d: 'OP_2MUL', 0x8e: 'OP_2DIV', 0x8f: 'OP_NEGATE',
  0x90: 'OP_ABS', 0x91: 'OP_NOT', 0x92: 'OP_0NOTEQUAL',
  0x93: 'OP_ADD', 0x94: 'OP_SUB', 0x95: 'OP_MUL', 0x96: 'OP_DIV', 0x97: 'OP_MOD', 0x98: 'OP_LSHIFT', 0x99: 'OP_RSHIFT',
  0x9a: 'OP_BOOLAND', 0x9b: 'OP_BOOLOR', 0x9c: 'OP_NUMEQUAL', 0x9d: 'OP_NUMEQUALVERIFY', 0x9e: 'OP_NUMNOTEQUAL',
  0x9f: 'OP_LESSTHAN', 0xa0: 'OP_GREATERTHAN', 0xa1: 'OP_LESSTHANOREQUAL', 0xa2: 'OP_GREATERTHANOREQUAL',
  0xa3: 'OP_MIN', 0xa4: 'OP_MAX', 0xa5: 'OP_WITHIN',
  0xa6: 'OP_RIPEMD160', 0xa7: 'OP_SHA1', 0xa8: 'OP_SHA256', 0xa9: 'OP_HASH160', 0xaa: 'OP_HASH256',
  0xab: 'OP_CODESEPARATOR',
  0xac: 'OP_CHECKSIG', 0xad: 'OP_CHECKSIGVERIFY', 0xae: 'OP_CHECKMULTISIG', 0xaf: 'OP_CHECKMULTISIGVERIFY',
  0xb0: 'OP_NOP1', 0xb1: 'OP_CHECKLOCKTIMEVERIFY', 0xb2: 'OP_CHECKSEQUENCEVERIFY',
  0xb3: 'OP_NOP4', 0xb4: 'OP_NOP5', 0xb5: 'OP_NOP6', 0xb6: 'OP_NOP7',
  0xb7: 'OP_NOP8', 0xb8: 'OP_NOP9', 0xb9: 'OP_NOP10', 0xba: 'OP_CHECKSIGADD',
  0xff: 'OP_INVALIDOPCODE',
};

// One opcode -> its HTML: the glyph (accent-styled, canonical OP_* name as
// its hover title), or the bare OP_* name for a byte with no glyph. The
// glyph is escaped -- a few marks (< > ≤-family) are HTML-significant.
function opToken(code) {
  const sym = OPCODE_SYMBOLS[code];
  const name = OPCODE_NAMES[code] || 'OP_UNKNOWN';
  if (sym) return `<span class="op" title="${name}">${escapeHtml(sym)}</span>`;
  return `<span class="op-name">${name}</span>`;
}

// A push opcode's mark. A direct push (OP_PUSHBYTES_n) is the quietest
// instruction in the set, so its mark is the quietest possible: the bare
// subscript byte count, ₙ. The arrows are reserved for OP_PUSHDATA1/2/4,
// whose length rides in a separate prefix -- arrow weight matching prefix
// width: ↧ₙ (1-byte), ⇊ₙ (2-byte), ⤋ₙ (4-byte). The pushed data itself
// follows the mark, as prose or an inline quote. (The coinbase preamble's
// βₙ and ηn marks fold their push opcode in -- the mark alone determines
// the exact bytes.)
const PUSH_GLYPHS = { 0: '', 1: '↧', 2: '⇊', 4: '⤋' };
function pushToken(form, byteLen) {
  const title = form
    ? `OP_PUSHDATA${form} — push ${byteLen} bytes, the length in a ${form}-byte prefix`
    : `OP_PUSHBYTES_${byteLen} — push the next ${byteLen} bytes`;
  return `<span class="op op-push" title="${title}">${PUSH_GLYPHS[form] || ''}${toSubscript(byteLen)}</span>`;
}

// ─── DER signature compaction ──────────────────────────────────────────
//
// A legacy / segwit-v0 ECDSA signature push is a DER envelope -- SEQUENCE and
// INTEGER tags, length bytes, canonical leading-zero padding -- wrapped around
// two 32-byte scalars, plus a trailing sighash byte. Only r, s and the sighash
// carry information; the ~7 envelope bytes are pure framing. derToCompact
// strips it to a fixed 65-byte r‖s‖sighash -- but ONLY when re-encoding those
// scalars reproduces the input byte-for-byte. That guard leaves every
// non-canonical signature (pre-BIP66 blocks) and every non-signature push
// untouched, so the compact form is always a faithful stand-in.

const SIGHASH_TYPES = new Set([0x01, 0x02, 0x03, 0x81, 0x82, 0x83]);
const byteAt = (hex, i) => parseInt(hex.substr(i * 2, 2), 16);

// Strip leading 0x00 bytes from a hex value, keeping at least one byte.
function stripLeadZeros(h) {
  let i = 0;
  while (i + 2 < h.length && h.substr(i, 2) === '00') i += 2;
  return h.slice(i);
}
// The canonical DER INTEGER *content* for a big-endian value: minimal length,
// with a single 0x00 prepended when the top bit would otherwise read negative.
function derInt(valHex) {
  const v = stripLeadZeros(valHex);
  return (byteAt(v, 0) & 0x80) ? '00' + v : v;
}
const lenByte = (h) => (h.length / 2).toString(16).padStart(2, '0');

// A signature push (DER sig + sighash byte) -> 65-byte r‖s‖sighash, or null when
// the bytes are not a strictly-canonical signature (so the caller keeps them).
function derToCompact(hex) {
  const n = hex.length / 2;
  if (n < 9 || n > 73 || byteAt(hex, 0) !== 0x30) return null;         // SEQUENCE
  if (2 + byteAt(hex, 1) + 1 !== n) return null;                       // header + body + sighash
  if (byteAt(hex, 2) !== 0x02) return null;                           // INTEGER r
  const rLen = byteAt(hex, 3);
  if (rLen < 1 || 6 + rLen > n || byteAt(hex, 4 + rLen) !== 0x02) return null;   // INTEGER s
  const sLen = byteAt(hex, 5 + rLen);
  if (sLen < 1 || 7 + rLen + sLen !== n) return null;
  const sighash = hex.substr((n - 1) * 2, 2);
  if (!SIGHASH_TYPES.has(parseInt(sighash, 16))) return null;
  const rVal = stripLeadZeros(hex.substr(8, rLen * 2));
  const sVal = stripLeadZeros(hex.substr((6 + rLen) * 2, sLen * 2));
  if (rVal.length > 64 || sVal.length > 64) return null;              // a scalar wider than 32 bytes
  const r32 = rVal.padStart(64, '0'), s32 = sVal.padStart(64, '0');
  // Re-encode the scalars as canonical DER and require an exact match -- the
  // fidelity guard that rejects non-canonical framing.
  const body = '02' + lenByte(derInt(r32)) + derInt(r32) + '02' + lenByte(derInt(s32)) + derInt(s32);
  const rebuilt = '30' + lenByte(body) + body + sighash;
  return rebuilt.toLowerCase() === hex.toLowerCase() ? r32 + s32 + sighash : null;
}

// A script (hex) -> its opcode-notation display string. `collect` encodes a
// ─── the early coinbase mining preamble ────────────────────────────────
//
// For the chain's first years, a coinbase scriptSig opened not with
// arbitrary tag data but with a small mining preamble: a 4-byte push
// restating the block's compact difficulty target (the header's nBits,
// byte for byte), then a small-integer push -- the extranonce, the counter
// a miner rolled once the header's 32-bit nonce was exhausted. Both are
// numbers, not entropy, so they render as decoded marks (βₙ, ηn) rather
// than payload words -- which also lets embedded text (the genesis
// headline) stand as the coinbase's first words instead of trailing runs
// of bytes-as-prose.

const reverseHexStr = (hex) => (hex.match(/../g) || []).reverse().join('');

// A 4-byte push -> its u32le value, when that value is a plausible compact
// difficulty target: a byte-length exponent in the range real targets
// occupy, and a positive nonzero mantissa. The exponent is the push's LAST
// byte, so printable-ASCII tag data (every byte ≥ 0x20) can never match;
// nor can a BIP34 height push, whose most-significant byte is far below
// 0x03 for any realistic height. Fixed-width, so the mark reconstructs the
// wire bytes exactly.
function compactBitsFromPush(push) {
  if (push.length !== 8) return null;
  const bits = parseInt(reverseHexStr(push), 16);
  const exponent = bits >>> 24, mantissa = bits & 0x00ffffff;
  if (exponent < 0x03 || exponent > 0x20 || mantissa === 0 || (mantissa & 0x800000) !== 0) return null;
  return bits;
}

// An extranonce push -> its decimal string: up to 8 little-endian bytes,
// minimally encoded (no most-significant zero byte), so the number alone
// reconstructs the exact bytes. A non-minimal encoding falls back to prose
// rather than risk a lossy round trip.
function extranonceFromPush(push) {
  if (push.length < 2 || push.length > 16 || push.slice(-2) === '00') return null;
  return BigInt('0x' + reverseHexStr(push)).toString();
}

// A decoded mark: glyph + value (when there is one to show), both carrying
// the same explanatory title.
const markToken = (glyph, text, title) => `<span class="op" title="${title}">${glyph}</span>${text ? `<span class="op-num" title="${title}">${text}</span>` : ''}`;

// data push to Glossia prose. Options: `eligible` (an OP_RETURN payload, or a
// coinbase) turns on inline ASCII quoting for legible pushes; `nested` reveals a
// script pushed as data -- a P2SH redeemScript, always the final push -- by
// rendering it as opcodes in turn; `preamble` (a coinbase) decodes the early
// mining preamble's leading pushes into β/η marks. Opcode glyphs, OP_* names
// and the preamble marks are the only HTML added here; pushed data is Glossia
// prose (safe) and quoted ASCII is escaped, so the result is safe to render
// via innerHTML like before.
function renderScript(hex, collect, { eligible = false, nested = false, preamble = false } = {}) {
  const toks = tokenizeScript(hex);
  // A P2SH scriptSig ends with its redeemScript, pushed as data; reveal that
  // final push as opcodes when it parses as a genuine script.
  const redeemIdx = nested ? toks.map((t) => t.push !== undefined).lastIndexOf(true) : -1;
  const parts = [];
  // The preamble is strictly positional -- the target must open the script,
  // the extranonce must directly follow it; anything else ends the hunt and
  // the push falls through to the ordinary treatment.
  let pre = preamble ? 'target' : 'done';
  toks.forEach((t, i) => {
    if (t.op !== undefined) {
      pre = 'done';
      parts.push(opToken(t.op));
    } else if (t.push !== undefined) {
      if (pre === 'target') {
        pre = 'done';
        const bits = compactBitsFromPush(t.push);
        if (bits !== null) {
          const info = bitsInfo(bits);
          parts.push(markToken(info.sym, info.num, `the difficulty target this block was mined against — ${info.title}`));
          pre = 'extranonce';
          return;
        }
      } else if (pre === 'extranonce') {
        pre = 'done';
        const n = extranonceFromPush(t.push);
        if (n !== null) {
          parts.push(markToken('η', n, `extranonce ${n} — the counter the miner rolled once the header's 32-bit nonce (η) was exhausted`));
          return;
        }
      }
      const mark = pushToken(t.pushForm || 0, t.push.length / 2);
      if (!t.push) { parts.push(mark); return; }              // a zero-length extended push -- the mark alone
      if (i === redeemIdx && looksLikeScript(t.push)) {
        parts.push(mark, renderScript(t.push, collect));      // reveal the redeemScript
        return;
      }
      if (eligible) {
        const found = findAsciiStrings(t.push);
        if (found.length) { parts.push(mark, found.map((s) => `“${escapeHtml(s)}”`).join(' ')); return; }
      }
      parts.push(mark, collect(derToCompact(t.push) || t.push));   // a DER signature is stripped to r‖s‖sighash
    } else {
      pre = 'done';
      parts.push(collect(t.trunc));                           // malformed tail -- carry it as prose
    }
  });
  return parts.join(' ');
}

// Every token is a data push or a defined opcode, with no malformed tail -- the
// test for whether a coinbase scriptSig (otherwise arbitrary miner data) is a
// clean script worth rendering as opcodes, as the earliest blocks' are.
const isDefinedOp = (code) => OPCODE_SYMBOLS[code] !== undefined || OPCODE_NAMES[code] !== undefined;
function isCleanScript(hex) {
  const toks = tokenizeScript(hex);
  return toks.length > 0 && toks.every((t) =>
    t.trunc === undefined && (t.push !== undefined || isDefinedOp(t.op)));
}

// ─── witness → per-item rendering ──────────────────────────────────────
//
// An input's witness is a stack of items. Rendering them individually (rather
// than as one blob) lets a signature, a key and a script read as distinct
// stack elements -- and the one item that is a script (a P2WSH witnessScript or
// a Taproot tapscript) is rendered in opcode notation like any other script.

const witHexLen = (h) => h.length / 2;
const witFirst = (h) => parseInt(h.slice(0, 2), 16);

// A witness item that is plainly data, never a script:
function isPubkey(h) {
  const n = witHexLen(h), b = witFirst(h);
  return (n === 33 && (b === 0x02 || b === 0x03)) || (n === 65 && b === 0x04);
}
function isSignature(h) {
  const n = witHexLen(h), b = witFirst(h);
  return n === 64 || n === 65 || (b === 0x30 && n >= 68 && n <= 73);   // Schnorr, or DER (+sighash byte)
}
// A Taproot script-path control block: a 0xc0/0xc1 leaf byte, then a 32-byte
// internal key and a merkle path of 32-byte hashes.
function isControlBlock(h) {
  const n = witHexLen(h);
  return n >= 33 && (n - 33) % 32 === 0 && (witFirst(h) & 0xfe) === 0xc0;
}
// A signature check or a timelock is the hallmark of a spending script and
// essentially never turns up by chance in a data item, so its presence (in an
// item that parses cleanly as script) marks the item as a witnessScript.
const SCRIPT_SIGNAL = new Set([0xac, 0xad, 0xae, 0xaf, 0xba, 0xb1, 0xb2]);
function looksLikeScript(h) {
  if (!h || isPubkey(h) || isSignature(h)) return false;
  const toks = tokenizeScript(h);
  if (!toks.length || toks.some((t) => t.trunc !== undefined)) return false;
  return toks.some((t) => t.op !== undefined && SCRIPT_SIGNAL.has(t.op));
}

// Which witness item (if any) is the script to render as opcodes: a Taproot
// tapscript (sitting just below its control block) or a P2WSH witnessScript
// (the last item). Returns -1 when the witness is all data -- P2WPKH, a
// key-path spend, a bare signature.
function witnessScriptIndex(items) {
  const n = items.length;
  if (n === 0) return -1;
  let last = n - 1;
  if (n >= 2 && witFirst(items[last]) === 0x50) last -= 1;    // strip an optional annex
  if (last >= 1 && isControlBlock(items[last])) return last - 1;
  const tail = items[n - 1];
  if (isPubkey(tail) || isSignature(tail)) return -1;
  return looksLikeScript(tail) ? n - 1 : -1;
}

// An input's witness stack (hex items) -> its footnote display. `encode` turns
// a data item's hex into Glossia prose; the one script item, if present,
// becomes opcode notation. Items are separated so each reads as its own stack
// element.
export function renderWitness(items, encode) {
  if (!items || !items.length) return '∅';
  const scriptIdx = witnessScriptIndex(items);
  return items
    .map((hex, i) => {
      if (!hex) return '<span class="wit-empty">∅</span>';    // an empty stack item
      if (i === scriptIdx) return renderScript(hex, encode);
      return encode(derToCompact(hex) || hex);                // a DER signature is stripped to r‖s‖sighash
    })
    .join('<span class="wit-sep"> · </span>');
}

// A parsed transaction (btc-tx.js's parseTransaction) -> a structured
// breakdown of every field's rendered text, in wire order, plus the payload
// words consumed. bitcoin-book.html's margin layout is built from this, each
// field Glossia-encoded exactly once.
// `bestOf` forwards to encodeSeedPhrase for cover-word quality (default 1).
export function composeTransactionFields(parsed, bestOf = 1) {
  const payloadWords = [];
  const collect = (hex) => {
    if (!hex) return '';
    const r = encodeSeedPhrase(hex, 'english', bestOf);
    payloadWords.push(...r.payloadWords);
    return r.prose;
  };
  const inputs = parsed.vin.map((v) => {
    const isNullPrevout = v.txid === '00'.repeat(32);
    // A coinbase scriptSig is arbitrary miner data -- but the earliest blocks'
    // are clean push-scripts, so render those in opcode notation, with the
    // mining preamble (restated difficulty target + extranonce) decoded to
    // marks and embedded text like the genesis headline quoted inline.
    // Messier ones keep the plain treatment, where a mining-pool tag is
    // surfaced as a quote block (`scriptAscii`). Every other scriptSig is
    // genuine script (with a P2SH redeemScript revealed as opcodes via
    // `nested`).
    let script, scriptAscii = null;
    if (isNullPrevout) {
      if (isCleanScript(v.scriptSig)) {
        script = renderScript(v.scriptSig, collect, { eligible: true, preamble: true });
      } else {
        const found = findAsciiStrings(v.scriptSig);
        if (found.length) {
          scriptAscii = found.map((s) => escapeHtml(s)).join(' ');
          script = found.map((s) => `“${escapeHtml(s)}”`).join(' ');
        } else {
          script = collect(v.scriptSig);
        }
      }
    } else {
      script = renderScript(v.scriptSig, collect, { nested: true });
    }
    const seq = sequenceInfo(v.sequence);
    // The prevout is carried as a reference (txid + output index), not encoded:
    // the book resolves it to a volume/book/chapter/verse citation for the left
    // margin. A coinbase has none. Raw per-input witness bytes (segwit only) ride
    // along so each input's witness can become its own footnote.
    return {
      isNullPrevout,
      prevTxid: isNullPrevout ? '' : v.txid,
      prevVout: v.vout,
      script, scriptAscii,
      sequence: seq.mark, sequenceKind: seq.kind, sequenceTitle: seq.title, sequenceRbf: seq.rbf,
      witnessHex: v.witnessHex || '',
      witnessItems: v.witness || [],
      // An all-zero witness (a coinbase's reserved value, or an empty stack) is
      // shown as ∅ rather than encoded to a run of zero-words.
      witnessZero: (v.witness || []).every((it) => /^0*$/.test(it)),
    };
  });

  const outputs = parsed.vout.map((o) => {
    // A scriptPubKey is always genuine script, rendered in opcode notation. An
    // OP_RETURN (¶) payload is `eligible` for inline ASCII quoting, so an
    // embedded message reads verbatim rather than as prose.
    const isOpReturn = o.scriptPubKey.slice(0, 2).toLowerCase() === '6a';
    return { script: renderScript(o.scriptPubKey, collect, { eligible: isOpReturn }), scriptAscii: null, value: groupDigits(String(o.value)) };
  });

  const lock = locktimeInfo(parsed.locktime);
  // Serialization framing is never encoded -- the input/output counts, the
  // witness item count and its per-item length prefixes, and a script's push
  // length prefixes are all structural and reconstructable from the parse, so
  // only genuine payload bytes become prose. (The counts are implicit in the
  // number of input/output rows; witness items render individually.)
  return {
    version: String(parsed.version),
    inputs,
    outputs,
    locktime: lock.mark, locktimeTitle: lock.title,
    payloadWords,
  };
}
