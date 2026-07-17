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
import { findAsciiStrings } from './btc-tx.js';

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

// A decimal integer string with thousands separators (an output amount, in
// satoshis): "407621551" -> "407,621,551". Operates on the string to avoid any
// precision loss on large values.
export function groupDigits(s) {
  return s.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

// Quoted script text comes directly from raw blockchain data -- a miner's
// coinbase tag, an OP_RETURN message -- not our own wordlist, so unlike the
// Glossia-generated prose it's untrusted content and must be escaped before
// it's spliced into a string callers render via innerHTML.
function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
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
  // Two spots carry deliberate human-readable text rather than
  // cryptographic material: a coinbase input's scriptSig (mining pool
  // tags) and an OP_RETURN output's scriptPubKey (arbitrary embedded
  // messages). When detected there, that text is quoted inline verbatim
  // instead of Glossia-encoded -- it's already legible, so routing it
  // through the wordlist would just swap real words for unrelated ones.
  // See btc-tx.js's findAsciiStrings for why this stays scoped to just
  // these two fields rather than scriptSig/scriptPubKey/witness generally.
  // Returns both forms of a script: `script` is the display string --
  // detected ASCII wrapped in inline curly quotes, otherwise Glossia prose --
  // and `ascii` is the same detected text unquoted (still HTML-escaped), or
  // null when the script wasn't legible ASCII. A flat renderer uses `script`
  // as-is; a manuscript renderer can set `ascii` as a quote block instead.
  const collectScript = (hex, eligible) => {
    if (eligible) {
      const found = findAsciiStrings(hex);
      if (found.length) {
        const ascii = found.map((s) => escapeHtml(s)).join(' ');
        return { script: found.map((s) => `“${escapeHtml(s)}”`).join(' '), ascii };
      }
    }
    return { script: collect(hex), ascii: null };
  };

  const inputs = parsed.vin.map((v) => {
    const isNullPrevout = v.txid === '00'.repeat(32);
    const { script, ascii } = collectScript(v.scriptSig, isNullPrevout);
    const seq = sequenceInfo(v.sequence);
    // The prevout is carried as a reference (txid + output index), not encoded:
    // the book resolves it to a volume/book/chapter/verse citation for the left
    // margin. A coinbase has none. Raw per-input witness bytes (segwit only) ride
    // along so each input's witness can become its own footnote.
    return {
      isNullPrevout,
      prevTxid: isNullPrevout ? '' : v.txid,
      prevVout: v.vout,
      script, scriptAscii: ascii,
      sequence: seq.mark, sequenceKind: seq.kind, sequenceTitle: seq.title, sequenceRbf: seq.rbf,
      witnessHex: v.witnessHex || '',
      // An all-zero witness (a coinbase's reserved value, or an empty stack) is
      // shown as ∅ rather than encoded to a run of zero-words.
      witnessZero: (v.witness || []).every((it) => /^0*$/.test(it)),
    };
  });

  const outputs = parsed.vout.map((o) => {
    const isOpReturn = o.scriptPubKey.slice(0, 2).toLowerCase() === '6a';
    const { script, ascii } = collectScript(o.scriptPubKey, isOpReturn);
    return { script, scriptAscii: ascii, value: groupDigits(String(o.value)) };
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
