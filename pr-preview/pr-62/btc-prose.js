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
import { findAsciiStrings, tokenizeScript } from './btc-tx.js';

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

// Opcode byte -> Glossia glyph, for the opcodes we've defined so far.
const OPCODE_SYMBOLS = {
  0x00: '⓪',                                                  // OP_0
  0x4f: '⊖',                                                  // OP_1NEGATE
  0x51: '①', 0x52: '②', 0x53: '③', 0x54: '④', 0x55: '⑤',      // OP_1 …
  0x56: '⑥', 0x57: '⑦', 0x58: '⑧', 0x59: '⑨', 0x5a: '⑩',
  0x5b: '⑪', 0x5c: '⑫', 0x5d: '⑬', 0x5e: '⑭', 0x5f: '⑮', 0x60: '⑯',   // … OP_16
  0x63: '⟨', 0x67: '│', 0x68: '⟩',                            // OP_IF / OP_ELSE / OP_ENDIF
  0x69: '✓',                                                  // OP_VERIFY
  0x6a: '¶',                                                  // OP_RETURN
  0x75: '↧', 0x76: '⧉',                                       // OP_DROP / OP_DUP
  0x87: '=', 0x88: '≡',                                       // OP_EQUAL / OP_EQUALVERIFY
  0xa6: 'ρ', 0xa7: 'σ', 0xa8: 'Σ', 0xa9: '⌖', 0xaa: '⌘',      // RIPEMD160 / SHA1 / SHA256 / HASH160 / HASH256
  0xac: '∇', 0xad: '▼', 0xae: '◇', 0xaf: '◆',                 // CHECKSIG(VERIFY) / CHECKMULTISIG(VERIFY)
  0xb1: 'τ', 0xb2: 'Δ',                                       // CLTV (absolute) / CSV (relative)
};

// Opcode byte -> Bitcoin Core name, the fallback for opcodes without a glyph.
// Data-push opcodes (0x01-0x4b, OP_PUSHDATA1/2/4) never reach this table --
// tokenizeScript surfaces them as their pushed data, not as an opcode.
const OPCODE_NAMES = {
  0x50: 'OP_RESERVED',
  0x61: 'OP_NOP', 0x62: 'OP_VER', 0x64: 'OP_NOTIF', 0x65: 'OP_VERIF', 0x66: 'OP_VERNOTIF',
  0x6b: 'OP_TOALTSTACK', 0x6c: 'OP_FROMALTSTACK', 0x6d: 'OP_2DROP', 0x6e: 'OP_2DUP', 0x6f: 'OP_3DUP',
  0x70: 'OP_2OVER', 0x71: 'OP_2ROT', 0x72: 'OP_2SWAP', 0x73: 'OP_IFDUP', 0x74: 'OP_DEPTH',
  0x77: 'OP_NIP', 0x78: 'OP_OVER', 0x79: 'OP_PICK', 0x7a: 'OP_ROLL', 0x7b: 'OP_ROT', 0x7c: 'OP_SWAP', 0x7d: 'OP_TUCK',
  0x7e: 'OP_CAT', 0x7f: 'OP_SUBSTR', 0x80: 'OP_LEFT', 0x81: 'OP_RIGHT', 0x82: 'OP_SIZE',
  0x83: 'OP_INVERT', 0x84: 'OP_AND', 0x85: 'OP_OR', 0x86: 'OP_XOR',
  0x89: 'OP_RESERVED1', 0x8a: 'OP_RESERVED2',
  0x8b: 'OP_1ADD', 0x8c: 'OP_1SUB', 0x8d: 'OP_2MUL', 0x8e: 'OP_2DIV', 0x8f: 'OP_NEGATE',
  0x90: 'OP_ABS', 0x91: 'OP_NOT', 0x92: 'OP_0NOTEQUAL',
  0x93: 'OP_ADD', 0x94: 'OP_SUB', 0x95: 'OP_MUL', 0x96: 'OP_DIV', 0x97: 'OP_MOD', 0x98: 'OP_LSHIFT', 0x99: 'OP_RSHIFT',
  0x9a: 'OP_BOOLAND', 0x9b: 'OP_BOOLOR', 0x9c: 'OP_NUMEQUAL', 0x9d: 'OP_NUMEQUALVERIFY', 0x9e: 'OP_NUMNOTEQUAL',
  0x9f: 'OP_LESSTHAN', 0xa0: 'OP_GREATERTHAN', 0xa1: 'OP_LESSTHANOREQUAL', 0xa2: 'OP_GREATERTHANOREQUAL',
  0xa3: 'OP_MIN', 0xa4: 'OP_MAX', 0xa5: 'OP_WITHIN',
  0xab: 'OP_CODESEPARATOR',
  0xb0: 'OP_NOP1', 0xb3: 'OP_NOP4', 0xb4: 'OP_NOP5', 0xb5: 'OP_NOP6', 0xb6: 'OP_NOP7',
  0xb7: 'OP_NOP8', 0xb8: 'OP_NOP9', 0xb9: 'OP_NOP10', 0xba: 'OP_CHECKSIGADD',
  0xff: 'OP_INVALIDOPCODE',
};

