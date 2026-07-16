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
// Some of those numerals get a symbol instead of a digit string when they
// carry a specific conventional meaning -- a coinbase's null prevout (∅),
// the sequence field's default "final" value (·), and locktime = 0, the
// overwhelmingly common case (◼︎) -- see the constants below.
//
// Shared by bitcoin.html (single transaction lookup) and bitcoin-book.html
// (block chapters) so both pages render a transaction identically.

import { encodeSeedPhrase } from './glossia-msg.js';
import { findAsciiStrings } from './btc-tx.js';

// Conventional values worth a symbol instead of a digit string: a coinbase's
// null prevout (txid all-zero, paired with vout = 0xffffffff -- together
// they mean "no real previous output", so shown once as ∅ rather than as
// two separate numbers), the sequence field's default "final, no RBF" value
// 0xffffffff (shown as ·, wherever it appears -- this isn't coinbase-
// specific, ordinary transactions set it just as often), and locktime = 0
// (shown as ◼︎ -- no timelock, the case for the overwhelming majority of
// transactions).
const FINAL_SEQUENCE = 4294967295;
const NULL_PREVOUT_MARK = '∅';
const FINAL_SEQUENCE_MARK = '·';
const LOCKTIME_ZERO_MARK = '◼︎';

// Quoted script text comes directly from raw blockchain data -- a miner's
// coinbase tag, an OP_RETURN message -- not our own wordlist, so unlike the
// Glossia-generated prose it's untrusted content and must be escaped before
// it's spliced into a string callers render via innerHTML.
function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// A parsed transaction (btc-tx.js's parseTransaction) -> a structured
// breakdown of every field's rendered text, in wire order, plus the
// payload words consumed. Both describeTransaction (the flat canonical
// string) and bitcoin-book.html's margin layout are built from this, so
// a field is only ever Glossia-encoded once.
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
    const prevout = isNullPrevout ? NULL_PREVOUT_MARK : `${collect(v.txid)} ${v.vout}`;
    const { script, ascii } = collectScript(v.scriptSig, isNullPrevout);
    const sequence = v.sequence === FINAL_SEQUENCE ? FINAL_SEQUENCE_MARK : String(v.sequence);
    return { prevout, script, scriptAscii: ascii, sequence };
  });

  const outputs = parsed.vout.map((o) => {
    const isOpReturn = o.scriptPubKey.slice(0, 2).toLowerCase() === '6a';
    const { script, ascii } = collectScript(o.scriptPubKey, isOpReturn);
    return { script, scriptAscii: ascii, value: String(o.value) };
  });

  return {
    version: String(parsed.version),
    inputCount: String(parsed.vin.length),
    inputs,
    outputCount: String(parsed.vout.length),
    outputs,
    locktime: parsed.locktime === 0 ? LOCKTIME_ZERO_MARK : String(parsed.locktime),
    payloadWords,
  };
}

// A parsed transaction -> { prose, payloadWords }, prose being the flat
// canonical text in exact wire order -- the linear, decodable form
// (Copy buttons, filtering against the payload wordlist). Any renderer
// (e.g. bitcoin-book.html's margin layout) is free to reposition these
// same fields visually without touching this canonical order.
export function describeTransaction(parsed, bestOf = 1) {
  const f = composeTransactionFields(parsed, bestOf);
  const parts = [f.version, f.inputCount];
  for (const inp of f.inputs) parts.push(`${inp.prevout} ${inp.script} ${inp.sequence}`);
  parts.push(f.outputCount);
  for (const out of f.outputs) parts.push(`${out.value} ${out.script}`);
  parts.push(f.locktime);
  return { prose: parts.join(' ').replace(/\s+/g, ' ').trim(), payloadWords: f.payloadWords };
}

// The witness section's raw hex (parsed.witnessHex) -> { prose, payloadWords },
// or { prose: '', payloadWords: [] } when the transaction carries no witness data.
export function describeWitness(parsed, bestOf = 1) {
  if (!parsed.witnessHex) return { prose: '', payloadWords: [] };
  return encodeSeedPhrase(parsed.witnessHex, 'english', bestOf);
}
