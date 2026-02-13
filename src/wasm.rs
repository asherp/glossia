use wasm_bindgen::prelude::*;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::HashSet;

use crate::generator::data::{
    get_available_languages, get_available_wordlists,
    load_payload_words_for_wordlist, build_pos_mapping_for_wordlist,
    load_cover_words_by_pos_for_wordlist,
};
use crate::generator::types::{PayloadTok, Lexicon, GenerationMode, SentenceLengthMode};
use crate::generator::core::generate_text;
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
    // 1. Load payload wordlist and build WordlistTree
    let payload_words = load_payload_words_for_wordlist(language, wordlist)?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 2. Encode input string to payload words via codec
    let encoded_words = codec::encode(input.as_bytes(), &payload_tree)
        .map_err(|e| format!("Encoding error: {}", e))?;

    // 3. Build POS mapping for payload words
    let pos_mapping = build_pos_mapping_for_wordlist(language, wordlist)?;

    // 4. Build PayloadTok vec with POS tags
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

    // 5. Build Lexicon from cover words
    let wordlist_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();
    let (cover_by_pos, refined_cover) =
        load_cover_words_by_pos_for_wordlist(&wordlist_set, language, wordlist);

    let mut lex = Lexicon::new(wordlist_set.clone(), wordlist_set);
    for (pos, words) in cover_by_pos {
        lex = lex.with_words(pos, &words.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
    lex = lex.with_refined_cover(refined_cover);

    // 6. Load grammar
    let _grammar = crate::grammar::Grammar::from_language_dialect(language, grammar_dialect)
        .map_err(|e| format!("Grammar error: {}", e))?;

    // 7. Generate text with seeded RNG
    let mut rng = StdRng::seed_from_u64(seed);
    let mode = match grammar_dialect {
        "subject" => GenerationMode::Subject,
        _ => GenerationMode::Body,
    };
    let (text, used_payload) = generate_text(
        &mut rng,
        &lex,
        &payload_toks,
        false, // verbose
        mode,
        language,
        5,  // k_min
        12, // k_max
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
        // Character-by-character extraction for CS-style grammars
        let payload_set: HashSet<String> = payload_words.iter()
            .map(|w| w.to_lowercase())
            .collect();
        text.split_whitespace()
            .flat_map(|token| {
                let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
                // Try individual characters first (for concatenated payload blocks)
                let chars: Vec<String> = trimmed.chars()
                    .map(|c| c.to_string())
                    .filter(|c| payload_set.contains(&c.to_lowercase()))
                    .collect();
                if chars.is_empty() {
                    // Fall back to whole-token match
                    let lower = trimmed.to_lowercase();
                    if payload_set.contains(&lower) {
                        vec![lower]
                    } else {
                        vec![]
                    }
                } else {
                    chars.iter().map(|c| c.to_lowercase()).collect()
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

    // 3. Decode payload words back to bytes
    let decoded_bytes = codec::decode(
        &extracted,
        &payload_tree,
    )
    .map_err(|e| format!("Decoding error: {}", e))?;

    let decoded_text = String::from_utf8(decoded_bytes)
        .map_err(|_| "Decoded bytes are not valid UTF-8".to_string())?;

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
