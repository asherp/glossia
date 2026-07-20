// btc-contents.js — the curated table of contents for the Bitcoin Book: notable
// blocks and transactions. Shared by bitcoin-book.html (the "Bookmarks" list in
// the lookup card) and bitcoin-contents.html (the table-of-contents page).
//
// Each `id` is handed straight to the book's lookup: a bare number is a block
// height, a 64-hex value a transaction id. Every entry is cited by its
// reference, never its raw id: a block's is known offline (volume·book·chapter
// from its height); a transaction's is resolved the same way the reader resolves
// a citation -- a /tx/<txid>/merkle-proof lookup gives its block height and
// index, yielding volume·book·chapter·§section. Ordered chronologically (reading
// order).

import { reference } from './btc-citation.js';

export const NOTABLE = [
  { title: 'The Times 03/Jan/2009 Chancellor on brink of second bailout for banks', id: '4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b' },
  { title: 'Block 1 mined', id: '1' },
  { title: 'First peer-to-peer transaction', id: '170' },
  { title: 'Bitcoin Pizza Day', id: '57043' },
  { title: '100K block milestone', id: '100000' },
  { title: 'Luke Dashjr’s Bible verses', id: '139690' },
  { title: 'First P2SH spend', id: 'e5779b9e78f9650debc2893fd9636d827b26b4ddfa6a8172fe8708c924f5c39d' },
  { title: 'First halving', id: '210000' },
  { title: 'First coinbase OP_RETURN', id: '246816' },
  { title: 'Second halving', id: '420000' },
  { title: 'Bitcoin Cash fork', id: '478558' },
  { title: 'SegWit activation', id: '481824' },
  { title: '500K block milestone', id: '500000' },
  { title: 'Third halving', id: '630000' },
  { title: 'Block 666,666', id: '666666' },
  { title: 'Romans 12:21 message', id: '057954bb28527ff9c7701c6fd2b7f770163718ded09745da56cc95e7606afe99' },
  { title: 'Taproot activation', id: '709632' },
  { title: 'First Ordinals inscription', id: '6fb976ab49dcec017f1e201e84395983204ae1a7c2abf7ced0a85d692e442799' },
  { title: 'Largest block (at the time)', id: '774628' },
  { title: 'Fourth halving', id: '840000' },
];

// More transaction-level entries still to confirm against the chain before
// adding: payment-type firsts (P2PKH, P2WPKH/P2WSH, P2TR key/script, OP_RETURN
// spend) and a Lightning force-close revealing an HTLC.

export const isBlockId = (id) => /^[0-9]+$/.test(id);

// The offline reference for a block id (volume·book·chapter). A transaction id
// has no offline height, so it returns '' and must be resolved at read time.
export function blockRef(id) {
  return isBlockId(id) ? reference(Number(id)) : '';
}

// Format a resolved citation -- a block height and the transaction's index
// within it -- as a full volume·book·chapter·§section reference.
export function refFromProof(height, pos) {
  return reference(height) + (pos != null ? ` §${pos + 1}` : '');
}

// A deep link into the book for a contents id. A bare height opens as ?block=;
// a 64-hex value (block hash or txid) opens as ?txid=, which the book resolves
// as a block first and a transaction second.
export function entryHref(id) {
  return isBlockId(id) ? `bitcoin-book.html?block=${id}` : `bitcoin-book.html?txid=${id}`;
}
