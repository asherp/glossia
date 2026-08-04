//! The canonical encoding's contract: deterministic, versioned, verifiable.
//!
//! The golden tests here are the freeze that makes version rules meaningful.
//! If one fails, a change altered what a shipped canonical version renders —
//! that is a format break for every existing artifact. The fix is never to
//! update the golden: add a new version in `src/canonical.rs`, bump
//! `CANONICAL_VERSION`, and pin new goldens for the new version alongside
//! these.

use glossia::{
    canonical_decode, canonical_decode_fixed, canonical_encode, canonical_encode_at,
    canonical_encode_fixed, canonical_encode_fixed_at, canonical_encode_fixed_traced, rules_for,
    CanonicalError, CANONICAL_VERSION,
};

/// `payload || version || crc32(payload || version)` — the envelope as
/// `src/canonical.rs` seals it, rebuilt here so a test can craft bytes the
/// public API would refuse to write.
fn envelope(payload: &[u8], version: u8) -> Vec<u8> {
    let mut bytes = payload.to_vec();
    bytes.push(version);
    bytes.extend_from_slice(&glossia::codec::crc32(&bytes).to_be_bytes());
    bytes
}

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
            // A language with no cover words for some POS (Latin's open
            // classes are almost all payload) must omit the slot, not emit an
            // empty token that widens whitespace and steals the sentence-
            // initial capital.
            assert!(!text.contains("  "), "{language} rendering has a double space: {text:?}");
            for sentence in text.split(". ") {
                let first = sentence.chars().next().unwrap();
                assert!(!first.is_lowercase(), "{language} sentence starts lowercase: {sentence:?}");
            }
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
fn payload_word_swap_is_caught_by_the_checksum() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    // "cage" is a payload word in this rendering; "zebra" is a different BIP39
    // word not present in it. The swap keeps the word count, so the words still
    // unpack — to different bytes — and it is the trailing checksum that refuses
    // them. Before the checksum this returned a decode with `verified: false`,
    // which put the burden on every caller to look at that flag.
    assert!(text.contains("cage"), "expected payload word missing: {text}");
    let damaged = text.replace("cage", "zebra");
    match canonical_decode(&damaged, "english", "bip39") {
        Err(CanonicalError::ChecksumMismatch { .. }) => {}
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
}

#[test]
fn cover_word_damage_keeps_payload_but_unverifies() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    // "via" is a cover word here — not in BIP39 — so mangling it leaves the
    // payload and its checksum intact, and only the wording disagrees. This is
    // the damage the checksum CANNOT see and the re-render can.
    assert!(text.contains(" via "), "expected cover word missing: {text}");
    let damaged = text.replacen(" via ", " vias ", 1);
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
    // Craft an artifact claiming version 200 through the unversioned seam. The
    // checksum has to be well-formed or the decoder would stop at it first.
    let tree = glossia::cached_payload_tree("english", "bip39").unwrap();
    let bytes = envelope(&(0u8..8).collect::<Vec<u8>>(), 200);
    let words = glossia::codec::encode_base_n(&bytes, &tree, "canonical_bitpack").unwrap();
    let (text, _) = glossia::pipeline::encode_words_into_language(
        &words, "english", "bip39", "body", 7, 1,
    )
    .unwrap();
    match canonical_decode(&text, "english", "bip39") {
        Err(CanonicalError::UnsupportedVersion(200)) => {}
        other => panic!("expected UnsupportedVersion(200), got {other:?}"),
    }
    // Encoding at an unknown version is refused the same way, both packings.
    match canonical_encode_at(&[1, 2, 3], "english", "bip39", 200) {
        Err(CanonicalError::UnsupportedVersion(200)) => {}
        other => panic!("expected UnsupportedVersion(200), got {other:?}"),
    }
    match canonical_encode_fixed_at(&[1, 2, 3], "english", "bip39", 200) {
        Err(CanonicalError::UnsupportedVersion(200)) => {}
        other => panic!("expected UnsupportedVersion(200), got {other:?}"),
    }
}

