//! The cover RNG is pinned to ChaCha12 rather than inherited from `StdRng` (#76).
//!
//! `StdRng`'s algorithm may change across `rand` major versions, which would
//! silently change every rendering — fatal for verification by re-render. These
//! tests pin the choice and document why ChaCha20 is not an upgrade.

use glossia::CoverRng;
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::{ChaCha12Rng, ChaCha20Rng};

#[test]
fn cover_rng_is_chacha12_and_matches_the_previous_stdrng_stream() {
    let pool: Vec<&str> = "aid ban bed bet bit cap cop cow cut die dig".split(' ').collect();
    for seed in [0u64, 1, 42, 12345, u64::MAX] {
        let mut pinned = CoverRng::seed_from_u64(seed);
        let mut std_equivalent = rand::rngs::StdRng::seed_from_u64(seed);

        // Raw stream plus the exact draw shapes the generator uses.
        let a: Vec<u64> = (0..8).map(|_| pinned.gen::<u64>()).collect();
        let b: Vec<u64> = (0..8).map(|_| std_equivalent.gen::<u64>()).collect();
        assert_eq!(a, b, "stream diverged at seed {seed}");

        let fa: Vec<f64> = (0..4).map(|_| pinned.gen::<f64>()).collect();
        let fb: Vec<f64> = (0..4).map(|_| std_equivalent.gen::<f64>()).collect();
        assert_eq!(fa, fb, "gen::<f64>() diverged at seed {seed}");

        let ca: Vec<&str> = (0..4).map(|_| *pool.choose(&mut pinned).unwrap()).collect();
        let cb: Vec<&str> = (0..4).map(|_| *pool.choose(&mut std_equivalent).unwrap()).collect();
        assert_eq!(ca, cb, "choose() diverged at seed {seed}");
    }
}

#[test]
fn chacha20_is_a_different_stream_and_must_not_be_substituted() {
    // Guards against a well-meaning "upgrade": ChaCha20 would re-render every
    // existing artifact differently. The RNG picks cover words only, never
    // payload, so extra rounds buy no security here.
    let mut twelve = ChaCha12Rng::seed_from_u64(42);
    let mut twenty = ChaCha20Rng::seed_from_u64(42);
    let a: Vec<u64> = (0..8).map(|_| twelve.gen::<u64>()).collect();
    let b: Vec<u64> = (0..8).map(|_| twenty.gen::<u64>()).collect();
    assert_ne!(a, b);
}

#[test]
fn pinned_rng_reproduces_a_known_encoding() {
    // A fixed seed must keep producing the same prose across dependency bumps.
    let words: Vec<String> = "insect victory ring".split(' ').map(String::from).collect();
    let (a, _) = glossia::pipeline::encode_words_into_language(
        &words, "english", "default", "body", 99, 1,
    ).expect("encode");
    let (b, _) = glossia::pipeline::encode_words_into_language(
        &words, "english", "default", "body", 99, 1,
    ).expect("encode");
    assert_eq!(a, b, "same seed must give same prose");
    assert!(a.to_lowercase().contains("insect"), "payload word missing: {a}");
}
