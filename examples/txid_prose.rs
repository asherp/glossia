//! A txid as prose, using the Book of Bitcoin notation (#76).
//!
//! A txid is 32 bytes, so its geometry is exactly the P2TR/P2WSH case already
//! settled: 256 data bits + 8 header bits = 264 = 24 words at 11 bits per word.
//! The header is free.
//!
//! What is NOT settled is the type mark. An address says what it is with opcode
//! glyphs, because a locking script really is opcodes. A txid is not script, so
//! it borrows from the other half of the notation — the citation scheme, where
//! `§` already means "the transaction". That keeps the rule intact: Glossia
//! encodes the entropy, notation says what the entropy is.
//!
//! Two things this checks rather than asserts:
//!   1. every candidate mark against all 24 shipped payload wordlists, so a mark
//!      cannot silently swallow an adjacent payload word;
//!   2. a real txid, round-tripped, in display byte order.
//!
//! Run: cargo run --release --example txid_prose

use glossia::codec::{checksum_seed, hex_decode, hex_encode, payload_tokens_with_markup, Markup};
use glossia::generator::data::{load_payload_words_for_wordlist, markup_collisions};
use glossia::pipeline::encode_words_into_language;
use std::collections::HashSet;

const BPW: usize = 11;
const BEST_OF: usize = 4;
const VERSION: u32 = 1;

/// Candidate marks, and what each would mean.
const CANDIDATES: [(char, &str); 12] = [
    ('\u{00A7}', "§  transaction — the citation scheme's section mark"),
    ('\u{25A0}', "■  block — the chapter mark (a block hash, not a txid)"),
    ('\u{03B2}', "β  difficulty window — the book mark"),
    ('\u{22D4}', "⋔  merkle proof — already taken, taptree siblings"),
    ('\u{2080}', "₀  subscript zero — the house index convention (⧉₂, °₄, βₙ)"),
    ('\u{2081}', "₁  subscript one"),
    ('\u{2089}', "₉  subscript nine"),
    ('.',        ".  outpoint separator, the book's §17.2 form"),
    ('#',        "#  a plain ASCII alternative, for comparison"),
    ('\u{00B7}', "·  middle dot"),
    ('\u{2236}', "∶  ratio — a colon that is not the ASCII colon"),
    (':',        ":  the txid:vout separator every tool prints"),
];

fn pack(bytes: &[u8], wl: &[String]) -> Vec<String> {
    let db = bytes.len() * 8;
    let hb = db.div_ceil(BPW) * BPW - db;      // 8 for 32 bytes
    let header = ((BPW as u32) << (hb - 4)) | VERSION;
    let bit = |i: usize| -> usize {
        if i < db { ((bytes[i / 8] >> (7 - (i % 8))) & 1) as usize }
        else { ((header >> (hb - 1 - (i - db))) & 1) as usize }
    };
    (0..(db + hb) / BPW)
        .map(|w| (0..BPW).fold(0, |a, b| (a << 1) | bit(w * BPW + b)))
        .map(|i| wl[i].clone())
        .collect()
}

fn unpack(words: &[String], n: usize, wl: &[String]) -> Option<Vec<u8>> {
    let mut bits = Vec::new();
    for w in words {
        let i = wl.iter().position(|x| x.eq_ignore_ascii_case(w))?;
        for b in (0..BPW).rev() { bits.push((i >> b) & 1); }
    }
    Some((0..n).map(|i| (0..8).fold(0u8, |a, b| (a << 1) | bits[i * 8 + b] as u8)).collect())
}

fn seed(bytes: &[u8]) -> u64 {
    let mut c = bytes.to_vec();
    c.extend_from_slice(&[BPW as u8, VERSION as u8]);
    checksum_seed(&c, 0)
}

fn render(bytes: &[u8], wl: &[String]) -> (String, Vec<String>) {
    let words = pack(bytes, wl);
    let (t, _) = encode_words_into_language(&words, "english", "default", "body", seed(bytes), BEST_OF)
        .expect("encode");
    (t, words)
}

