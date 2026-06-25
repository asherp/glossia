use wasm_bindgen::prelude::*;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::HashSet;

use crate::generator::data::{
    get_available_languages, get_available_wordlists,
    load_payload_words_for_wordlist, build_pos_mapping_for_wordlist,
    load_cover_words_by_pos_for_wordlist,
    detect_dialect,
};
use crate::generator::types::{PayloadTok, Lexicon, GenerationMode, SentenceLengthMode};
use crate::generator::core::generate_text_with_original_payload;
use crate::merkle::WordlistTree;
use crate::codec;

/// Return JSON array of available language names.
#[wasm_bindgen]
pub fn get_languages() -> String {
    let langs = get_available_languages();
    serde_json::to_string(langs).unwrap_or_else(|_| "[]".to_string())
}

/// Return JSON array of available wordlist profiles for a language.
#[wasm_bindgen]
pub fn get_wordlists(language: &str) -> String {
    let wordlists = get_available_wordlists(language);
    serde_json::to_string(&wordlists).unwrap_or_else(|_| "[]".to_string())
}

/// Return the default wordlist profile name for a language.
///
/// Uses the grammar-declared `default_wordlist` if present, otherwise
/// falls back to the first available profile.
#[wasm_bindgen]
pub fn get_default_wordlist(language: &str) -> String {
    crate::generator::data::default_wordlist(language).to_string()
}

