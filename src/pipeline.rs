//! Pipeline: Meta-language-driven translation between Glossia languages.
//!
//! Every Glossia operation — encode, decode, transcode — factors through
//! binary as the universal intermediate representation:
//!
//! ```text
//! Source → [decode to bytes] → bytes → [encode from bytes] → Target
//! ```
//!
//! The meta language's prepositions (`from`, `into`) determine direction:
//!
//! ```text
//! "translate from english into latin"
//!            ^^^^          ^^^^
//!            source        target
//! ```
//!
//! Existing CLI flags are sugar for constructing pipelines:
//! - `glossia -l english --from-ascii "hello"` → `encode into english`
//! - `glossia --decode -l english`              → `decode from english`
//! - `glossia --meta "translate from english into latin"` → transcode

use std::collections::HashSet;
use std::fmt;

use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::codec::{self, DataMode};
use crate::generator::data::{
    load_payload_words_for_wordlist, build_pos_mapping_for_wordlist,
    load_cover_words_by_pos_for_wordlist, detect_dialect,
    default_wordlist,
};
use crate::generator::types::{PayloadTok, Lexicon, GenerationMode, SentenceLengthMode};
use crate::generator::core::generate_text_with_original_payload;
use crate::grammar::Grammar;
use crate::merkle::WordlistTree;

// ═══════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════

/// A dialect endpoint — either a Glossia language or a raw data format.
#[derive(Clone, Debug, PartialEq)]
pub enum Endpoint {
    /// A Glossia language: prose that embeds payload words.
    Language {
        language: String,
        wordlist: String,
        dialect: String,
    },
    /// A raw data format (the codec layer).
    Format(DataMode),
    /// Auto-detect from input content.
    Auto,
}

impl Endpoint {
    /// Shorthand for a Language endpoint with default wordlist and body dialect.
    pub fn language(name: &str) -> Self {
        Endpoint::Language {
            language: name.to_string(),
            wordlist: "default".to_string(),
            dialect: "body".to_string(),
        }
    }

    /// Shorthand for a Language endpoint with specific dialect.
    pub fn language_with_dialect(name: &str, dialect: &str) -> Self {
        Endpoint::Language {
            language: name.to_string(),
            wordlist: "default".to_string(),
            dialect: dialect.to_string(),
        }
    }

    /// Shorthand for a Language endpoint with specific wordlist and dialect.
    pub fn language_full(name: &str, wordlist: &str, dialect: &str) -> Self {
        Endpoint::Language {
            language: name.to_string(),
            wordlist: wordlist.to_string(),
            dialect: dialect.to_string(),
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Endpoint::Language { language, wordlist, dialect } => {
                write!(f, "{}/{}/{}", language, wordlist, dialect)
            }
            Endpoint::Format(mode) => write!(f, "{}", mode),
            Endpoint::Auto => write!(f, "auto"),
        }
    }
}

/// Errors from pipeline operations.
#[derive(Debug, Clone)]
pub enum PipelineError {
    /// Could not parse a meta sentence into a pipeline.
    ParseError(String),
    /// The source/target combination is invalid.
    InvalidPipeline(String),
    /// Encoding failed.
    EncodeError(String),
    /// Decoding failed.
    DecodeError(String),
    /// No dialect could be auto-detected from the input.
    DetectionFailed(String),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineError::ParseError(msg) => write!(f, "parse error: {}", msg),
            PipelineError::InvalidPipeline(msg) => write!(f, "invalid pipeline: {}", msg),
            PipelineError::EncodeError(msg) => write!(f, "encode error: {}", msg),
            PipelineError::DecodeError(msg) => write!(f, "decode error: {}", msg),
            PipelineError::DetectionFailed(msg) => write!(f, "detection failed: {}", msg),
        }
    }
}

impl std::error::Error for PipelineError {}

/// Rich result from pipeline execution, including stats for UI display.
pub struct PipelineResult {
    pub output: String,
    pub payload_words: Vec<String>,
    pub data_mode: Option<DataMode>,
    pub stats: Option<PipelineStats>,
}

/// Statistics about an encode/transcode operation.
pub struct PipelineStats {
    pub payload_count: usize,
    pub cover_count: usize,
    pub total_words: usize,
    pub ratio: f64,
}

/// A pipeline specification parsed from meta-language words or constructed
/// from explicit parameters.
#[derive(Clone, Debug)]
pub struct Pipeline {
    pub source: Endpoint,
    pub target: Endpoint,
    pub seed: u64,
    pub verbose: bool,
    /// When true, replace `\n` with `<br>` in output for HTML rendering.
    pub html: bool,
}

// ═══════════════════════════════════════════════════════════════════════
// Meta Sentence Parsing
// ═══════════════════════════════════════════════════════════════════════

