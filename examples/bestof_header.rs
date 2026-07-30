//! Encode the fluency budget in 2 header bits: 00→1, 01→4, 10→8, 11→16 (#76).
//!
//! Recording the sample size the encoder used means the verifier runs exactly
//! that budget rather than trying every budget it might have used (1+4+8+16 = 29
//! generations). The encoder keeps its fluency search; verification stays a
//! single deterministic encode.
//!
//! Note the header is inside the checksummed bits, so the budget field feeds back
//! into the seed: each candidate N gives a different header, hence a different
//! checksum, hence different prose. The encoder must therefore evaluate each N
//! against its own seed, which is what this does.
//!
//! Run: cargo run --release --example bestof_header

use glossia::codec::{checksum_seed, hex_decode};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;
use std::time::Instant;

const BPW: usize = 11;
/// 2-bit field -> sample size.
const BUDGETS: [usize; 4] = [1, 4, 8, 16];

fn header_bits(n: usize) -> usize { (n * 8).div_ceil(BPW) * BPW - n * 8 }

/// Header layout under test:
///   20-byte (5 slack bits): [log2-8 : 3][best_of : 2]        — version dropped
///   32-byte (8 slack bits): [log2 : 4][version : 2][best_of : 2]
fn build_header(program_len: usize, budget_code: u32, version: u32) -> u32 {
    match header_bits(program_len) {
        5 => (((BPW as u32) - 8) << 2) | budget_code,
        8 => ((BPW as u32) << 4) | (version << 2) | budget_code,
        hb => panic!("unexpected slack: {hb} bits"),
    }
}