#[test]
fn version_1_artifacts_are_refused_rather_than_misread() {
    // v1 put the version byte FIRST and carried no checksum. This release packs
    // neither, so a v1 artifact must fail loudly. What it must NOT do is hand
    // back a payload shifted by five bytes as though it had read it.
    let tree = glossia::cached_payload_tree("english", "bip39").unwrap();
    let mut v1_bytes = vec![1u8];
    v1_bytes.extend(0u8..20);
    let words = glossia::codec::encode_base_n(&v1_bytes, &tree, "bitpack").unwrap();
    let (text, _) = glossia::pipeline::encode_words_into_language(
        &words, "english", "bip39", "body", 7, 1,
    )
    .unwrap();
    match canonical_decode(&text, "english", "bip39") {
        Err(CanonicalError::ChecksumMismatch { .. }) => {}
        Err(CanonicalError::UnsupportedVersion(_)) => {}
        Err(CanonicalError::Decode(_)) => {}
        other => panic!("a v1 artifact must be refused, got {other:?}"),
    }
    // And v1 is gone from the registry, so nothing can be written at it either.
    assert!(rules_for(1).is_none(), "v1 rules must not be reachable");
}

#[test]
fn traced_and_raw_variants_agree_with_the_plain_pair() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();

    let (traced_text, placements) =
        glossia::canonical_encode_traced(&payload, "english", "bip39").unwrap();
    assert_eq!(traced_text, text, "traced text must be the canonical rendering");
    assert!(!placements.is_empty());
    let toks: Vec<&str> = text.split_whitespace().collect();
    for p in &placements {
        let tok: String = toks[p.token_index]
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        assert_eq!(tok.to_lowercase(), p.word.to_lowercase(), "placement resolves to its token");
    }

    let d = canonical_decode(&text, "english", "bip39").unwrap();
    assert_eq!(d.canonical_text, text, "clean artifact's reference is itself");
    let (version, raw_payload) =
        glossia::canonical_decode_raw(&text, "english", "bip39").unwrap();
    assert_eq!((version, raw_payload), (d.version, d.payload));
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
// The fixed packing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fixed_round_trips_and_verifies_across_languages() {
    for (language, wordlist) in LANGS {
        for payload in payloads() {
            let text = canonical_encode_fixed(&payload, language, wordlist)
                .unwrap_or_else(|e| panic!("{language} fixed encode: {e}"));
            let d = canonical_decode_fixed(&text, language, wordlist, payload.len())
                .unwrap_or_else(|e| panic!("{language} fixed decode: {e}"));
            assert_eq!(d.payload, payload, "{language} fixed payload round trip");
            assert_eq!(d.version, CANONICAL_VERSION, "{language} fixed version");
            assert!(d.verified, "{language} clean fixed artifact must verify");
        }
    }
}

#[test]
fn fixed_spends_one_word_less_than_self_describing() {
    // The padding word is the whole difference: same bytes, same rules, one
    // fewer word to carry a length the caller already knows.
    for len in [1usize, 20, 32, 65] {
        let payload: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(37)).collect();
        let self_words = glossia::canonical_encode_traced(&payload, "english", "bip39")
            .unwrap()
            .1
            .len();
        let fixed_words = canonical_encode_fixed_traced(&payload, "english", "bip39")
            .unwrap()
            .1
            .len();
        assert_eq!(
            self_words,
            fixed_words + 1,
            "len {len}: fixed must be exactly one word shorter"
        );
    }
}

#[test]
fn the_opening_word_carries_payload_not_envelope() {
    // The regression this layout exists for. Under v1 the leading padding word
    // was a function of payload LENGTH alone, so every same-sized payload opened
    // on the same word — every 32-byte hash began "abandon". Behind the payload,
    // the opening word is payload.
    for encode_traced in [
        glossia::canonical_encode_traced
            as fn(&[u8], &str, &str) -> Result<_, CanonicalError>,
        canonical_encode_fixed_traced,
    ] {
        let mut firsts = std::collections::HashSet::new();
        for i in 0..8u8 {
            let payload: Vec<u8> = (0..32u8).map(|j| j ^ i.wrapping_mul(37)).collect();
            let (_text, placements) = encode_traced(&payload, "english", "bip39").unwrap();
            firsts.insert(placements[0].word.clone());
        }
        assert!(
            firsts.len() > 1,
            "opening word is fixed across distinct 32-byte payloads: {firsts:?}"
        );
    }
}