/// Encode input text into natural language prose.
///
/// Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... } }`
#[wasm_bindgen]
pub fn encode(input: &str, language: &str, wordlist: &str, grammar_dialect: &str, seed: u64) -> String {
    let result = encode_inner(input, language, wordlist, grammar_dialect, seed);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn encode_inner(
    input: &str,
    language: &str,
    wordlist: &str,
    grammar_dialect: &str,
    seed: u64,
) -> Result<String, String> {
    // Auto-detect subject prefix from input (Re: / Fwd:).
    // When dialect is "subject", check if input starts with a known prefix,
    // strip it, and route to the appropriate dialect variant.
    let (input, grammar_dialect) = if grammar_dialect == "subject" {
        let trimmed = input.trim_start();
        if trimmed.to_lowercase().starts_with("re:") {
            (trimmed[3..].trim_start(), "subject_re")
        } else if trimmed.to_lowercase().starts_with("fwd:") {
            (trimmed[4..].trim_start(), "subject_fwd")
        } else {
            (input, grammar_dialect)
        }
    } else {
        (input, grammar_dialect)
    };

    // 1. Load grammar
    let grammar = crate::grammar::Grammar::from_language_dialect(language, grammar_dialect)
        .map_err(|e| format!("Grammar error: {}", e))?;

    // 2. Load payload wordlist and build WordlistTree
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 3. Encode input string to payload words via codec (grammar-controlled)
    let (encoded_words, data_mode) = codec::encode_str_base_n(input, &payload_tree, grammar.codec())
        .map_err(|e| format!("Encoding error: {}", e))?;

    // 4. Build POS mapping for payload words
    let pos_mapping = build_pos_mapping_for_wordlist(language, wordlist)?;

    // 5. Build PayloadTok vec with POS tags
    let payload_toks: Vec<PayloadTok> = encoded_words
        .iter()
        .map(|word| {
            let allowed = pos_mapping
                .get(&word.to_lowercase())
                .cloned()
                .unwrap_or_default();
            PayloadTok::new(word.clone(), &allowed)
        })
        .collect();

    // 6. Build Lexicon from cover words
    let wordlist_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();
    let (cover_by_pos, refined_cover) =
        load_cover_words_by_pos_for_wordlist(&wordlist_set, language, wordlist);

    let mut lex = Lexicon::new(wordlist_set.clone(), wordlist_set);
    for (pos, words) in cover_by_pos {
        lex = lex.with_words(pos, &words.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
    lex = lex.with_refined_cover(refined_cover);

    // Derive k_min/k_max from the grammar's structure and payload size.
    // Concatenated-payload grammars (payload_separator="") encode all payload
    // in a single sentence — force k_min = k_max = overhead + payload_count.
    let min_k = grammar.min_sentence_length().unwrap_or(5);
    let concat_payload = grammar.payload_separator().is_empty();
    let (k_min, k_max) = if concat_payload {
        // CS-style grammar: force single sentence sized for all payload
        let k = min_k + payload_toks.len().saturating_sub(1);
        (k, k)
    } else {
        // Standard grammar (e.g., English): multi-sentence, short sentences
        (5, 12)
    };

    // 7. Generate text with seeded RNG
    let mut rng = StdRng::seed_from_u64(seed);
    let mode = if grammar_dialect.starts_with("subject") {
        GenerationMode::Subject
    } else {
        GenerationMode::Body
    };
    let (text, used_payload) = generate_text_with_original_payload(
        &mut rng,
        &lex,
        &payload_toks,
        None,
        false, // verbose
        mode,
        language,
        Some(grammar_dialect),
        k_min,
        k_max,
        SentenceLengthMode::Natural,
        " ", // delimiter
    );

    // 8. Build response JSON
    let payload_count = encoded_words.len();
    let total_words = text.split_whitespace().count();
    let cover_count = total_words.saturating_sub(used_payload.len());

    let response = serde_json::json!({
        "encoded_text": text,
        "payload_words": encoded_words,
        "used_payload": used_payload.into_iter().collect::<Vec<String>>(),
        "data_mode": data_mode.to_string(),
        "stats": {
            "payload_count": payload_count,
            "cover_count": cover_count,
            "total_words": total_words,
            "ratio": if total_words > 0 {
                (payload_count as f64) / (total_words as f64)
            } else {
                0.0
            }
        }
    });

    Ok(response.to_string())
}

/// Decode encoded text back to original input.
///
/// Returns JSON: `{ "payload_words": [...], "decoded_text": "..." }`
#[wasm_bindgen]
pub fn decode(text: &str, language: &str, wordlist: &str) -> String {
    let result = decode_inner(text, language, wordlist);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn decode_inner(text: &str, language: &str, wordlist: &str) -> Result<String, String> {
    // 1. Load payload wordlist
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 2. Load grammar to check payload separator
    let grammar = crate::grammar::Grammar::from_language_dialect(language, "body")
        .map_err(|e| format!("Grammar error: {}", e))?;
    let payload_separator = grammar.payload_separator();

    // 3. Filter input text to extract payload words
    // For concatenated payloads (separator=""), split each whitespace token
    // into individual characters and check each against the wordlist.
    let extracted: Vec<String> = if payload_separator.is_empty() {
        // For concatenated payloads (CS grammar), extract chars only from tokens
        // where ALL characters are in the payload set (pure payload blocks).
        // Tokens with mixed characters (like "BEGIN") are cover words — skip them.
        let payload_set: HashSet<String> = payload_words.iter()
            .map(|w| w.to_lowercase())
            .collect();
        text.split_whitespace()
            .flat_map(|token| {
                let trimmed = token.trim_matches(|c: char| {
                    !c.is_alphanumeric() && !payload_set.contains(&c.to_lowercase().to_string())
                });
                let all_in_payload = !trimmed.is_empty() && trimmed.chars()
                    .all(|c| payload_set.contains(&c.to_lowercase().to_string()));
                if all_in_payload {
                    trimmed.chars()
                        .map(|c| c.to_lowercase().to_string())
                        .collect::<Vec<_>>()
                } else {
                    vec![]  // Skip cover words
                }
            })
            .collect()
    } else {
        // Standard whitespace-delimited extraction
        text.split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| payload_tree.contains(w))
            .collect()
    };

    if extracted.is_empty() {
        return Ok(serde_json::json!({
            "payload_words": [],
            "decoded_text": "",
            "error": "No payload words found in input"
        })
        .to_string());
    }

    // 4. Decode payload words back to original string via codec (grammar-controlled)
    let bytes = codec::decode_base_n(&extracted, &payload_tree, grammar.codec())
        .map_err(|e| format!("Decoding error: {}", e))?;
    let decoded_text = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => codec::hex_encode(&bytes),
    };

    let response = serde_json::json!({
        "payload_words": extracted,
        "decoded_text": decoded_text,
    });

    Ok(response.to_string())
}

/// Generate random payload words.
///
/// Returns JSON array of random words from the specified wordlist.
#[wasm_bindgen]
pub fn random_words(count: usize, language: &str, wordlist: &str, seed: u64) -> String {
    let result = random_words_inner(count, language, wordlist, seed);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn random_words_inner(
    count: usize,
    language: &str,
    wordlist: &str,
    seed: u64,
) -> Result<String, String> {
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    if payload_words.is_empty() {
        return Err("Wordlist is empty".to_string());
    }

    let mut rng = StdRng::seed_from_u64(seed);
    let mut selected = Vec::with_capacity(count);
    for _ in 0..count {
        use rand::seq::SliceRandom;
        selected.push(payload_words.choose(&mut rng).unwrap().clone());
    }

    Ok(serde_json::to_string(&selected).unwrap_or_else(|_| "[]".to_string()))
}

/// Generate random words and encode them directly as word indices.
///
/// This bypasses the codec layer (no hex/base64/ascii detection) and directly
/// uses the random words as payload. Much more efficient for BIP39-style use cases.
///
/// Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... }, "data_mode": "words" }`
#[wasm_bindgen]
pub fn encode_random_words(
    count: usize,
    language: &str,
    wordlist: &str,
    grammar_dialect: &str,
    seed: u64,
) -> String {
    let result = encode_random_words_inner(count, language, wordlist, grammar_dialect, seed);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Auto-detect which dialect (language + wordlist) best matches the given text.
///
/// Uses compile-time precomputed word indices for fast O(log n) binary search.
/// Returns all matches sorted by score (best first).
///
/// Returns JSON array: `[{ "language": "english", "wordlist": "bip39", "dialects": ["body", "subject"],
///                         "hits": 10, "total": 12, "hit_rate": 0.83, "wordlist_size": 2048 }, ...]`
#[wasm_bindgen]
pub fn detect_dialect_from_text(text: &str) -> String {
    // Split text into words and detect
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect();

    let matches = detect_dialect(&words);

    // Convert to JSON
    let json_matches: Vec<serde_json::Value> = matches
        .iter()
        .map(|m| {
            serde_json::json!({
                "language": m.language,
                "wordlist": m.wordlist,
                "dialects": m.dialects,
                "hits": m.hits,
                "total": m.total,
                "hit_rate": m.hit_rate,
                "wordlist_size": m.wordlist_size
            })
        })
        .collect();

    serde_json::to_string(&json_matches).unwrap_or_else(|_| "[]".to_string())
}

/// Get all available dialects across all languages with full metadata.
///
/// Returns a hierarchical structure for building a dialect selector UI.
///
/// Returns JSON:
/// ```json
/// [
///   {
///     "language": "english",
///     "language_display": "English",
///     "dialects": [
///       {
///         "dialect": "body",
///         "display_name": "BIP39 (Natural Body)",
///         "full_id": "english-bip39-body",
///         "payload_wordlist": "bip39",
///         "cover_wordlist": "default",
///         "wordlist_size": 2048,
///         "bits_per_word": 11.0,
///         "is_character_level": false,
///         "description": "Natural multi-sentence prose"
///       },
///       ...
///     ]
///   },
///   ...
/// ]
/// ```
#[wasm_bindgen]
pub fn get_all_dialects() -> String {
    use crate::grammar::DialectConfig;

    let languages = get_available_languages();
    let mut result = Vec::new();

    for &lang in languages {
        let dialects = DialectConfig::available_dialects(lang);
        let mut dialect_list = Vec::new();

        for dialect_name in dialects {
            // Try to load the dialect config to get wordlist info
            match DialectConfig::from_language_dialect(lang, &dialect_name) {
                Ok(config) => {
                    let payload_wl = config.payload_wordlist();
                    let cover_wl = config.cover_wordlist();
                    let wordlist_size = crate::generator::data::get_wordlist_size(lang, payload_wl);

                    let is_pow2 = wordlist_size.is_power_of_two();
                    let bits_per_word = if is_pow2 {
                        wordlist_size.trailing_zeros() as f64
                    } else {
                        (wordlist_size as f64).log2()
                    };

                    // Check if this is character-level encoding
                    let is_character_level = config.grammar.payload_separator().is_empty();

                    // Generate friendly display name
                    let display_name = generate_dialect_display_name(
                        lang,
                        &dialect_name,
                        payload_wl,
                        is_character_level
                    );

                    // Generate full ID for unique identification
                    let full_id = format!("{}-{}-{}", lang, payload_wl, dialect_name);

                    dialect_list.push(serde_json::json!({
                        "dialect": dialect_name,
                        "display_name": display_name,
                        "full_id": full_id,
                        "payload_wordlist": payload_wl,
                        "cover_wordlist": cover_wl,
                        "wordlist_size": wordlist_size,
                        "bits_per_word": bits_per_word,
                        "is_power_of_two": is_pow2,
                        "is_character_level": is_character_level,
                    }));
                }
                Err(_) => {
                    // Skip dialects that fail to load
                    continue;
                }
            }
        }

        if !dialect_list.is_empty() {
            let language_display = match lang {
                "english" => "English",
                "latin" => "Latin",
                "cs" => "Cryptographic Signatures",
                "hp" => "Harry Potter",
                "math" => "Mathematics",
                _ => lang,
            };

            result.push(serde_json::json!({
                "language": lang,
                "language_display": language_display,
                "dialects": dialect_list,
            }));
        }
    }

    serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
}

/// Generate a friendly display name for a dialect
fn generate_dialect_display_name(
    language: &str,
    dialect: &str,
    payload_wordlist: &str,
    is_character_level: bool,
) -> String {
    match (language, dialect, payload_wordlist) {
        // English BIP39 dialects
        ("english", "body", "bip39") => "BIP39 (Natural Body)".to_string(),
        ("english", "subject", "bip39") => "BIP39 (Subject Lines)".to_string(),
        ("english", "prose", "bip39") => "BIP39 (Literary Prose)".to_string(),
        ("english", "payload_only", "bip39") => "BIP39 (Words Only)".to_string(),

        // Latin dialects
        ("latin", "body", _) => "Latin (Natural Body)".to_string(),
        ("latin", "subject", _) => "Latin (Subject Lines)".to_string(),
        ("latin", "spells", "hp") => "Harry Potter Spells".to_string(),
        ("latin", "payload_only", _) => "Latin (Words Only)".to_string(),

        // CS (Cryptographic Signature) dialects
        ("cs", "nip04", "base58") => "NIP-04 Encrypted Message".to_string(),
        ("cs", "nip44", "base58") => "NIP-44 Encrypted Message".to_string(),
        ("cs", "pgp", "base58") => "PGP Encrypted Message".to_string(),
        ("cs", "ascii7", "ascii7") => "Plain ASCII Message".to_string(),
        ("cs", "sig", "base58") => "Plain Signature".to_string(),
        ("cs", "sig_pgp", "base58") => "PGP Signature".to_string(),
        ("cs", "sig_nostr", "base16") => "Nostr Signature".to_string(),
        ("cs", "sig_latin", "default") => "Nostr Signature (Latin)".to_string(),
        ("cs", "seal_nostr", "bech32") => "Nostr Seal".to_string(),
        ("cs", "raw", _) => "Raw (No Armor)".to_string(),

        // Generic fallback
        _ if is_character_level => {
            format!("{} ({})", dialect, payload_wordlist.to_uppercase())
        }
        _ => {
            format!("{} ({})", dialect, payload_wordlist)
        }
    }
}

/// Transcode text from one language to another via Pipeline.
///
/// The meta instruction is a natural-language pipeline specification:
/// - `"translate from english into latin"` — transcode English prose to Latin
/// - `"encode into english"` — encode raw data into English prose
/// - `"decode from english"` — decode English prose back to raw data
///
/// Returns JSON: `{ "output": "...", "source": "...", "target": "..." }` or `{ "error": "..." }`
#[wasm_bindgen]
pub fn transcode(input: &str, meta_instruction: &str, seed: u64) -> String {
    let result = transcode_inner(input, meta_instruction, seed);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn transcode_inner(input: &str, meta_instruction: &str, seed: u64) -> Result<String, String> {
    use crate::pipeline::Pipeline;

    let pipeline = Pipeline::from_meta(meta_instruction)
        .map_err(|e| format!("Pipeline parse error: {}", e))?;

    let pipeline = pipeline.with_seed(seed);

    let result = pipeline.execute_rich(input)
        .map_err(|e| format!("Pipeline error: {}", e))?;

    let mut response = serde_json::json!({
        "output": result.output,
        "source": format!("{}", pipeline.source),
        "target": format!("{}", pipeline.target),
        "payload_words": result.payload_words,
    });

    if let Some(mode) = &result.data_mode {
        response["data_mode"] = serde_json::json!(mode.to_string());
    }

    if let Some(stats) = &result.stats {
        response["stats"] = serde_json::json!({
            "payload_count": stats.payload_count,
            "cover_count": stats.cover_count,
            "total_words": stats.total_words,
            "ratio": stats.ratio,
        });
    }

    if let Some(ref resolved) = result.resolved_source {
        response["resolved_source"] = serde_json::json!(format!("{}", resolved));
    }

    Ok(response.to_string())
}

/// Execute a pipeline with explicit source and target endpoints.
///
/// Source/target follow the same format as `--from`/`--into` CLI flags:
/// - Language: `"english"`, `"latin"`, `"english/bip39/body"`
/// - Format: `"hex"`, `"base64"`, `"ascii7"`, `"bytes"`
/// - Auto: `"auto"` — auto-detect from input content
///
/// Returns JSON: `{ "output": "...", "source": "...", "target": "..." }` or `{ "error": "..." }`
#[wasm_bindgen]
pub fn pipeline_execute(input: &str, source: &str, target: &str, seed: u64) -> String {
    let result = pipeline_execute_inner(input, source, target, seed);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn pipeline_execute_inner(
    input: &str,
    source_str: &str,
    target_str: &str,
    seed: u64,
) -> Result<String, String> {
    use crate::pipeline::{Pipeline, Endpoint};
    use crate::generator::data::default_wordlist;

    let parse_ep = |s: &str| -> Endpoint {
        match s.to_lowercase().as_str() {
            "hex" => Endpoint::Format(codec::DataMode::Hex),
            "base64" => Endpoint::Format(codec::DataMode::Base64),
            "ascii7" | "ascii" => Endpoint::Format(codec::DataMode::Ascii7),
            "bytes" | "bytes8" => Endpoint::Format(codec::DataMode::Bytes8),
            "auto" => Endpoint::Auto,
            _ => {
                let parts: Vec<&str> = s.split('/').collect();
                match parts.len() {
                    3 => Endpoint::language_full(parts[0], parts[1], parts[2]),
                    2 => Endpoint::Language {
                        language: parts[0].to_string(),
                        wordlist: parts[1].to_string(),
                        dialect: "body".to_string(),
                    },
                    _ => {
                        let lang = parts[0];
                        let wl = default_wordlist(lang);
                        Endpoint::Language {
                            language: lang.to_string(),
                            wordlist: wl.to_string(),
                            dialect: "body".to_string(),
                        }
                    }
                }
            }
        }
    };

    let source = parse_ep(source_str);
    let target = parse_ep(target_str);
    let pipeline = Pipeline::from_params(source, target).with_seed(seed);

    let output = pipeline.execute(input)
        .map_err(|e| format!("Pipeline error: {}", e))?;

    let response = serde_json::json!({
        "output": output,
        "source": format!("{}", pipeline.source),
        "target": format!("{}", pipeline.target),
    });

    Ok(response.to_string())
}

/// Get the exact wordlist size for a language/wordlist combination.
///
/// Returns JSON: `{ "size": 2048 }` or `{ "error": "..." }`
#[wasm_bindgen]
pub fn get_wordlist_size(language: &str, wordlist: &str) -> String {
    use crate::generator::data;

    let size = data::get_wordlist_size(language, wordlist);

    if size == 0 {
        return serde_json::json!({
            "error": format!("Unknown wordlist: {}/{}", language, wordlist)
        }).to_string();
    }

    serde_json::json!({ "size": size }).to_string()
}

/// Get the bits per word for a language/wordlist combination.
///
/// For power-of-two wordlists (BIP39, etc.): returns exact integer bits
/// For non-power-of-two wordlists (base58, base64): returns fractional bits
///
/// Returns JSON: `{ "bits_per_word": 11.0, "is_power_of_two": true }` or `{ "error": "..." }`
#[wasm_bindgen]
pub fn get_bits_per_word(language: &str, wordlist: &str) -> String {
    use crate::generator::data;

    let size = data::get_wordlist_size(language, wordlist);

    if size == 0 {
        return serde_json::json!({
            "error": format!("Unknown wordlist: {}/{}", language, wordlist)
        }).to_string();
    }

    let is_pow2 = size.is_power_of_two();
    let bits = if is_pow2 {
        // Exact bits for power-of-two sizes
        size.trailing_zeros() as f64
    } else {
        // Fractional bits for non-power-of-two sizes (e.g., base58 = 58 chars)
        (size as f64).log2()
    };

    serde_json::json!({
        "bits_per_word": bits,
        "is_power_of_two": is_pow2,
        "note": if !is_pow2 {
            Some("Character-level encoding - use characters directly as payload words")
        } else {
            None
        }
    }).to_string()
}

/// Encode pre-formatted data (hex, base58, base64) using character-level encoding.
///
/// This bypasses the codec layer and uses each character directly as a payload word.
/// Designed for CS (cryptographic signature) dialects that use payload_separator: "".
///
/// Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... } }`
#[wasm_bindgen]
pub fn encode_characters(
    input: &str,
    language: &str,
    wordlist: &str,
    grammar_dialect: &str,
    seed: u64,
) -> String {
    let result = encode_characters_inner(input, language, wordlist, grammar_dialect, seed);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn encode_characters_inner(
    input: &str,
    language: &str,
    wordlist: &str,
    grammar_dialect: &str,
    seed: u64,
) -> Result<String, String> {
    // 1. Load payload wordlist
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    let payload_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();

    // 2. Split input into characters and validate each is in the wordlist
    let character_words: Vec<String> = input
        .chars()
        .map(|c| c.to_string())
        .collect();

    // Validate that all characters are in the wordlist
    for ch in &character_words {
        if !payload_set.contains(&ch.to_lowercase()) {
            return Err(format!(
                "Character '{}' not found in {}/{} wordlist",
                ch, language, wordlist
            ));
        }
    }

    // 3. Build POS mapping
    let pos_mapping = build_pos_mapping_for_wordlist(language, wordlist)?;

    // 4. Build PayloadTok vec with POS tags
    let payload_toks: Vec<PayloadTok> = character_words
        .iter()
        .map(|word| {
            let allowed = pos_mapping
                .get(&word.to_lowercase())
                .cloned()
                .unwrap_or_default();
            PayloadTok::new(word.clone(), &allowed)
        })
        .collect();

    // 5. Build Lexicon from cover words
    let (cover_by_pos, refined_cover) =
        load_cover_words_by_pos_for_wordlist(&payload_set, language, wordlist);

    let mut lex = Lexicon::new(payload_set.clone(), payload_set);
    for (pos, words) in cover_by_pos {
        lex = lex.with_words(pos, &words.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
    lex = lex.with_refined_cover(refined_cover);

    // 6. Load grammar and compute dynamic k_min/k_max
    let grammar = crate::grammar::Grammar::from_language_dialect(language, grammar_dialect)
        .map_err(|e| format!("Grammar error: {}", e))?;

    let min_k = grammar.min_sentence_length().unwrap_or(5);
    let concat_payload = grammar.payload_separator().is_empty();
    let (k_min, k_max) = if concat_payload {
        let k = min_k + payload_toks.len().saturating_sub(1);
        (k, k)
    } else {
        (5, 12)
    };

    // 7. Generate text
    let mut rng = StdRng::seed_from_u64(seed);
    let mode = if grammar_dialect.starts_with("subject") {
        GenerationMode::Subject
    } else {
        GenerationMode::Body
    };
    let (text, used_payload) = generate_text_with_original_payload(
        &mut rng,
        &lex,
        &payload_toks,
        None,
        false, // verbose
        mode,
        language,
        Some(grammar_dialect),
        k_min,
        k_max,
        SentenceLengthMode::Natural,
        " ", // delimiter
    );

    // 8. Build response JSON
    let payload_count = character_words.len();
    let total_words = text.split_whitespace().count();
    let cover_count = total_words.saturating_sub(used_payload.len());

    let response = serde_json::json!({
        "encoded_text": text,
        "payload_words": character_words,
        "used_payload": used_payload.into_iter().collect::<Vec<String>>(),
        "data_mode": "characters", // Special mode for character-level encoding
        "stats": {
            "payload_count": payload_count,
            "cover_count": cover_count,
            "total_words": total_words,
            "ratio": if total_words > 0 {
                (payload_count as f64) / (total_words as f64)
            } else {
                0.0
            }
        }
    });

    Ok(response.to_string())
}

/// Render text notation (containing color tokens) as an SVG string.
///
/// Mirrors the CLI `render_text_to_svg` logic: extracts hex colors from the
/// text notation, maps the dialect name to a layout, and returns SVG markup.
///
/// `dialect` selects the layout: "voronoi" (default), "grid", "constellation", "patches".
/// `circular` enables circular (disk) clipping on the canvas.
///
/// Returns the raw SVG string on success, or JSON `{"error":"..."}` on failure.
#[wasm_bindgen]
pub fn render_image_svg(text: &str, dialect: &str, width: f64, height: f64, seed: u64, circular: bool) -> String {
    match render_image_svg_inner(text, dialect, width, height, seed, circular) {
        Ok(svg) => svg,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn render_image_svg_inner(
    text: &str,
    dialect: &str,
    width: f64,
    height: f64,
    seed: u64,
    circular: bool,
) -> Result<String, String> {
    use crate::image_codec::render;
    use crate::image_codec::svg::{self, SvgConfig, Layout};

    let hex_colors = render::extract_hex_colors(text);
    if hex_colors.is_empty() {
        return Err("No color tokens found in text notation".to_string());
    }

    let layout = match dialect {
        "grid" | "patches" => Layout::Grid,
        "constellation" => Layout::Constellation,
        _ => Layout::Voronoi,
    };

    let cols = match dialect {
        "patches" => 4,
        "grid" => 8,
        _ => 8,
    };

    let config = SvgConfig {
        width,
        height,
        layout,
        seed,
        cols,
        circular,
        color_scatter: true,
        ..Default::default()
    };

    let color_refs: Vec<&str> = hex_colors.iter().map(|s| s.as_str()).collect();
    Ok(svg::render_svg(&color_refs, &config))
}

/// Decode an image from extracted hex colors back to original data.
///
/// Takes a JSON array of CSS hex colors (e.g., `["#440255", "#2a788e", ...]`)
/// extracted from SVG fill attributes, plus the palette name (e.g., "viridis").
///
/// Each hex color is matched to the nearest CIELAB payload word in the palette
/// wordlist. The matched words are then decoded back to the original payload.
///
/// Returns JSON: `{ "decoded_text": "...", "payload_words": [...],
///   "color_words": [...], "n_colors": N, "bits_per_color": B }`
/// or `{ "error": "..." }` on failure.
#[wasm_bindgen]
pub fn decode_image_from_colors(hex_colors_json: &str, palette: &str) -> String {
    match decode_image_from_colors_inner(hex_colors_json, palette) {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn decode_image_from_colors_inner(
    hex_colors_json: &str,
    palette: &str,
) -> Result<String, String> {
    use crate::image_codec::color::{srgb_to_lab, Srgb, Lab};

    // 1. Parse hex colors from JSON array
    let hex_colors: Vec<String> = serde_json::from_str(hex_colors_json)
        .map_err(|e| format!("Invalid hex colors JSON: {}", e))?;

    if hex_colors.is_empty() {
        return Err("No colors provided".to_string());
    }

    // 2. Load payload wordlist for image/palette
    let payload_words = load_payload_words_for_wordlist("image", palette)?;
    if payload_words.is_empty() {
        return Err(format!("Empty payload wordlist for image/{}", palette));
    }

    // 3. Parse each payload word as CIELAB coordinates
    let palette_labs: Vec<(String, Lab)> = payload_words.iter().filter_map(|word| {
        let parts: Vec<&str> = word.split('_').collect();
        if parts.len() != 3 { return None; }
        let l: f64 = parts[0].parse().ok()?;
        let a: f64 = parts[1].parse().ok()?;
        let b: f64 = parts[2].parse().ok()?;
        Some((word.clone(), Lab { l, a, b }))
    }).collect();

    if palette_labs.is_empty() {
        return Err("No valid CIELAB tokens in payload wordlist".to_string());
    }

    // 4. For each hex color, convert to Lab and find nearest palette word
    let mut matched_words: Vec<String> = Vec::with_capacity(hex_colors.len());
    for hex in &hex_colors {
        let h = hex.trim_start_matches('#');
        if h.len() != 6 {
            return Err(format!("Invalid hex color: {}", hex));
        }
        let r = u8::from_str_radix(&h[0..2], 16).map_err(|_| format!("Bad hex: {}", hex))?;
        let g = u8::from_str_radix(&h[2..4], 16).map_err(|_| format!("Bad hex: {}", hex))?;
        let b = u8::from_str_radix(&h[4..6], 16).map_err(|_| format!("Bad hex: {}", hex))?;

        let lab = srgb_to_lab(&Srgb { r, g, b });

        // Find nearest palette center (minimum deltaE in Lab space)
        let nearest = palette_labs.iter()
            .min_by(|(_, a), (_, b)| {
                let da = (lab.l - a.l).powi(2) + (lab.a - a.a).powi(2) + (lab.b - a.b).powi(2);
                let db = (lab.l - b.l).powi(2) + (lab.a - b.a).powi(2) + (lab.b - b.b).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|(word, _)| word.clone())
            .unwrap();

        matched_words.push(nearest);
    }

    // 5. Build text notation from matched words (space-separated CIELAB tokens)
    let text_notation = matched_words.join(" ");

    // 6. Decode using base-N conversion (image grammar declares codec: base_n)
    let payload_tree = crate::merkle::WordlistTree::new(payload_words.clone());
    let bytes = codec::decode_base_n(&matched_words, &payload_tree, "base_n")
        .map_err(|e| format!("Decoding error: {}", e))?;
    let decoded_text = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => codec::hex_encode(&bytes),
    };

    // 7. Compute bits per color
    let n_colors = palette_labs.len();
    let bits_per_color = (n_colors as f64).log2();

    let response = serde_json::json!({
        "decoded_text": decoded_text,
        "payload_words": matched_words,
        "text_notation": text_notation,
        "n_colors": n_colors,
        "bits_per_color": bits_per_color,
    });

    Ok(response.to_string())
}

/// Encode a hex payload into a banner SVG.
///
/// Creates a Voronoi banner with RS error correction encoding the payload
/// bytes. The SVG contains self-describing header + payload cells.
///
/// Args:
///   - `payload_hex`: hex-encoded payload bytes (e.g., "deadbeef...")
///   - `width`, `height`: banner dimensions
///   - `seed`: random seed for Voronoi layout
///
/// Returns SVG string on success, or JSON `{"error":"..."}` on failure.
#[wasm_bindgen]
pub fn encode_image_banner(payload_hex: &str, width: f64, height: f64, seed: u64) -> String {
    match encode_image_banner_inner(payload_hex, width, height, seed) {
        Ok(svg) => svg,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn encode_image_banner_inner(
    payload_hex: &str,
    width: f64,
    height: f64,
    seed: u64,
) -> Result<String, String> {
    use crate::image_codec::render::viridis_approx_curve;
    use crate::image_codec::frame::BishopFrame;
    use crate::image_codec::capacity::select_encoding_params;
    use crate::image_codec::banner::encode_banner;

    // Parse hex payload
    let payload = parse_hex_bytes(payload_hex)?;

    // Build curve and frame
    let curve = viridis_approx_curve();
    let frame = BishopFrame::new(&curve, 500);

    // Select optimal encoding params
    let params = select_encoding_params(&curve, &frame, 50)
        .ok_or_else(|| "No valid encoding configuration found".to_string())?;

    let nsym = 16; // Default RS parity bytes

    // Encode banner
    let encoded = encode_banner(
        &payload, &curve, &frame,
        params.n, params.epsilon, nsym,
        width as usize, height as usize, seed, 10,
    )?;

    // Render as SVG
    let svg = render_banner_svg_from_encoded(&encoded, width, height);

    Ok(svg)
}

/// Render a BannerEncoded as SVG string (WASM-compatible, no image crate needed).
fn render_banner_svg_from_encoded(
    encoded: &crate::image_codec::banner::BannerEncoded,
    width: f64,
    height: f64,
) -> String {
    use crate::image_codec::voronoi::voronoi_cells;

    let n = encoded.seeds.len();

    // Build sRGB color per seed
    let mut seed_colors = vec![crate::image_codec::color::Srgb::new(10, 10, 25); n];
    for (cell_idx, &seed_idx) in encoded.cell_to_seed.iter().enumerate() {
        if cell_idx < encoded.cells_srgb.len() && seed_idx < n {
            seed_colors[seed_idx] = encoded.cells_srgb[cell_idx];
        }
    }

    // Generate Voronoi cells
    let cells = voronoi_cells(&encoded.seeds, width, height);

    let mut lines = Vec::new();
    lines.push(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\">",
        width, height, width, height
    ));
    lines.push(format!(
        "<rect width=\"{}\" height=\"{}\" fill=\"#0a0a19\"/>",
        width, height
    ));

    for (i, cell) in cells.iter().enumerate() {
        let c = &seed_colors[i];
        let points_str = cell.svg_points();
        lines.push(format!(
            "<polygon points=\"{}\" fill=\"rgb({},{},{})\" stroke=\"#0a0a19\" stroke-width=\"2\"/>",
            points_str, c.r, c.g, c.b
        ));
    }

    lines.push("</svg>".to_string());
    lines.join("\n")
}

/// Decode a banner from hex colors extracted from SVG cells.
///
/// Takes a JSON array of hex color strings (one per Voronoi cell, in scan order)
/// and decodes the embedded payload bytes.
///
/// Returns JSON: `{ "payload_hex": "...", "n_palette": N, "epsilon": E, "success": true }`
/// or `{ "error": "..." }` on failure.
#[wasm_bindgen]
pub fn decode_image_banner(hex_colors_json: &str, nsym: usize) -> String {
    match decode_image_banner_inner(hex_colors_json, nsym) {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn decode_image_banner_inner(
    hex_colors_json: &str,
    nsym: usize,
) -> Result<String, String> {
    use crate::image_codec::render::viridis_approx_curve;
    use crate::image_codec::frame::BishopFrame;
    use crate::image_codec::capacity::derive_config_table;
    use crate::image_codec::codec::decode_header;
    use crate::image_codec::rs_encoding::RSEncoder;
    use crate::image_codec::color::{srgb_to_lab, Srgb, Lab};

    // Parse hex colors from JSON
    let hex_colors: Vec<String> = serde_json::from_str(hex_colors_json)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    if hex_colors.is_empty() {
        return Err("No colors provided".to_string());
    }

    // Convert to Lab
    let labs: Vec<Lab> = hex_colors.iter().map(|hex| {
        let h = hex.trim_start_matches('#');
        let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
        srgb_to_lab(&Srgb::new(r, g, b))
    }).collect();

    // Build curve and decode header
    let curve = viridis_approx_curve();
    let frame = BishopFrame::new(&curve, 500);
    let (configs, header_eps) = derive_config_table(&curve, &frame, 50);

    let config = decode_header(&labs[0], &curve, &frame, &configs, header_eps)?;
    let n_palette = config.n;
    let epsilon = config.epsilon;

    // Decode payload cells (skip header)
    let payload_labs = &labs[1..];
    let rse = RSEncoder::from_curve(&curve, &frame, n_palette, epsilon, Some(nsym), 0.5);

    let n_payload = payload_labs.len();
    let rs_total = (n_payload as f64 * rse.bits_per_cell / 8.0).floor() as usize;
    if rs_total <= nsym {
        return Err("Too few cells for RS decode".to_string());
    }

    let (recovered, dec_meta) = rse.decode_bytes_with_params(payload_labs, Some(rs_total), Some(nsym))?;

    if dec_meta.success {
        let hex_str: String = recovered.iter().map(|b| format!("{:02x}", b)).collect();
        let response = serde_json::json!({
            "payload_hex": hex_str,
            "n_palette": n_palette,
            "epsilon": epsilon,
            "success": true,
            "errors_corrected": dec_meta.errors_corrected,
            "cells_decoded": dec_meta.cells_decoded,
        });
        Ok(response.to_string())
    } else {
        Ok(serde_json::json!({
            "success": false,
            "error": dec_meta.error_message.unwrap_or_else(|| "RS decode failed".to_string()),
            "n_palette": n_palette,
            "epsilon": epsilon,
        }).to_string())
    }
}

fn encode_random_words_inner(
    count: usize,
    language: &str,
    wordlist: &str,
    grammar_dialect: &str,
    seed: u64,
) -> Result<String, String> {
    // 1. Load payload wordlist
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    if payload_words.is_empty() {
        return Err("Wordlist is empty".to_string());
    }

    // 2. Generate random words
    let mut rng = StdRng::seed_from_u64(seed);
    let mut selected_words = Vec::with_capacity(count);
    for _ in 0..count {
        use rand::seq::SliceRandom;
        selected_words.push(payload_words.choose(&mut rng).unwrap().clone());
    }

    // 3. Build POS mapping for payload words
    let pos_mapping = build_pos_mapping_for_wordlist(language, wordlist)?;

    // 4. Build PayloadTok vec with POS tags (directly from random words)
    let payload_toks: Vec<PayloadTok> = selected_words
        .iter()
        .map(|word| {
            let allowed = pos_mapping
                .get(&word.to_lowercase())
                .cloned()
                .unwrap_or_default();
            PayloadTok::new(word.clone(), &allowed)
        })
        .collect();

    // 5. Build Lexicon from cover words
    let wordlist_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();
    let (cover_by_pos, refined_cover) =
        load_cover_words_by_pos_for_wordlist(&wordlist_set, language, wordlist);

    let mut lex = Lexicon::new(wordlist_set.clone(), wordlist_set);
    for (pos, words) in cover_by_pos {
        lex = lex.with_words(pos, &words.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
    lex = lex.with_refined_cover(refined_cover);

    // 6. Load grammar and compute dynamic k_min/k_max
    let grammar = crate::grammar::Grammar::from_language_dialect(language, grammar_dialect)
        .map_err(|e| format!("Grammar error: {}", e))?;

    let min_k = grammar.min_sentence_length().unwrap_or(5);
    let concat_payload = grammar.payload_separator().is_empty();
    let (k_min, k_max) = if concat_payload {
        let k = min_k + payload_toks.len().saturating_sub(1);
        (k, k)
    } else {
        (5, 12)
    };

    // 7. Generate text with the same RNG (re-seeded for text generation)
    let mut text_rng = StdRng::seed_from_u64(seed.wrapping_add(1)); // Offset seed for text gen
    let mode = if grammar_dialect.starts_with("subject") {
        GenerationMode::Subject
    } else {
        GenerationMode::Body
    };
    let (text, used_payload) = generate_text_with_original_payload(
        &mut text_rng,
        &lex,
        &payload_toks,
        None,
        false, // verbose
        mode,
        language,
        Some(grammar_dialect),
        k_min,
        k_max,
        SentenceLengthMode::Natural,
        " ", // delimiter
    );

    // 8. Build response JSON
    let payload_count = selected_words.len();
    let total_words = text.split_whitespace().count();
    let cover_count = total_words.saturating_sub(used_payload.len());

    let response = serde_json::json!({
        "encoded_text": text,
        "payload_words": selected_words,
        "used_payload": used_payload.into_iter().collect::<Vec<String>>(),
        "data_mode": "words", // Special mode for direct word encoding
        "stats": {
            "payload_count": payload_count,
            "cover_count": cover_count,
            "total_words": total_words,
            "ratio": if total_words > 0 {
                (payload_count as f64) / (total_words as f64)
            } else {
                0.0
            }
        }
    });

    Ok(response.to_string())
}

/// Encode raw bytes (hex or base64 input) into base-N payload words.
///
/// If `dialect` is empty, returns space-joined bare payload words.
/// If `dialect` is provided (e.g., "body"), wraps the payload words in prose.
///
/// Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... } }`
#[wasm_bindgen]
pub fn encode_raw_base_n(input: &str, language: &str, wordlist: &str, dialect: &str, seed: u64) -> String {
    let result = encode_raw_base_n_inner(input, language, wordlist, dialect, seed);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn encode_raw_base_n_inner(
    input: &str,
    language: &str,
    wordlist: &str,
    dialect: &str,
    seed: u64,
) -> Result<String, String> {
    // 1. Auto-detect hex/base64 and decode to raw bytes
    let (_mode, bytes) = codec::detect_mode(input);

    // 2. Load payload wordlist and build WordlistTree
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 3. Encode bytes to payload words via bitpack_fixed codec
    let byte_count = bytes.len();
    let encoded_words = codec::encode_base_n(&bytes, &payload_tree, "bitpack_fixed")
        .map_err(|e| format!("Encoding error: {}", e))?;

    // 4. If no dialect, return bare words
    if dialect.is_empty() {
        let response = serde_json::json!({
            "encoded_text": encoded_words.join(" "),
            "payload_words": encoded_words,
            "data_mode": "bitpack_fixed",
            "byte_count": byte_count,
            "stats": {
                "payload_count": encoded_words.len(),
                "cover_count": 0,
                "total_words": encoded_words.len(),
                "ratio": 1.0
            }
        });
        return Ok(response.to_string());
    }

    // 5. Wrap in prose using the specified dialect
    let pos_mapping = build_pos_mapping_for_wordlist(language, wordlist)?;

    let payload_toks: Vec<PayloadTok> = encoded_words
        .iter()
        .map(|word| {
            let allowed = pos_mapping
                .get(&word.to_lowercase())
                .cloned()
                .unwrap_or_default();
            PayloadTok::new(word.clone(), &allowed)
        })
        .collect();

    let wordlist_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();
    let (cover_by_pos, refined_cover) =
        load_cover_words_by_pos_for_wordlist(&wordlist_set, language, wordlist);

    let mut lex = Lexicon::new(wordlist_set.clone(), wordlist_set);
    for (pos, words) in cover_by_pos {
        lex = lex.with_words(pos, &words.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
    lex = lex.with_refined_cover(refined_cover);

    let grammar = crate::grammar::Grammar::from_language_dialect(language, dialect)
        .map_err(|e| format!("Grammar error: {}", e))?;

    let min_k = grammar.min_sentence_length().unwrap_or(5);
    let concat_payload = grammar.payload_separator().is_empty();
    let (k_min, k_max) = if concat_payload {
        let k = min_k + payload_toks.len().saturating_sub(1);
        (k, k)
    } else {
        (5, 12)
    };

    let mut rng = StdRng::seed_from_u64(seed);
    let mode = if dialect.starts_with("subject") {
        GenerationMode::Subject
    } else {
        GenerationMode::Body
    };
    let (text, used_payload) = generate_text_with_original_payload(
        &mut rng,
        &lex,
        &payload_toks,
        None,
        false,
        mode,
        language,
        Some(dialect),
        k_min,
        k_max,
        SentenceLengthMode::Natural,
        " ",
    );

    let payload_count = encoded_words.len();
    let total_words = text.split_whitespace().count();
    let cover_count = total_words.saturating_sub(used_payload.len());

    let response = serde_json::json!({
        "encoded_text": text,
        "payload_words": encoded_words,
        "used_payload": used_payload.into_iter().collect::<Vec<String>>(),
        "data_mode": "bitpack_fixed",
        "byte_count": byte_count,
        "stats": {
            "payload_count": payload_count,
            "cover_count": cover_count,
            "total_words": total_words,
            "ratio": if total_words > 0 {
                (payload_count as f64) / (total_words as f64)
            } else {
                0.0
            }
        }
    });

    Ok(response.to_string())
}

/// Decode base-N encoded text back to raw bytes (hex output).
///
/// Works with both bare payload words and prose-wrapped text —
/// cover words are automatically filtered out.
///
/// `expected_byte_count`: the known payload size in bytes (e.g. 32 for a pubkey).
/// Pass 0 to infer from word count (exact when bits_per_word divides payload evenly).
///
/// Returns JSON: `{ "decoded_hex": "...", "payload_words": [...] }`
#[wasm_bindgen]
pub fn decode_raw_base_n(text: &str, language: &str, wordlist: &str, expected_byte_count: usize) -> String {
    let result = decode_raw_base_n_inner(text, language, wordlist, expected_byte_count);
    match result {
        Ok(json) => json,
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn decode_raw_base_n_inner(text: &str, language: &str, wordlist: &str, expected_byte_count: usize) -> Result<String, String> {
    // 1. Load payload wordlist
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 2. Build payload set for filtering
    let payload_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();

    // 3. Extract payload words (filter out cover/prose words)
    let extracted: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| payload_set.contains(w))
        .collect();

    if extracted.is_empty() {
        return Ok(serde_json::json!({
            "decoded_hex": "",
            "payload_words": [],
            "error": "No payload words found in input"
        }).to_string());
    }

    // 4. Decode via bitpack_fixed codec with known byte count
    let bytes = if expected_byte_count > 0 {
        codec::decode_base_n_fixed(&extracted, &payload_tree, "bitpack_fixed", expected_byte_count)
    } else {
        codec::decode_base_n(&extracted, &payload_tree, "bitpack_fixed")
    }.map_err(|e| format!("Decoding error: {}", e))?;

    // 5. Return hex-encoded bytes
    let hex = codec::hex_encode(&bytes);

    let response = serde_json::json!({
        "decoded_hex": hex,
        "payload_words": extracted,
    });

    Ok(response.to_string())
}

/// Parse a hex string to bytes (no external dependency).
fn parse_hex_bytes(hex: &str) -> Result<Vec<u8>, String> {
    let hex = hex.trim().trim_start_matches("0x").trim_start_matches("0X");
    if hex.len() % 2 != 0 {
        return Err("Hex string must have even length".to_string());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16)
            .map_err(|_| format!("Invalid hex at position {}: '{}'", i, &hex[i..i + 2])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 64-byte signature (hex-encoded = 128 hex chars)
    const SIG_HEX_64: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";
    // 32-byte pubkey
    const PUBKEY_HEX_32: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa3f4a18446b0b8d183f8e8";

    #[test]
    fn roundtrip_bare_64_bytes() {
        let encoded_json = encode_raw_base_n(SIG_HEX_64, "latin", "default", "", 42);
        let enc: serde_json::Value = serde_json::from_str(&encoded_json).unwrap();
        assert!(enc.get("error").is_none(), "encode error: {}", encoded_json);

        let bare_text = enc["encoded_text"].as_str().unwrap();
        // Bare mode: no cover words, just payload
        assert!(!bare_text.is_empty());

        let decoded_json = decode_raw_base_n(bare_text, "latin", "default", 64);
        let dec: serde_json::Value = serde_json::from_str(&decoded_json).unwrap();
        assert!(dec.get("error").is_none(), "decode error: {}", decoded_json);
        assert_eq!(dec["decoded_hex"].as_str().unwrap(), SIG_HEX_64);
    }

    #[test]
    fn roundtrip_bare_32_bytes() {
        let encoded_json = encode_raw_base_n(PUBKEY_HEX_32, "latin", "default", "", 42);
        let enc: serde_json::Value = serde_json::from_str(&encoded_json).unwrap();
        assert!(enc.get("error").is_none(), "encode error: {}", encoded_json);

        let bare_text = enc["encoded_text"].as_str().unwrap();
        let decoded_json = decode_raw_base_n(bare_text, "latin", "default", 32);
        let dec: serde_json::Value = serde_json::from_str(&decoded_json).unwrap();
        assert!(dec.get("error").is_none(), "decode error: {}", decoded_json);
        assert_eq!(dec["decoded_hex"].as_str().unwrap(), PUBKEY_HEX_32);
    }

    #[test]
    fn roundtrip_prose_64_bytes() {
        let encoded_json = encode_raw_base_n(SIG_HEX_64, "latin", "default", "body", 42);
        let enc: serde_json::Value = serde_json::from_str(&encoded_json).unwrap();
        assert!(enc.get("error").is_none(), "encode error: {}", encoded_json);

        let prose_text = enc["encoded_text"].as_str().unwrap();
        // Prose mode: should have more words than payload alone
        let payload_count = enc["stats"]["payload_count"].as_u64().unwrap();
        let total_words = enc["stats"]["total_words"].as_u64().unwrap();
        assert!(total_words > payload_count, "prose should have cover words");

        let decoded_json = decode_raw_base_n(prose_text, "latin", "default", 64);
        let dec: serde_json::Value = serde_json::from_str(&decoded_json).unwrap();
        assert!(dec.get("error").is_none(), "decode error: {}", decoded_json);
        assert_eq!(dec["decoded_hex"].as_str().unwrap(), SIG_HEX_64);
    }

    #[test]
    fn roundtrip_various_pubkeys() {
        // Decode bech32 npub to hex (5-bit to 8-bit conversion)
        fn npub_to_hex(npub: &str) -> String {
            let charset = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
            let data_part = npub.rsplit('1').next().unwrap();
            let data5: Vec<u8> = data_part.chars()
                .map(|c| charset.find(c).unwrap() as u8)
                .collect();
            // Drop 6-char checksum, convert 5-bit to 8-bit
            let data5 = &data5[..data5.len() - 6];
            let mut acc: u32 = 0;
            let mut bits = 0;
            let mut result = Vec::new();
            for &v in data5 {
                acc = (acc << 5) | v as u32;
                bits += 5;
                while bits >= 8 {
                    bits -= 8;
                    result.push((acc >> bits) as u8 & 0xff);
                }
            }
            result.iter().map(|b| format!("{:02x}", b)).collect()
        }

        let npubs = [
            // Jack Dorsey
            "npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f237lmu63q0uf63m",
            // fiatjaf
            "npub180cvv07tjdrrgpa0j7j7tmnyl2yr6yr7l8j4s3evf6u64th6gkwsyjh6w6",
            // Vitor Pamplona
            "npub1gcxzte5zlkncx26j68ez60fzkvtkm9e0vrwdcvsjakxf9mu9qewqlfnj5z",
            // Random test key (all zeros would be edge case)
            "npub1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqsclp2ue",
        ];

        for npub in &npubs {
            let hex = npub_to_hex(npub);
            assert_eq!(hex.len(), 64, "npub {} decoded to {} hex chars", npub, hex.len());

            let encoded_json = encode_raw_base_n(&hex, "latin", "default", "", 0);
            let enc: serde_json::Value = serde_json::from_str(&encoded_json).unwrap();
            assert!(enc.get("error").is_none(), "encode error for {}: {}", npub, encoded_json);

            let text = enc["encoded_text"].as_str().unwrap();
            let words: Vec<&str> = text.split_whitespace().collect();
            eprintln!("{}: {} words -> {}", npub, words.len(), text);

            // Roundtrip
            let decoded_json = decode_raw_base_n(text, "latin", "default", 32);
            let dec: serde_json::Value = serde_json::from_str(&decoded_json).unwrap();
            assert!(dec.get("error").is_none(), "decode error for {}: {}", npub, decoded_json);
            assert_eq!(
                dec["decoded_hex"].as_str().unwrap(), hex,
                "roundtrip failed for {}", npub
            );
        }
    }

    #[test]
    fn roundtrip_prose_arbitrary_len_prefix_recoverable() {
        // The encryption demo tab frames its ciphertext as [len:u32][blob] and
        // decodes prose with expected_byte_count = 0 (unknown length), relying on
        // the decoded hex PREFIX matching the encoded bytes (trailing bit-pack
        // padding is dropped via the length prefix). Verify that assumption for
        // several odd payload lengths and both prose-wrapped and bare output.
        for len in [29usize, 41, 60, 73, 128] {
            let bytes: Vec<u8> = (0..len).map(|i| (i as u32 * 37 + 11) as u8).collect();
            let hex_in = codec::hex_encode(&bytes);

            for dialect in ["", "body"] {
                let enc_json = encode_raw_base_n(&hex_in, "english", "bip39", dialect, 42);
                let enc: serde_json::Value = serde_json::from_str(&enc_json).unwrap();
                assert!(enc.get("error").is_none(), "encode error (len {}, dialect {:?}): {}", len, dialect, enc_json);
                let text = enc["encoded_text"].as_str().unwrap();

                let dec_json = decode_raw_base_n(text, "english", "bip39", 0);
                let dec: serde_json::Value = serde_json::from_str(&dec_json).unwrap();
                assert!(dec.get("error").is_none(), "decode error (len {}, dialect {:?}): {}", len, dialect, dec_json);
                let hex_out = dec["decoded_hex"].as_str().unwrap();

                assert!(
                    hex_out.starts_with(&hex_in),
                    "len {} dialect {:?}: decoded hex prefix mismatch\n  in : {}\n  out: {}",
                    len, dialect, hex_in, hex_out
                );
            }
        }
    }

    #[test]
    fn empty_dialect_has_no_cover_words() {
        let encoded_json = encode_raw_base_n(SIG_HEX_64, "latin", "default", "", 42);
        let enc: serde_json::Value = serde_json::from_str(&encoded_json).unwrap();
        assert_eq!(enc["stats"]["cover_count"].as_u64().unwrap(), 0);
        assert_eq!(
            enc["stats"]["payload_count"].as_u64().unwrap(),
            enc["stats"]["total_words"].as_u64().unwrap()
        );
    }

}