fn pack(program: &[u8], header: u32, wl: &[String]) -> Vec<String> {
    let hb = header_bits(program.len());
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

fn seed_for(program: &[u8], header: u32) -> u64 {
    let mut c = program.to_vec();
    c.extend_from_slice(&header.to_be_bytes());
    checksum_seed(&c, 0)
}

fn render(program: &[u8], header: u32, budget: usize, wl: &[String]) -> String {
    let words = pack(program, header, wl);
    encode_words_into_language(&words, "english", "default", "body",
                               seed_for(program, header), budget)
        .map(|(t, _)| t).unwrap_or_default()
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let programs: Vec<Vec<u8>> = [
        "751e76e8199196d454941c45d1b3a323f1433bd6",
        "62e907b15cbf27d5425399ebf6f0fb50ebb88f18",
        "b472a266d0bd89c13706a4132ccfb16f7c3b9fcb",
        "da4710964f7852695de2da025290e24af6d8c281de5a0b902b7135fd9fd74d21",
        "1863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262",
    ].iter().map(|h| hex_decode(h).unwrap()).collect();

    println!("HEADER BIT BUDGET\n");
    println!("  20-byte program: 5 slack bits  = [log2-8 : 3][best_of : 2]  — no room for a version field");
    println!("  32-byte program: 8 slack bits  = [log2 : 4][version : 2][best_of : 2]");
    println!("\n  A 3-bit offset log2 covers 2^8..2^15, which spans both shipped wordlists");
    println!("  (bip39 = 2^11, latin = 2^15). Anything outside that needs the wider layout.\n");

    println!("WORD COUNT BY BUDGET (each N has its own header, hence its own prose)\n");
    println!("  {:<10} {:>7} {:>7} {:>7} {:>7}   {:>10}", "program", "N=1", "N=4", "N=8", "N=16", "best");
    let mut totals = [0usize; 4];
    let mut wins = [0usize; 4];
    for p in &programs {
        let mut counts = [0usize; 4];
        for (k, &n) in BUDGETS.iter().enumerate() {
            let h = build_header(p.len(), k as u32, 1);
            counts[k] = render(p, h, n, &wl).split_whitespace().count();
            totals[k] += counts[k];
        }
        let best = counts.iter().enumerate().min_by_key(|(_, c)| **c).unwrap().0;
        wins[best] += 1;
        println!("  {:<10} {:>7} {:>7} {:>7} {:>7}   N={:<8}",
                 format!("{}B", p.len()), counts[0], counts[1], counts[2], counts[3], BUDGETS[best]);
    }
    println!("\n  totals    {:>7} {:>7} {:>7} {:>7}", totals[0], totals[1], totals[2], totals[3]);
    println!("  chosen as best: N=1 x{}, N=4 x{}, N=8 x{}, N=16 x{}",
             wins[0], wins[1], wins[2], wins[3]);
    let best_total: usize = totals.iter().copied().min().unwrap();
    println!("\n  fixed at N=1        : {} words", totals[0]);
    println!("  per-address choice  : {} words (each address takes its own best)",
             {
                 let mut s = 0;
                 for p in &programs {
                     let mut c = usize::MAX;
                     for (k, &n) in BUDGETS.iter().enumerate() {
                         let h = build_header(p.len(), k as u32, 1);
                         c = c.min(render(p, h, n, &wl).split_whitespace().count());
                     }
                     s += c;
                 }
                 s
             });
    println!("  best single budget  : {best_total} words");

    // ── verification cost ───────────────────────────────────────────────
    let p = &programs[0];
    println!("\nVERIFICATION COST\n");
    for (k, &n) in BUDGETS.iter().enumerate() {
        let h = build_header(p.len(), k as u32, 1);
        let t = Instant::now();
        for _ in 0..5 { let _ = render(p, h, n, &wl); }
        println!("  budget recorded as N={:<3} -> verifier does {:>2} generation(s): {:?}",
                 n, n, t.elapsed() / 5);
    }
    let t = Instant::now();
    for (k, &n) in BUDGETS.iter().enumerate() {
        let h = build_header(p.len(), k as u32, 1);
        let _ = render(p, h, n, &wl);
    }
    println!("\n  without the field, a verifier must try every budget: {:?}", t.elapsed());
    println!("  ...which is also wrong, since each N yields DIFFERENT prose — the");
    println!("  verifier would accept any of four renderings for one address.");

    // ── the cheaper reading of the same 2 bits ──────────────────────────
    //
    // The field sits inside the checksummed bits, so changing it changes the seed:
    // the four values are four DIFFERENT prose pools, not four sample sizes of one
    // pool. That is why quality is non-monotonic in N. But it also means most of
    // the gain comes from having four pools to choose from, not from sampling each
    // deeply — so interpret the 2 bits as a pure SEED SELECTOR and render every
    // variant with best_of=1. The encoder still picks the shortest of four; the
    // verifier always does exactly one generation.
    println!("\n{}", "═".repeat(70));
    println!("\nSAME 2 BITS AS A PURE SEED SELECTOR (every variant at best_of=1)\n");
    println!("  {:<10} {:>7} {:>7} {:>7} {:>7}   {:>8}", "program", "sel=0", "sel=1", "sel=2", "sel=3", "best");
    let mut sel_total = 0usize;
    let mut budget_total = 0usize;
    for p in &programs {
        let mut counts = [0usize; 4];
        for k in 0..4 {
            let h = build_header(p.len(), k as u32, 1);
            counts[k] = render(p, h, 1, &wl).split_whitespace().count();
        }
        let best = *counts.iter().min().unwrap();
        sel_total += best;
        let mut bb = usize::MAX;
        for (k, &n) in BUDGETS.iter().enumerate() {
            let h = build_header(p.len(), k as u32, 1);
            bb = bb.min(render(p, h, n, &wl).split_whitespace().count());
        }
        budget_total += bb;
        println!("  {:<10} {:>7} {:>7} {:>7} {:>7}   {:>8}",
                 format!("{}B", p.len()), counts[0], counts[1], counts[2], counts[3], best);
    }

    let t = Instant::now();
    for _ in 0..5 { let _ = render(&programs[0], build_header(programs[0].len(), 0, 1), 1, &wl); }
    let verify_sel = t.elapsed() / 5;

    println!("\n  COMPARISON over {} addresses\n", programs.len());
    println!("  {:<28} {:>8}  {:>18}", "design", "words", "verify cost");
    println!("  {:<28} {:>8}  {:>18}", "no field, fixed N=1", totals[0], format!("{verify_sel:?}"));
    println!("  {:<28} {:>8}  {:>18}", "2 bits = seed selector", sel_total, format!("{verify_sel:?} (constant)"));
    println!("  {:<28} {:>8}  {:>18}", "2 bits = sample budget", budget_total, "31-290ms (varies)");
    println!("\n  The selector captures {:.0}% of the budget field's saving at constant,",
             (totals[0] - sel_total) as f64 / (totals[0] - budget_total).max(1) as f64 * 100.0);
    println!("  minimum verification cost — and keeps the encoder's search at 4 generations");
    println!("  instead of 29.");
}
