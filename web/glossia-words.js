// Reversible codec: arbitrary bytes <-> hyphenated Latin payload words.
//
// Used to carry keys (nsec / npub / read key) and passphrases in URL fragments
// as Latin words instead of raw bech32, so an nsec doesn't sit in browser
// history as an obvious `nsec1…`. This is OBFUSCATION, not security — anyone
// with the link (or history) and this decoder recovers the value.
//
// Encoding is Glossia's raw base-N over the Latin/default wordlist with a fixed
// seed (deterministic) and an empty dialect (bare payload words, no prose). It
// is fixed-width per byte count, so a 2-byte length prefix always occupies a
// fixed 2 words and can be split off to make arbitrary lengths self-describing.
import { encode_raw_base_n, decode_raw_base_n } from './glossia.js';

const LANG = 'latin', WL = 'default', SEED = 0n;
const enc = (hex) => JSON.parse(encode_raw_base_n(hex, LANG, WL, '', SEED)).payload_words;
const dec = (words, n) => JSON.parse(decode_raw_base_n(words, LANG, WL, n)).decoded_hex || '';

const TE = new TextEncoder(), TD = new TextDecoder();
const bytesToHex = (u8) => Array.from(u8, (b) => b.toString(16).padStart(2, '0')).join('');
const hexToBytes = (h) => new Uint8Array((h.match(/../g) || []).map((x) => parseInt(x, 16)));
export const textToHex = (s) => bytesToHex(TE.encode(s));
export const hexToText = (h) => TD.decode(hexToBytes(h));

// hex string (any length ≤ 65535 bytes) -> hyphenated Latin words. A 2-byte
// length prefix (always 2 words) precedes the data words so decode is unambiguous.
export function hexToWords(hex) {
  const n = hex.length / 2;
  const lenWords = enc(n.toString(16).padStart(4, '0'));
  const dataWords = n ? enc(hex) : [];
  return lenWords.concat(dataWords).join('-');
}

// hyphenated Latin words -> hex string, or null if it isn't a valid word blob.
export function wordsToHex(str) {
  const t = String(str).split('-').filter(Boolean);
  if (t.length < 2) return null;
  let n;
  try { n = parseInt(dec(t.slice(0, 2).join(' '), 2), 16); } catch { return null; }
  if (!Number.isInteger(n) || n <= 0) return null;
  let hex;
  try { hex = dec(t.slice(2).join(' '), n); } catch { return null; }
  return hex && hex.length === n * 2 ? hex : null;
}

// A hyphenated run of latin letters (our word form), as opposed to a bech32
// string (npub1…/nsec1…/nread1…, which contain digits and no hyphens).
export function isWordForm(s) { return /^[a-zà-ÿ]+(-[a-zà-ÿ]+)+$/i.test((s || '').trim()); }
