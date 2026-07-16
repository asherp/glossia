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
// Shared by bitcoin.html (single transaction lookup) and bitcoin-book.html
// (block chapters) so both pages render a transaction identically.

import { encodeSeedPhrase } from './glossia-msg.js';

function endSentence(s) { s = s.trim(); return s.endsWith('.') ? s : s + '.'; }

// A parsed transaction (btc-tx.js's parseTransaction) -> { prose, payloadWords }.
// `bestOf` forwards to encodeSeedPhrase for cover-word quality (default 1).
export function describeTransaction(parsed, bestOf = 1) {
  const parts = [`${parsed.version}.`, `${parsed.vin.length}.`];
  const payloadWords = [];
  const collect = (hex) => {
    if (!hex) return '';
    const r = encodeSeedPhrase(hex, 'english', bestOf);
    payloadWords.push(...r.payloadWords);
    return r.prose;
  };

  for (const v of parsed.vin) {
    const isNullPrevout = v.txid === '00'.repeat(32);
    const txidPart = isNullPrevout ? '0' : collect(v.txid);
    const scriptProse = collect(v.scriptSig);
    parts.push(endSentence(`${txidPart} ${v.vout} ${scriptProse} ${v.sequence}`));
  }

  parts.push(`${parsed.vout.length}.`);
  for (const o of parsed.vout) {
    const scriptProse = collect(o.scriptPubKey);
    parts.push(endSentence(`${o.value} ${scriptProse}`));
  }

  parts.push(`${parsed.locktime}.`);
  return { prose: parts.join(' ').replace(/\s+/g, ' ').trim(), payloadWords };
}

// The witness section's raw hex (parsed.witnessHex) -> { prose, payloadWords },
// or { prose: '', payloadWords: [] } when the transaction carries no witness data.
export function describeWitness(parsed, bestOf = 1) {
  if (!parsed.witnessHex) return { prose: '', payloadWords: [] };
  return encodeSeedPhrase(parsed.witnessHex, 'english', bestOf);
}