#[test]
fn canonical_bitpack_puts_the_padding_word_last() {
    let tree = glossia::cached_payload_tree("english", "bip39").unwrap();
    let bits_per_word = 11usize;
    for len in [1usize, 8, 20, 32] {
        let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(53)).collect();
        let words = glossia::codec::encode_base_n(&data, &tree, "canonical_bitpack").unwrap();
        let n_data = (data.len() * 8).div_ceil(bits_per_word);
        let pad = n_data * bits_per_word - data.len() * 8;
        assert_eq!(words.len(), n_data + 1, "len {len}: word count");
        assert_eq!(
            tree.position(words.last().unwrap()),
            Some(pad),
            "len {len}: last word must be the padding count"
        );
        let back = glossia::codec::decode_base_n(&words, &tree, "canonical_bitpack").unwrap();
        assert_eq!(back, data, "len {len}: round trip");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Wordlist sizes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn payload_wordlists_are_powers_of_two() {
    // `encode_base_n` gates every bitpack codec on this, falling back to
    // big-integer base-N without a word of complaint. German and Latin failed it
    // silently for one release because YAML resolved `null:` (die Null) and
    // `false:` to non-string scalars and the loader skipped them — 2048 -> 2047
    // and 32768 -> 32767. The words are the guard that would have caught it.
    for (language, wordlist, expected) in [
        ("english", "bip39", 2048usize),
        ("latin", "default", 32768),
        ("czech", "default", 2048),
        ("german", "default", 2048),
    ] {
        let tree = glossia::cached_payload_tree(language, wordlist).unwrap();
        assert_eq!(tree.len(), expected, "{language} wordlist size");
        assert!(tree.len().is_power_of_two(), "{language} must stay a power of two");
    }
    assert!(
        glossia::cached_payload_tree("german", "default").unwrap().contains("null"),
        "German's `null` must survive YAML's core schema"
    );
    assert!(
        glossia::cached_payload_tree("latin", "default").unwrap().contains("false"),
        "Latin's `false` must survive YAML's core schema"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Version 2 freeze
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn v2_rules_are_frozen() {
    let rules = rules_for(2).expect("v2 must always exist");
    // Changing any of these re-renders every v2 artifact. If a change here is
    // intended, it belongs in a NEW version — see the module docs.
    assert_eq!(rules.best_of, 4);
    assert_eq!(rules.dialect, "body");
    assert_eq!(rules.semantics_languages, &["english"]);
}

#[test]
fn v2_golden_renderings() {
    // Exact canonical renderings, pinned. These freeze everything the
    // rendering reads: grammar, dialect config, cover wordlists, English's
    // semantics.yaml, the RNG, and the best-of selection. A diff here means a
    // v2 artifact in the wild no longer verifies.
    let hash160: Vec<u8> = (0u8..20).collect();
    let zeros = vec![0u8; 8];

    assert_eq!(
        canonical_encode(&hash160, "english", "bip39").unwrap(),
        "Abandon amount a liar. Amount expire to adjust via cage. Candy arch to gather. Drum get bullet out an absurd math via equal. Some cop are frozen. Method pistol scale to abuse."
    );
    assert_eq!(
        canonical_encode_fixed(&hash160, "english", "bip39").unwrap(),
        "Abandon amount a liar. Amount expire to adjust via cage. Candy arch to gather. Drum get bullet out an absurd math via equal. Some cop are frozen. Method may pistol the scale."
    );
    assert_eq!(
        canonical_encode(&zeros, "english", "bip39").unwrap(),
        "Abandon abandon abandon to abandon. Abandon may abandon the amused abstract. A tap is intact. Son avoid to absorb."
    );
    assert_eq!(
        canonical_encode(&zeros, "latin", "default").unwrap(),
        "Is abs eo. Tu abs eo abs tu. Is eo abs is. Tu aro colonus. Torminalis aca is."
    );
    assert_eq!(
        canonical_encode(&zeros, "czech", "default").unwrap(),
        "Abdikace ne mít abdikace. Abdikace jít na abdikace. Abdikace mít abdikace si biftek ku alkohol. Nastat mít butik za alej."
    );
}
