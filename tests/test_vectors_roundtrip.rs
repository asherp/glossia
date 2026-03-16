//! Integration tests driven by tests/test_vectors.json.
//!
//! The primary invariant under test: decode(encode(x)) == x

#![allow(dead_code)]

use glossia::codec::DataMode;
use glossia::pipeline::{Endpoint, Pipeline};
use serde::Deserialize;

// ── JSON schema ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TestVectors {
    round_trip_vectors: Vec<RoundTripVector>,
    #[serde(default)]
    error_vectors: Vec<ErrorVector>,
    #[serde(default)]
    decode_stability_vectors: Option<DecodeStabilitySection>,
}

#[derive(Deserialize)]
struct DecodeStabilitySection {
    vectors: Vec<DecodeStabilityVector>,
}

#[derive(Deserialize)]
struct DecodeStabilityVector {
    id: String,
    pinned_since: String,
    encoded_prose: String,
    decode_from: DecodeFrom,
    expected_decoded: ValueWithFormat,
    #[serde(default)]
    decode_meta: Option<String>,
}

#[derive(Deserialize)]
struct RoundTripVector {
    id: String,
    #[serde(default)]
    description: String,
    input: ValueWithFormat,
    encoding: Encoding,
    expected_decoded: ValueWithFormat,
    #[serde(default)]
    transcode_to: Option<TranscodeTo>,
    #[serde(default)]
    expected_payload_words: Option<Vec<String>>,
    #[serde(default)]
    expected_rendered: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct ValueWithFormat {
    value: String,
    format: String,
}

#[derive(Deserialize)]
struct Encoding {
    language: String,
    wordlist: String,
    dialect: String,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    codec: Option<String>,
}

#[derive(Deserialize)]
struct TranscodeTo {
    language: String,
    wordlist: String,
    dialect: String,
    #[serde(default)]
    seed: Option<u64>,
}

#[derive(Deserialize)]
struct ErrorVector {
    id: String,
    #[serde(default)]
    description: String,
    input_prose: String,
    decode_from: DecodeFrom,
    expected: String,
}

#[derive(Deserialize)]
struct DecodeFrom {
    language: String,
    wordlist: String,
    dialect: String,
}

// ── Helpers ─────────────────────────────────────────────────────────

fn load_vectors() -> TestVectors {
    let json = include_str!("test_vectors.json");
    serde_json::from_str(json).expect("Failed to parse test_vectors.json")
}

fn format_to_source_endpoint(fmt: &str) -> Endpoint {
    match fmt {
        "hex" => Endpoint::Format(DataMode::Hex),
        "bip39" => Endpoint::Auto,
        f if f.starts_with("crypto/") => Endpoint::Auto,
        _ => Endpoint::Auto,
    }
}

fn decode_target(_fmt: &str) -> Endpoint {
    // The decode side uses Auto — the pipeline auto-detects the output
    // format from the binary payload (hex, bip39, etc.)
    Endpoint::Auto
}

/// If the string is already hex, return it. Otherwise, hex-encode its bytes.
/// This handles the pipeline's behavior of returning raw UTF-8 when bytes
/// are valid UTF-8 (e.g., \0 instead of "00").
fn normalize_to_hex(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_hexdigit()) {
        s.to_string()
    } else {
        s.bytes().map(|b| format!("{:02x}", b)).collect()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn round_trip_vectors() {
    let vectors = load_vectors();

    for v in &vectors.round_trip_vectors {
        // Skip empty input — encode produces empty output, nothing to decode
        if v.input.value.is_empty() {
            continue;
        }

        // Skip vectors that need special decode routing (crypto formats, transcode)
        if v.transcode_to.is_some() {
            continue; // tested separately
        }

        let source = format_to_source_endpoint(&v.input.format);
        let target = Endpoint::language_full(
            &v.encoding.language,
            &v.encoding.wordlist,
            &v.encoding.dialect,
        );

        let mut encode_pipe = Pipeline::from_params(source, target);
        if let Some(seed) = v.encoding.seed {
            encode_pipe = encode_pipe.with_seed(seed);
        }

        let encoded = encode_pipe
            .execute(&v.input.value)
            .unwrap_or_else(|e| panic!("[{}] encode failed: {:?}", v.id, e));

        assert!(!encoded.is_empty(), "[{}] encoded text is empty", v.id);

        // Layer 3: rendered text (optional)
        if let Some(ref expected_rendered) = v.expected_rendered {
            assert_eq!(
                encoded.trim(),
                expected_rendered.trim(),
                "[{}] rendered text mismatch",
                v.id
            );
        }

        // Layer 2: payload words (optional)
        if let Some(ref expected_words) = v.expected_payload_words {
            let payload_words = glossia::generator::data::load_payload_words_for_wordlist(
                &v.encoding.language,
                &v.encoding.wordlist,
            )
            .unwrap_or_else(|e| panic!("[{}] load payload words failed: {}", v.id, e));
            let payload_set: std::collections::HashSet<String> =
                payload_words.iter().map(|w| w.to_lowercase()).collect();
            let extracted: Vec<String> = encoded
                .split_whitespace()
                .map(|w| {
                    w.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .filter(|w| !w.is_empty() && payload_set.contains(w))
                .collect();
            assert_eq!(
                &extracted, expected_words,
                "[{}] payload words mismatch",
                v.id
            );
        }

        // Layer 1 (normative): decode(encode(x)) == x
        // For crypto/btc formats, use meta pipeline for decode
        if v.input.format.starts_with("crypto/") {
            let meta = format!(
                "translate from {} {} into {}",
                v.encoding.language, v.encoding.dialect, v.input.format
            );
            let decode_pipe =
                Pipeline::from_meta(&meta).unwrap_or_else(|e| {
                    panic!("[{}] decode meta parse failed: {:?}", v.id, e)
                });
            let decoded = decode_pipe
                .execute_rich(&encoded)
                .unwrap_or_else(|e| panic!("[{}] decode failed: {:?}", v.id, e));
            assert_eq!(
                decoded.output.trim(),
                v.expected_decoded.value,
                "[{}] round-trip FAILED: decode(encode(x)) != x",
                v.id
            );
        } else {
            let decode_source = Endpoint::language_full(
                &v.encoding.language,
                &v.encoding.wordlist,
                &v.encoding.dialect,
            );
            let decode_target = decode_target(&v.expected_decoded.format);
            let decode_pipe = Pipeline::from_params(decode_source, decode_target);

            let decoded = decode_pipe
                .execute(&encoded)
                .unwrap_or_else(|e| panic!("[{}] decode failed: {:?}", v.id, e));

            // The pipeline returns raw UTF-8 when bytes are valid UTF-8,
            // but hex-encodes when they aren't. For hex-format expected
            // values, normalize both sides to hex for comparison.
            let actual = if v.expected_decoded.format == "hex" {
                normalize_to_hex(decoded.trim())
            } else {
                decoded.trim().to_string()
            };
            assert_eq!(
                actual,
                v.expected_decoded.value,
                "[{}] round-trip FAILED: decode(encode(x)) != x",
                v.id
            );
        }
    }
}

#[test]
fn transcode_vectors() {
    let vectors = load_vectors();

    for v in vectors.round_trip_vectors.iter().filter(|v| v.transcode_to.is_some()) {
        let tc = v.transcode_to.as_ref().unwrap();

        // Step 1: encode input → language A
        let source = format_to_source_endpoint(&v.input.format);
        let target_a = Endpoint::language_full(
            &v.encoding.language,
            &v.encoding.wordlist,
            &v.encoding.dialect,
        );
        let mut pipe_a = Pipeline::from_params(source, target_a.clone());
        if let Some(seed) = v.encoding.seed {
            pipe_a = pipe_a.with_seed(seed);
        }
        let prose_a = pipe_a
            .execute(&v.input.value)
            .unwrap_or_else(|e| panic!("[{}] encode step 1 failed: {:?}", v.id, e));

        // Step 2: transcode language A → language B
        let target_b = Endpoint::language_full(&tc.language, &tc.wordlist, &tc.dialect);
        let mut pipe_tc = Pipeline::from_params(target_a.clone(), target_b.clone());
        if let Some(seed) = tc.seed {
            pipe_tc = pipe_tc.with_seed(seed);
        }
        let prose_b = pipe_tc
            .execute(&prose_a)
            .unwrap_or_else(|e| panic!("[{}] transcode step 2 failed: {:?}", v.id, e));

        // Step 3: decode language B → original format
        let decode_target = decode_target(&v.expected_decoded.format);
        let decode_pipe = Pipeline::from_params(target_b, decode_target);
        let decoded = decode_pipe
            .execute(&prose_b)
            .unwrap_or_else(|e| panic!("[{}] decode step 3 failed: {:?}", v.id, e));

        let actual = if v.expected_decoded.format == "hex" {
            normalize_to_hex(decoded.trim())
        } else {
            decoded.trim().to_string()
        };
        assert_eq!(
            actual,
            v.expected_decoded.value,
            "[{}] transcode round-trip FAILED: decode(transcode(encode(x))) != x",
            v.id
        );
    }
}

#[test]
fn error_vectors() {
    let vectors = load_vectors();

    for v in &vectors.error_vectors {
        assert_eq!(v.expected, "error", "[{}] only 'error' expected type supported", v.id);

        let source = Endpoint::language_full(
            &v.decode_from.language,
            &v.decode_from.wordlist,
            &v.decode_from.dialect,
        );
        let pipe = Pipeline::from_params(source, Endpoint::Auto);
        let result = pipe.execute(&v.input_prose);

        assert!(
            result.is_err(),
            "[{}] expected decode to fail but got: {:?}",
            v.id,
            result.unwrap()
        );
    }
}

/// Decode stability: frozen prose from past versions must decode identically forever.
/// These vectors are append-only. A failure here means backward compatibility is broken.
#[test]
fn decode_stability_vectors() {
    let vectors = load_vectors();
    let section = match vectors.decode_stability_vectors {
        Some(s) => s,
        None => return, // no stability vectors yet
    };

    for v in &section.vectors {
        // For crypto formats that need special routing, use the meta pipeline
        if let Some(ref meta) = v.decode_meta {
            let pipe = Pipeline::from_meta(meta).unwrap_or_else(|e| {
                panic!("[{}] (pinned {}) meta parse failed: {:?}", v.id, v.pinned_since, e)
            });
            let result = pipe.execute_rich(&v.encoded_prose).unwrap_or_else(|e| {
                panic!("[{}] (pinned {}) decode failed: {:?}", v.id, v.pinned_since, e)
            });
            let actual = if v.expected_decoded.format == "hex" {
                normalize_to_hex(result.output.trim())
            } else {
                result.output.trim().to_string()
            };
            assert_eq!(
                actual, v.expected_decoded.value,
                "[{}] (pinned {}) BACKWARD COMPAT BROKEN: prose from v{} no longer decodes correctly",
                v.id, v.pinned_since, v.pinned_since
            );
            continue;
        }

        let source = Endpoint::language_full(
            &v.decode_from.language,
            &v.decode_from.wordlist,
            &v.decode_from.dialect,
        );
        let pipe = Pipeline::from_params(source, Endpoint::Auto);
        let decoded = pipe.execute(&v.encoded_prose).unwrap_or_else(|e| {
            panic!("[{}] (pinned {}) decode failed: {:?}", v.id, v.pinned_since, e)
        });

        let actual = if v.expected_decoded.format == "hex" {
            normalize_to_hex(decoded.trim())
        } else {
            decoded.trim().to_string()
        };
        assert_eq!(
            actual, v.expected_decoded.value,
            "[{}] (pinned {}) BACKWARD COMPAT BROKEN: prose from v{} no longer decodes correctly",
            v.id, v.pinned_since, v.pinned_since
        );
    }
}
