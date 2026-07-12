//! Integration tests for semantic sentence-planning wiring.
//!
//! Verifies the real embedded dataset loads and — critically — that turning the
//! feature on never breaks decoding: payload words always survive, in order.

use glossia::generator::data::load_semantics;
use glossia::pipeline::encode_into_language;

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
