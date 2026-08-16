//! The canonical encoding's contract: deterministic, versioned, verifiable.
//!
//! The golden tests here are the freeze that makes version rules meaningful.
//! If one fails, a change altered what a shipped canonical version renders —
//! that is a format break for every existing artifact. The fix is never to
//! update the golden: add a new version in `src/canonical.rs`, bump
//! `CANONICAL_VERSION`, and pin new goldens for the new version alongside
//! these.

use glossia::align::{align, Op};
use glossia::{
    canonical_decode, canonical_decode_fixed, canonical_encode, canonical_encode_at,
    canonical_encode_fixed, canonical_encode_fixed_at, canonical_encode_fixed_traced, rules_for,
    CanonicalError, Envelope, Verdict, CANONICAL_VERSION,
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
fn version_1_artifacts_still_decode_and_verify() {
    // The point of versioned rules. v1 frames `version || payload` with a
    // leading padding word and carries no checksum; v2 frames the other way
    // round. A v1 artifact must still decode, still report version 1, and still
    // VERIFY — its wording re-rendered under v1's rules, not the current ones.
    //
    // English and Czech only: the 0.4.0 wordlist fix renumbered Latin and
    // German, which is a wordlist change rather than a format one and is
    // outside what a canonical version can freeze.
    for (language, wordlist) in [("english", "bip39"), ("czech", "default")] {
        for payload in payloads() {
            let text = canonical_encode_at(&payload, language, wordlist, 1)
                .unwrap_or_else(|e| panic!("{language} v1 encode: {e}"));
            let d = canonical_decode(&text, language, wordlist)
                .unwrap_or_else(|e| panic!("{language} v1 decode: {e}"));
            assert_eq!(d.version, 1, "{language} must report the artifact's own version");
            assert_eq!(d.payload, payload, "{language} v1 payload round trip");
            assert!(d.verified, "{language} v1 artifact must still verify");
        }
    }
}

#[test]
fn the_two_framings_do_not_read_each_other() {
    // Each version vouches only for the framing it declares, so neither
    // framing can quietly claim the other's artifact. Without that check the v1
    // attempt would accept any artifact whose leading byte happened to read 1.
    let payload: Vec<u8> = (0u8..20).collect();
    let v1 = canonical_encode_at(&payload, "english", "bip39", 1).unwrap();
    let v2 = canonical_encode_at(&payload, "english", "bip39", 2).unwrap();
    assert_ne!(v1, v2, "the framings must produce different prose");

    assert_eq!(canonical_decode(&v1, "english", "bip39").unwrap().version, 1);
    assert_eq!(canonical_decode(&v2, "english", "bip39").unwrap().version, 2);

    // The fixed packing exists only under v2's framing, so asking for it at v1
    // is refused by name rather than inventing a layout that never shipped.
    match canonical_encode_fixed_at(&payload, "english", "bip39", 1) {
        Err(CanonicalError::NoFixedForm(1)) => {}
        other => panic!("expected NoFixedForm(1), got {other:?}"),
    }
    // And a v1 artifact cannot be read through the fixed decoder.
    match canonical_decode_fixed(&v1, "english", "bip39", payload.len()) {
        Err(_) => {}
        Ok(d) => panic!("fixed decoder must not accept a v1 artifact, got {d:?}"),
    }
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
fn a_truncated_fixed_artifact_is_refused_by_word_count() {
    // `decode_bitpack_fixed` used to fill `expected_bytes` from whatever words
    // it had and leave the rest zero, so a paragraph with its tail cut off
    // decoded to a plausible zero-padded payload. It now counts first, which
    // catches a truncation as the wrong SHAPE rather than the wrong bytes —
    // a better error, and the one that lets a host tell "not canonical prose"
    // (fall back to another decoder) from "canonical prose, damaged" (do not).
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode_fixed(&payload, "english", "bip39").unwrap();
    let words: Vec<&str> = text.split_whitespace().collect();
    let truncated = words[..words.len() * 2 / 3].join(" ");
    match canonical_decode_fixed(&truncated, "english", "bip39", payload.len()) {
        Err(CanonicalError::Decode(msg)) => {
            assert!(msg.contains("expected"), "error should name the count: {msg}");
        }
        other => panic!("expected a Decode error on a truncated artifact, got {other:?}"),
    }
}

#[test]
fn prose_of_another_length_does_not_pass_as_a_short_payload() {
    // The same guard from the other side: prose encoding a 32-byte payload must
    // not decode as a 20-byte one just because the words are all in the list.
    let long: Vec<u8> = (0u8..32).collect();
    let text = canonical_encode_fixed(&long, "english", "bip39").unwrap();
    match canonical_decode_fixed(&text, "english", "bip39", 20) {
        Err(CanonicalError::Decode(_)) => {}
        other => panic!("expected a Decode error for the wrong length, got {other:?}"),
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
fn v1_rules_are_frozen() {
    let rules = rules_for(1).expect("v1 must stay reachable — artifacts exist");
    assert_eq!(rules.best_of, 4);
    assert_eq!(rules.dialect, "body");
    assert_eq!(rules.semantics_languages, &["english"]);
    assert_eq!(rules.envelope, Envelope::VersionLeading);
}

#[test]
fn v2_rules_are_frozen() {
    let rules = rules_for(2).expect("v2 must always exist");
    // Changing any of these re-renders every v2 artifact. If a change here is
    // intended, it belongs in a NEW version — see the module docs.
    assert_eq!(rules.best_of, 4);
    assert_eq!(rules.dialect, "body");
    assert_eq!(rules.semantics_languages, &["english"]);
    assert_eq!(rules.envelope, Envelope::PayloadLeading);
}

#[test]
fn v1_golden_renderings() {
    // The 0.3.0 renderings, unchanged. These are the artifacts in the wild that
    // v1's continued registration exists to keep readable — a diff here means
    // one of them stopped verifying.
    //
    // Latin's v1 golden is deliberately absent: the 0.4.0 wordlist fix
    // renumbered that list, so no v1 Latin rendering from 0.3.0 survives. See
    // the retired `v1.0-cafe-latin` vector in tests/test_vectors.json.
    let hash160: Vec<u8> = (0u8..20).collect();
    let zeros = vec![0u8; 8];

    assert_eq!(
        canonical_encode_at(&hash160, "english", "bip39", 1).unwrap(),
        "Its absurd absurd abandon dog. Alcohol may doctor its odd loan. Son bring abuse to anxiety. Flash addict its bright valve. An ancient cow embark our gas."
    );
    assert_eq!(
        canonical_encode_at(&zeros, "english", "bip39", 1).unwrap(),
        "The absent absurd abandon abandon. Our abandon abandon abandon to abandon."
    );
    assert_eq!(
        canonical_encode_at(&zeros, "czech", "default", 1).unwrap(),
        "Aktovka mít amputace se abdikace. Abdikace dát ze abdikace. Abdikace mít abdikace se abdikace."
    );
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

// ═══════════════════════════════════════════════════════════════════════
// Alignment: locating damage rather than only detecting it
// ═══════════════════════════════════════════════════════════════════════
//
// The split below is not arbitrary. Damage to a COVER word leaves the payload
// and its crc32 intact, so `canonical_decode` succeeds and carries an alignment
// with it. Damage to a PAYLOAD word changes the bytes, the checksum fails, and
// the decode returns `Err` — there is no payload to render a comparison from.
//
// That is the checksum doing its job, and it is also precisely why alignment
// cannot bootstrap. A payload-damage alignment has to be taken against a
// CANDIDATE's re-render, which is the position an error-correcting decoder is
// in once it has proposed a correction. Those tests call `align` directly
// against the known-correct rendering, standing in for that candidate.

/// The payload wordlist as a predicate, for tests that align by hand.
fn payload_pred(language: &str, wordlist: &str) -> impl Fn(&str) -> bool {
    let tree = glossia::cached_payload_tree(language, wordlist).expect("payload tree");
    let set: std::collections::HashSet<String> =
        tree.words().iter().map(|w| w.to_lowercase()).collect();
    move |w: &str| set.contains(w)
}

/// A cover word occurring in `text`, whitespace-delimited so a replacement
/// cannot land inside a longer word. Chosen from the text rather than named
/// outright, so these tests do not break when a rendering changes which cover
/// words it happens to draw.
fn a_cover_word_in(text: &str, is_payload: &impl Fn(&str) -> bool) -> String {
    text.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .find(|t| !t.is_empty() && !is_payload(t) && text.contains(&format!(" {t} ")))
        .expect("the rendering must contain at least one cover word")
}

/// A payload word that does NOT occur in `text` — a symbol nobody sent, for
/// staging a spurious arrival.
fn a_payload_word_absent_from(
    text: &str,
    language: &str,
    wordlist: &str,
) -> String {
    let tree = glossia::cached_payload_tree(language, wordlist).expect("payload tree");
    let present: std::collections::HashSet<String> = text
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .collect();
    tree.words()
        .iter()
        .map(|w| w.to_lowercase())
        .find(|w| !present.contains(w))
        .expect("the wordlist must hold a word this rendering did not use")
}

#[test]
fn a_clean_decode_carries_a_clean_alignment() {
    let payload: Vec<u8> = (0u8..20).collect();
    for (language, wordlist) in LANGS {
        let text = canonical_encode(&payload, language, wordlist).unwrap();
        let d = canonical_decode(&text, language, wordlist).unwrap();
        assert_eq!(d.verdict, Verdict::Verified);
        assert!(d.alignment.is_clean(), "{language}: clean text must align clean");
        assert!(d.alignment.erasures.is_empty(), "{language}");
        assert!(d.alignment.spurious.is_empty(), "{language}");
        assert!(
            d.alignment.payload_slots.iter().all(|s| s.is_some()),
            "{language}: every slot filled"
        );
    }
}

#[test]
fn cover_damage_is_localized_and_costs_no_payload_slot() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    assert!(text.contains(" via "), "expected cover word missing: {text}");
    let damaged = text.replacen(" via ", " vias ", 1);

    let d = canonical_decode(&damaged, "english", "bip39").unwrap();
    assert_eq!(d.payload, payload, "cover damage must not touch the payload");
    assert_eq!(d.verdict, Verdict::Unverified);

    // The whole gain over a boolean: the disagreement is one token, and it is
    // named. A cover word is not a payload word, so no slot is a hole.
    let damaged_tokens: Vec<_> = d.alignment.tokens.iter().filter(|t| t.op != Op::Same).collect();
    assert_eq!(damaged_tokens.len(), 1, "exactly one token disagrees");
    assert_eq!(damaged_tokens[0].received.as_deref(), Some("vias"));
    assert_eq!(damaged_tokens[0].expected.as_deref(), Some("via"));
    assert!(d.alignment.erasures.is_empty(), "cover damage costs no payload slot");
    assert_eq!(d.alignment.payload_intact(), d.alignment.payload_slots.len());
}

#[test]
fn a_payload_word_knocked_off_the_list_becomes_an_erasure_at_its_own_slot() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    let is_payload = payload_pred("english", "bip39");

    // The payload words of the correct rendering, in order — the slots.
    let slots = glossia::codec::payload_tokens(&text, &is_payload);
    assert!(slots.len() > 3, "need several slots to damage a middle one");

    // Mangle the third payload word off the wordlist. The harvest loses it
    // entirely, which is the damage mode a positional code cannot see unaided.
    let target = &slots[2];
    let damaged = text.replacen(target.as_str(), &format!("{target}zz"), 1);
    assert_ne!(damaged, text, "the substitution must actually land");

    // Payload damage fails the checksum: there is no decode to align from.
    assert!(
        canonical_decode(&damaged, "english", "bip39").is_err(),
        "payload damage must be caught, not silently decoded"
    );

    // An error-correcting decoder holding the right candidate aligns against
    // its re-render — and gets the slot number, which is what it needs.
    let a = align(&damaged, &text, None, &is_payload);
    assert_eq!(a.erasures, vec![2], "the hole is at the damaged slot, and only there");
    assert_eq!(a.payload_slots[2], None);
    assert_eq!(a.payload_slots.len(), slots.len(), "slot count follows the rendering");
    for (k, w) in slots.iter().enumerate().filter(|(k, _)| *k != 2) {
        assert_eq!(a.payload_slots[k].as_ref(), Some(w), "slot {k} must be undisturbed");
    }
}

#[test]
fn a_dropped_payload_word_does_not_shift_the_slots_after_it() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    let is_payload = payload_pred("english", "bip39");
    let slots = glossia::codec::payload_tokens(&text, &is_payload);

    // Delete the second payload word outright. Every later payload word now
    // arrives one position early; without alignment the harvest reads as
    // damaged from that point to the end.
    let target = format!(" {} ", slots[1]);
    assert!(text.contains(&target), "expected a whitespace-delimited payload word");
    let damaged = text.replacen(&target, " ", 1);

    let a = align(&damaged, &text, None, &is_payload);
    assert_eq!(a.erasures, vec![1], "only the dropped slot is a hole");
    for (k, w) in slots.iter().enumerate().filter(|(k, _)| *k != 1) {
        assert_eq!(
            a.payload_slots[k].as_ref(),
            Some(w),
            "slot {k} kept its own word across the shift"
        );
    }
}

#[test]
fn a_cover_word_mangled_onto_the_list_is_reported_spurious_and_takes_no_slot() {
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    let is_payload = payload_pred("english", "bip39");
    let slots = glossia::codec::payload_tokens(&text, &is_payload);

    // A word that was never sent arrives on the payload wordlist. The harvest
    // gains a symbol, shifting everything after it the other way.
    let cover = a_cover_word_in(&text, &is_payload);
    let intruder = a_payload_word_absent_from(&text, "english", "bip39");
    let damaged = text.replacen(&format!(" {cover} "), &format!(" {intruder} "), 1);
    assert_ne!(damaged, text, "the substitution must actually land");

    let a = align(&damaged, &text, None, &is_payload);
    assert_eq!(a.spurious.len(), 1, "the arrival is spurious, not a slot");
    assert!(a.erasures.is_empty(), "no slot actually lost its word");
    assert_eq!(a.payload_slots.len(), slots.len());
    for (k, w) in slots.iter().enumerate() {
        assert_eq!(a.payload_slots[k].as_ref(), Some(w), "slot {k} undisturbed");
    }
}

#[test]
fn a_deletion_and_an_insertion_together_stay_two_local_faults() {
    let payload: Vec<u8> = (100u8..132).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    let is_payload = payload_pred("english", "bip39");
    let slots = glossia::codec::payload_tokens(&text, &is_payload);

    // The case that defeats a positional code outright: one payload word lost
    // and one spurious word gained leaves the harvest the RIGHT LENGTH but
    // shifted through the middle, so a naive decode is wrong across that whole
    // span while looking perfectly well-formed.
    let lost = format!(" {} ", slots[1]);
    assert!(text.contains(&lost));
    let damaged = text.replacen(&lost, " ", 1);
    let cover = a_cover_word_in(&damaged, &is_payload);
    let intruder = a_payload_word_absent_from(&text, "english", "bip39");
    let damaged = damaged.replacen(&format!(" {cover} "), &format!(" {intruder} "), 1);

    let a = align(&damaged, &text, None, &is_payload);
    assert_eq!(a.erasures, vec![1], "the loss stays one hole");
    assert_eq!(a.spurious.len(), 1, "the gain stays one spurious token");
    for (k, w) in slots.iter().enumerate().filter(|(k, _)| *k != 1) {
        assert_eq!(a.payload_slots[k].as_ref(), Some(w), "slot {k} survived the desync");
    }
}

#[test]
fn erasure_count_is_the_parity_budget_an_ecc_would_need() {
    // The whole point of erasures over errors: an erasure costs one parity
    // symbol where an unlocated error costs two. This pins the accounting that
    // #81's parity sizing rests on, so a regression in localization shows up as
    // a doubled budget rather than as a silent quality loss.
    let payload: Vec<u8> = (0u8..20).collect();
    let text = canonical_encode(&payload, "english", "bip39").unwrap();
    let is_payload = payload_pred("english", "bip39");
    let slots = glossia::codec::payload_tokens(&text, &is_payload);

    let mut damaged = text.clone();
    for k in [0usize, 2, 4] {
        let target = format!(" {} ", slots[k]);
        if let Some(_) = damaged.find(&target) {
            damaged = damaged.replacen(&target, &format!(" {}zz ", slots[k]), 1);
        }
    }

    let a = align(&damaged, &text, None, &is_payload);
    assert!(!a.erasures.is_empty(), "damage must be located");
    assert!(
        a.erasures.iter().all(|&k| [0, 2, 4].contains(&k)),
        "every hole must be one we made: {:?}",
        a.erasures
    );
    assert_eq!(a.payload_intact(), a.payload_slots.len() - a.erasures.len());
}