/// Meta payload words that map to raw data formats.
fn meta_word_to_format(word: &str) -> Option<DataMode> {
    match word {
        "hex" => Some(DataMode::Hex),
        "base64" => Some(DataMode::Base64),
        "ascii7" => Some(DataMode::Ascii7),
        "bytes" => Some(DataMode::Bytes8),
        "bits" => Some(DataMode::Bytes8), // bits = raw bytes for now
        _ => None,
    }
}

/// Meta payload words that map to Glossia languages.
/// Returns `(language, default_dialect)` — the dialect is used when no
/// explicit dialect modifier precedes the keyword.  This ensures that
/// shorthand like `"encode into pgp"` selects the PGP-specific CS dialect
/// rather than falling back to the generic "body" rules.
fn meta_word_to_language(word: &str) -> Option<(&str, &str)> {
    match word {
        "english" => Some(("english", "body")),
        "latin" => Some(("latin", "body")),
        "nostr" => Some(("cs", "nip04")),
        "pgp" => Some(("cs", "pgp")),
        "primes" => Some(("math", "body")),
        _ => None,
    }
}

/// Meta payload words that are dialect modifiers (applied to the nearest Language endpoint).
fn meta_word_to_dialect(word: &str) -> Option<&str> {
    match word {
        "body" => Some("body"),
        "subject" => Some("subject"),
        "prose" => Some("prose"),
        "spells" => Some("spells"),
        _ => None,
    }
}

/// Direction role assigned by prepositions.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Source,
    Target,
}

/// Classify a meta-language preposition.
fn classify_preposition(word: &str) -> Option<Role> {
    match word {
        "from" => Some(Role::Source),
        "into" | "as" | "to" => Some(Role::Target),
        _ => None,
    }
}

impl Pipeline {
    // ─── Constructors ────────────────────────────────────────────────

    /// Parse a meta-language sentence (or fragment) into a Pipeline.
    ///
    /// Prepositions determine direction:
    /// - `from X` → X is source
    /// - `into Y` / `as Y` / `to Y` → Y is target
    /// - No preposition → first payload word = source, second = target
    /// - Single endpoint with no preposition → target (source = Auto)
    ///
    /// Dialect modifiers (`body`, `subject`, `prose`, `spells`) attach to the
    /// most recently assigned endpoint.
    pub fn from_meta(meta_sentence: &str) -> Result<Self, PipelineError> {
        let tokens: Vec<&str> = meta_sentence
            .split_whitespace()
            .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|t| !t.is_empty())
            .collect();

        if tokens.is_empty() {
            return Err(PipelineError::ParseError(
                "empty meta instruction".to_string(),
            ));
        }

        // Meta payload words for classification.
        let meta_payload: HashSet<&str> = [
            "latin", "english", "hex", "base64", "base58", "ascii7", "bits",
            "bytes", "nostr", "pgp", "prose", "body", "subject", "spells",
            "primes", "merkle",
        ]
        .iter()
        .copied()
        .collect();

        let mut current_role: Option<Role> = None;
        let mut source: Option<Endpoint> = None;
        let mut target: Option<Endpoint> = None;
        let mut pending_dialect: Option<String> = None;
        let mut unscoped_endpoints: Vec<Endpoint> = Vec::new();
        // Track whether source/target were set via explicit prepositions.
        let mut source_explicit = false;
        let mut target_explicit = false;
        let mut html = false;

        for &token in &tokens {
            let lower = token.to_lowercase();
            let lower = lower.as_str();

            // Preposition sets role context for the NEXT payload word.
            if let Some(role) = classify_preposition(lower) {
                current_role = Some(role);
                continue;
            }

            // Output rendering modifier: "html" → replace \n with <br> in output.
            if lower == "html" {
                html = true;
                continue;
            }

            // Skip non-meta-payload words (cover verbs: "translate", "encode", etc.)
            if !meta_payload.contains(lower) {
                continue;
            }

            // Dialect modifier — save it to apply to the next endpoint.
            if let Some(dialect) = meta_word_to_dialect(lower) {
                pending_dialect = Some(dialect.to_string());
                continue;
            }

            // Build an Endpoint from this payload word.
            let endpoint = if let Some(mode) = meta_word_to_format(lower) {
                Endpoint::Format(mode)
            } else if let Some((lang, default_dialect)) = meta_word_to_language(lower) {
                let dialect = pending_dialect.take()
                    .unwrap_or_else(|| default_dialect.to_string());
                Endpoint::language_with_dialect(lang, &dialect)
            } else {
                // Unrecognized meta payload word — skip.
                continue;
            };

            // Assign based on preposition context.
            match current_role {
                Some(Role::Source) => {
                    source = Some(endpoint);
                    source_explicit = true;
                    current_role = None;
                }
                Some(Role::Target) => {
                    target = Some(endpoint);
                    target_explicit = true;
                    current_role = None;
                }
                None => {
                    unscoped_endpoints.push(endpoint);
                }
            }

            // Apply trailing dialect modifier to the most recently set endpoint.
            if let Some(dialect) = pending_dialect.take() {
                apply_dialect_modifier(&mut source, &mut target, &mut unscoped_endpoints, &dialect);
            }
        }

