//! Is the fluency counter worth its verification cost? (#76)
//!
//! The counter lets the ENCODER shop for a more fluent rendering among N seeds.
//! Verification does not generate anything — it reproduces a fixed artifact — so
//! every counter value the encoder was allowed to use is a value the verifier
//! must try, and a repair search must try per candidate.
//!
//! Fixing the counter at 0 makes encoding deterministic: one render to encode,
//! one to verify, one per repair candidate. This measures what that costs in
//! prose quality, which is the only thing the counter buys.
//!
//! Run: cargo run --release --example counter_cost

use glossia::codec::{checksum_seed, hex_decode};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;
use std::time::Instant;

const BPW: usize = 11;

fn pack(program: &[u8], wl: &[String]) -> Vec<String> {
    let hb = (program.len() * 8).div_ceil(BPW) * BPW - program.len() * 8;
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

fn seed_for(program: &[u8]) -> u64 {
    let mut c = program.to_vec();
    c.extend_from_slice(&[BPW as u8, 1u8]);
    checksum_seed(&c, 0)
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();

    // A spread of real programs, not one lucky case.
    let programs: Vec<Vec<u8>> = [
        "751e76e8199196d454941c45d1b3a323f1433bd6",
        "62e907b15cbf27d5425399ebf6f0fb50ebb88f18",
        "b472a266d0bd89c13706a4132ccfb16f7c3b9fcb",
        "da4710964f7852695de2da025290e24af6d8c281de5a0b902b7135fd9fd74d21",
        "1863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262",
    ].iter().map(|h| hex_decode(h).unwrap()).collect();

    println!("PROSE QUALITY: counter fixed at 0 vs swept over 4\n");
    println!("  {:<10} {:>10} {:>10} {:>8}", "program", "fixed", "best-of-4", "saved");
    let (mut sum_fixed, mut sum_swept) = (0usize, 0usize);
    let mut identical = 0;
    for p in &programs {
        let words = pack(p, &wl);
        let seed = seed_for(p);
        let (a, _) = encode_words_into_language(&words, "english", "default", "body", seed, 1).unwrap();
        let (b, _) = encode_words_into_language(&words, "english", "default", "body", seed, 4).unwrap();
        let (na, nb) = (a.split_whitespace().count(), b.split_whitespace().count());
        sum_fixed += na;
        sum_swept += nb;
        if a == b { identical += 1; }
        println!("  {:<10} {:>10} {:>10} {:>8}",
                 format!("{}B", p.len()), na, nb, na as i64 - nb as i64);
    }
    println!("\n  total words : {sum_fixed} fixed vs {sum_swept} swept ({:+} words over {} addresses)",
             sum_fixed as i64 - sum_swept as i64, programs.len());
    println!("  renderings the sweep did not improve at all: {identical} of {}", programs.len());

    // ── cost side ───────────────────────────────────────────────────────
    let words = pack(&programs[0], &wl);
    let seed = seed_for(&programs[0]);

    let t = Instant::now();
    for _ in 0..10 { let _ = encode_words_into_language(&words, "english", "default", "body", seed, 1); }
    let one = t.elapsed() / 10;

    let t = Instant::now();
    for _ in 0..10 { let _ = encode_words_into_language(&words, "english", "default", "body", seed, 4); }
    let four = t.elapsed() / 10;

    println!("\nVERIFICATION COST\n");
    println!("  render, counter fixed : {one:?}");
    println!("  render, swept over 4  : {four:?}   ({:.1}x)",
             four.as_secs_f64() / one.as_secs_f64());
    println!("\n  verify one artifact     : {one:?} vs {four:?}");
    println!("  repair search, 14 cands : {:?} vs {:?}", one * 14, four * 14);

    println!("\n  Verification reproduces a fixed artifact rather than generating a new");
    println!("  one, so every counter the encoder MAY have used is a counter the verifier");
    println!("  MUST try. Fixing it removes a loop from the verifier, a multiplier from");
    println!("  the repair search, and a whole failure mode: two artifacts for one address.");
}
