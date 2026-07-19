// btc-contents.js — the curated table of contents for the Bitcoin Book: famous
// transactions and the first occurrence of each payment type. Shared by
// bitcoin-book.html (the "Bookmarks" list in the lookup card) and
// bitcoin-contents.html (the full table-of-contents page).
//
// Each `id` is handed straight to the book's lookup, so it may be a block
// height, a block hash, or a txid. Ordered chronologically (reading order).
// Extend this with more famous, notable transactions and payment-type firsts.

export const NOTABLE = [
  { title: 'Genesis block', id: '0', label: 'block', note: 'Block 0 · Jan 3, 2009 — first block; embedded Times headline.' },
  { title: 'Block 1 mined', id: '1', label: 'block', note: 'Block 1 · Jan 9, 2009 — first block after genesis (6-day gap).' },
  { title: 'First peer-to-peer transaction', id: '170', label: 'block', note: 'Block 170 · Jan 12, 2009 — Satoshi sent 10 BTC to Hal Finney.' },
  { title: 'Bitcoin Pizza Day', id: '57043', label: 'block', note: 'Block 57,043 · May 22, 2010 — 10,000 BTC for two pizzas: first commercial use.' },
  { title: '100K block milestone', id: '100000', label: 'block', note: 'Block 100,000 · Dec 29, 2010 — network maturity milestone.' },
  { title: 'First halving', id: '210000', label: 'block', note: 'Block 210,000 · Nov 28, 2012 — reward 50 → 25 BTC.' },
  { title: 'Second halving', id: '420000', label: 'block', note: 'Block 420,000 · Jul 9, 2016 — reward 25 → 12.5 BTC.' },
  { title: 'Bitcoin Cash fork', id: '478558', label: 'block', note: 'Block 478,558 · Aug 1, 2017 — last shared BTC/BCH block.' },
  { title: 'SegWit activation', id: '481824', label: 'block', note: 'Block 481,824 · Aug 24, 2017 — fixed malleability; increased capacity.' },
  { title: '500K block milestone', id: '500000', label: 'block', note: 'Block 500,000 · Dec 18, 2017 — reached during the 2017 bull run.' },
  { title: 'Third halving', id: '630000', label: 'block', note: 'Block 630,000 · May 11, 2020 — reward 12.5 → 6.25 BTC.' },
  { title: 'Block 666,666', id: '666666', label: 'block', note: 'Block 666,666 · Jan 18, 2021 — a memorable vanity block number.' },
  { title: 'Taproot activation', id: '709632', label: 'block', note: 'Block 709,632 · Nov 14, 2021 — Schnorr signatures and smart contracts.' },
  { title: 'First Ordinals inscription', id: '767430', label: 'block', note: 'Block 767,430 · Dec 14, 2022 — inscription #0 by Casey Rodarmor.' },
  { title: 'Largest block (at the time)', id: '774628', label: 'block', note: 'Block 774,628 · Feb 1, 2023 — 3.96 MB Taproot Wizard inscription.' },
  { title: 'Fourth halving', id: '840000', label: 'block', note: 'Block 840,000 · Apr 20, 2024 — reward 6.25 → 3.125 BTC; Runes launched.' },
];

// Using block-number milestones for now. Transaction-level entries are deferred
// until we include txids. Confirmed ones to add:
//   - Romans 12:21 "overcome evil with good", embedded via vanity addresses --
//     tx 057954bb28527ff9c7701c6fd2b7f770163718ded09745da56cc95e7606afe99
// And still to confirm against the chain:
//   - Payment-type firsts (P2PKH, P2SH, P2WPKH/P2WSH, P2TR, OP_RETURN)
//   - A Lightning force-close revealing an HTLC on-chain

// A deep link into the book for a contents id. A bare height opens as ?block=;
// a 64-hex value (block hash or txid) opens as ?txid=, which the book resolves
// as a block first and a transaction second.
export function entryHref(id) {
  return /^[0-9]+$/.test(id) ? `bitcoin-book.html?block=${id}` : `bitcoin-book.html?txid=${id}`;
}