fn main() {
    // ── 1. which marks are safe? ────────────────────────────────────
    //
    // The condition is NOT "non-alphanumeric" — that rule is what lets ⓪ and ①
    // through — it is "not a character any payload word uses". Checked against
    // every shipped wordlist, not just bip39.
    println!("MARK SAFETY — checked against all shipped payload wordlists\n");
    let hits = markup_collisions(CANDIDATES.iter().map(|(c, _)| *c));
    for (c, desc) in CANDIDATES {
        let bad: Vec<String> = hits.iter()
            .filter(|(_, chars)| chars.contains(&c))
            .map(|(list, _)| list.clone())
            .collect();
        println!("  {:<8} {:<62} {}", format!("U+{:04X}", c as u32), desc,
                 if bad.is_empty() { "safe".to_string() }
                 else { format!("COLLIDES: {}", bad.join(", ")) });
    }

    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let set: HashSet<String> = wl.iter().map(|w| w.to_lowercase()).collect();

    // ── 2. a real txid ──────────────────────────────────────────────
    //
    // The pizza transaction, in DISPLAY byte order — the order a person copies
    // from an explorer, and therefore the order the prose has to mean. Wire
    // order is the reverse; encoding that instead would round-trip perfectly and
    // still name the wrong transaction to every tool the reader owns.
    let txid = "a1075db55d416d3ca199f55b6084e2115b9345e16c5cf302fc80e9d5fbf5d48d";
    let bytes = hex_decode(txid).unwrap();
    let mark = '\u{00A7}';
    let markup = Markup::new([mark], &wl).expect("§ validates against bip39");

    let (prose, words) = render(&bytes, &wl);
    let artifact = format!("{mark} {prose}");

    println!("\n{}", "═".repeat(78));
    println!("\nTXID  {txid}\n");
    println!("  {artifact}\n");
    println!("  {} payload words / {} words total ({} bytes -> {} data bits + {} header)",
             words.len(), prose.split_whitespace().count(), bytes.len(), bytes.len() * 8,
             24 * BPW - bytes.len() * 8);

    // ── 3. it decodes ───────────────────────────────────────────────
    let harvested = payload_tokens_with_markup(&artifact, &markup, |w| set.contains(w));
    let got = unpack(&harvested, bytes.len(), &wl).expect("unpack");
    println!("\n  harvested {} words, decodes to {}", harvested.len(), hex_encode(&got));
    println!("  round trip: {}", if got == bytes { "exact" } else { "FAILED" });

    // ── 4. and it self-verifies ─────────────────────────────────────
    //
    // Worth more here than for an address. base58check and bech32 already carry
    // a checksum; a txid carries NONE — 64 hex characters with no redundancy at
    // all, where a typo is indistinguishable from a transaction that has not
    // propagated yet. The cover words supply what the format never had.
    let (rerender, _) = render(&got, &wl);
    println!("  re-render matches: {}", rerender == prose);

    let mut flipped = bytes.clone();
    flipped[17] ^= 0x08;
    let (other, ow) = render(&flipped, &wl);
    let same_words = words.iter().zip(ow.iter()).filter(|(a, b)| a == b).count();
    let toks: Vec<&str> = prose.split_whitespace().collect();
    let otoks: Vec<&str> = other.split_whitespace().collect();
    let same_toks = toks.iter().zip(otoks.iter()).filter(|(a, b)| a == b).count();
    println!("\n  one bit flipped:");
    println!("    {other}");
    println!("\n    payload words shared with the original: {same_words} of {}", words.len());
    println!("    token positions shared                 : {same_toks} of {}", toks.len().max(otoks.len()));

    // ── 5. an outpoint ──────────────────────────────────────────────
    //
    // The book prints an outpoint as §17.2 — section dot verse. That form cannot
    // be lifted verbatim: `.` and `:` are payload characters in cs/ascii7 (and
    // `.` in every image colormap list), so declaring either as markup would
    // strip a character out of real payload words. The subscript digits are
    // clean across all 24 lists, and they are already the house index
    // convention — ⧉₂, °₄, βₙ, ηₙ. So the index rides the mark: §₀.
    let vout_mark = ['\u{00A7}', '\u{2080}'];
    let outpoint = format!("\u{00A7}\u{2080} {prose}");
    let om = Markup::new(vout_mark, &wl).expect("§₀ validates");
    let oh = payload_tokens_with_markup(&outpoint, &om, |w| set.contains(w));
    println!("\n{}", "═".repeat(78));
    println!("\nOUTPOINT — output 0 of that transaction\n");
    println!("  \u{00A7}\u{2080} {prose}\n");
    println!("  harvested {} words, still decodes to the same txid: {}",
             oh.len(), unpack(&oh, bytes.len(), &wl).as_deref() == Some(&bytes[..]));
    println!("\n  The index carries no entropy and a wrong one is caught by the");
    println!("  transaction it names, so it stays a numeral. Only the 32 bytes");
    println!("  become words.");

    println!("\n{}", "═".repeat(78));
    println!("\nWHEN NOT TO DO THIS\n");
    println!("  For a CONFIRMED transaction the book already has a shorter handle:");
    println!("  the citation. III \u{03B2}2 \u{25A0}5 \u{00A7}7 is four tokens against twenty-four");
    println!("  words, and it is speakable in a breath. The trade is self-containment:");
    println!("  a citation needs the chain to resolve, the prose does not. So prose for");
    println!("  a txid in flight or quoted outside the book, citation once it is mined");
    println!("  and the reader is inside the book.");
}
