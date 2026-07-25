//! Does flipping one payload bit produce a recognizably different paragraph? (#76)
//!
//! This is the reader-facing validation criterion: not "how many bits of entropy
//! does the cover carry" but "would a human notice the artifact changed". The
//! figure that matters is the WORST case — the single most similar rendering
//! across the whole sweep — because that is the one a reader could wave through.
//!
//! A single flipped bit lands inside exactly one 11-bit group, so exactly one of
//! the 15 payload words changes and fourteen stay put. That alone is easy to miss.
//! The checksum-seeded cover is what turns a one-word difference into a different
//! paragraph.
//!
//! Run: cargo run --release --example bit_flip_legibility

use glossia::codec::{checksum_seed, hex_decode};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;

const COUNTER_RANGE: u64 = 1;

fn pack(program: &[u8], bits_per_word: usize, header: u32, header_bits: usize) -> Vec<usize> {
    let data_bits = program.len() * 8;
    let n_words = (data_bits + header_bits) / bits_per_word;
    let bit = |i: usize| -> usize {
        if i < data_bits {
            ((program[i / 8] >> (7 - (i % 8))) & 1) as usize
        } else {
            ((header >> (header_bits - 1 - (i - data_bits))) & 1) as usize
        }
    };
    (0..n_words)
        .map(|w| (0..bits_per_word).fold(0, |acc, b| (acc << 1) | bit(w * bits_per_word + b)))
        .collect()
}

/// Render a payload with checksum-seeded cover. No counter sweep — the address
/// determines its prose exactly.
fn render(program: &[u8], header: u32, header_bits: usize, wl: &[String], bpw: usize) -> (String, Vec<String>) {
    let idx = pack(program, bpw, header, header_bits);
    let words: Vec<String> = idx.iter().map(|&i| wl[i].clone()).collect();
    let mut checked = program.to_vec();
    checked.extend_from_slice(&[bpw as u8, 1u8]);
    let seed = checksum_seed(&checked, 0);
    let (text, _) = encode_words_into_language(&words, "english", "default", "body", seed, COUNTER_RANGE as usize)
        .expect("encode");
    (text, words)
}