// One opcode -> its HTML: a defined glyph (accent-styled), else the OP_* name.
function opToken(code) {
  const sym = OPCODE_SYMBOLS[code];
  if (sym) return `<span class="op">${sym}</span>`;
  return `<span class="op-name">${OPCODE_NAMES[code] || 'OP_UNKNOWN'}</span>`;
}

// A script (hex) -> its opcode-notation display string. `collect` encodes a
// data push to Glossia prose; `eligible` (an OP_RETURN payload) turns on inline
// ASCII quoting for legible pushes. Opcode glyphs and OP_* names are the only
// HTML added here; pushed data is Glossia prose (safe) and quoted ASCII is
// escaped, so the result is safe to render via innerHTML like before.
function renderScript(hex, collect, eligible) {
  const parts = [];
  for (const t of tokenizeScript(hex)) {
    if (t.op !== undefined) {
      parts.push(opToken(t.op));
    } else if (t.push !== undefined) {
      if (!t.push) continue;                                  // empty push -- nothing to show
      if (eligible) {
        const found = findAsciiStrings(t.push);
        if (found.length) { parts.push(found.map((s) => `“${escapeHtml(s)}”`).join(' ')); continue; }
      }
      parts.push(collect(t.push));
    } else {
      parts.push(collect(t.trunc));                           // malformed tail -- carry it as prose
    }
  }
  return parts.join(' ');
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
      return i === scriptIdx ? renderScript(hex, encode, false) : encode(hex);
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
    // A coinbase scriptSig is arbitrary data, not valid script, so tokenizing
    // it as opcodes would be noise -- it keeps the legible-text treatment,
    // where a mining-pool tag is surfaced as a quote block (`scriptAscii`).
    // Every other scriptSig is genuine script, rendered in opcode notation.
    let script, scriptAscii = null;
    if (isNullPrevout) {
      const found = findAsciiStrings(v.scriptSig);
      if (found.length) {
        scriptAscii = found.map((s) => escapeHtml(s)).join(' ');
        script = found.map((s) => `“${escapeHtml(s)}”`).join(' ');
      } else {
        script = collect(v.scriptSig);
      }
    } else {
      script = renderScript(v.scriptSig, collect, false);
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
    return { script: renderScript(o.scriptPubKey, collect, isOpReturn), scriptAscii: null, value: groupDigits(String(o.value)) };
  });

  const lock = locktimeInfo(parsed.locktime);
  return {
    version: String(parsed.version),
    inputCount: String(parsed.vin.length),
    inputs,
    outputCount: String(parsed.vout.length),
    outputs,
    locktime: lock.mark, locktimeTitle: lock.title,
    payloadWords,
  };
}
