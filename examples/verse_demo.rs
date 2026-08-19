//! Verse dialects, end to end: how often do the lines come out whole, and what
//! does the output actually look like?
//!
//! Run: cargo run --release --example verse_demo [samples] [best_of]

use glossia::generator::data::{load_payload_words_for_wordlist, load_prosody_cached};
use glossia::generator::prosody::{layout, scans_text};
use glossia::grammar::DialectConfig;
use glossia::pipeline::encode_words_into_language;

fn payload(n: usize, nonce: u64, wl: &[String]) -> Vec<String> {
    let mut x = nonce.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13; x ^= x >> 7; x ^= x << 17;
            wl[(x as usize) % 2048].clone()
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let samples: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(50);
    let best_of: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);

    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let model = load_prosody_cached("english").expect("english ships prosody.yaml");

    println!("\n{samples} payloads x best_of={best_of}\n");
    println!("  {:>8} {:>7} {:>8} {:>9} {:>9}", "dialect", "words", "scans", "density", "lines");
    for dialect in ["body", "verse", "haiku", "blank"] {
        let spec = DialectConfig::from_language_dialect_cached("english", dialect)
            .unwrap()
            .meter()
            .cloned();
        let (mut scanned, mut words, mut lines) = (0usize, 0usize, 0usize);
        for i in 0..samples {
            let p = payload(13, i as u64, &wl);
            let (text, _) = encode_words_into_language(
                &p, "english", "default", dialect, (i as u64).wrapping_mul(7919), best_of,
            ).unwrap();
            let toks: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
            words += toks.len();
            // A meterless dialect is scored against blank verse's line length so
            // the columns are comparable — that is the "today" baseline.
            let against = spec.clone().unwrap_or(glossia::generator::prosody::MeterSpec {
                lines: vec![10],
                mode: glossia::generator::prosody::StressMode::Free,
                rise: true,
            });
            if scans_text(&toks, &model, &against) {
                scanned += 1;
            }
            lines += layout(&toks, &model, &against).len();
        }
        println!(
            "  {:>8} {:>7.1} {:>7.0}% {:>9.3} {:>9.1}",
            dialect,
            words as f64 / samples as f64,
            100.0 * scanned as f64 / samples as f64,
            13.0 * samples as f64 / words as f64,
            lines as f64 / samples as f64,
        );
    }

    for dialect in ["verse", "haiku", "blank"] {
        let spec = DialectConfig::from_language_dialect_cached("english", dialect)
            .unwrap().meter().cloned().unwrap();
        let p = payload(13, 4, &wl);
        let (text, _) = encode_words_into_language(
            &p, "english", "default", dialect, 31_337, best_of.max(8),
        ).unwrap();
        let toks: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
        println!("\n── {dialect}  scans={}", scans_text(&toks, &model, &spec));
        for l in layout(&toks, &model, &spec) {
            println!("   {l}");
        }
    }
}