        // Apply any remaining pending dialect.
        if let Some(dialect) = pending_dialect {
            apply_dialect_modifier(&mut source, &mut target, &mut unscoped_endpoints, &dialect);
        }

        // Fill in source/target gaps from unscoped endpoints.
        // If source was explicitly set (via `from`), unscoped words fill target.
        // If target was explicitly set (via `into`), unscoped words fill source.
        if source_explicit && !target_explicit && target.is_none() && !unscoped_endpoints.is_empty() {
            target = Some(unscoped_endpoints.remove(0));
        } else if target_explicit && !source_explicit && source.is_none() && !unscoped_endpoints.is_empty() {
            source = Some(unscoped_endpoints.remove(0));
        } else {
            // No explicit prepositions — assign by order: first=source, second=target.
            if source.is_none() && !unscoped_endpoints.is_empty() {
                source = Some(unscoped_endpoints.remove(0));
            }
            if target.is_none() && !unscoped_endpoints.is_empty() {
                target = Some(unscoped_endpoints.remove(0));
            }
        }

        // Resolve final (source, target) pair.
        let (source, target) = match (source, target) {
            (Some(s), Some(t)) => (s, t),
            // Only source set (e.g., "decode from english") → target = Auto
            (Some(s), None) if source_explicit => (s, Endpoint::Auto),
            // Only one endpoint, no explicit preposition → treat as target (Auto source)
            (Some(s), None) => (Endpoint::Auto, s),
            (None, Some(t)) => (Endpoint::Auto, t),
            (None, None) => {
                return Err(PipelineError::ParseError(
                    "no dialect identifiers found in meta instruction".to_string(),
                ));
            }
        };

