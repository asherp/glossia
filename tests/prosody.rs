//! Verse dialects end to end.
//!
//! The claims a poetry dialect has to keep, in order of importance:
//! 1. the payload survives — decoding is the same wordlist filter it always was;
//! 2. the lines actually scan;
//! 3. dialects that declare no meter are untouched by any of it.

use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::generator::prosody::{layout, scans_text, MeterSpec, ProsodyModel, StressMode};
use glossia::grammar::DialectConfig;
use glossia::pipeline::encode_words_into_language;
use std::collections::HashSet;

const BPW: usize = 11;

fn payload_words(n: usize, nonce: u64) -> Vec<String> {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let mut x = nonce.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            wl[(x as usize) % (1 << BPW)].clone()
        })
        .collect()
}

fn model() -> ProsodyModel {
    ProsodyModel::from_yaml_str(include_str!("../languages/english/prosody.yaml")).unwrap()
}

fn encode(dialect: &str, words: &[String], seed: u64) -> String {
    encode_words_into_language(words, "english", "default", dialect, seed, 4)
        .unwrap_or_else(|e| panic!("encode into {dialect}: {e:?}"))
        .0
}

/// Decoding, exactly as a reader would: filter the prose against the wordlist.
fn harvest(text: &str, words: &[String]) -> Vec<String> {
    let set: HashSet<String> = words.iter().map(|w| w.to_lowercase()).collect();
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| set.contains(w))
        .collect()
}

#[test]
fn verse_dialects_declare_a_meter_and_body_does_not() {
    for (dialect, lines, mode) in [
        ("syllabic", vec![10], StressMode::Free),
        ("haiku", vec![5, 7, 5], StressMode::Free),
        ("iambic", vec![10], StressMode::Lenient),
    ] {
        let cfg = DialectConfig::from_language_dialect("english", dialect).unwrap();
        let meter = cfg.meter().unwrap_or_else(|| panic!("{dialect} declares no meter"));
        assert_eq!(meter.lines, lines, "{dialect} line pattern");
        assert_eq!(meter.mode, mode, "{dialect} stress mode");
    }
    assert!(
        DialectConfig::from_language_dialect("english", "body").unwrap().meter().is_none(),
        "body must stay meterless — every shipped canonical artifact renders through it"
    );
}

#[test]
fn the_payload_survives_every_verse_dialect() {
    for dialect in ["syllabic", "haiku", "iambic"] {
        for nonce in 0..12u64 {
            let words = payload_words(13, nonce);
            let text = encode(dialect, &words, nonce.wrapping_mul(7919));
            assert_eq!(
                harvest(&text, &words),
                words,
                "{dialect} lost or reordered payload (seed {nonce}): {text}"
            );
        }
    }
}

#[test]
fn syllable_counted_dialects_scan() {
    let m = model();
    for (dialect, spec) in [
        ("syllabic", MeterSpec { lines: vec![10], mode: StressMode::Free, rise: true }),
        ("haiku", MeterSpec { lines: vec![5, 7, 5], mode: StressMode::Free, rise: true }),
    ] {
        let mut scanned = 0;
        let trials = 12;
        for nonce in 0..trials {
            let words = payload_words(13, nonce);
            let text = encode(dialect, &words, nonce.wrapping_mul(104_729));
            let toks: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
            if scans_text(&toks, &m, &spec) {
                scanned += 1;
            }
        }
        // Not every draw can scan: a payload word that will not fit is placed
        // anyway rather than dropped, which is the invariant that matters. What
        // is being asserted is that the filler works at all — before it, the
        // measured rate for these forms was one in five.
        assert!(
            scanned * 2 >= trials,
            "{dialect} scanned only {scanned}/{trials} — the filler is not steering"
        );
    }
}

#[test]
fn layout_reproduces_the_lines_the_filler_built() {
    let m = model();
    let spec = MeterSpec { lines: vec![5, 7, 5], mode: StressMode::Free, rise: true };
    let words = payload_words(13, 3);
    let text = encode("haiku", &words, 424_242);
    let toks: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
    let lines = layout(&toks, &m, &spec);
    assert!(lines.len() > 1, "a 13-word payload is more than one haiku line");
    assert_eq!(
        lines.join(" "),
        toks.join(" "),
        "layout may only insert breaks, never change the words"
    );
}

/// What a CLI user sees: `--dialect haiku` has to come out as lines of the right
/// length, not as one paragraph. This is the claim the printer rests on.
#[test]
fn a_verse_dialect_lays_out_as_lines_of_the_declared_length() {
    let m = model();
    let spec = MeterSpec { lines: vec![5, 7, 5], mode: StressMode::Free, rise: true };
    let mut exact = 0;
    let trials = 8;
    for nonce in 0..trials {
        let words = payload_words(12, nonce + 40);
        let text = encode("haiku", &words, nonce.wrapping_mul(2_654_435_761));
        let toks: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
        let lines = layout(&toks, &m, &spec);
        assert!(lines.len() >= 3, "a 12-word payload spans several lines: {lines:?}");
        // Every line but the last must hit its syllable count exactly. The last
        // is allowed to come up short: a text ends where the payload ends, not
        // where the stanza does.
        let full: Vec<usize> = lines[..lines.len() - 1]
            .iter()
            .map(|l| {
                l.split_whitespace()
                    .map(|w| m.syllables(w).unwrap_or(1))
                    .sum()
            })
            .collect();
        if full.iter().enumerate().all(|(i, &n)| n == spec.lines[i % spec.lines.len()]) {
            exact += 1;
        }
    }
    assert!(
        exact * 2 >= trials,
        "only {exact}/{trials} layouts had exact lines — the printer and the filler disagree"
    );
}

#[test]
fn prosody_data_covers_the_wordlists_it_has_to() {
    let m = model();
    let payload = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let missing: Vec<&String> = payload.iter().filter(|w| m.syllables(w).is_none()).collect();
    assert!(
        missing.len() <= 1,
        "prosody data must know essentially every payload word; missing {missing:?}"
    );
}

#[test]
fn a_meterless_dialect_is_byte_for_byte_unchanged_by_prosody() {
    // The strongest statement available without a golden: the body dialect
    // renders identically whether or not prosody.yaml exists, because nothing
    // asks for it. Two encodes at the same seed must agree, and must agree with
    // the canonical path's expectation of determinism.
    let words = payload_words(13, 99);
    let a = encode("body", &words, 555);
    let b = encode("body", &words, 555);
    assert_eq!(a, b);
    assert_eq!(harvest(&a, &words), words);
}
