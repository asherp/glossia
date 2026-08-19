//! Dump candidate encodings so an offline rig can score them for meter.
//!
//! Companion to `experiments/prosody/measure.py`. Mirrors `bestof_sweep`'s
//! method: for each payload, generate K candidates ONCE at best_of=1 over seeds
//! base+0..base+K-1, which is exactly the pool `generate_text_best_of` picks
//! from — so "best-of-N" downstream means "best of the first N of these" and the
//! curve stays properly nested.
//!
//! Emits TSV on stdout: bytes, sample, k, payload_words, total_words, text.
//! Prosody scoring lives in Python because the whole point of the rig is to
//! decide whether the Rust generator should learn about meter at all.
//!
//! Run: cargo run --release --example prosody_candidates [samples] [max_n] > candidates.tsv

use glossia::codec::checksum_seed;
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;
use std::sync::{Arc, Mutex};

const BPW: usize = 11;
/// 16 bytes ≈ a 12-word mnemonic, 20/32 the two address sizes, 64 a small blob.
const BYTE_COUNTS: [usize; 4] = [16, 20, 32, 64];

fn header_bits(n: usize) -> usize {
    let mut hb = (n * 8).div_ceil(BPW) * BPW - n * 8;
    if hb < 5 { hb += BPW; }
    hb
}

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
    let max_n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(64);

    let wl = Arc::new(load_payload_words_for_wordlist("english", "bip39").unwrap());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    eprintln!("dumping {} sizes x {samples} payloads x {max_n} candidates on {threads} threads...",
              BYTE_COUNTS.len());

    let jobs: Vec<(usize, usize)> = (0..BYTE_COUNTS.len())
        .flat_map(|b| (0..samples).map(move |s| (b, s)))
        .collect();
    let jobs = Arc::new(Mutex::new(jobs));
    let out = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut handles = Vec::new();
    for _ in 0..threads {
        let (wl, jobs, out) = (wl.clone(), jobs.clone(), out.clone());
        handles.push(std::thread::spawn(move || loop {
            let job = { jobs.lock().unwrap().pop() };
            let Some((bi, si)) = job else { break };
            let nbytes = BYTE_COUNTS[bi];
            let program = payload_bytes(nbytes, (bi as u64) << 32 | si as u64);
            let words = pack(&program, &wl);
            let mut checked = program.clone();
            checked.extend_from_slice(&[BPW as u8, 1u8]);
            let base = checksum_seed(&checked, 0);

            let mut lines = Vec::with_capacity(max_n);
            for k in 0..max_n {
                if let Ok((text, _)) = encode_words_into_language(
                    &words, "english", "default", "body", base.wrapping_add(k as u64), 1)
                {
                    let total = text.split_whitespace().count();
                    lines.push(format!("{nbytes}\t{si}\t{k}\t{}\t{total}\t{}",
                                       words.len(), text.replace('\n', " ")));
                }
            }
            out.lock().unwrap().extend(lines);
        }));
    }
    for h in handles { h.join().unwrap(); }

    let mut lines = Arc::try_unwrap(out).unwrap().into_inner().unwrap();
    lines.sort();
    println!("bytes\tsample\tk\tpayload_words\ttotal_words\ttext");
    for l in lines { println!("{l}"); }
}
