//! Would a precomputed word-similarity table speed up repair search? (#76)
//!
//! The search has two halves: proposing candidates, and re-rendering each one to
//! see if it explains the received text. Precomputation removes the first. This
//! measures whether that is the half that costs anything, and what a neighbour
//! table over BIP39 actually looks like.
//!
//! Run: cargo run --release --example similarity_table

use glossia::codec::{checksum_seed, hex_decode};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

const BPW: usize = 11;

/// Words within one edit of `w` that are themselves in the list.
fn edit1(w: &str, set: &HashSet<String>) -> Vec<String> {
    let mut out = HashSet::new();
    let cs: Vec<char> = w.chars().collect();
    for i in 0..cs.len() {
        for c in 'a'..='z' {
            let mut v = cs.clone();
            v[i] = c;
            let s: String = v.into_iter().collect();
            if s != w && set.contains(&s) { out.insert(s); }
        }
        let mut v = cs.clone();
        v.remove(i);
        let s: String = v.into_iter().collect();
        if set.contains(&s) { out.insert(s); }
    }
    for i in 0..=cs.len() {
        for c in 'a'..='z' {
            let mut v = cs.clone();
            v.insert(i, c);
            let s: String = v.into_iter().collect();
            if set.contains(&s) { out.insert(s); }
        }
    }
    out.into_iter().collect()
}

