// btc-contents.js — the curated table of contents for the Bitcoin Book: famous
// transactions and the first occurrence of each payment type. Shared by
// bitcoin-book.html (the "Bookmarks" list in the lookup card) and
// bitcoin-contents.html (the full table-of-contents page).
//
// Each `id` is handed straight to the book's lookup, so it may be a block
// height, a block hash, or a txid. Ordered chronologically (reading order).
// Extend this with more famous, notable transactions and payment-type firsts.

export const NOTABLE = [
  { title: 'Genesis block', id: '0', note: 'Block 0 · Jan 3, 2009 — first block; embedded Times headline.' },
  { title: 'Block 1 mined', id: '1', note: 'Block 1 · Jan 9, 2009 — first block after genesis (6-day gap).' },
  { title: 'First peer-to-peer transaction', id: '170', note: 'Block 170 · Jan 12, 2009 — Satoshi sent 10 BTC to Hal Finney.' },
  { title: 'Bitcoin Pizza Day', id: '57043', note: 'Block 57,043 · May 22, 2010 — 10,000 BTC for two pizzas: first commercial use.' },
  { title: '100K block milestone', id: '100000', note: 'Block 100,000 · Dec 29, 2010 — network maturity milestone.' },
  { title: 'First halving', id: '210000', note: 'Block 210,000 · Nov 28, 2012 — reward 50 → 25 BTC.' },
  { title: 'First coinbase OP_RETURN', id: '246816', note: 'Block 246,816 · Jul 16, 2013 — the first coinbase transaction to carry an OP_RETURN output.' },
  { title: 'Second halving', id: '420000', note: 'Block 420,000 · Jul 9, 2016 — reward 25 → 12.5 BTC.' },
  { title: 'Bitcoin Cash fork', id: '478558', note: 'Block 478,558 · Aug 1, 2017 — last shared BTC/BCH block.' },
  { title: 'SegWit activation', id: '481824', note: 'Block 481,824 · Aug 24, 2017 — fixed malleability; increased capacity.' },
  { title: '500K block milestone', id: '500000', note: 'Block 500,000 · Dec 18, 2017 — reached during the 2017 bull run.' },
  { title: 'Third halving', id: '630000', note: 'Block 630,000 · May 11, 2020 — reward 12.5 → 6.25 BTC.' },
  { title: 'Block 666,666', id: '666666', note: 'Block 666,666 · Jan 18, 2021 — a memorable vanity block number.' },
  { title: 'Romans 12:21 message', id: '057954bb28527ff9c7701c6fd2b7f770163718ded09745da56cc95e7606afe99', label: 'tx', note: 'Jan 2021 — “overcome evil with good” (Romans 12:21), embedded via vanity addresses.' },
  { title: 'Taproot activation', id: '709632', note: 'Block 709,632 · Nov 14, 2021 — Schnorr signatures and smart contracts.' },
  { title: 'First Ordinals inscription', id: '6fb976ab49dcec017f1e201e84395983204ae1a7c2abf7ced0a85d692e442799', label: 'tx', note: 'Dec 14, 2022 (block 767,430) — inscription #0, the first Ordinal, by Casey Rodarmor.' },
  { title: 'Largest block (at the time)', id: '774628', note: 'Block 774,628 · Feb 1, 2023 — 3.96 MB Taproot Wizard inscription.' },
  { title: 'Fourth halving', id: '840000', note: 'Block 840,000 · Apr 20, 2024 — reward 6.25 → 3.125 BTC; Runes launched.' },
];

// Block-height entries carry no label; transaction entries are tagged 'tx' and
// open by txid. More transaction-level entries still to confirm against the
// chain before adding:
//   - Payment-type firsts (P2PKH, P2SH, P2WPKH/P2WSH, P2TR, OP_RETURN)
//   - A Lightning force-close revealing an HTLC on-chain

// A deep link into the book for a contents id. A bare height opens as ?block=;
// a 64-hex value (block hash or txid) opens as ?txid=, which the book resolves
// as a block first and a transaction second.
export function entryHref(id) {
  return /^[0-9]+$/.test(id) ? `bitcoin-book.html?block=${id}` : `bitcoin-book.html?txid=${id}`;
}