        Ok(Pipeline {
            source,
            target,
            seed: 0,
            verbose: false,
            html,
        })
    }

    /// Construct from explicit parameters (backward compat with CLI flags).
    pub fn from_params(source: Endpoint, target: Endpoint) -> Self {
        Pipeline {
            source,
            target,
            seed: 0,
            verbose: false,
            html: false,
        }
    }

    /// Set the RNG seed.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Enable verbose output.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Enable HTML output rendering (cover wordlist override to "html").
    pub fn with_html(mut self, html: bool) -> Self {
        self.html = html;
        self
    }

    /// Cover wordlist override derived from pipeline flags.
    /// Returns `Some("html")` when the `html` flag is set.
    fn cover_override(&self) -> Option<&str> {
        if self.html { Some("html") } else { None }
    }

    // ─── Execution ───────────────────────────────────────────────────

    /// Execute the pipeline on the given input.
    ///
    /// Every pipeline factors through binary:
    ///   Source → decode to bytes → encode from bytes → Target
    pub fn execute(&self, input: &str) -> Result<String, PipelineError> {
        let source = self.resolve_source(input)?;
        let target = &self.target;

        match (&source, target) {
            // Encode: raw data string → binary → language prose
            (Endpoint::Format(_) | Endpoint::Auto, Endpoint::Language { .. }) => {
                self.do_encode(input, target)
            }
            // Decode: language prose → binary → raw data string
            (Endpoint::Language { .. }, Endpoint::Format(_) | Endpoint::Auto) => {
                self.do_decode(input, &source)
            }
            // Transcode: lang A prose → binary → lang B prose
            (Endpoint::Language { .. }, Endpoint::Language { .. }) => {
                self.do_transcode(input, &source, target)
            }
            (Endpoint::Format(_), Endpoint::Format(_)) => {
                Err(PipelineError::InvalidPipeline(
                    "format-to-format: use a language as intermediary".to_string(),
                ))
            }
            _ => Err(PipelineError::InvalidPipeline(format!(
                "unsupported pipeline: {} -> {}",
                source, target
            ))),
        }
    }

    /// Resolve an Auto source by detecting the dialect from input text.
    ///
    /// When the target is a Language (encode direction), we skip dialect
    /// detection and always treat the input as raw data. Dialect detection
    /// only runs when the target is Auto or Format (decode/transcode direction),
    /// meaning we need to figure out which Glossia language the input is in.
    fn resolve_source(&self, input: &str) -> Result<Endpoint, PipelineError> {
        match &self.source {
            Endpoint::Auto => {
                // If target is a Language, the user wants to encode raw data
                // into prose. Don't mistake payload words in the input for
                // Glossia prose — go straight to format detection.
                if matches!(&self.target, Endpoint::Language { .. }) {
                    let (mode, _) = codec::detect_mode(input);
                    if self.verbose {
                        eprintln!("Source auto-detected as format: {} (target is Language)", mode);
                    }
                    return Ok(Endpoint::Format(mode));
                }

                let words: Vec<String> = input
                    .split_whitespace()
                    .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                    .filter(|w| !w.is_empty())
                    .collect();

                if words.is_empty() {
                    let (mode, _) = codec::detect_mode(input);
                    return Ok(Endpoint::Format(mode));
                }

                // Try dialect detection — if enough words match a payload wordlist,
                // the input is Glossia prose.
                let matches = detect_dialect(&words);
                if let Some(best) = matches.first() {
                    if best.hit_rate > 0.3 {
                        if self.verbose {
                            eprintln!(
                                "Auto-detected source: {}/{} ({:.0}% hit rate)",
                                best.language, best.wordlist, best.hit_rate * 100.0
                            );
                        }
                        let dialect = best.dialects.first()
                            .cloned()
                            .unwrap_or_else(|| "body".to_string());
                        return Ok(Endpoint::Language {
                            language: best.language.clone(),
                            wordlist: best.wordlist.clone(),
                            dialect,
                        });
                    }
                }

                // Not Glossia prose — treat as raw data.
                let (mode, _) = codec::detect_mode(input);
                Ok(Endpoint::Format(mode))
            }
            other => Ok(other.clone()),
        }
    }

    // ─── Internal: everything goes through binary ────────────────────

    /// Encode: raw data string → binary → language prose.
    fn do_encode(&self, input: &str, target: &Endpoint) -> Result<String, PipelineError> {
        let (language, wordlist, dialect) = match target {
            Endpoint::Language { language, wordlist, dialect } => {
                (language.as_str(), wordlist.as_str(), dialect.as_str())
            }
            _ => return Err(PipelineError::InvalidPipeline(
                "encode target must be a Language".to_string(),
            )),
        };

        let (text, _, _, _) = encode_into_language(
            input, language, wordlist, dialect, None, self.seed, self.verbose,
            self.cover_override(),
        )?;
        Ok(text)
    }

    /// Encode with rich results: returns text, payload words, data_mode, and stats.
    fn do_encode_rich(&self, input: &str, target: &Endpoint) -> Result<PipelineResult, PipelineError> {
        let (language, wordlist, dialect) = match target {
            Endpoint::Language { language, wordlist, dialect } => {
                (language.as_str(), wordlist.as_str(), dialect.as_str())
            }
            _ => return Err(PipelineError::InvalidPipeline(
                "encode target must be a Language".to_string(),
            )),
        };

        let (text, _payload_set, encoded_words, data_mode) = encode_into_language(
            input, language, wordlist, dialect, None, self.seed, self.verbose,
            self.cover_override(),
        )?;

        let payload_count = encoded_words.len();
        let total_words = text.split_whitespace().count();
        let cover_count = total_words.saturating_sub(payload_count);
        let ratio = if total_words > 0 {
            payload_count as f64 / total_words as f64
        } else {
            0.0
        };

        Ok(PipelineResult {
            output: text,
            payload_words: encoded_words,
            data_mode: Some(data_mode),
            stats: Some(PipelineStats {
                payload_count,
                cover_count,
                total_words,
                ratio,
            }),
        })
    }

    /// Decode: language prose → extract payload words → binary → data string.
    fn do_decode(&self, input: &str, source: &Endpoint) -> Result<String, PipelineError> {
        let (language, wordlist) = match source {
            Endpoint::Language { language, wordlist, .. } => {
                (language.as_str(), wordlist.as_str())
            }
            _ => return Err(PipelineError::InvalidPipeline(
                "decode source must be a Language".to_string(),
            )),
        };

        decode_from_language(input, language, wordlist, self.verbose)
    }

    /// Transcode: source prose → binary → target prose.
    ///
    /// This is the universal translation path. Every dialect factors
    /// through binary (the zero object in the dialect calculus).
    fn do_transcode(
        &self,
        input: &str,
        source: &Endpoint,
        target: &Endpoint,
    ) -> Result<String, PipelineError> {
        // Step 1: Decode source prose to binary (data string).
        let raw = self.do_decode(input, source)?;

        if self.verbose {
            eprintln!("Transcode intermediate: {} bytes", raw.len());
        }

        // Step 2: Encode binary into target prose.
        self.do_encode(&raw, target)
    }

    // ─── Rich execution (with payload_words, stats, data_mode) ──────

    /// Execute the pipeline, returning rich results for UI display.
    pub fn execute_rich(&self, input: &str) -> Result<PipelineResult, PipelineError> {
        let source = self.resolve_source(input)?;
        let target = &self.target;

        match (&source, target) {
            // Encode: raw data → language prose
            (Endpoint::Format(_) | Endpoint::Auto, Endpoint::Language { .. }) => {
                self.do_encode_rich(input, target)
            }
            // Decode: language prose → raw data
            (Endpoint::Language { .. }, Endpoint::Format(_) | Endpoint::Auto) => {
                self.do_decode_rich(input, &source)
            }
            // Transcode: lang A → binary → lang B
            (Endpoint::Language { .. }, Endpoint::Language { .. }) => {
                self.do_transcode_rich(input, &source, target)
            }
            (Endpoint::Format(_), Endpoint::Format(_)) => {
                Err(PipelineError::InvalidPipeline(
                    "format-to-format: use a language as intermediary".to_string(),
                ))
            }
            _ => Err(PipelineError::InvalidPipeline(format!(
                "unsupported pipeline: {} -> {}",
                source, target
            ))),
        }
    }

    /// Decode with rich results: returns decoded text and extracted payload words.
    fn do_decode_rich(&self, input: &str, source: &Endpoint) -> Result<PipelineResult, PipelineError> {
        let (language, wordlist) = match source {
            Endpoint::Language { language, wordlist, .. } => {
                (language.as_str(), wordlist.as_str())
            }
            _ => return Err(PipelineError::InvalidPipeline(
                "decode source must be a Language".to_string(),
            )),
        };

        let (decoded, extracted) = decode_from_language_rich(input, language, wordlist, self.verbose)?;

        Ok(PipelineResult {
            output: decoded,
            payload_words: extracted,
            data_mode: None,
            stats: None,
        })
    }

    /// Transcode with rich results: decode source, then encode into target with stats.
    fn do_transcode_rich(
        &self,
        input: &str,
        source: &Endpoint,
        target: &Endpoint,
    ) -> Result<PipelineResult, PipelineError> {
        let raw = self.do_decode(input, source)?;

        if self.verbose {
            eprintln!("Transcode intermediate: {} bytes", raw.len());
        }

        self.do_encode_rich(&raw, target)
    }
}

