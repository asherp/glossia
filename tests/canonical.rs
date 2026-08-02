//! The canonical encoding's contract: deterministic, versioned, verifiable.
//!
//! The golden tests here are the freeze that makes version rules meaningful.
//! If one fails, a change altered what a shipped canonical version renders —
//! that is a format break for every existing artifact. The fix is never to
//! update the golden: add a new version in `src/canonical.rs`, bump
//! `CANONICAL_VERSION`, and pin new goldens for the new version alongside
//! these.

use glossia::{
    canonical_decode, canonical_encode, canonical_encode_at, rules_for, CanonicalError,
    CANONICAL_VERSION,
};

const LANGS: &[(&str, &str)] = &[
    ("english", "bip39"),
    ("latin", "default"),
    ("czech", "default"),
];

fn payloads() -> Vec<Vec<u8>> {
    vec![
        vec![0xAB],                  // single byte
        vec![0u8; 8],                // all zeros (leading zeros must survive)
        (0u8..20).collect(),         // hash160-sized
        (100u8..132).collect(),      // witness-program-sized
    ]
}

#[test]
fn round_trips_and_verifies_across_languages() {
    for (language, wordlist) in LANGS {
        for payload in payloads() {
            let text = canonical_encode(&payload, language, wordlist)
                .unwrap_or_else(|e| panic!("{language} encode: {e}"));
            let d = canonical_decode(&text, language, wordlist)
                .unwrap_or_else(|e| panic!("{language} decode: {e}"));
            assert_eq!(d.payload, payload, "{language} payload round trip");
            assert_eq!(d.version, CANONICAL_VERSION, "{language} version byte");
            assert!(d.verified, "{language} clean artifact must verify");
        }
    }
}

#[test]
fn encoding_is_deterministic() {
    let payload: Vec<u8> = (0u8..20).collect();
    for (language, wordlist) in LANGS {
        let a = canonical_encode(&payload, language, wordlist).unwrap();
        let b = canonical_encode(&payload, language, wordlist).unwrap();
        let c = canonical_encode_at(&payload, language, wordlist, CANONICAL_VERSION).unwrap();
        assert_eq!(a, b, "{language} repeat encode");
        assert_eq!(a, c, "{language} explicit-version encode");
    }
}

#[test]
fn payload_word_swap_decodes_different_and_unverified() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    // "valve" is a payload word in this rendering; "zebra" is a different
    // BIP39 word not present in it. The swap keeps the word count, so it
    // decodes cleanly — to different bytes — and only the wording check
    // catches it.
    assert!(text.contains("valve"), "expected payload word missing: {text}");
    let damaged = text.replace("valve", "zebra");
    let d = canonical_decode(&damaged, "english", "bip39").unwrap();
    assert_ne!(d.payload, payload, "swapped payload word must change the bytes");
    assert!(!d.verified, "damaged payload must not verify");
}

#[test]
fn cover_word_damage_keeps_payload_but_unverifies() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    // "may" is a cover word here — not in BIP39 — so mangling it leaves the
    // payload intact and only the wording disagrees.
    assert!(text.contains(" may "), "expected cover word missing: {text}");
    let damaged = text.replacen(" may ", " mays ", 1);
    let d = canonical_decode(&damaged, "english", "bip39").unwrap();
    assert_eq!(d.payload, payload, "cover damage must not change the payload");
    assert!(!d.verified, "cover damage must not verify");
}

#[test]
fn verification_ignores_punctuation_and_case() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    let reformatted = text.replace('.', " .").to_uppercase();
    let d = canonical_decode(&reformatted, "english", "bip39").unwrap();
    assert_eq!(d.payload, payload);
    assert!(d.verified, "punctuation spacing and case are formatting, not damage");
}

#[test]
fn unknown_version_is_refused_by_name() {
    // Craft an artifact claiming version 200 through the unversioned seam.
    let tree = glossia::cached_payload_tree("english", "bip39").unwrap();
    let mut bytes = vec![200u8];
    bytes.extend(0u8..8);
    let words = glossia::codec::encode_base_n(&bytes, &tree, "bitpack").unwrap();
    let (text, _) = glossia::pipeline::encode_words_into_language(
        &words, "english", "bip39", "body", 7, 1,
    )
    .unwrap();
    match canonical_decode(&text, "english", "bip39") {
        Err(CanonicalError::UnsupportedVersion(200)) => {}
        other => panic!("expected UnsupportedVersion(200), got {other:?}"),
    }
    // Encoding at an unknown version is refused the same way.
    match canonical_encode_at(&[1, 2, 3], "english", "bip39", 200) {
        Err(CanonicalError::UnsupportedVersion(200)) => {}
        other => panic!("expected UnsupportedVersion(200), got {other:?}"),
    }
}

#[test]
fn empty_payload_is_refused() {
    match canonical_encode(&[], "english", "bip39") {
        Err(CanonicalError::EmptyPayload) => {}
        other => panic!("expected EmptyPayload, got {other:?}"),
    }
}

#[test]
fn canonical_ignores_the_semantics_escape_hatch() {
    // GLOSSIA_DISABLE_SEMANTICS changes what the unversioned entry points
    // render. It must NOT change a canonical rendering, or a verifier with the
    // hatch set would reject every valid artifact. (Safe to toggle here: every
    // test in this file goes through the canonical path, which ignores it.)
    let payload: Vec<u8> = (0u8..20).collect();
    let before = canonical_encode(&payload, "english", "bip39").unwrap();
    std::env::set_var("GLOSSIA_DISABLE_SEMANTICS", "1");
    let during = canonical_encode(&payload, "english", "bip39").unwrap();
    std::env::remove_var("GLOSSIA_DISABLE_SEMANTICS");
    assert_eq!(before, during, "env hatch must not affect canonical renderings");
}

// ═══════════════════════════════════════════════════════════════════════
// Version 1 freeze
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn v1_rules_are_frozen() {
    let rules = rules_for(1).expect("v1 must always exist");
    // Changing any of these re-renders every v1 artifact. If a change here is
    // intended, it belongs in a NEW version — see the module docs.
    assert_eq!(rules.best_of, 4);
    assert_eq!(rules.dialect, "body");
    assert_eq!(rules.semantics_languages, &["english"]);
}

#[test]
fn v1_golden_renderings() {
    // Exact canonical renderings, pinned. These freeze everything the
    // rendering reads: grammar, dialect config, cover wordlists, English's
    // semantics.yaml, the RNG, and the best-of selection. A diff here means a
    // v1 artifact in the wild no longer verifies.
    let hash160: Vec<u8> = (0u8..20).collect();
    let zeros = vec![0u8; 8];

    assert_eq!(
        canonical_encode(&hash160, "english", "bip39").unwrap(),
        "Its absurd absurd abandon dog. Alcohol may doctor its odd loan. Son bring abuse to anxiety. Flash addict its bright valve. An ancient cow embark our gas."
    );
    assert_eq!(
        canonical_encode(&zeros, "english", "bip39").unwrap(),
        "The absent absurd abandon abandon. Our abandon abandon abandon to abandon."
    );
    assert_eq!(
        canonical_encode(&zeros, "latin", "default").unwrap(),
        "Tu aro fas e jul. Fas aro   se  is."
    );
    assert_eq!(
        canonical_encode(&zeros, "czech", "default").unwrap(),
        "Aktovka mít amputace se abdikace. Abdikace dát ze abdikace. Abdikace mít abdikace se abdikace."
    );
}
