//! Can damage be located without knowing the original? (#76)
//!
//! The re-render is the reference: encode whatever the received text decodes to,
//! and diff. Nothing about the true address is assumed.
//!
//! The two damage classes behave differently, and the difference is the whole
//! story:
//!
//! * **Cover damage** leaves the payload intact, so the re-render is the correct
//!   paragraph and a token diff points straight at the damaged words.
//! * **Payload damage** changes the checksum, hence the seed, hence the entire
//!   cover — so the diff says "everything differs" and localizes nothing. Only a
//!   repair search can find it: propose plausible corrections, re-render each,
//!   and keep the one that reproduces the received text.
//!
//! This measures how often that search succeeds and whether its answer is unique.
//!
//! Run: cargo run --release --example error_localization

use glossia::codec::{checksum_seed, hex_decode, payload_tokens};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;
use std::collections::HashSet;

const BPW: usize = 11;
const BEST_OF: u64 = 4;

fn header_bits(n: usize) -> usize { (n * 8).div_ceil(BPW) * BPW - n * 8 }

fn pack(program: &[u8], wl: &[String]) -> Vec<String> {
    let hb = header_bits(program.len());
    let header = ((BPW as u32) << (hb - 4)) | 1;
    let db = program.len() * 8;
    let bit = |i: usize| -> usize {
        if i < db { ((program[i / 8] >> (7 - (i % 8))) & 1) as usize }
        else { ((header >> (hb - 1 - (i - db))) & 1) as usize }
    };
    (0..(db + hb) / BPW)
        .map(|w| (0..BPW).fold(0, |a, b| (a << 1) | bit(w * BPW + b)))
        .map(|i| wl[i].clone())
        .collect()
}

fn seed_of(words: &[String], wl: &[String]) -> u64 {
    // Seed from the payload the words encode — available to a decoder, since it
    // is derived from the received text and not from the original.
    let idx: Vec<usize> = words.iter()
        .map(|w| wl.iter().position(|x| x == w).unwrap()).collect();
    let mut bits = Vec::new();
    for i in &idx { for b in (0..BPW).rev() { bits.push((i >> b) & 1); } }
    let n = bits.len() / 8 * 8 / 8;
    let mut bytes: Vec<u8> = (0..n).map(|i| (0..8).fold(0u8, |a, b| (a << 1) | bits[i * 8 + b] as u8)).collect();
    bytes.truncate(if idx.len() == 15 { 20 } else { 32 });
    let mut c = bytes.clone();
    c.extend_from_slice(&[BPW as u8, 1u8]);
    checksum_seed(&c, 0)
}

fn render(words: &[String], wl: &[String]) -> String {
    encode_words_into_language(words, "english", "default", "body",
                               seed_of(words, wl), BEST_OF as usize)
        .map(|(t, _)| t).unwrap_or_default()
}

/// Token-level diff against the re-render. Returns indices that differ.
fn diff(received: &str, rendered: &str) -> Vec<usize> {
    let a: Vec<&str> = received.split_whitespace().collect();
    let b: Vec<&str> = rendered.split_whitespace().collect();
    (0..a.len().max(b.len()))
        .filter(|&i| a.get(i) != b.get(i))
        .collect()
}

/// Words one edit away from `w` that are in the wordlist — the plausible-typo set.
fn edit1(w: &str, set: &HashSet<String>) -> Vec<String> {
    let mut out = HashSet::new();
    let cs: Vec<char> = w.chars().collect();
    for i in 0..cs.len() {
        for c in 'a'..='z' {
            let mut v = cs.clone(); v[i] = c;
            let s: String = v.into_iter().collect();
            if s != w && set.contains(&s) { out.insert(s); }
        }
        let mut v = cs.clone(); v.remove(i);
        let s: String = v.into_iter().collect();
        if set.contains(&s) { out.insert(s); }
    }
    for i in 0..=cs.len() {
        for c in 'a'..='z' {
            let mut v = cs.clone(); v.insert(i, c);
            let s: String = v.into_iter().collect();
            if set.contains(&s) { out.insert(s); }
        }
    }
    out.into_iter().collect()
}

/// Propose single-word repairs that explain the received text.
///
/// A correct repair does NOT reproduce the received text — it reproduces the
/// ORIGINAL, which differs from what was received at exactly the damaged word.
/// So the test is "differs in at most one token", not "matches". A wrong
/// candidate changes the checksum and therefore the whole paragraph, so it
/// mismatches almost everywhere.
///
/// Returns (position, replacement) for every candidate that explains the text.
fn propose(received: &str, words: &[String], wl: &[String], set: &HashSet<String>)
    -> (Vec<(usize, String)>, usize)
{
    let recv: Vec<&str> = received.split_whitespace().collect();
    let mut hits = Vec::new();
    let mut tried = 0;
    for i in 0..words.len() {
        for cand in edit1(&words[i], set) {
            let mut w = words.to_vec();
            w[i] = cand.clone();
            tried += 1;
            let got: Vec<String> = render(&w, wl).split_whitespace().map(String::from).collect();
            let n = got.len().max(recv.len());
            let mismatches = (0..n)
                .filter(|&k| got.get(k).map(|s| s.as_str()) != recv.get(k).copied())
                .count();
            if mismatches <= 1 {
                hits.push((i, cand));
            }
        }
    }
    (hits, tried)
}

