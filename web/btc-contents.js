// btc-contents.js — the curated table of contents for the Bitcoin Book: famous
// transactions and the first occurrence of each payment type. Shared by
// bitcoin-book.html (the "Bookmarks" list in the lookup card) and
// bitcoin-contents.html (the full table-of-contents page).
//
// Each `id` is handed straight to the book's lookup, so it may be a block
// height, a block hash, or a txid. Ordered chronologically (reading order).
// Extend this with more famous, notable transactions and payment-type firsts.

export const NOTABLE = [
  { title: 'The Genesis block', id: '0', label: 'block', note: 'Block 0, 3 Jan 2009 — “The Times 03/Jan/2009 Chancellor on brink of second bailout for banks.”' },
  { title: 'First transaction — Satoshi to Hal Finney', id: 'f4184fc596403b9d638783cf57adfe4c75c605f6356fbc91338530e9831e9e8f', label: 'transaction', note: 'Block 170, 12 Jan 2009 — the first bitcoin sent between two people, to a Pay-to-PubKey (P2PK) output.' },
  { title: 'The bitcoin pizza — 10,000 BTC', id: 'a1075db55d416d3ca199f55b6084e2115b9345e16c5cf302fc80e9d5fbf5d48d', label: 'transaction', note: '22 May 2010 — 10,000 BTC for two pizzas, the first real-world purchase.' },
  { title: 'The value overflow incident', id: '74638', label: 'block', note: 'Block 74638, 15 Aug 2010 — a bug briefly created 184 billion BTC; fixed by a soft fork.' },
  { title: 'First bare multisig (P2MS)', id: '60a20bd93aa49ab4b28d514ec10b06e1829ce6818ec06cd3aabd013ebcdc4bb1', label: 'transaction', note: 'Block 164467, 30 Jan 2012 — the first send to a bare 1-of-2 multisig output.' },
  { title: 'Peter Todd’s SHA-1 collision bounty', id: '9c08a4d78931342b37fd5f72900fb9983087e6f46c4a097d8a1f52c74e28eaf6', label: 'transaction', note: '23 Feb 2017 — a P2SH bounty payable to anyone who found a SHA-1 collision, claimed days after the first was published.' },
  { title: 'SegWit activates', id: '481824', label: 'block', note: 'Block 481824, 24 Aug 2017 — the first block under BIP141, enabling native P2WPKH and P2WSH.' },
  { title: 'First dual-funded Lightning channel', id: '91538cbc4aca767cb77aa0690c2a6e710e095c8eb6d8f73d53a3a29682cb7581', label: 'transaction', note: 'Block 681753, May 2021 — c-lightning opens the first dual-funded mainnet channel; the funding output is a P2WSH 2-of-2.' },
  { title: 'Taproot activates', id: '709632', label: 'block', note: 'Block 709632, 14 Nov 2021 — the first block under BIP341, enabling P2TR (key- and script-path spends).' },
];

// Type-firsts still to add (each needs its exact txid confirmed against the
// chain -- fill in and drop into NOTABLE above, chronologically):
//   - First P2PKH output (mid-Jan 2009, days after the P2PK above)
//   - First P2SH output that was later spent (BIP16, ~April 2012)
//   - First native P2WPKH spend and first P2WSH spend (block 481824+)
//   - First P2TR key-path spend and first script-path spend (block 709635)
//   - First standard OP_RETURN data output (after Bitcoin Core 0.9.0, Mar 2014)
//   - A Lightning force-close revealing an HTLC on-chain (the marquee Lightning
//     example — showcases the HTLC row in the Notation guide)

// A deep link into the book for a contents id. A bare height opens as ?block=;
// a 64-hex value (block hash or txid) opens as ?txid=, which the book resolves
// as a block first and a transaction second.
export function entryHref(id) {
  return /^[0-9]+$/.test(id) ? `bitcoin-book.html?block=${id}` : `bitcoin-book.html?txid=${id}`;
}
