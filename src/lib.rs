pub mod types;
pub mod semantic_types;
pub mod lambda_terms;
pub mod lambda_parser;
pub mod type_driven_grammar;
pub mod grammar;
pub mod generator;
pub mod merkle;
pub mod codec;
pub mod canonical;
pub mod pipeline;
pub mod scale;
pub mod image_codec;

#[cfg(feature = "wasm")]
pub mod wasm;

// Re-export key types
/// The RNG that realizes cover words and sentence shape.
///
/// Pinned to ChaCha12 explicitly rather than inherited from `rand::rngs::StdRng`.
/// `StdRng` is documented as not reproducible across `rand` major versions -- its
/// algorithm may change -- which makes it unsafe for anything whose *output* is
/// part of a format. Verification by re-render (#76) is exactly that: a dependency
/// bump could otherwise change every rendering while the version field still
/// claimed compatibility.
///
/// `StdRng` in rand 0.8 *is* ChaCha12, and `seed_from_u64` is a `SeedableRng`
/// provided method, so this pin is byte-identical to the previous behaviour --
/// verified over raw stream, `gen::<f64>()` and `choose()` draws. ChaCha20 is a
/// different stream and would invalidate every existing encoding; do not
/// "upgrade" to it. The RNG never selects payload words, only cover, so its
/// strength is not a security property.
pub type CoverRng = rand_chacha::ChaCha12Rng;

pub use types::Pos;
pub use grammar::{Grammar, SequenceWithProbability, DialectConfig};

// Re-export generator types and functions
pub use generator::{
    GenerationMode, SentenceLengthMode, PayloadTok, Lexicon,
    generate_text, generate_text_with_original_payload, generate_text_best_of,
    max_subsequence_embedding, plan_sentence, fill_slots,
    SequenceCache, print_sentence_kinds_once,
    normalize_token_for_bip39, wrap_payload_with_bars, wrap_payload_with_color,
    word_wrap, capitalize, payload_fits, get_grammar, get_dialect_config, mode_to_dialect,
    starts_with_vowel_sound,
    load_payload_words, load_payload_words_for_wordlist,
    load_cover_words_by_pos, load_cover_words_by_pos_for_wordlist,
    build_pos_mapping, build_pos_mapping_for_wordlist,
    tag_word, select_random_words, wordlist_filenames, default_wordlist,
    load_payload_tree, load_cover_tree, load_cover_word_pos_tags,
    get_embedded_yaml, has_embedded_files, get_available_languages,
    get_available_wordlists,
    detect_dialect, detect_dialect_filtered, detect_dialect_with, detect_dialect_best,
    DialectMatch, DialectFilter,
    inject_scale_payload,
};
#[cfg(not(target_arch = "wasm32"))]
pub use generator::get_wordlist_path;

// Re-export merkle functions
pub use merkle::{WordlistTree, merkleize, parse_merkleized, verify_merkleized};

// Re-export codec functions and types
pub use codec::{
    encode_base_n, decode_base_n, encode_str_base_n,
    detect_mode, DataMode, DecodeError,
    hex_encode, hex_decode, base64_encode, base64_decode,
};

// Re-export pipeline types and functions
pub use pipeline::{Pipeline, Endpoint, Verb, PipelineError, encode_into_language, decode_from_language, cached_payload_tree};

pub use canonical::{
    canonical_decode, canonical_decode_raw, canonical_encode, canonical_encode_at,
    canonical_encode_traced, rules_for,
    CanonicalDecoded, CanonicalError, VersionRules, CANONICAL_VERSION,
};

#[cfg(feature = "native")]
use nlprule::{Tokenizer, Rules};
#[cfg(feature = "native")]
use anyhow::{Result, Context};

/// Helper enum to represent supported languages
#[cfg(feature = "native")]
#[derive(Debug, Clone, Copy)]
pub enum Language {
    English,
}

#[cfg(feature = "native")]
impl Language {
    /// Get the language code (ISO 639-1)
    fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
        }
    }

    /// Get the tokenizer filename for this language
    fn tokenizer_filename(&self) -> String {
        format!("{}_tokenizer.bin", self.code())
    }

    /// Get the rules filename for this language
    fn rules_filename(&self) -> String {
        format!("{}_rules.bin", self.code())
    }
}

/// Grammar checker that wraps nlprule functionality
#[cfg(feature = "native")]
pub struct GrammarChecker {
    tokenizer: Tokenizer,
    rules: Rules,
}

#[cfg(feature = "native")]
impl GrammarChecker {
    /// Create a new GrammarChecker from language, loading tokenizer and rules from paths
    pub fn from_paths(tokenizer_path: &str, rules_path: &str) -> Result<Self> {
        let tokenizer = Tokenizer::new(tokenizer_path)
            .with_context(|| format!("Failed to load tokenizer from {}", tokenizer_path))?;
        let rules = Rules::new(rules_path)
            .with_context(|| format!("Failed to load rules from {}", rules_path))?;

        Ok(Self { tokenizer, rules })
    }

    /// Create a new GrammarChecker from language, using default paths
    /// Checks multiple locations: current directory, data/, and /app/data (for Docker)
    pub fn from_language(language: Language) -> Result<Self> {
        let tokenizer_filename = language.tokenizer_filename();
        let rules_filename = language.rules_filename();

        // Try multiple locations (check Docker location first to avoid corrupted local files)
        let locations = vec![
            "/opt/nlprule-data/", // Docker persistent location (check first)
            "/app/data/",         // Docker location
            "data/",              // data subdirectory
            "",                   // current directory
        ];

        for location in &locations {
            let tokenizer_path = format!("{}{}", location, tokenizer_filename);
            let rules_path = format!("{}{}", location, rules_filename);

            if std::path::Path::new(&tokenizer_path).exists() &&
               std::path::Path::new(&rules_path).exists() {
                return Self::from_paths(&tokenizer_path, &rules_path);
            }
        }

        // If none found, try the default (current directory) and let it error with a helpful message
        Self::from_paths(&tokenizer_filename, &rules_filename)
            .with_context(|| format!(
                "Could not find {} and {} in any of: current directory, data/, /app/data/, or /opt/nlprule-data/",
                tokenizer_filename, rules_filename
            ))
    }

    /// Check grammar of a sentence and return suggestions
    pub fn check(&self, text: &str) -> Vec<nlprule::types::Suggestion> {
        self.rules.suggest(text, &self.tokenizer)
    }

    /// Correct a sentence based on grammar rules
    pub fn correct(&self, text: &str) -> String {
        self.rules.correct(text, &self.tokenizer)
    }

    /// Tokenize text and return sentences
    pub fn tokenize<'a>(&'a self, text: &'a str) -> impl Iterator<Item = nlprule::types::Sentence<'a>> {
        self.tokenizer.pipe(text)
    }

    /// Check if a sentence is grammatically correct
    pub fn is_correct(&self, text: &str) -> bool {
        let suggestions = self.check(text);
        suggestions.is_empty()
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;

    #[test]
    fn test_grammar_checking() -> Result<()> {
        // This test requires the binary files to be present
        let grammar_checker = match GrammarChecker::from_language(Language::English) {
            Ok(checker) => checker,
            Err(_) => {
                // Skip test if files are not available
                return Ok(());
            }
        };

        let sentence = "The quick brown fox jumps over the lazy dog.";
        let suggestions = grammar_checker.check(sentence);

        // This is a complete, grammatically correct sentence
        // It should have few or no suggestions
        println!("Suggestions for test sentence: {:?}", suggestions);

        Ok(())
    }
}
