//! Payload word placements reported by the encoder (#76).
//!
//! The generator chooses which grammatical slot each payload word fills. These
//! tests verify the reported placement actually describes the emitted text —
//! index alignment is the part that would fail silently if it were wrong.

use glossia::codec::normalize_token;
use glossia::generator::core::{Placement, Role};
use glossia::pipeline::{encode_words_into_language, encode_words_into_language_traced};

fn payload() -> Vec<String> {
    "insect victory ring creek bonus health logic mirror elevator abandon"
        .split(' ')
        .map(String::from)
        .collect()
}

fn trace(seed: u64) -> (String, Vec<Placement>) {
    let (text, _c, p) =
        encode_words_into_language_traced(&payload(), "english", "default", "body", seed, 1)
            .expect("encode");
    (text, p)
}

#[test]
fn traced_encode_returns_the_same_text_as_untraced() {
    for seed in [1u64, 42, 9999] {
        let (a, _) = encode_words_into_language(&payload(), "english", "default", "body", seed, 1)
            .expect("encode");
        let (b, _c, _p) =
            encode_words_into_language_traced(&payload(), "english", "default", "body", seed, 1)
                .expect("encode");
        assert_eq!(a, b, "traced and untraced diverged at seed {seed}");
    }
}

#[test]
fn every_payload_word_is_placed_exactly_once_in_order() {
    for seed in [1u64, 42, 9999] {
        let (_text, p) = trace(seed);
        let words = payload();
        assert_eq!(p.len(), words.len(), "seed {seed}: expected one placement per payload word");
        for (i, pl) in p.iter().enumerate() {
            assert_eq!(pl.payload_index, i, "seed {seed}: placements out of order");
            assert_eq!(pl.word, words[i], "seed {seed}: wrong word at index {i}");
        }
    }
}

#[test]
fn token_index_points_at_the_word_in_the_emitted_text() {
    // The alignment claim. If this is wrong, every downstream use is wrong.
    for seed in [1u64, 42, 9999, 123456] {
        let (text, p) = trace(seed);
        let toks: Vec<&str> = text.split_whitespace().collect();
        for pl in &p {
            assert!(
                pl.token_index < toks.len(),
                "seed {seed}: token_index {} out of range ({} tokens)",
                pl.token_index,
                toks.len()
            );
            assert_eq!(
                normalize_token(toks[pl.token_index]),
                pl.word.to_lowercase(),
                "seed {seed}: token {} is {:?}, placement claims {:?}",
                pl.token_index,
                toks[pl.token_index],
                pl.word
            );
        }
    }
}

#[test]
fn sentence_numbers_are_monotonic_and_match_the_text() {
    for seed in [1u64, 42, 9999] {
        let (text, p) = trace(seed);
        let toks: Vec<&str> = text.split_whitespace().collect();
        let mut prev = 0;
        for pl in &p {
            assert!(pl.sentence >= prev, "seed {seed}: sentence numbers went backwards");
            prev = pl.sentence;
            // Count sentence boundaries before this token in the text.
            let ends = toks[..pl.token_index].iter().filter(|w| w.ends_with('.')).count();
            assert_eq!(
                ends, pl.sentence,
                "seed {seed}: {:?} claims sentence {} but {} sentences end before it",
                pl.word, pl.sentence, ends
            );
        }
    }
}

#[test]
fn roles_are_only_assigned_to_nominal_slots() {
    use glossia::Pos;
    for seed in [1u64, 42, 9999] {
        let (_text, p) = trace(seed);
        for pl in &p {
            if pl.role.is_some() {
                assert!(
                    matches!(pl.pos, Pos::N | Pos::Pron),
                    "seed {seed}: {:?} has role {:?} but POS {:?}",
                    pl.word, pl.role, pl.pos
                );
            }
        }
    }
}

#[test]
fn reseeding_reassigns_roles_for_the_same_payload() {
    // The property that makes role change a usable signal: identical payload,
    // different cover seed, different grammatical roles.
    let mut seen: Vec<Vec<Option<Role>>> = Vec::new();
    for seed in [1u64, 42, 9999, 123456, 777] {
        let (_text, p) = trace(seed);
        seen.push(p.iter().map(|x| x.role).collect());
    }
    assert!(
        seen.windows(2).any(|w| w[0] != w[1]),
        "role assignment never changed across seeds — not a usable signal"
    );
}

#[test]
fn non_payload_words_are_rejected_rather_than_silently_dropped() {
    // "map" is an English COVER word, not a BIP39 payload word. It has no allowed
    // POS, so the generator cannot place it and would omit it — producing prose
    // that decodes to a different payload than the caller passed in.
    let mut words = payload();
    words.insert(3, "map".to_string());
    let err = encode_words_into_language(&words, "english", "default", "body", 1, 1)
        .expect_err("a non-payload word must be rejected");
    let msg = format!("{err:?}");
    assert!(msg.contains("map"), "error should name the offending word: {msg}");

    // And the traced entry point must reject it identically.
    assert!(
        encode_words_into_language_traced(&words, "english", "default", "body", 1, 1).is_err(),
        "traced entry point must validate too"
    );
}

#[test]
fn placements_account_for_every_word_that_reaches_the_text() {
    // Cross-check against the decoder: what the tracer reports and what a decoder
    // harvests must be the same words in the same order.
    use glossia::codec::payload_tokens;
    use glossia::generator::data::load_payload_words_for_wordlist;
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let set: std::collections::HashSet<String> = wl.iter().map(|w| w.to_lowercase()).collect();
    for seed in [1u64, 42, 9999, 5, 777] {
        let (text, p) = trace(seed);
        let harvested = payload_tokens(&text, |w| set.contains(w));
        let traced: Vec<String> = p.iter().map(|x| x.word.to_lowercase()).collect();
        assert_eq!(harvested, traced, "seed {seed}: tracer and decoder disagree");
    }
}
