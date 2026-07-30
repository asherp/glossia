//! The WASM `get_cover_words` export is the complement of `get_payload_words`,
//! and a verifier uses it to tell "this token is connective prose" from "this
//! token is a mangled address word". The export itself is wasm32-only, but the
//! property it depends on is not: the two vocabularies have to be disjoint and
//! the cover list has to be populated. An empty list silently degrades the
//! locate-a-misspelling search back to trying every token; an overlapping one
//! makes it skip a real payload word.

use glossia::generator::data::{load_cover_words_by_pos_for_wordlist, load_payload_words_for_wordlist};
use std::collections::{BTreeSet, HashSet};

/// Exactly what the export flattens: every cover word for a language/wordlist.
fn cover(language: &str, wordlist: &str) -> BTreeSet<String> {
    let payload: HashSet<String> = load_payload_words_for_wordlist(language, wordlist)
        .unwrap()
        .iter()
        .map(|w| w.to_lowercase())
        .collect();
    let (by_pos, refined) = load_cover_words_by_pos_for_wordlist(&payload, language, "cover");
    let mut out = BTreeSet::new();
    for words in by_pos.into_values() {
        out.extend(words.into_iter().map(|w| w.to_lowercase()));
    }
    for words in refined.into_values() {
        out.extend(words.into_iter().map(|w| w.to_lowercase()));
    }
    out
}

#[test]
fn cover_vocabulary_is_populated() {
    let c = cover("english", "bip39");
    assert!(c.len() > 50, "english cover vocabulary is only {} words", c.len());
}

#[test]
fn cover_and_payload_are_disjoint() {
    let payload: HashSet<String> = load_payload_words_for_wordlist("english", "bip39")
        .unwrap()
        .iter()
        .map(|w| w.to_lowercase())
        .collect();
    let overlap: Vec<String> = cover("english", "bip39")
        .into_iter()
        .filter(|w| payload.contains(w))
        .collect();
    assert!(overlap.is_empty(), "cover words also in the payload wordlist: {overlap:?}");
}

#[test]
fn the_prose_of_a_real_encode_is_covered_by_the_two_vocabularies() {
    // The point of the export: every word of a rendering is either payload or
    // cover, so a token in neither set is a transcription error and nothing
    // else. If this ever fails, the search would treat honest prose as damage.
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let words: Vec<String> = (0..15).map(|i| wl[i * 137].clone()).collect();
    let (text, _) =
        glossia::pipeline::encode_words_into_language(&words, "english", "default", "body", 7, 4)
            .expect("encode");

    let payload: HashSet<String> = wl.iter().map(|w| w.to_lowercase()).collect();
    let cover_set = cover("english", "bip39");
    let unknown: Vec<String> = text
        .split_whitespace()
        .map(glossia::codec::normalize_token)
        .filter(|t| !t.is_empty() && !payload.contains(t) && !cover_set.contains(t))
        .collect();
    assert!(unknown.is_empty(), "rendered words in neither vocabulary: {unknown:?}");
}
