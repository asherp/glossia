//! Where does a single encode call spend its time? (#76)
//! Run: cargo run --release --example encode_prep_cost
use glossia::generator::data::{load_payload_words_for_wordlist, load_semantics, load_semantics_cached,
                               load_cover_words_by_pos_for_wordlist, build_pos_mapping_for_wordlist};
use glossia::grammar::{DialectConfig, Grammar};
use glossia::pipeline::{build_zone_generator, encode_words_into_language};
use std::collections::HashSet;
use std::time::Instant;

fn bench<T>(n: u32, label: &str, mut f: impl FnMut() -> T) {
    let _ = f();
    let t = Instant::now();
    for _ in 0..n { let _ = f(); }
    println!("  {:<46} {:>8.2} ms", label, t.elapsed().as_secs_f64() * 1000.0 / n as f64);
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let words: Vec<String> = (0..15).map(|i| wl[i * 137].clone()).collect();
    let set: HashSet<String> = wl.iter().map(|w| w.to_lowercase()).collect();

    println!("PER-CALL COST (release, native)\n");
    bench(50, "load_payload_words_for_wordlist (cached)", || load_payload_words_for_wordlist("english", "bip39").unwrap());
    bench(50, "build_pos_mapping_for_wordlist", || build_pos_mapping_for_wordlist("english", "bip39").unwrap());
    bench(50, "Grammar::from_language_dialect", || Grammar::from_language_dialect("english", "body").unwrap());
    bench(50, "Grammar::from_language_dialect_cached", || Grammar::from_language_dialect_cached("english", "body").unwrap());
    bench(20, "DialectConfig::from_language_dialect", || DialectConfig::from_language_dialect("english", "body").unwrap());
    bench(50, "DialectConfig::..._cached", || DialectConfig::from_language_dialect_cached("english", "body").unwrap());
    bench(50, "load_cover_words_by_pos_for_wordlist", || load_cover_words_by_pos_for_wordlist(&set, "english", "cover"));
    bench(20, "load_semantics (uncached)", || load_semantics("english"));
    bench(50, "load_semantics_cached", || load_semantics_cached("english"));
    bench(50, "build_zone_generator", || build_zone_generator(15, "english", "body", &set, false, None, None, None, None).unwrap());
    println!();
    bench(20, "encode_words_into_language best_of=1", || encode_words_into_language(&words, "english", "default", "body", 42, 1).unwrap());
    bench(20, "encode_words_into_language best_of=4", || encode_words_into_language(&words, "english", "default", "body", 42, 4).unwrap());
}
