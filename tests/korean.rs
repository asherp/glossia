//! Korean: data invariants that make attached particles safe to decode, and
//! round trips through the free-form and canonical paths.
//!
//! Korean prose writes case and topic particles onto the noun before them
//! (사과 + 를 → 사과를), so a payload word reaches the decoder with a suffix.
//! Decoding detaches it (`codec::payload_form`), which is only correct if the
//! wordlists cannot be confused by that step. The first group of tests pins
//! the properties that guarantee it.

use std::collections::HashSet;

use glossia::codec::{compose_hangul, payload_form, KOREAN_PARTICLES};
use glossia::{canonical_decode, canonical_encode, decode_from_language, encode_into_language};

const WORDLIST_TXT: &str = include_str!("../languages/korean/wordlist.txt");
const PAYLOAD_YAML: &str = include_str!("../languages/korean/payload.yaml");
const COVER_YAML: &str = include_str!("../languages/korean/cover.yaml");

fn yaml_keys(src: &str) -> Vec<String> {
    let doc: serde_yaml::Mapping = serde_yaml::from_str(src).expect("yaml parses");
    doc.keys().map(|k| k.as_str().expect("string key").to_string()).collect()
}

fn payload() -> Vec<String> {
    yaml_keys(PAYLOAD_YAML)
}

fn payload_set() -> HashSet<String> {
    payload().into_iter().collect()
}

/// Decompose precomposed Hangul syllables into conjoining jamo (the Hangul
/// part of NFD), to produce input the way a decomposing system would send it.
fn decompose_hangul(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let cp = c as u32;
        if (0xAC00..=0xD7A3).contains(&cp) {
            let i = cp - 0xAC00;
            out.push(char::from_u32(0x1100 + i / (21 * 28)).unwrap());
            out.push(char::from_u32(0x1161 + (i % (21 * 28)) / 28).unwrap());
            if i % 28 != 0 {
                out.push(char::from_u32(0x11A7 + i % 28).unwrap());
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn payload_is_the_official_list_in_order_composed() {
    let official: Vec<&str> = WORDLIST_TXT.lines().filter(|l| !l.is_empty()).collect();
    let words = payload();
    assert_eq!(official.len(), 2048);
    assert_eq!(words.len(), 2048);
    for (i, (raw, w)) in official.iter().zip(&words).enumerate() {
        assert_eq!(&compose_hangul(raw), w, "word {i} differs from the official list");
        assert_eq!(&decompose_hangul(w), raw, "word {i} does not decompose to the official form");
    }
}

#[test]
fn no_payload_word_plus_particle_is_a_payload_word() {
    let set = payload_set();
    for w in &set {
        for p in KOREAN_PARTICLES {
            let joined = format!("{w}{p}");
            assert!(!set.contains(&joined), "{w} + {p} = {joined} is itself a payload word");
            // And detaching recovers exactly the word that was written.
            assert_eq!(
                payload_form(&joined, &|t: &str| set.contains(t)).as_deref(),
                Some(w.as_str()),
                "{joined} does not decode to {w}"
            );
        }
    }
}

#[test]
fn no_cover_word_decodes_as_payload() {
    let set = payload_set();
    let cover: serde_yaml::Mapping = serde_yaml::from_str(COVER_YAML).expect("yaml parses");
    for (key, entry) in &cover {
        let c = key.as_str().expect("string key");
        assert!(
            payload_form(&c, &|t: &str| set.contains(t)).is_none(),
            "cover word {c} would decode as a payload word"
        );
        // A cover noun or pronoun with a particle attached must not decode
        // either: 길 + 이 is the payload word 길이.
        let takes_particles = ["N", "Pron"].iter().any(|pos| entry.get(*pos).is_some());
        if !takes_particles {
            continue;
        }
        for p in KOREAN_PARTICLES {
            let joined = format!("{c}{p}");
            assert!(
                payload_form(&joined, &|t: &str| set.contains(t)).is_none(),
                "{joined} (cover + particle) would decode as a payload word"
            );
        }
    }
}

#[test]
fn hangul_composition_round_trips_and_leaves_other_text_alone() {
    for w in payload() {
        assert_eq!(compose_hangul(&decompose_hangul(&w)), w);
    }
    assert_eq!(compose_hangul("abdikace žák"), "abdikace žák");
}

fn is_particle(token: &str) -> bool {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    KOREAN_PARTICLES.contains(&t)
}

#[test]
fn free_form_round_trip_across_dialects() {
    let input = "Glossia speaks Korean now.";
    for dialect in ["body", "subject", "prose", "payload_only"] {
        for seed in [0u64, 1, 7, 42] {
            let (text, _, _, _) = encode_into_language(
                input, "korean", "default", dialect, None, seed, false, None, None, None, None,
            )
            .unwrap_or_else(|e| panic!("{dialect}/{seed} encode: {e}"));
            assert!(
                !text.split_whitespace().any(is_particle),
                "{dialect}/{seed}: a particle was left as its own word: {text}"
            );
            let decoded = decode_from_language(&text, "korean", "default", false)
                .unwrap_or_else(|e| panic!("{dialect}/{seed} decode: {e}\n{text}"));
            assert_eq!(decoded, input, "{dialect}/{seed}: {text}");
        }
    }
}

#[test]
fn particles_are_attached_in_body_prose() {
    let (text, _, _, _) = encode_into_language(
        "attached particles", "korean", "default", "body", None, 3, false, None, None, None, None,
    )
    .expect("encode");
    let set = payload_set();
    // At least one payload word must carry a particle, or the test proves nothing.
    let attached = text
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !set.contains(*t))
        .any(|t| payload_form(t, &|w: &str| set.contains(w)).is_some());
    assert!(attached, "no payload word carried a particle: {text}");
}

#[test]
fn canonical_round_trip_and_nfd_input() {
    let payloads: Vec<Vec<u8>> = vec![
        vec![0xAB],
        vec![0u8; 8],
        (0u8..20).collect(),
        (100u8..132).collect(),
    ];
    for payload in payloads {
        let text = canonical_encode(&payload, "korean", "default").expect("encode");
        let d = canonical_decode(&text, "korean", "default").expect("decode");
        assert_eq!(d.payload, payload);
        assert!(d.verified, "clean Korean artifact must verify: {text}");

        // The same prose, decomposed the way the official BIP39 file is, still
        // decodes to the same bytes.
        let nfd = decompose_hangul(&text);
        assert_ne!(nfd, text);
        let d = canonical_decode(&nfd, "korean", "default").expect("decode NFD");
        assert_eq!(d.payload, payload, "NFD input decoded differently");
    }
}
