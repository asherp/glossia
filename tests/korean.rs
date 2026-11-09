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
const PAYLOAD_YAML: &str = include_str!("../languages/korean/payload_bip39.yaml");
const COVER_YAML: &str = include_str!("../languages/korean/cover.yaml");
const CORPUS_YAML: &str = include_str!("../languages/korean/payload.yaml");

/// The two Korean payload lists: `default` is the 2^14-word list drawn from
/// the Shiberu frequency corpus, `bip39` the official BIP39 Korean list.
const WORDLISTS: [(&str, &str); 2] = [("default", CORPUS_YAML), ("bip39", PAYLOAD_YAML)];

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

fn set_of(yaml: &str) -> HashSet<String> {
    yaml_keys(yaml).into_iter().collect()
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
    for (name, yaml) in WORDLISTS {
        let set = set_of(yaml);
        for w in &set {
            for p in KOREAN_PARTICLES {
                let joined = format!("{w}{p}");
                assert!(!set.contains(&joined), "{name}: {w} + {p} = {joined} is itself a payload word");
                // And detaching recovers exactly the word that was written.
                assert_eq!(
                    payload_form(&joined, &|t: &str| set.contains(t)).as_deref(),
                    Some(w.as_str()),
                    "{name}: {joined} does not decode to {w}"
                );
            }
        }
    }
}

#[test]
fn no_cover_word_decodes_as_payload() {
    let cover: serde_yaml::Mapping = serde_yaml::from_str(COVER_YAML).expect("yaml parses");
    for (name, yaml) in WORDLISTS {
        let set = set_of(yaml);
        for (key, entry) in &cover {
            let c = key.as_str().expect("string key");
            assert!(
                payload_form(&c, &|t: &str| set.contains(t)).is_none(),
                "{name}: cover word {c} would decode as a payload word"
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
                    "{name}: {joined} (cover + particle) would decode as a payload word"
                );
            }
        }
    }
}

#[test]
fn corpus_list_is_two_to_the_fourteen_composed_hangul_in_order() {
    let words = yaml_keys(CORPUS_YAML);
    assert_eq!(words.len(), 1 << 14, "the corpus list carries 14 bits per word");
    assert_eq!(set_of(CORPUS_YAML).len(), words.len(), "duplicate word");
    for w in &words {
        assert!(
            w.chars().all(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)),
            "{w} is not all precomposed Hangul syllables"
        );
        assert_eq!(&compose_hangul(&decompose_hangul(w)), w);
    }
    // Index order is Hangul code-point order (가나다), frozen once shipped.
    assert!(words.windows(2).all(|p| p[0] < p[1]), "payload.yaml is not in Hangul order");
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
    for (wl, _) in WORDLISTS {
        for dialect in ["body", "subject", "prose", "payload_only"] {
            for seed in [0u64, 1, 7, 42] {
                let (text, _, _, _) = encode_into_language(
                    input, "korean", wl, dialect, None, seed, false, None, None, None, None,
                )
                .unwrap_or_else(|e| panic!("{wl}/{dialect}/{seed} encode: {e}"));
                assert!(
                    !text.split_whitespace().any(is_particle),
                    "{wl}/{dialect}/{seed}: a particle was left as its own word: {text}"
                );
                let decoded = decode_from_language(&text, "korean", wl, false)
                    .unwrap_or_else(|e| panic!("{wl}/{dialect}/{seed} decode: {e}\n{text}"));
                assert_eq!(decoded, input, "{wl}/{dialect}/{seed}: {text}");
            }
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
    for (wl, _) in WORDLISTS {
        for payload in &payloads {
            let text = canonical_encode(payload, "korean", wl).expect("encode");
            let d = canonical_decode(&text, "korean", wl).expect("decode");
            assert_eq!(&d.payload, payload);
            assert!(d.verified, "{wl}: clean Korean artifact must verify: {text}");

            // The same prose, decomposed the way the official BIP39 file is, still
            // decodes to the same bytes.
            let nfd = decompose_hangul(&text);
            assert_ne!(nfd, text);
            let d = canonical_decode(&nfd, "korean", wl).expect("decode NFD");
            assert_eq!(&d.payload, payload, "{wl}: NFD input decoded differently");
        }
    }
}

#[test]
fn canonical_prose_does_not_decode_under_the_other_list() {
    // The lists share 1,864 words at different indices, so the words alone do
    // not say which list a text used. The canonical envelope does: decoding
    // under the wrong list must fail or refuse to verify, never return bytes.
    let payload: Vec<u8> = (0u8..32).collect();
    for (enc, dec) in [("default", "bip39"), ("bip39", "default")] {
        let text = canonical_encode(&payload, "korean", enc).expect("encode");
        match canonical_decode(&text, "korean", dec) {
            Err(_) => {}
            Ok(d) => assert!(
                !d.verified && d.payload != payload,
                "{enc} prose decoded as {dec}: {text}"
            ),
        }
    }
}