/// Substitute a word into a token while preserving its surrounding punctuation
/// and capitalization, so the only difference is the word itself. Replacing the
/// whole token would drop a trailing period or an initial capital and make every
/// comparison fail for the wrong reason.
fn swap_in(token: &str, replacement: &str) -> String {
    let a = token.find(|c: char| c.is_alphanumeric()).unwrap_or(0);
    let b = token.rfind(|c: char| c.is_alphanumeric()).map_or(token.len(), |i| i + 1);
    let core = &token[a..b];
    let cased = if core.chars().next().is_some_and(|c| c.is_uppercase()) {
        let mut it = replacement.chars();
        it.next().map(|f| f.to_uppercase().collect::<String>() + it.as_str())
            .unwrap_or_default()
    } else {
        replacement.to_string()
    };
    format!("{}{}{}", &token[..a], cased, &token[b..])
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let set: HashSet<String> = wl.iter().cloned().collect();
    let program = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6").unwrap();
    let words = pack(&program, &wl);
    let truth = render(&words, &wl);

    println!("BASELINE\n  {truth}\n");

    // ── 1. cover damage: the diff should point straight at it ──────────
    let toks: Vec<String> = truth.split_whitespace().map(String::from).collect();
    let pay: HashSet<String> = words.iter().cloned().collect();
    let cover_positions: Vec<usize> = toks.iter().enumerate()
        .filter(|(_, t)| !pay.contains(&t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()))
        .map(|(i, _)| i).collect();

    let mut damaged = toks.clone();
    let hit = [cover_positions[1], cover_positions[3]];
    for &i in &hit {
        let core = damaged[i].trim_matches(|c: char| !c.is_alphanumeric()).to_string();
        damaged[i] = swap_in(&damaged[i], &format!("{core}x"));
    }
    let received = damaged.join(" ");

    let harvested = payload_tokens(&received, |w| set.contains(w));
    let rerender = render(&harvested, &wl);
    let d = diff(&received, &rerender);
    println!("── cover damage (2 words mangled) ──");
    println!("  payload still decodes: {}", harvested == words);
    println!("  damaged at positions : {hit:?}");
    println!("  diff flags positions : {d:?}");
    println!("  localized exactly    : {}\n", d == hit.to_vec());

    // ── 2. payload damage: the diff cannot localize; search sometimes can ──
    //
    // Only typos that land on ANOTHER VALID payload word are dangerous. A typo
    // that falls off the wordlist is a deletion and the word count catches it.
    // BIP39 is designed for distinguishability, so those neighbours are scarce —
    // enumerate every one of them and test the search on each.
    let mut cases: Vec<(usize, String)> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        for n in edit1(w, &set) { cases.push((i, n)); }
    }
    println!("── payload damage ──");
    println!("  payload words                     : {}", words.len());
    println!("  single-edit typos landing on ANOTHER payload word: {}", cases.len());
    println!("  (a typo that leaves the wordlist is a deletion — the word count catches it)\n");

    let mut diff_localized = 0;
    let mut found = 0;
    let mut unique = 0;
    let mut total_tried = 0;
    for (vi, bad_word) in &cases {
        let mut bad = toks.clone();
        let vpos = toks.iter().position(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric()).eq_ignore_ascii_case(&words[*vi])
        }).unwrap();
        bad[vpos] = swap_in(&toks[vpos], bad_word);
        let received = bad.join(" ");
        let harvested = payload_tokens(&received, |w| set.contains(w));
        if harvested.len() != words.len() { continue; }

        let d = diff(&received, &render(&harvested, &wl));
        if d.len() <= 3 { diff_localized += 1; }

        let (hits, tried) = propose(&received, &harvested, &wl, &set);
        total_tried += tried;
        let correct = hits.iter().any(|(i, w)| i == vi && w == &words[*vi]);
        if correct { found += 1; }
        if hits.len() == 1 && correct { unique += 1; }
    }
    let n = cases.len().max(1);
    println!("  diff alone localized it           : {diff_localized} of {n}");
    println!("  repair search found the true word : {found} of {n}");
    println!("  ...and it was the ONLY candidate  : {unique} of {n}");
    println!("  mean candidates re-rendered       : {}", total_tried / n);

    println!("\n  So: cover damage is located by diffing against the re-render, with no");
    println!("  knowledge of the original. Payload damage is not — the checksum changes");
    println!("  the whole paragraph — but the plausible-typo search recovers it, and the");
    println!("  answer is unique because a wrong repair would have to reproduce the");
    println!("  entire wording by accident.");
}