/// Apply a dialect modifier to the most recently assigned endpoint.
fn apply_dialect_modifier(
    source: &mut Option<Endpoint>,
    target: &mut Option<Endpoint>,
    unscoped: &mut Vec<Endpoint>,
    dialect: &str,
) {
    // Priority: target (most recently set) > source > last unscoped.
    let ep = if target.is_some() {
        target.as_mut()
    } else if source.is_some() {
        source.as_mut()
    } else {
        unscoped.last_mut()
    };
    if let Some(Endpoint::Language { dialect: ref mut d, .. }) = ep {
        *d = dialect.to_string();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Reusable encode/decode helpers
// ═══════════════════════════════════════════════════════════════════════

/// Encode raw data into language prose.
///
/// The data string is first converted to binary (via the codec layer,
/// which auto-detects hex/base64/ascii/bytes), then the binary is
/// encoded into payload words, which are embedded into grammatical prose.
///
/// `cover_override` optionally selects an alternate cover wordlist (e.g.,
/// `Some("html")` loads `cover_html.yaml` instead of the default cover).
///
/// Returns `(generated_text, payload_word_set, encoded_word_list, data_mode)`.
pub fn encode_into_language(
    input: &str,
    language: &str,
    wordlist: &str,
    dialect: &str,
    forced_data_mode: Option<DataMode>,
    seed: u64,
    verbose: bool,
    cover_override: Option<&str>,
) -> Result<(String, HashSet<String>, Vec<String>, DataMode), PipelineError> {
    // Resolve "default" wordlist to actual name (e.g., "bip39" for English).
    let wordlist = if wordlist == "default" {
        let dw = default_wordlist(language);
        if dw == "default" { wordlist } else { dw }
    } else {
        wordlist
    };

    // 1. Load payload wordlist.
    let payload_words = load_payload_words_for_wordlist(language, wordlist)
        .map_err(|e| PipelineError::EncodeError(e))?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 2. Input string → binary → payload words.
    let (encoded_words, data_mode) = if let Some(mode) = forced_data_mode {
        // Explicit mode: pre-decode the input string to raw bytes.
        let data = match mode {
            DataMode::Hex => codec::hex_decode(input)
                .ok_or_else(|| PipelineError::EncodeError("invalid hex input".into()))?,
            DataMode::Base64 => codec::base64_decode(input)
                .ok_or_else(|| PipelineError::EncodeError("invalid base64 input".into()))?,
            DataMode::Ascii7 | DataMode::Bytes8 => input.as_bytes().to_vec(),
        };
        codec::encode_with_mode(&data, &payload_tree, mode)
            .map(|words| (words, mode))
            .map_err(|e| PipelineError::EncodeError(format!("{}", e)))?
    } else {
        // Auto-detect: codec::encode_str_with_mode handles hex/base64/ascii/bytes.
        codec::encode_str_with_mode(input, &payload_tree)
            .map_err(|e| PipelineError::EncodeError(format!("{}", e)))?
    };

    // 3. POS-tag each payload word.
    let pos_mapping = build_pos_mapping_for_wordlist(language, wordlist)
        .map_err(|e| PipelineError::EncodeError(e))?;

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

    // 4. Build Lexicon (cover words).
    //    cover_override selects an alternate cover file (e.g., "html" → cover_html.yaml).
    let wordlist_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();
    let cover_wl = cover_override.unwrap_or(wordlist);
    let (cover_by_pos, refined_cover) =
        load_cover_words_by_pos_for_wordlist(&wordlist_set, language, cover_wl);

    let mut lex = Lexicon::new(wordlist_set.clone(), wordlist_set);
    for (pos, words) in cover_by_pos {
        lex = lex.with_words(pos, &words.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
    lex = lex.with_refined_cover(refined_cover);

    // 5. Grammar-derived sentence parameters.
    let grammar = Grammar::from_language_dialect(language, dialect)
        .map_err(|e| PipelineError::EncodeError(format!("Grammar error: {}", e)))?;

    let min_k = grammar.min_sentence_length().unwrap_or(5);
    let concat_payload = grammar.payload_separator().is_empty();
    let (k_min, k_max) = if concat_payload {
        let k = min_k + payload_toks.len().saturating_sub(1);
        (k, k)
    } else {
        (5, 12)
    };

    // 6. Generate text.
    let mut rng = StdRng::seed_from_u64(seed);
    let mode = match dialect {
        "subject" => GenerationMode::Subject,
        _ => GenerationMode::Body,
    };
    let (text, payload_set) = generate_text_with_original_payload(
        &mut rng,
        &lex,
        &payload_toks,
        None,
        verbose,
        mode,
        language,
        Some(dialect),
        k_min,
        k_max,
        SentenceLengthMode::Natural,
        " ",
    );

    Ok((text, payload_set, encoded_words, data_mode))
}

/// Decode language prose back to the original data string.
///
/// Prose → extract payload words (filter out cover words) → binary → string.
/// The codec layer handles mode detection (hex/base64/ascii/bytes) from the
/// header word.
pub fn decode_from_language(
    text: &str,
    language: &str,
    wordlist: &str,
    verbose: bool,
) -> Result<String, PipelineError> {
    // Resolve "default" wordlist to actual name (e.g., "bip39" for English).
    let wordlist = if wordlist == "default" {
        let dw = default_wordlist(language);
        if dw == "default" { wordlist } else { dw }
    } else {
        wordlist
    };

    // 1. Load payload wordlist.
    let payload_words = load_payload_words_for_wordlist(language, wordlist)
        .map_err(|e| PipelineError::DecodeError(e))?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 2. Grammar tells us how payload words are separated.
    let grammar = Grammar::from_language_dialect(language, "body")
        .map_err(|e| PipelineError::DecodeError(format!("Grammar error: {}", e)))?;
    let payload_separator = grammar.payload_separator();

    // 3. Extract payload words from prose.
    let extracted: Vec<String> = if payload_separator.is_empty() {
        // Concatenated payload (CS grammar): chars from pure-payload blocks.
        let payload_set: HashSet<String> = payload_words.iter()
            .map(|w| w.to_lowercase())
            .collect();
        text.split_whitespace()
            .flat_map(|token| {
                let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
                let all_in_payload = !trimmed.is_empty() && trimmed.chars()
                    .all(|c| payload_set.contains(&c.to_lowercase().to_string()));
                if all_in_payload {
                    trimmed.chars()
                        .map(|c| c.to_lowercase().to_string())
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            })
            .collect()
    } else {
        // Standard: whitespace-delimited, filter against payload wordlist.
        text.split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| payload_tree.contains(w))
            .collect()
    };

    if extracted.is_empty() {
        return Err(PipelineError::DecodeError(
            "no payload words found in input".to_string(),
        ));
    }

    if verbose {
        eprintln!("Extracted {} payload words", extracted.len());
    }

    // 4. Payload words → binary → data string.
    codec::decode_str(&extracted, &payload_tree)
        .map_err(|e| PipelineError::DecodeError(format!("{}", e)))
}

/// Decode language prose, returning both the decoded text and extracted payload words.
pub fn decode_from_language_rich(
    text: &str,
    language: &str,
    wordlist: &str,
    verbose: bool,
) -> Result<(String, Vec<String>), PipelineError> {
    // Resolve "default" wordlist to actual name.
    let wordlist = if wordlist == "default" {
        let dw = default_wordlist(language);
        if dw == "default" { wordlist } else { dw }
    } else {
        wordlist
    };

    // 1. Load payload wordlist.
    let payload_words = load_payload_words_for_wordlist(language, wordlist)
        .map_err(|e| PipelineError::DecodeError(e))?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    // 2. Grammar tells us how payload words are separated.
    let grammar = Grammar::from_language_dialect(language, "body")
        .map_err(|e| PipelineError::DecodeError(format!("Grammar error: {}", e)))?;
    let payload_separator = grammar.payload_separator();

    // 3. Extract payload words from prose.
    let extracted: Vec<String> = if payload_separator.is_empty() {
        let payload_set: HashSet<String> = payload_words.iter()
            .map(|w| w.to_lowercase())
            .collect();
        text.split_whitespace()
            .flat_map(|token| {
                let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
                let all_in_payload = !trimmed.is_empty() && trimmed.chars()
                    .all(|c| payload_set.contains(&c.to_lowercase().to_string()));
                if all_in_payload {
                    trimmed.chars()
                        .map(|c| c.to_lowercase().to_string())
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            })
            .collect()
    } else {
        text.split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| payload_tree.contains(w))
            .collect()
    };

    if extracted.is_empty() {
        return Err(PipelineError::DecodeError(
            "no payload words found in input".to_string(),
        ));
    }

    if verbose {
        eprintln!("Extracted {} payload words", extracted.len());
    }

    // 4. Payload words → binary → data string.
    let decoded = codec::decode_str(&extracted, &payload_tree)
        .map_err(|e| PipelineError::DecodeError(format!("{}", e)))?;

    Ok((decoded, extracted))
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── from_meta() parsing ──────────────────────────────────────────

    #[test]
    fn test_parse_from_into() {
        let p = Pipeline::from_meta("translate from english into latin").unwrap();
        assert_eq!(p.source, Endpoint::language("english"));
        assert_eq!(p.target, Endpoint::language("latin"));
    }

    #[test]
    fn test_parse_into_only() {
        let p = Pipeline::from_meta("encode into latin").unwrap();
        assert_eq!(p.source, Endpoint::Auto);
        assert_eq!(p.target, Endpoint::language("latin"));
    }

    #[test]
    fn test_parse_format_source_unscoped() {
        // "encode hex into english" — hex is unscoped, english is target via "into".
        // Unscoped hex should fill the source gap.
        let p = Pipeline::from_meta("encode hex into english").unwrap();
        assert!(matches!(p.source, Endpoint::Format(DataMode::Hex)),
            "Expected hex source, got {:?}", p.source);
        assert_eq!(p.target, Endpoint::language("english"));
    }

    #[test]
    fn test_parse_from_format() {
        let p = Pipeline::from_meta("encode from hex into english").unwrap();
        assert!(matches!(p.source, Endpoint::Format(DataMode::Hex)));
        assert_eq!(p.target, Endpoint::language("english"));
    }

    #[test]
    fn test_parse_dialect_modifier_on_target() {
        let p = Pipeline::from_meta("translate from english into latin prose").unwrap();
        assert_eq!(p.source, Endpoint::language("english"));
        assert_eq!(p.target, Endpoint::language_with_dialect("latin", "prose"));
    }

    #[test]
    fn test_parse_single_word_is_target() {
        // Just "latin" → source=Auto, target=latin
        let p = Pipeline::from_meta("latin").unwrap();
        assert_eq!(p.source, Endpoint::Auto);
        assert_eq!(p.target, Endpoint::language("latin"));
    }

    #[test]
    fn test_parse_no_prepositions_fallback_order() {
        // "english latin" → first=source, second=target
        let p = Pipeline::from_meta("english latin").unwrap();
        assert_eq!(p.source, Endpoint::language("english"));
        assert_eq!(p.target, Endpoint::language("latin"));
    }

    #[test]
    fn test_parse_decode_from_keeps_source() {
        // "decode from english" → source=english (explicit), target=Auto
        let p = Pipeline::from_meta("decode from english").unwrap();
        assert_eq!(p.source, Endpoint::language("english"));
        assert_eq!(p.target, Endpoint::Auto);
    }

    #[test]
    fn test_parse_empty_fails() {
        assert!(Pipeline::from_meta("").is_err());
    }

    #[test]
    fn test_parse_no_payload_words_fails() {
        assert!(Pipeline::from_meta("translate from into").is_err());
    }

    #[test]
    fn test_parse_format_to_language() {
        let p = Pipeline::from_meta("from ascii7 into english").unwrap();
        assert!(matches!(p.source, Endpoint::Format(DataMode::Ascii7)));
        assert_eq!(p.target, Endpoint::language("english"));
    }

    // ── from_params() ────────────────────────────────────────────────

    #[test]
    fn test_from_params_roundtrip_types() {
        let p = Pipeline::from_params(
            Endpoint::language("english"),
            Endpoint::language("latin"),
        );
        assert_eq!(p.source, Endpoint::language("english"));
        assert_eq!(p.target, Endpoint::language("latin"));
    }

    // ── Encode / Decode / Transcode round-trips ──────────────────────
    //
    // These tests verify the binary-intermediate architecture:
    //   encode: string → binary → payload words → prose
    //   decode: prose → payload words → binary → string

    #[test]
    fn test_encode_decode_english_round_trip() {
        // Use a hex string so the codec layer uses Hex mode (unambiguous).
        let input = "deadbeef";
        let pipeline_encode = Pipeline::from_params(
            Endpoint::Format(DataMode::Hex),
            Endpoint::language_full("english", "bip39", "body"),
        ).with_seed(42);

        let encoded = pipeline_encode.execute(input).unwrap();
        assert!(!encoded.is_empty(), "Encoded text should not be empty");

        // Decode back.
        let pipeline_decode = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "body"),
            Endpoint::Auto,
        );
        let decoded = pipeline_decode.execute(&encoded).unwrap();
        assert_eq!(decoded, input, "Round-trip should recover original hex string");
    }

    #[test]
    fn test_transcode_english_to_latin() {
        // Encode hex into English prose.
        let input = "cafe";
        let encode_pipeline = Pipeline::from_params(
            Endpoint::Format(DataMode::Hex),
            Endpoint::language_full("english", "bip39", "body"),
        ).with_seed(42);

        let english_prose = encode_pipeline.execute(input).unwrap();

        // Transcode English → Latin (via binary).
        let transcode_pipeline = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "body"),
            Endpoint::language("latin"),
        ).with_seed(42);

        let latin_prose = transcode_pipeline.execute(&english_prose).unwrap();
        assert!(!latin_prose.is_empty(), "Latin prose should not be empty");

        // Decode Latin back to verify data survived.
        let decode_pipeline = Pipeline::from_params(
            Endpoint::language("latin"),
            Endpoint::Auto,
        );
        let decoded = decode_pipeline.execute(&latin_prose).unwrap();
        assert_eq!(decoded, input, "Transcode round-trip should recover original");
    }

    #[test]
    fn test_meta_parse_encode_into_english() {
        // Verify parsing works even if we can't execute (wordlist may not be embedded in debug).
        let p = Pipeline::from_meta("encode into english").unwrap();
        assert_eq!(p.source, Endpoint::Auto);
        assert_eq!(p.target, Endpoint::language("english"));
    }

    #[test]
    fn test_meta_transcode_parse() {
        let p = Pipeline::from_meta("translate from english into latin").unwrap();
        assert_eq!(p.source, Endpoint::language("english"));
        assert_eq!(p.target, Endpoint::language("latin"));
    }

    // ── HTML cover override ─────────────────────────────────────────

    #[test]
    fn test_parse_html_flag() {
        let p = Pipeline::from_meta("encode into pgp html").unwrap();
        assert!(p.html, "html flag should be true");
        assert_eq!(p.target, Endpoint::language_with_dialect("cs", "pgp"));
    }

    #[test]
    fn test_parse_html_flag_order_independent() {
        // "html" can appear before or after the language word
        let p = Pipeline::from_meta("encode into html pgp").unwrap();
        assert!(p.html, "html flag should be true regardless of order");
        assert_eq!(p.target, Endpoint::language_with_dialect("cs", "pgp"));
    }

    #[test]
    fn test_parse_no_html_flag() {
        let p = Pipeline::from_meta("encode into pgp").unwrap();
        assert!(!p.html, "html flag should be false when not specified");
    }

    #[test]
    fn test_cover_override_with_html() {
        let p = Pipeline::from_meta("encode into pgp html").unwrap();
        assert_eq!(p.cover_override(), Some("html"));
    }

    #[test]
    fn test_cover_override_without_html() {
        let p = Pipeline::from_meta("encode into pgp").unwrap();
        assert_eq!(p.cover_override(), None);
    }

    // ── PGP routes to cs/pgp dialect ────────────────────────────────

    #[test]
    fn test_pgp_routes_to_cs_pgp_dialect() {
        let p = Pipeline::from_meta("encode into pgp").unwrap();
        assert_eq!(p.target, Endpoint::language_with_dialect("cs", "pgp"),
            "pgp should route to cs language with pgp dialect, not body");
    }

    #[test]
    fn test_nostr_routes_to_cs_nip04_dialect() {
        let p = Pipeline::from_meta("encode into nostr").unwrap();
        assert_eq!(p.target, Endpoint::language_with_dialect("cs", "nip04"),
            "nostr should route to cs language with nip04 dialect");
    }
}