/// Fraction of token positions holding the same word.
fn token_match(a: &str, b: &str) -> f64 {
    let (x, y): (Vec<&str>, Vec<&str>) = (a.split_whitespace().collect(), b.split_whitespace().collect());
    let n = x.len().max(y.len()).max(1);
    x.iter().zip(y.iter()).filter(|(p, q)| p == q).count() as f64 / n as f64
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let bpw = wl.len().trailing_zeros() as usize;
    let program = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6").unwrap();
    let header_bits = 15 * bpw - program.len() * 8;
    let header = ((bpw as u32) << (header_bits - 4)) | 1;

    let (base_text, base_words) = render(&program, header, header_bits, &wl, bpw);
    println!("BASELINE  (P2WPKH, bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4)\n");
    println!("  {base_text}\n");
    println!("  {} payload / {} total words\n", base_words.len(), base_text.split_whitespace().count());
    println!("{}", "═".repeat(78));

    // Every single-bit flip of the 160-bit program.
    let mut results: Vec<(f64, usize, String, Vec<String>)> = Vec::new();
    for b in 0..program.len() * 8 {
        let mut p = program.clone();
        p[b / 8] ^= 1 << (7 - (b % 8));
        let (text, words) = render(&p, header, header_bits, &wl, bpw);
        let diff_words = base_words.iter().zip(words.iter()).filter(|(a, c)| a != c).count();
        results.push((token_match(&base_text, &text), diff_words, text, words));
    }

    let n = results.len();
    let mean = results.iter().map(|r| r.0).sum::<f64>() / n as f64;
    let payload_diffs: Vec<usize> = results.iter().map(|r| r.1).collect();
    let max_payload_diff = *payload_diffs.iter().max().unwrap();
    let min_payload_diff = *payload_diffs.iter().min().unwrap();

    let mut sorted = results;
    sorted.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    println!("\n{n} single-bit flips of the 160-bit program\n");
    println!("  payload words differing : {min_payload_diff}–{max_payload_diff} of {} (a bit lands in one 11-bit group)", base_words.len());
    println!("  mean token match        : {:.0}%", mean * 100.0);
    println!("  WORST CASE (most similar): {:.0}%", sorted[0].0 * 100.0);
    println!("  best case (most different): {:.0}%", sorted[n - 1].0 * 100.0);

    let buckets = [(0.0, 0.2), (0.2, 0.4), (0.4, 0.6), (0.6, 0.8), (0.8, 1.01)];
    println!("\n  distribution of token match:");
    for (lo, hi) in buckets {
        let c = sorted.iter().filter(|r| r.0 >= lo && r.0 < hi).count();
        println!("    {:>3.0}–{:<3.0}%  {:<4} {}", lo * 100.0, hi * 100.0, c, "█".repeat(c * 40 / n.max(1)));
    }

    // Where does divergence START? A reader anchors on the opening, so a shared
    // prefix matters more than an overall match rate.
    let base_toks: Vec<&str> = base_text.split_whitespace().collect();
    let prefixes: Vec<usize> = sorted
        .iter()
        .map(|(_, _, text, _)| {
            let t: Vec<&str> = text.split_whitespace().collect();
            base_toks.iter().zip(t.iter()).take_while(|(a, b)| a == b).count()
        })
        .collect();
    let first_sentence_len = base_toks.iter().position(|w| w.ends_with('.')).map_or(0, |i| i + 1);
    let share_first_sentence = prefixes.iter().filter(|&&p| p >= first_sentence_len).count();
    let mut sorted_pref = prefixes.clone();
    sorted_pref.sort_unstable();

    println!("\n  divergence onset (shared leading tokens):");
    println!("    first sentence is {first_sentence_len} tokens");
    println!("    median shared prefix : {} tokens", sorted_pref[n / 2]);
    println!("    longest shared prefix: {} tokens", sorted_pref[n - 1]);
    println!(
        "    flips reproducing the WHOLE first sentence: {share_first_sentence} of {n} ({:.0}%)",
        share_first_sentence as f64 / n as f64 * 100.0
    );
    println!(
        "    flips differing at token 1: {} of {n}",
        prefixes.iter().filter(|&&p| p == 0).count()
    );

    println!("\n{}", "═".repeat(78));
    println!("\nTHE THREE MOST SIMILAR RENDERINGS — the hardest cases to notice:\n");
    for (m, d, text, _) in sorted.iter().take(3) {
        println!("  [{:.0}% token match, {d} payload word(s) changed]", m * 100.0);
        println!("  {text}\n");
    }
    println!("A TYPICAL CASE:\n");
    let mid = &sorted[n / 2];
    println!("  [{:.0}% token match, {} payload word(s) changed]", mid.0 * 100.0, mid.1);
    println!("  {}\n", mid.2);

    // Same sweep for the 32-byte size (P2TR / P2WSH), summary only.
    println!("{}", "═".repeat(78));
    let p2tr = hex_decode("da4710964f7852695de2da025290e24af6d8c281de5a0b902b7135fd9fd74d21").unwrap();
    let hb32 = 24 * bpw - p2tr.len() * 8;
    let hdr32 = ((bpw as u32) << (hb32 - 4)) | 1;
    let (base32, words32) = render(&p2tr, hdr32, hb32, &wl, bpw);
    let base32_toks: Vec<&str> = base32.split_whitespace().collect();
    let fs32 = base32_toks.iter().position(|w| w.ends_with('.')).map_or(0, |i| i + 1);

    let mut m32: Vec<f64> = Vec::new();
    let mut pref32: Vec<usize> = Vec::new();
    let mut worst32 = (0.0f64, String::new());
    for b in 0..p2tr.len() * 8 {
        let mut q = p2tr.clone();
        q[b / 8] ^= 1 << (7 - (b % 8));
        let (text, w) = render(&q, hdr32, hb32, &wl, bpw);
        assert_eq!(words32.iter().zip(w.iter()).filter(|(a, c)| a != c).count(), 1);
        let m = token_match(&base32, &text);
        if m > worst32.0 { worst32 = (m, text.clone()); }
        let t: Vec<&str> = text.split_whitespace().collect();
        pref32.push(base32_toks.iter().zip(t.iter()).take_while(|(a, b)| a == b).count());
        m32.push(m);
    }
    let n32 = m32.len();
    pref32.sort_unstable();
    println!("\n32-BYTE PROGRAM (P2TR), {n32} single-bit flips — {} payload / {} total words\n",
             words32.len(), base32_toks.len());
    println!("  {base32}\n");
    println!("  mean token match         : {:.0}%", m32.iter().sum::<f64>() / n32 as f64 * 100.0);
    println!("  WORST CASE (most similar): {:.0}%", worst32.0 * 100.0);
    println!("  median shared prefix     : {} tokens (first sentence is {fs32})", pref32[n32 / 2]);
    println!("  flips reproducing the whole first sentence: {} of {n32}",
             pref32.iter().filter(|&&x| x >= fs32).count());
    println!("\n  worst case:\n  {}", worst32.1);
}