/// Full Levenshtein, for the edit-2 table.
fn lev(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let set: HashSet<String> = wl.iter().cloned().collect();
    let chars: Vec<Vec<char>> = wl.iter().map(|w| w.chars().collect()).collect();

    // ── build the edit-1 table ──────────────────────────────────────────
    let t0 = Instant::now();
    let table: HashMap<&str, Vec<String>> =
        wl.iter().map(|w| (w.as_str(), edit1(w, &set))).collect();
    let build1 = t0.elapsed();

    let with: Vec<&&str> = table.iter().filter(|(_, v)| !v.is_empty()).map(|(k, _)| k).collect();
    let total1: usize = table.values().map(|v| v.len()).sum();
    let max1 = table.iter().max_by_key(|(_, v)| v.len()).unwrap();

    println!("EDIT-1 NEIGHBOUR TABLE over english/bip39 ({} words)\n", wl.len());
    println!("  build time            : {:?}", build1);
    println!("  words with a neighbour: {} of {} ({:.0}%)",
             with.len(), wl.len(), with.len() as f64 / wl.len() as f64 * 100.0);
    println!("  directed pairs        : {total1}");
    println!("  mean neighbours/word  : {:.2}", total1 as f64 / wl.len() as f64);
    println!("  most neighbours       : {:?} -> {:?}", max1.0, max1.1);
    println!("  table size (as pairs) : ~{} KB", total1 * 10 / 1024);

    // ── the edit-2 table, i.e. a looser similarity ──────────────────────
    let t1 = Instant::now();
    let mut total2 = 0usize;
    let mut with2 = 0usize;
    for i in 0..wl.len() {
        let mut n = 0;
        for j in 0..wl.len() {
            if i != j && (chars[i].len() as i32 - chars[j].len() as i32).abs() <= 2
                && lev(&chars[i], &chars[j]) <= 2 { n += 1; }
        }
        total2 += n;
        if n > 0 { with2 += 1; }
    }
    let build2 = t1.elapsed();
    println!("\nEDIT-2 TABLE (looser, catches two-character slips)\n");
    println!("  build time            : {:?}  (all-pairs: 2048² Levenshtein)", build2);
    println!("  words with a neighbour: {} of {} ({:.0}%)",
             with2, wl.len(), with2 as f64 / wl.len() as f64 * 100.0);
    println!("  directed pairs        : {total2}");
    println!("  mean neighbours/word  : {:.2}", total2 as f64 / wl.len() as f64);

    // ── where the search time actually goes ─────────────────────────────
    let program = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6").unwrap();
    let hb = (program.len() * 8).div_ceil(BPW) * BPW - program.len() * 8;
    let header = ((BPW as u32) << (hb - 4)) | 1;
    let db = program.len() * 8;
    let bit = |i: usize| -> usize {
        if i < db { ((program[i / 8] >> (7 - (i % 8))) & 1) as usize }
        else { ((header >> (hb - 1 - (i - db))) & 1) as usize }
    };
    let words: Vec<String> = (0..(db + hb) / BPW)
        .map(|w| (0..BPW).fold(0, |a, b| (a << 1) | bit(w * BPW + b)))
        .map(|i| wl[i].clone()).collect();

    let mut c = program.clone();
    c.extend_from_slice(&[BPW as u8, 1u8]);
    let seed = checksum_seed(&c, 0);

    // candidate generation, on the fly vs looked up
    let t2 = Instant::now();
    let mut n_gen = 0;
    for w in &words { n_gen += edit1(w, &set).len(); }
    let gen_time = t2.elapsed();

    let t3 = Instant::now();
    let mut n_lookup = 0;
    for w in &words { n_lookup += table[w.as_str()].len(); }
    let lookup_time = t3.elapsed();

    // one re-render
    let t4 = Instant::now();
    let _ = encode_words_into_language(&words, "english", "default", "body", seed, 4);
    let render_time = t4.elapsed();

    println!("\nWHERE THE SEARCH TIME GOES (15-word address, {n_gen} candidates)\n");
    println!("  generating candidates on the fly : {gen_time:?}");
    println!("  looking them up in the table     : {lookup_time:?}");
    println!("  ONE re-render                    : {render_time:?}");
    println!("  {n_gen} re-renders (the actual search): {:?}", render_time * n_gen as u32);
    println!("\n  candidate generation is {:.4}% of the search",
             gen_time.as_secs_f64() / (render_time.as_secs_f64() * n_gen as f64) * 100.0);
    assert_eq!(n_gen, n_lookup, "table and on-the-fly must agree");

    // ── so what DOES the table buy? coverage, not speed ─────────────────
    //
    // 63% of words have no edit-1 neighbour, so a single-character slip on them
    // leaves the wordlist and becomes a deletion the word count already catches.
    // The dangerous silent case is a slip that lands on another valid word. An
    // edit-2 table covers two-character slips too — 16x the candidates, which is
    // free to LOOK UP and expensive to RENDER. Measure what that extra coverage
    // is worth.
    let render_one = render_time.as_secs_f64();
    println!("\nWHAT THE TABLE ACTUALLY BUYS\n");
    println!("  edit-1: {:.2} candidates/word -> {:.0} per 15-word address -> {:.1}s to search",
             total1 as f64 / wl.len() as f64,
             total1 as f64 / wl.len() as f64 * 15.0,
             total1 as f64 / wl.len() as f64 * 15.0 * render_one);
    println!("  edit-2: {:.2} candidates/word -> {:.0} per 15-word address -> {:.1}s to search",
             total2 as f64 / wl.len() as f64,
             total2 as f64 / wl.len() as f64 * 15.0,
             total2 as f64 / wl.len() as f64 * 15.0 * render_one);
    println!("\n  Looking up 16x more candidates costs 4us either way. RENDERING them is");
    println!("  the difference between a second and half a minute — so the table is worth");
    println!("  building for candidate QUALITY and ORDER (try likely repairs first, stop");
    println!("  at the first explanation), not for lookup speed.");

    // Early exit is the lever the table enables: rank candidates and stop early.
    println!("\n  With ranked candidates and early exit, expected renders is about half");
    println!("  the candidate count, so edit-2 lands near {:.0}s rather than {:.0}s.",
             total2 as f64 / wl.len() as f64 * 15.0 * render_one / 2.0,
             total2 as f64 / wl.len() as f64 * 15.0 * render_one);

    // The single biggest cost lever is unrelated to the table.
    let t5 = Instant::now();
    let _ = encode_words_into_language(&words, "english", "default", "body", seed, 1);
    let one_candidate = t5.elapsed();
    println!("\n  Bigger lever, unrelated to the table: a render with best_of=1 takes {:?}",
             one_candidate);
    println!("  versus {:?} at best_of=4 — the verifier repeats the encoder's counter", render_time);
    println!("  sweep, so the format's counter range sets the search cost directly.");
}
