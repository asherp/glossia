//! Integration tests for semantic sentence-planning wiring.
//!
//! Verifies the real embedded dataset loads and — critically — that turning the
//! feature on never breaks decoding: payload words always survive, in order.

use glossia::generator::data::load_semantics;
use glossia::pipeline::{decode_from_language, encode_into_language, encode_into_language_best_of};

#[test]
fn english_semantics_dataset_loads() {
    let model =
        load_semantics("english").expect("english/semantics.yaml should be embedded and parse");
    let (classes, frames) = model.stats();
    // Full BIP39 dataset: ~1582 noun classes, ~1108 verb frames.
    assert!(classes > 1000, "expected many classified words, got {classes}");
    assert!(frames > 500, "expected many verb frames, got {frames}");
}

#[test]
fn other_language_has_no_semantics() {
    // Languages without a semantics.yaml must load nothing (feature stays inert).
    assert!(load_semantics("latin").is_none());
}

/// The core safety property: with the semantic model active, every payload word
/// still appears in the output, in order. Cover words carry no payload, so a
/// decoder that filters against the wordlist recovers the exact payload.
#[test]
fn semantic_planning_preserves_payload_order() {
    let (text, _payload_set, encoded_words, _mode) = encode_into_language(
        "the quick brown fox jumps over the lazy dog and the sleeping cat",
        "english",
        "default",
        "body",
        None,
        12345,
        false,
        None,
        None,
        None,
        None,
    )
    .expect("encode should succeed");

    assert!(!encoded_words.is_empty(), "expected some payload words");

    // Every encoded payload word must appear, in order, as a subsequence of the
    // output tokens. This is exactly what the decoder relies on.
    let toks: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .collect();
    let mut it = toks.iter();
    for p in &encoded_words {
        let target = p.to_lowercase();
        assert!(
            it.any(|w| *w == target),
            "payload word '{p}' missing or out of order in output:\n{text}"
        );
    }
}

/// Best-of-N selection must preserve the same safety property: whichever
/// candidate wins, every payload word still appears in order.
#[test]
fn best_of_n_preserves_payload_order() {
    let (text, _set, encoded_words, _mode) = encode_into_language_best_of(
        "semantic planning selects the most coherent candidate without dropping payload",
        "english",
        "default",
        "body",
        None,
        999,
        false,
        None,
        None,
        None,
        None,
        8, // candidates
    )
    .expect("best-of-N encode should succeed");

    assert!(!encoded_words.is_empty());
    let toks: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .collect();
    let mut it = toks.iter();
    for p in &encoded_words {
        let target = p.to_lowercase();
        assert!(
            it.any(|w| *w == target),
            "payload word '{p}' missing or out of order in best-of-N output:\n{text}"
        );
    }
}

/// The normative safety property: selecting among best-of-N candidates must not
/// change what the text decodes to. decode(encode_best_of(x)) == x, and it
/// matches the plain single-encode decode. With semantics attached by default for
/// English, this also proves coherence biasing never corrupts the payload.
#[test]
fn best_of_n_decode_round_trips() {
    let input = "meet me at the old pier at noon";

    for n in [1usize, 8, 16] {
        let (text, _set, _words, _mode) = encode_into_language_best_of(
            input, "english", "default", "body", None, 4242, false, None, None, None, None, n,
        )
        .unwrap_or_else(|e| panic!("best-of-{n} encode failed: {e:?}"));

        let decoded = decode_from_language(&text, "english", "default", false)
            .unwrap_or_else(|e| panic!("best-of-{n} decode failed: {e:?}"));

        assert_eq!(
            decoded.trim(),
            input,
            "best-of-{n}: decode(encode(x)) != x\n  text: {text}"
        );
    }
}

/// Sanity that semantics is on by default for English (the merge decision) and
/// still round-trips through the ordinary single-encode path.
#[test]
fn default_english_encode_has_semantics_and_round_trips() {
    assert!(
        load_semantics("english").is_some(),
        "English should have semantics attached by default"
    );
    let input = "the quiet harbor at dawn";
    let (text, _set, _words, _mode) =
        encode_into_language(input, "english", "default", "body", None, 77, false, None, None, None, None)
            .expect("encode");
    let decoded = decode_from_language(&text, "english", "default", false).expect("decode");
    assert_eq!(decoded.trim(), input, "default English: decode(encode(x)) != x\n  text: {text}");
}
