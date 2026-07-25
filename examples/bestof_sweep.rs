//! Optimal best-of-N as a function of payload BYTE COUNT (#76).
//!
//! For each (byte count, N) pair, sample many payloads and measure the density
//! that best-of-N achieves — payload words / total words, the fraction of the
//! prose that is actually address data.
//!
//! Method note: the candidate pool is generated ONCE per payload, at best_of=1,
//! over seeds base+0..base+K-1. best-of-N then means "the best of the first N of
//! those", which is exactly what `generate_text_best_of` picks. That makes the
//! curve properly nested and monotone, and costs K generations per payload rather
//! than sum(N) — and it holds the header (hence the seed) fixed, so N is not
//! confounded with the seed the way it is when the budget lives in the header.
//!
//! Run: cargo run --release --example bestof_sweep [samples] [max_n]

use glossia::codec::checksum_seed;
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;
use std::sync::{Arc, Mutex};

const BPW: usize = 11;
const BYTE_COUNTS_ALL: [usize; 11] = [4, 8, 12, 16, 20, 24, 28, 32, 40, 48, 64];
/// The two sizes the address format actually uses.
const BYTE_COUNTS_ADDR: [usize; 2] = [20, 32];
const NS_ALL: [usize; 6] = [1, 2, 4, 8, 16, 32];
const NS_DEEP: [usize; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

/// Slack bits after packing `n` bytes, widened to a whole extra word when the
/// natural slack cannot hold the 5-bit header.
fn header_bits(n: usize) -> usize {
    let mut hb = (n * 8).div_ceil(BPW) * BPW - n * 8;
    if hb < 5 { hb += BPW; }
    hb
}
fn word_count(n: usize) -> usize { (n * 8 + header_bits(n)) / BPW }

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

/// Deterministic pseudo-random payload bytes, so the sweep is reproducible.
fn payload_bytes(n: usize, nonce: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    let mut x = nonce.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    while out.len() < n {
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(n);
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let samples: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(24);
    let max_n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(32);
    // "addr" restricts the sweep to the two address sizes so N can be pushed deeper.
    let focus = args.get(3).map(|s| s == "addr").unwrap_or(false);
    let byte_counts: &[usize] = if focus { &BYTE_COUNTS_ADDR } else { &BYTE_COUNTS_ALL };
    let ns: &[usize] = if focus { &NS_DEEP } else { &NS_ALL };

    let wl = Arc::new(load_payload_words_for_wordlist("english", "bip39").unwrap());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    eprintln!("sweeping {} byte counts x {samples} payloads x {max_n} candidates on {threads} threads...",
              byte_counts.len());

    // results[byte_idx][sample] = densities of the K candidates, in seed order
    let results: Arc<Mutex<Vec<Vec<Vec<f64>>>>> =
        Arc::new(Mutex::new(vec![vec![Vec::new(); samples]; byte_counts.len()]));

    let jobs: Vec<(usize, usize)> = (0..byte_counts.len())
        .flat_map(|b| (0..samples).map(move |s| (b, s)))
        .collect();
    let jobs = Arc::new(Mutex::new(jobs));

    let mut handles = Vec::new();
    for _ in 0..threads {
        let (wl, results, jobs) = (wl.clone(), results.clone(), jobs.clone());
        let byte_counts_t: Vec<usize> = byte_counts.to_vec();
        handles.push(std::thread::spawn(move || loop {
            let job = { jobs.lock().unwrap().pop() };
            let Some((bi, si)) = job else { break };
            let nbytes = byte_counts_t[bi];
            let program = payload_bytes(nbytes, (bi as u64) << 32 | si as u64);
            let words = pack(&program, &wl);
            let mut checked = program.clone();
            checked.extend_from_slice(&[BPW as u8, 1u8]);
            let base = checksum_seed(&checked, 0);

            let mut densities = Vec::with_capacity(max_n);
            for k in 0..max_n {
                let d = encode_words_into_language(
                    &words, "english", "default", "body", base.wrapping_add(k as u64), 1)
                    .map(|(t, _)| words.len() as f64 / t.split_whitespace().count().max(1) as f64)
                    .unwrap_or(0.0);
                densities.push(d);
            }
            results.lock().unwrap()[bi][si] = densities;
        }));
    }
    for h in handles { h.join().unwrap(); }
    let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();

    // ── density table ───────────────────────────────────────────────────
    println!("\nDENSITY (payload words / total words), mean over {samples} payloads\n");
    print!("  {:>6} {:>6}", "bytes", "words");
    for &n in ns { print!("{:>9}", format!("N={n}")); }
    println!("{:>10}", "gain");
    let mut best_n_for = Vec::new();
    for (bi, &nbytes) in byte_counts.iter().enumerate() {
        print!("  {:>6} {:>6}", nbytes, word_count(nbytes));
        let mut means = Vec::new();
        for &n in ns.iter() {
            let n = n.min(max_n);
            let m: f64 = results[bi].iter()
                .map(|ds| ds[..n].iter().cloned().fold(f64::MIN, f64::max))
                .sum::<f64>() / samples as f64;
            means.push(m);
            print!("{:>9.3}", m);
        }
        let gain = (means[means.len() - 1] - means[0]) / means[0] * 100.0;
        println!("{:>9.1}%", gain);

        // Smallest N whose mean density is within 1% (relative) of the N=max value.
        let target = means[means.len() - 1] * 0.99;
        let pick = ns.iter().zip(means.iter()).find(|(_, m)| **m >= target).map(|(n, _)| *n).unwrap();
        best_n_for.push((nbytes, pick, means[0], means[means.len() - 1]));
    }

    // ── total words, the number a reader sees ───────────────────────────
    println!("\nTOTAL WORDS, mean over {samples} payloads\n");
    print!("  {:>6}", "bytes");
    for &n in ns { print!("{:>9}", format!("N={n}")); }
    println!("{:>10}", "saved");
    for (bi, &nbytes) in byte_counts.iter().enumerate() {
        print!("  {:>6}", nbytes);
        let pw = word_count(nbytes) as f64;
        let mut firstw = 0.0;
        let mut lastw = 0.0;
        for (j, &n) in ns.iter().enumerate() {
            let n = n.min(max_n);
            let m: f64 = results[bi].iter()
                .map(|ds| pw / ds[..n].iter().cloned().fold(f64::MIN, f64::max))
                .sum::<f64>() / samples as f64;
            if j == 0 { firstw = m; }
            lastw = m;
            print!("{:>9.1}", m);
        }
        println!("{:>9.1}", firstw - lastw);
    }

    // ── marginal analysis ───────────────────────────────────────────────
    //
    // Density never plateaus: it is the max of N draws, so it keeps creeping up
    // roughly logarithmically. "Where does it converge" therefore has no answer —
    // the question is where the words saved stop paying for the verification
    // cost, which is linear in N (the verifier must reproduce the encoder's
    // selection, so it repeats all N generations).
    println!("\nMARGINAL COST OF EACH DOUBLING\n");
    println!("  {:>6} {:>8} {:>10} {:>12} {:>14}", "bytes", "N", "words", "words saved", "verify cost");
    for (bi, &nbytes) in byte_counts.iter().enumerate() {
        let pw = word_count(nbytes) as f64;
        let mut prev = f64::NAN;
        for &n in ns.iter() {
            let n = n.min(max_n);
            let d: f64 = results[bi].iter()
                .map(|ds| ds[..n].iter().cloned().fold(f64::MIN, f64::max))
                .sum::<f64>() / samples as f64;
            let w = pw / d;
            let saved = if prev.is_nan() { 0.0 } else { prev - w };
            println!("  {:>6} {:>8} {:>10.1} {:>12} {:>14}",
                     if prev.is_nan() { nbytes.to_string() } else { String::new() },
                     n, w,
                     if prev.is_nan() { "—".to_string() } else { format!("{saved:.2}") },
                     format!("{}x", n));
            prev = w;
        }
        println!();
    }
    println!("  Address sizes: 20 bytes (P2PKH/P2SH/P2WPKH) and 32 (P2WSH/P2TR).");
}
