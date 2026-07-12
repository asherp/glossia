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

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

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
use crate::grammar::{Grammar, DialectConfig};
use crate::merkle::WordlistTree;
use crate::types::Pos;

// ─── WordlistTree cache ─────────────────────────────────────────────────
// WordlistTree::new(words) walks every word in the payload list (5-10k
// entries for Latin), lowercases each (one allocation per word), and
// inserts into a HashMap — ~tens of ms for big wordlists. Decode-heavy
// callers (email pipeline, message verifiers) hit this on every block
// and were spending the bulk of decrypt time rebuilding the same index.
// Cache the built tree per (language, wordlist) so it's paid exactly
// once per process.
static PAYLOAD_TREE_CACHE: OnceLock<Mutex<HashMap<String, Arc<WordlistTree>>>> = OnceLock::new();

/// Return a process-wide cached payload [`WordlistTree`] for `(language, wordlist)`.
///
/// Building a `WordlistTree` is O(n) over the payload wordlist (tens of ms for
/// large lists like English `lemmas`/`ngram` at 2¹⁷ words). This getter is
/// backed by a global cache, so the build cost is paid at most once per
/// `(language, wordlist)` per process; subsequent calls return a cheap
/// `Arc` clone of the shared tree.
///
/// Prefer this over [`crate::generator::data::load_payload_tree`], which
/// rebuilds the tree on every call.
pub fn cached_payload_tree(language: &str, wordlist: &str) -> Result<Arc<WordlistTree>, PipelineError> {
    let key = format!("{}:{}", language, wordlist);
    let cache = PAYLOAD_TREE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(tree) = cache.lock().unwrap().get(&key) {
        return Ok(tree.clone());
    }
    let words = load_payload_words_for_wordlist(language, wordlist)
        .map_err(|e| PipelineError::DecodeError(e))?;
    let tree = Arc::new(WordlistTree::new(words));
    cache.lock().unwrap().insert(key, tree.clone());
    Ok(tree)
}

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
    /// The resolved source endpoint (after auto-detection).
    /// Useful for constructing the reverse pipeline instruction.
    pub resolved_source: Option<Endpoint>,
}

/// Statistics about an encode/transcode operation.
pub struct PipelineStats {
    pub payload_count: usize,
    pub cover_count: usize,
    pub total_words: usize,
    pub ratio: f64,
}

/// The intent verb parsed from a meta instruction.
///
/// Authoritative for how an `Auto` source is interpreted when the target is a
/// Language: `Encode` treats the input as **raw data** to encode (never guessing
/// it is a seed or existing prose — this is what makes #29 impossible);
/// `Transcode` treats the input as an **existing Glossia encoding** to detect and
/// re-encode. `Unspecified` (shorthand meta with no verb, or `from_params`) falls
/// back to inferring the direction from the endpoint types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Verb {
    #[default]
    Unspecified,
    Encode,
    Decode,
    Transcode,
}

/// Classify a meta word as an intent verb, if it is one.
fn classify_verb(word: &str) -> Option<Verb> {
    match word {
        "encode" => Some(Verb::Encode),
        "decode" => Some(Verb::Decode),
        // "translate" is a synonym for transcode (language → language).
        "transcode" | "translate" => Some(Verb::Transcode),
        _ => None,
    }
}

/// A pipeline specification parsed from meta-language words or constructed
/// from explicit parameters.
#[derive(Clone, Debug)]
pub struct Pipeline {
    pub source: Endpoint,
    pub target: Endpoint,
    /// The intent verb (encode/transcode/decode). Disambiguates how an `Auto`
    /// source is treated when the target is a Language. See [`Verb`].
    pub verb: Verb,
    pub seed: u64,
    pub verbose: bool,
    /// When true, replace `\n` with `<br>` in output for HTML rendering.
    pub html: bool,
    /// Sentence length selection mode (compact vs. natural).
    pub length_mode: Option<SentenceLengthMode>,
    /// Generate N variations and select the most compact.
    pub variations: Option<usize>,
    /// Maximum sentence length (POS slots).
    pub k_max: Option<usize>,
    /// Minimum sentence length (POS slots).
    pub k_min: Option<usize>,
}

// ═══════════════════════════════════════════════════════════════════════
// Crypto Format Detection
// ═══════════════════════════════════════════════════════════════════════

/// Detect if input is a known cryptocurrency format.
/// Returns `(language, dialect, data_part_after_prefix)` — the prefix is stripped
/// so only the data portion needs to be encoded with the dialect's alphabet.
fn detect_crypto_format(input: &str) -> Option<(&str, &str, &str)> {
    let lower = input.to_lowercase();
    // Bech32 formats: HRP + "1" separator + witness-version + data
    // Longer prefixes first to discriminate SegWit v0 (bc1q) from Taproot (bc1p).
    if lower.starts_with("bc1q")  { return Some(("crypto/btc",   "bip173",      &input[4..])); }
    if lower.starts_with("bc1p")  { return Some(("crypto/btc",   "bip350",      &input[4..])); }
    if lower.starts_with("tb1q")  { return Some(("crypto/btc",   "bip173-test", &input[4..])); }
    if lower.starts_with("tb1p")  { return Some(("crypto/btc",   "bip350-test", &input[4..])); }
    if lower.starts_with("npub1") { return Some(("crypto/nostr", "pub",  &input[5..])); }
    if lower.starts_with("nsec1") { return Some(("crypto/nostr", "sec",  &input[5..])); }
    if lower.starts_with("note1") { return Some(("crypto/nostr", "note", &input[5..])); }
    if lower.starts_with("lnbc")  { return Some(("crypto/ln",    "main", &input[4..])); }
    if lower.starts_with("lntb")  { return Some(("crypto/ln",    "test", &input[4..])); }
    // Cashu tokens (case-sensitive prefixes)
    if input.starts_with("cashuA") { return Some(("crypto/cashu", "a", &input[6..])); }
    if input.starts_with("cashuB") { return Some(("crypto/cashu", "b", &input[6..])); }
    None
}

/// Map a crypto sub-language to its primary payload type.
/// All dialects within a sub-language share the same alphabet,
/// except btc/legacy which uses base58 (handled as a dialect override).
fn crypto_payload_type(language: &str, dialect: &str) -> &'static str {
    // BTC P2PKH uses base58 instead of bech32.
    if language == "crypto/btc" && (dialect == "p2pkh" || dialect == "p2pkh-test") {
        return "base58";
    }
    match language {
        "crypto/btc" | "crypto/nostr" | "crypto/ln" => "bech32",
        "crypto/cashu" => "base64url",
        _ => "bech32",
    }
}

/// Map a crypto (language, dialect) pair to its prefix string.
fn crypto_dialect_prefix(language: &str, dialect: &str) -> &'static str {
    match (language, dialect) {
        ("crypto/btc",   "bip173")      => "bc1q",
        ("crypto/btc",   "bip173-test") => "tb1q",
        ("crypto/btc",   "bip350")      => "bc1p",
        ("crypto/btc",   "bip350-test") => "tb1p",
        ("crypto/btc",   "p2pkh")       => "",
        ("crypto/btc",   "p2pkh-test")  => "",
        ("crypto/nostr", "pub")  => "npub1",
        ("crypto/nostr", "sec")  => "nsec1",
        ("crypto/nostr", "note") => "note1",
        ("crypto/ln",    "main") => "lnbc",
        ("crypto/ln",    "test") => "lntb",
        ("crypto/cashu", "a")    => "cashuA",
        ("crypto/cashu", "b")    => "cashuB",
        _ => "",
    }
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
        "czech" => Some(("czech", "body")),
        "nostr" => Some(("cs", "nip04")),
        "pgp" => Some(("cs", "pgp")),
        "primes" => Some(("math", "body")),
        "image" => Some(("image", "voronoi")),
        "bitcoin" | "btc" => Some(("crypto/btc", "bip173")),
        "npub" => Some(("crypto/nostr", "pub")),
        "nsec" => Some(("crypto/nostr", "sec")),
        "lightning" | "ln" => Some(("crypto/ln", "main")),
        "cashu" => Some(("crypto/cashu", "a")),
        "email" => Some(("cs", "email")),
        "bip39" => Some(("cs", "sig_bip39")),
        _ => None,
    }
}

/// Meta payload words that are palette (wordlist) modifiers for the image language.
fn meta_word_to_palette(word: &str) -> Option<&str> {
    match word {
        "viridis" => Some("viridis"),
        "plasma" => Some("plasma"),
        "inferno" => Some("inferno"),
        "magma" => Some("magma"),
        "cividis" => Some("cividis"),
        "turbo" => Some("turbo"),
        "mako" => Some("mako"),
        "rocket" => Some("rocket"),
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
        "voronoi" => Some("voronoi"),
        "grid" => Some("grid"),
        "mosaic" => Some("mosaic"),
        "constellation" => Some("constellation"),
        "patches" => Some("patches"),
        "raw" => Some("raw"),
        "sig" => Some("sig_nostr"),
        "seal" => Some("seal_nostr"),
        "sig_bip39" => Some("sig_bip39"),
        "sig_latin" => Some("sig_latin"),
        "nip04_bip39" => Some("nip04_bip39"),
        "nip44_bip39" => Some("nip44_bip39"),
        "bip39" => Some("bip39"),
        "email_alt" => Some("email_alt"),
        "email_mime" => Some("email_mime"),
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
            "latin", "english", "czech", "hex", "base64", "base58", "ascii7", "bits",
            "bytes", "nostr", "pgp", "prose", "body", "subject", "spells", "bip39", "sig", "seal",
            "primes", "merkle", "image", "voronoi", "grid", "mosaic",
            "constellation", "patches", "raw",
            // Crypto language keywords
            "bitcoin", "btc", "npub", "nsec", "lightning", "ln", "cashu",
            // Email dialect keywords
            "email", "email_alt", "email_mime",
            // Image palette names (wordlist modifiers)
            "viridis", "plasma", "inferno", "magma", "cividis", "turbo", "mako", "rocket",
        ]
        .iter()
        .copied()
        .collect();

        // Adverbs that modify pipeline behavior (not payload words).
        let meta_adverbs: HashSet<&str> = [
            "compactly", "efficiently", "naturally", "minimally",
            "optimally", "thoroughly",
        ]
        .iter()
        .copied()
        .collect();

        let mut current_role: Option<Role> = None;
        let mut verb = Verb::Unspecified;
        let mut source: Option<Endpoint> = None;
        let mut target: Option<Endpoint> = None;
        let mut pending_dialect: Option<String> = None;
        let mut pending_wordlist: Option<String> = None;
        let mut unscoped_endpoints: Vec<Endpoint> = Vec::new();
        // Track whether source/target were set via explicit prepositions.
        let mut source_explicit = false;
        let mut target_explicit = false;
        let mut html = false;

        // Pipeline parameters from adverbs.
        let mut length_mode: Option<SentenceLengthMode> = None;
        let mut variations: Option<usize> = None;
        let mut k_max: Option<usize> = None;
        let mut k_min: Option<usize> = None;

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

            // Adverb modifiers: control compactness, variations, sentence length.
            if meta_adverbs.contains(lower) {
                match lower {
                    "compactly" => {
                        length_mode = Some(SentenceLengthMode::Compact);
                    }
                    "efficiently" => {
                        length_mode = Some(SentenceLengthMode::Compact);
                        k_max = Some(20);
                    }
                    "naturally" => {
                        length_mode = Some(SentenceLengthMode::Natural);
                    }
                    "minimally" => {
                        length_mode = Some(SentenceLengthMode::Compact);
                        k_min = Some(2);
                        k_max = Some(12);
                    }
                    "optimally" => {
                        variations = Some(50);
                    }
                    "thoroughly" => {
                        variations = Some(100);
                    }
                    _ => {}
                }
                continue;
            }

            // Slash-separated endpoint spec (e.g. "english/bip39/raw",
            // "crypto/btc/bech32/bip173").
            if token.contains('/') {
                let parts: Vec<&str> = token.split('/').collect();
                let endpoint = match parts.len() {
                    // "english/bip39/body"
                    3 => Endpoint::language_full(parts[0], parts[1], parts[2]),
                    // "crypto/btc/bech32/bip173" — 2-part language name
                    4 => Endpoint::Language {
                        language: format!("{}/{}", parts[0], parts[1]),
                        wordlist: parts[2].to_string(),
                        dialect: parts[3].to_string(),
                    },
                    // "english/bip39"
                    2 => Endpoint::Language {
                        language: parts[0].to_string(),
                        wordlist: parts[1].to_string(),
                        dialect: "body".to_string(),
                    },
                    _ => continue,
                };
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
                continue;
            }

            // Intent verb: authoritative for source interpretation (first wins).
            if let Some(v) = classify_verb(lower) {
                if verb == Verb::Unspecified {
                    verb = v;
                }
                continue;
            }

            // Skip remaining non-meta-payload words (cover words).
            if !meta_payload.contains(lower) {
                continue;
            }

            // Palette (wordlist) modifier — save it to apply to the next image endpoint.
            if let Some(palette) = meta_word_to_palette(lower) {
                pending_wordlist = Some(palette.to_string());
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
                let wordlist = pending_wordlist.take()
                    .unwrap_or_else(|| "default".to_string());
                Endpoint::language_full(lang, &wordlist, &dialect)
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
            // Apply trailing wordlist (palette) modifier to the most recently set endpoint.
            if let Some(wl) = pending_wordlist.take() {
                apply_wordlist_modifier(&mut source, &mut target, &mut unscoped_endpoints, &wl);
            }
        }

        // Apply any remaining pending dialect.
        if let Some(dialect) = pending_dialect {
            apply_dialect_modifier(&mut source, &mut target, &mut unscoped_endpoints, &dialect);
        }
        // Apply any remaining pending wordlist.
        if let Some(wl) = pending_wordlist {
            apply_wordlist_modifier(&mut source, &mut target, &mut unscoped_endpoints, &wl);
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

        // Resolve dialect-declared wordlists: if a dialect declares
        // payload_wordlist (e.g., spells → "hp"), upgrade "default" to that.
        let source = resolve_dialect_wordlist(source);
        let target = resolve_dialect_wordlist(target);

        Ok(Pipeline {
            source,
            target,
            verb,
            seed: 0,
            verbose: false,
            html,
            length_mode,
            variations,
            k_max,
            k_min,
        })
    }

    /// Construct from explicit parameters (backward compat with CLI flags).
    ///
    /// The verb is `Unspecified`, so an `Auto` source is treated as raw data when
    /// the target is a Language (the encode direction). Use [`Self::with_verb`] to
    /// request transcode auto-detection of the source instead.
    pub fn from_params(source: Endpoint, target: Endpoint) -> Self {
        // Resolve dialect-declared wordlists (same as from_meta).
        let source = resolve_dialect_wordlist(source);
        let target = resolve_dialect_wordlist(target);

        Pipeline {
            source,
            target,
            verb: Verb::Unspecified,
            seed: 0,
            verbose: false,
            html: false,
            length_mode: None,
            variations: None,
            k_max: None,
            k_min: None,
        }
    }

    /// Set the intent verb (encode/transcode/decode). See [`Verb`].
    pub fn with_verb(mut self, verb: Verb) -> Self {
        self.verb = verb;
        self
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

    /// Set sentence length mode (compact vs. natural).
    pub fn with_length_mode(mut self, mode: SentenceLengthMode) -> Self {
        self.length_mode = Some(mode);
        self
    }

    /// Set number of variations to generate (select most compact).
    pub fn with_variations(mut self, count: usize) -> Self {
        self.variations = Some(count);
        self
    }

    /// Set maximum sentence length (POS slots).
    pub fn with_k_max(mut self, k_max: usize) -> Self {
        self.k_max = Some(k_max);
        self
    }

    /// Set minimum sentence length (POS slots).
    pub fn with_k_min(mut self, k_min: usize) -> Self {
        self.k_min = Some(k_min);
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

    /// Detect which Glossia language/dialect `input` is encoded in, for the
    /// transcode/decode directions. Returns the matched `Language` endpoint when a
    /// payload wordlist matches confidently (hit rate > 0.3), else `None`.
    fn detect_source_language(&self, input: &str) -> Option<Endpoint> {
        let words: Vec<String> = input
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            return None;
        }
        let matches = detect_dialect(&words);
        let best = matches.first()?;
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
            Some(Endpoint::Language {
                language: best.language.clone(),
                wordlist: best.wordlist.clone(),
                dialect,
            })
        } else {
            None
        }
    }

    /// Resolve an `Auto` source. The verb the user typed is authoritative.
    ///
    /// When the target is a Language, the verb decides how the input is read:
    /// - `encode` (or `Unspecified`): the input is **raw data** to encode. We do
    ///   not inspect it for "is this secretly a seed/prose?" — that content
    ///   guessing is exactly what silently corrupted ordinary payload-word text
    ///   (#29). `encode into <lang>` always means "encode this data".
    /// - `transcode`/`translate`: the input is an **existing Glossia encoding**;
    ///   we detect its source dialect and re-encode. If no encoding is detected,
    ///   we error rather than guess — name the source with `from <source>`.
    ///
    /// Structured crypto formats (a bech32 address, etc.) are still recognized in
    /// both cases: that is unambiguous format recognition, not intent guessing.
    ///
    /// When the target is Auto/Format (the decode direction), identifying the
    /// source dialect *is* the job, so detection always runs.
    fn resolve_source(&self, input: &str) -> Result<Endpoint, PipelineError> {
        match &self.source {
            Endpoint::Auto => {
                if matches!(&self.target, Endpoint::Language { .. }) {
                    // Crypto format recognition (native alphabet), unless the
                    // target is itself crypto. Applies regardless of verb.
                    if !matches!(&self.target, Endpoint::Language { language, .. } if language.starts_with("crypto/")) {
                        if let Some((language, dialect, _)) = detect_crypto_format(input) {
                            if self.verbose {
                                eprintln!("Source auto-detected as {}/{}", language, dialect);
                            }
                            return Ok(Endpoint::Language {
                                language: language.to_string(),
                                wordlist: crypto_payload_type(language, dialect).to_string(),
                                dialect: dialect.to_string(),
                            });
                        }
                    }

                    if self.verb == Verb::Transcode {
                        // User asserts the input is an existing encoding: detect it.
                        if let Some(src) = self.detect_source_language(input) {
                            return Ok(src);
                        }
                        return Err(PipelineError::InvalidPipeline(
                            "transcode: could not detect the source encoding of the input; \
                             name it explicitly, e.g. `transcode from english into <target>`"
                                .to_string(),
                        ));
                    }

                    // encode / unspecified: raw data to encode. A bare BIP39
                    // mnemonic is encoded as text and round-trips verbatim; to
                    // re-encode it as a seed use `transcode` or an explicit
                    // `from <lang>/<wordlist>/raw` source.
                    let (mode, _) = codec::detect_mode(input);
                    if self.verbose {
                        eprintln!("Source auto-detected as format: {} (target is Language)", mode);
                    }
                    return Ok(Endpoint::Format(mode));
                }

                // Decode direction (target is Auto/Format): detect the source
                // dialect; fall back to treating the input as raw data.
                if let Some(src) = self.detect_source_language(input) {
                    return Ok(src);
                }
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

        // Crypto encoding: bytes → chars → prefix + concatenated chars.
        if language.starts_with("crypto/") {
            return encode_crypto(input, language, dialect, self.verbose);
        }

        // Raw encoding: base-N encode into bare payload words (no header, no cover).
        if dialect == "raw" {
            let wordlist_name = if wordlist == "default" {
                let dw = crate::generator::default_wordlist(language);
                if dw == "default" { wordlist } else { dw }
            } else {
                wordlist
            };
            let payload_words = load_payload_words_for_wordlist(language, wordlist_name)
                .map_err(|e| PipelineError::EncodeError(e))?;
            let payload_tree = WordlistTree::new(payload_words);

            // Raw output is bare payload words with no padding header, so it must
            // use base-N — the same codec the raw decode path uses. (bitpack would
            // prepend a padding-count word that the bare-word decoder doesn't
            // expect, breaking the round-trip.)
            let (_, data) = codec::detect_mode(input);
            let words = codec::encode_base_n(&data, &payload_tree, "base_n")
                .map_err(|e| PipelineError::EncodeError(format!("{}", e)))?;

            return Ok(words.join(" "));
        }

        let (text, _, _, _) = encode_into_language(
            input, language, wordlist, dialect, None, self.seed, self.verbose,
            self.cover_override(),
            self.length_mode,
            self.k_min,
            self.k_max,
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

        // Raw encoding: base-N encode into bare payload words (no header, no cover).
        // This is the inverse of "raw" decode — used to reconstruct a BIP39 mnemonic.
        if dialect == "raw" {
            let wordlist_name = if wordlist == "default" {
                let dw = crate::generator::default_wordlist(language);
                if dw == "default" { wordlist } else { dw }
            } else {
                wordlist
            };
            let payload_words = load_payload_words_for_wordlist(language, wordlist_name)
                .map_err(|e| PipelineError::EncodeError(e))?;
            let payload_tree = WordlistTree::new(payload_words);

            // Raw output is bare payload words with no padding header, so it must
            // use base-N — the same codec the raw decode path uses. (bitpack would
            // prepend a padding-count word that the bare-word decoder doesn't
            // expect, breaking the round-trip.)
            let (_, data) = codec::detect_mode(input);
            let words = codec::encode_base_n(&data, &payload_tree, "base_n")
                .map_err(|e| PipelineError::EncodeError(format!("{}", e)))?;

            let output = words.join(" ");
            let payload_count = words.len();
            return Ok(PipelineResult {
                output,
                payload_words: words,
                data_mode: None,
                stats: Some(PipelineStats {
                    payload_count,
                    cover_count: 0,
                    total_words: payload_count,
                    ratio: 1.0,
                }),
                resolved_source: None,
            });
        }

        // Crypto encoding: return rich result with minimal stats.
        if language.starts_with("crypto/") {
            let text = encode_crypto(input, language, dialect, self.verbose)?;
            let prefix = crypto_dialect_prefix(language, dialect);
            let data_chars: Vec<String> = text[prefix.len()..].chars()
                .map(|c: char| c.to_string())
                .collect();
            let payload_count = data_chars.len();
            return Ok(PipelineResult {
                output: text,
                payload_words: data_chars,
                data_mode: None,
                stats: Some(PipelineStats {
                    payload_count,
                    cover_count: if prefix.is_empty() { 0 } else { 1 },
                    total_words: payload_count + if prefix.is_empty() { 0 } else { 1 },
                    ratio: 1.0,
                }),
                resolved_source: None,
            });
        }

        let (text, _payload_set, encoded_words, data_mode) = encode_into_language(
            input, language, wordlist, dialect, None, self.seed, self.verbose,
            self.cover_override(),
            self.length_mode,
            self.k_min,
            self.k_max,
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
            resolved_source: None,
        })
    }

    /// Decode bare/raw payload words (e.g. a BIP39 mnemonic) into hex bytes.
    ///
    /// Raw input carries no Glossia header or cover words, so every payload word
    /// is data and decoding is a plain base-N integer (no bitpack padding word).
    /// Returns the hex-encoded bytes and the extracted payload words. Shared by
    /// [`Self::do_decode`] and [`Self::do_decode_rich`] so both agree.
    fn decode_raw_words(
        &self,
        input: &str,
        language: &str,
        wordlist: &str,
    ) -> Result<(String, Vec<String>), PipelineError> {
        let wordlist_name = if wordlist == "default" {
            let dw = crate::generator::default_wordlist(language);
            if dw == "default" { wordlist } else { dw }
        } else {
            wordlist
        };
        let payload_words = load_payload_words_for_wordlist(language, wordlist_name)
            .map_err(|e| PipelineError::DecodeError(e))?;
        let payload_tree = WordlistTree::new(payload_words);

        let words: Vec<String> = input
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| !w.is_empty() && payload_tree.contains(w))
            .collect();

        if words.is_empty() {
            return Err(PipelineError::DecodeError(
                "no payload words found in input".to_string(),
            ));
        }

        if self.verbose {
            eprintln!("Raw decode: {} payload words via base-N", words.len());
        }

        // Raw mnemonics have no bitpack header — always use base_n.
        let bytes = codec::decode_base_n(&words, &payload_tree, "base_n")
            .map_err(|e| PipelineError::DecodeError(format!("{}", e)))?;
        // Raw output defaults to hex (back-compat); honor an explicit format target.
        let output = match self.target_format() {
            Some(fmt) => render_decoded_bytes(&bytes, Some(fmt)),
            None => codec::hex_encode(&bytes),
        };
        Ok((output, words))
    }

    /// The explicit output format requested by the pipeline target, if any.
    ///
    /// `Some(mode)` when the target is a `Format` endpoint (e.g. `… into hex`);
    /// `None` for an `Auto` or `Language` target, so each decode path keeps its
    /// own default rendering.
    fn target_format(&self) -> Option<DataMode> {
        match &self.target {
            Endpoint::Format(mode) => Some(*mode),
            _ => None,
        }
    }

    /// Decode: language prose → extract payload words → binary → data string.
    fn do_decode(&self, input: &str, source: &Endpoint) -> Result<String, PipelineError> {
        let (language, wordlist, dialect) = match source {
            Endpoint::Language { language, wordlist, dialect } => {
                (language.as_str(), wordlist.as_str(), dialect.as_str())
            }
            _ => return Err(PipelineError::InvalidPipeline(
                "decode source must be a Language".to_string(),
            )),
        };

        // Crypto addresses are single tokens — strip prefix, extract chars directly.
        if language.starts_with("crypto/") {
            return decode_crypto(input, language, dialect, self.verbose);
        }

        // "raw" dialect: input is raw payload words (e.g. BIP39 mnemonic) without
        // Glossia header or cover words. Decode directly via base-N.
        if dialect == "raw" {
            let (output, _) = self.decode_raw_words(input, language, wordlist)?;
            return Ok(output);
        }

        // Resolve payload_language: if the dialect declares a different language
        // for payload files, pass it so decode loads the correct wordlist.
        let payload_lang = DialectConfig::from_language_dialect(language, dialect)
            .map(|c| c.payload_language().to_string())
            .unwrap_or_else(|_| language.to_string());

        decode_from_language_with_payload_lang(input, language, &payload_lang, wordlist, self.verbose, dialect, self.target_format())
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

        let mut result = match (&source, target) {
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
        }?;

        // Attach the resolved source so callers (e.g. the UI) can construct
        // the reverse pipeline instruction accurately.
        result.resolved_source = Some(source);
        Ok(result)
    }

    /// Decode with rich results: returns decoded text and extracted payload words.
    fn do_decode_rich(&self, input: &str, source: &Endpoint) -> Result<PipelineResult, PipelineError> {
        let (language, wordlist, dialect) = match source {
            Endpoint::Language { language, wordlist, dialect } => {
                (language.as_str(), wordlist.as_str(), dialect.as_str())
            }
            _ => return Err(PipelineError::InvalidPipeline(
                "decode source must be a Language".to_string(),
            )),
        };

        // Crypto decoding: strip prefix, extract chars, decode.
        if language.starts_with("crypto/") {
            let decoded = decode_crypto(input, language, dialect, self.verbose)?;
            let data_part = if let Some((_, _, data)) = detect_crypto_format(input) {
                data
            } else {
                input
            };
            let extracted: Vec<String> = data_part.chars()
                .map(|c| c.to_lowercase().to_string())
                .collect();
            return Ok(PipelineResult {
                output: decoded,
                payload_words: extracted,
                data_mode: None,
                stats: None,
                resolved_source: None,
            });
        }

        // "raw" dialect: bare payload words (e.g. BIP39 mnemonic), all payload and
        // no cover — decode via base-N, same as the plain do_decode raw branch.
        if dialect == "raw" {
            let (output, words) = self.decode_raw_words(input, language, wordlist)?;
            let payload_count = words.len();
            return Ok(PipelineResult {
                output,
                payload_words: words,
                data_mode: None,
                stats: Some(PipelineStats {
                    payload_count,
                    cover_count: 0,
                    total_words: payload_count,
                    ratio: 1.0,
                }),
                resolved_source: None,
            });
        }

        // Resolve payload_language for cross-language payload (e.g., sig_bip39).
        let payload_lang = DialectConfig::from_language_dialect(language, dialect)
            .map(|c| c.payload_language().to_string())
            .unwrap_or_else(|_| language.to_string());

        let (decoded, extracted) = decode_from_language_rich_with_payload_lang(
            input, language, &payload_lang, wordlist, self.verbose, dialect, self.target_format())?;

        Ok(PipelineResult {
            output: decoded,
            payload_words: extracted,
            data_mode: None,
            stats: None,
            resolved_source: None,
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

fn apply_wordlist_modifier(
    source: &mut Option<Endpoint>,
    target: &mut Option<Endpoint>,
    unscoped: &mut Vec<Endpoint>,
    wordlist: &str,
) {
    let ep = if target.is_some() {
        target.as_mut()
    } else if source.is_some() {
        source.as_mut()
    } else {
        unscoped.last_mut()
    };
    if let Some(Endpoint::Language { wordlist: ref mut w, .. }) = ep {
        *w = wordlist.to_string();
    }
}

/// If the endpoint is a Language with `wordlist == "default"`, consult DialectConfig
/// to see if the dialect declares a non-default payload_wordlist (e.g., spells → "hp").
fn resolve_dialect_wordlist(ep: Endpoint) -> Endpoint {
    match ep {
        Endpoint::Language { ref language, ref wordlist, ref dialect } if wordlist == "default" => {
            if let Ok(config) = DialectConfig::from_language_dialect(language, dialect) {
                let declared = config.payload_wordlist();
                if declared != "default" {
                    return Endpoint::Language {
                        language: language.clone(),
                        wordlist: declared.to_string(),
                        dialect: dialect.clone(),
                    };
                }
            }
            ep
        }
        other => other,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Reusable encode/decode helpers
// ═══════════════════════════════════════════════════════════════════════

/// Resolve "default" wordlist to the actual wordlist name for a language.
fn resolve_wordlist_name<'a>(language: &str, wordlist: &'a str) -> &'a str {
    if wordlist == "default" {
        let dw = default_wordlist(language);
        if dw == "default" { wordlist } else { dw }
    } else {
        wordlist
    }
}

/// Shared first stage: input string → binary → POS-tagged payload tokens.
///
/// Resolves the wordlist, loads payload words, encodes input to payload words
/// via the codec layer, and POS-tags each word.
///
/// Returns `(payload_toks, encoded_words, data_mode, payload_word_set)`.
fn prepare_payload(
    input: &str,
    language: &str,
    wordlist: &str,
    dialect: &str,
    forced_data_mode: Option<DataMode>,
) -> Result<(Vec<PayloadTok>, Vec<String>, DataMode, HashSet<String>), PipelineError> {
    // Resolve payload_language: if the dialect declares a different language for
    // payload files (e.g., sig_bip39 uses English BIP39 words inside CS grammar),
    // load from that language's directory instead.
    let payload_lang = DialectConfig::from_language_dialect(language, dialect)
        .map(|c| c.payload_language().to_string())
        .unwrap_or_else(|_| language.to_string());

    // 1. Load payload wordlist.
    let payload_words = load_payload_words_for_wordlist(&payload_lang, wordlist)
        .map_err(|e| PipelineError::EncodeError(e))?;
    let payload_tree = WordlistTree::new(payload_words.clone());

    let grammar = Grammar::from_language_dialect(language, dialect)
        .map_err(|e| PipelineError::EncodeError(format!("Failed to load grammar: {}", e)))?;

    // 2. Input string → binary → payload words (base-N conversion).
    let (encoded_words, data_mode) = if let Some(mode) = forced_data_mode {
        // Explicit mode: pre-decode the input string to raw bytes.
        let data = match mode {
            DataMode::Hex => codec::hex_decode(input)
                .ok_or_else(|| PipelineError::EncodeError("invalid hex input".into()))?,
            DataMode::Base64 => codec::base64_decode(input)
                .ok_or_else(|| PipelineError::EncodeError("invalid base64 input".into()))?,
            DataMode::Ascii7 | DataMode::Bytes8 => input.as_bytes().to_vec(),
        };
        let words = codec::encode_base_n(&data, &payload_tree, grammar.codec())
            .map_err(|e| PipelineError::EncodeError(format!("{}", e)))?;
        (words, mode)
    } else {
        codec::encode_str_base_n(input, &payload_tree, grammar.codec())
            .map_err(|e| PipelineError::EncodeError(format!("{}", e)))?
    };

    // 3. POS-tag each payload word.
    // For CS grammars (dot_is_punctuation=false), all payload slots are N.
    // When cross-language payload is used (e.g., BIP39 words in CS grammar),
    // override all POS tags to N to ensure correct slot matching.
    let is_cs_grammar = !grammar.dot_is_punctuation();
    let pos_mapping = if is_cs_grammar {
        // CS grammar: all payload words are nouns (entity predicates).
        HashMap::new()  // empty mapping → all words get default [N] below
    } else {
        build_pos_mapping_for_wordlist(&payload_lang, wordlist)
            .map_err(|e| PipelineError::EncodeError(e))?
    };

    let payload_toks: Vec<PayloadTok> = encoded_words
        .iter()
        .map(|word| {
            let allowed = if is_cs_grammar {
                vec![Pos::N]
            } else {
                pos_mapping
                    .get(&word.to_lowercase())
                    .cloned()
                    .unwrap_or_default()
            };
            PayloadTok::new(word.clone(), &allowed)
        })
        .collect();

    let wordlist_set: HashSet<String> = payload_words.iter().map(|w| w.to_lowercase()).collect();

    Ok((payload_toks, encoded_words, data_mode, wordlist_set))
}

/// Per-zone second stage: embed a slice of payload tokens into grammatical prose.
///
/// Loads the dialect config, cover words, builds a Lexicon, determines sentence
/// parameters from the grammar, and generates text for the given token range.
fn generate_prose_for_zone(
    payload_toks: &[PayloadTok],
    word_range: std::ops::Range<usize>,
    language: &str,
    _wordlist: &str,
    dialect: &str,
    payload_word_set: &HashSet<String>,
    seed: u64,
    verbose: bool,
    cover_override: Option<&str>,
    length_mode: Option<SentenceLengthMode>,
    k_min_override: Option<usize>,
    k_max_override: Option<usize>,
) -> Result<(String, HashSet<String>), PipelineError> {
    // 1. Load grammar and dialect config.
    let dialect_config = crate::grammar::DialectConfig::from_language_dialect(language, dialect)
        .map_err(|e| PipelineError::EncodeError(format!("Dialect config error: {}", e)))?;
    let grammar = Grammar::from_language_dialect(language, dialect)
        .map_err(|e| PipelineError::EncodeError(format!("Grammar error: {}", e)))?;

    // 2. Build Lexicon (cover words).
    let cover_wl = cover_override.unwrap_or_else(|| dialect_config.cover_wordlist());
    // For CS grammars with cross-language payload (e.g., sig_bip39 uses English BIP39
    // words in CS armor), skip the payload/cover overlap filter. CS cover words are
    // structural (uppercase, function-word POS) and won't be confused with payload.
    let skip_filter = !grammar.dot_is_punctuation()
        && dialect_config.payload_language() != language;
    if verbose {
        eprintln!("Cover filter: dot_is_punct={}, payload_lang={}, grammar_lang={}, skip_filter={}",
            grammar.dot_is_punctuation(), dialect_config.payload_language(), language, skip_filter);
    }
    if verbose && skip_filter {
        eprintln!("Cover filter: using empty filter set (cross-language payload)");
    }
    let cover_filter_set = if skip_filter {
        HashSet::new()
    } else {
        payload_word_set.clone()
    };
    let (cover_by_pos, refined_cover) =
        load_cover_words_by_pos_for_wordlist(&cover_filter_set, language, cover_wl);

    // Use cover_filter_set for the Lexicon's internal payload filter too.
    // When skip_filter is true (cross-language payload), this is empty so that
    // cover words like BEGIN/END are not rejected by pick_cover/pick_cover_refined.
    let mut lex = Lexicon::new(cover_filter_set.clone(), cover_filter_set.clone());
    for (pos, words) in cover_by_pos {
        lex = lex.with_words(pos, &words.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
    lex = lex.with_refined_cover(refined_cover);

    // Optional semantic model: softly biases sentence planning toward coherent
    // verb-argument pairings. Absent for languages without a semantics.yaml, in
    // which case planning is unchanged. Never affects payload order or decoding.
    if let Some(model) = crate::generator::data::load_semantics(language) {
        lex = lex.with_semantics(std::sync::Arc::new(model));
    }

    // 3. Grammar-derived sentence parameters.

    let zone_toks = &payload_toks[word_range];
    let min_k = grammar.min_sentence_length().unwrap_or(5);
    // CS-style grammars (dot_is_punctuation=false) need exact k sizing because the
    // sentence structure is deterministic (HEADER BODY FOOTER with fixed cover slots).
    // Human-language grammars (dot_is_punctuation=true) use a configurable range.
    let is_cs_grammar = !grammar.dot_is_punctuation();
    let (k_min, k_max) = if is_cs_grammar {
        let k = min_k + zone_toks.len().saturating_sub(1);
        (k, k)
    } else {
        let default_k_min = 5;
        let default_k_max = 12;
        (k_min_override.unwrap_or(default_k_min), k_max_override.unwrap_or(default_k_max))
    };

    // 3. Generate text.
    let mut rng = StdRng::seed_from_u64(seed);
    let mode = if dialect.starts_with("subject") {
        GenerationMode::Subject
    } else {
        GenerationMode::Body
    };
    let length_mode = length_mode.unwrap_or_else(|| match mode {
        GenerationMode::Subject => SentenceLengthMode::Compact,
        GenerationMode::Body => SentenceLengthMode::Natural,
        GenerationMode::PayloadOnly => SentenceLengthMode::Compact,
    });
    let (text, payload_set) = generate_text_with_original_payload(
        &mut rng,
        &lex,
        zone_toks,
        None,
        verbose,
        mode,
        language,
        Some(dialect),
        k_min,
        k_max,
        length_mode,
        " ",
    );

    Ok((text, payload_set))
}

/// Check if a dialect is a prose-in-email composition dialect.
///
/// These dialects use two-pass pipeline composition: prose content (English/Latin)
/// is generated separately for subject and body zones, then assembled into an
/// email template.
fn is_prose_email_dialect(dialect: &str) -> bool {
    matches!(dialect, "email" | "email_alt" | "email_mime")
}

/// Determine how many payload words go into the subject line.
///
/// The subject gets up to 6 words (enough for a natural-sounding phrase).
/// If the total is 6 or fewer, all words go in the subject (no body).
fn compute_subject_word_count(total: usize) -> usize {
    if total <= 6 { total } else { 6 }
}

/// Assemble an email from subject and body prose using hardcoded templates.
///
/// Templates mirror the CS email dialect structure but with prose content
/// instead of base64 gibberish. Header values (From, To, Date) use fixed
/// placeholders that contain no BIP39 words.
fn assemble_email(dialect: &str, subject: &str, body: &str) -> String {
    match dialect {
        "email_alt" => format!(
            "From: <sender@glossia.local>\r\n\
             To: <recipient@glossia.local>\r\n\
             Date: Thu, 01 Jan 2026 00:00:00 +0000\r\n\
             Subject: {subject}\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: multipart/alternative; boundary=\"glossia-alt\"\r\n\
             \r\n\
             --glossia-alt\r\n\
             Content-Type: text/plain; charset=\"UTF-8\"\r\n\
             \r\n\
             {body}\r\n\
             \r\n\
             --glossia-alt\r\n\
             Content-Type: text/html; charset=\"UTF-8\"\r\n\
             \r\n\
             <html><body></body></html>\r\n\
             \r\n\
             --glossia-alt--"
        ),
        "email_mime" => format!(
            "From: <sender@glossia.local>\r\n\
             To: <recipient@glossia.local>\r\n\
             Date: Thu, 01 Jan 2026 00:00:00 +0000\r\n\
             Subject: {subject}\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: multipart/mixed; boundary=\"glossia\"\r\n\
             \r\n\
             --glossia\r\n\
             Content-Type: text/plain; charset=\"UTF-8\"\r\n\
             \r\n\
             {body}\r\n\
             \r\n\
             --glossia\r\n\
             Content-Type: application/octet-stream\r\n\
             Content-Disposition: attachment; filename=\"payload.bin\"\r\n\
             \r\n\
             \r\n\
             --glossia--"
        ),
        // "email" (text/plain) is the default
        _ => format!(
            "From: <sender@glossia.local>\r\n\
             To: <recipient@glossia.local>\r\n\
             Date: Thu, 01 Jan 2026 00:00:00 +0000\r\n\
             Subject: {subject}\r\n\
             Content-Type: text/plain; charset=\"UTF-8\"\r\n\
             \r\n\
             {body}"
        ),
    }
}

/// Compose prose content inside an email structure (two-pass pipeline).
///
/// This is the entry point for `english/bip39/email`, `latin/default/email`, etc.
/// It encodes the input into payload words once, splits them between subject and
/// body zones, generates prose for each zone using the content language's grammar,
/// and assembles the result into an email template.
fn compose_email(
    input: &str,
    language: &str,
    wordlist: &str,
    dialect: &str,
    forced_data_mode: Option<DataMode>,
    seed: u64,
    verbose: bool,
    cover_override: Option<&str>,
    length_mode: Option<SentenceLengthMode>,
    k_min_override: Option<usize>,
    k_max_override: Option<usize>,
) -> Result<(String, HashSet<String>, Vec<String>, DataMode), PipelineError> {
    // Stage 1: shared — bytes → POS-tagged payload tokens.
    let (payload_toks, encoded_words, data_mode, wordlist_set) =
        prepare_payload(input, language, wordlist, "body", forced_data_mode)?;

    let total = payload_toks.len();
    let subject_count = compute_subject_word_count(total);

    // Stage 2a: generate subject prose.
    let (subject_text, subject_set) = generate_prose_for_zone(
        &payload_toks,
        0..subject_count,
        language,
        wordlist,
        "subject",
        &wordlist_set,
        seed,
        verbose,
        cover_override,
        Some(SentenceLengthMode::Compact),
        k_min_override,
        k_max_override,
    )?;

    // Stage 2b: generate body prose (remaining words).
    let (body_text, body_set) = if subject_count < total {
        generate_prose_for_zone(
            &payload_toks,
            subject_count..total,
            language,
            wordlist,
            "body",
            &wordlist_set,
            seed.wrapping_add(1),
            verbose,
            cover_override,
            length_mode,
            k_min_override,
            k_max_override,
        )?
    } else {
        (String::new(), HashSet::new())
    };

    // Stage 3: assemble email.
    let email = assemble_email(dialect, &subject_text, &body_text);

    // Merge payload sets from both zones.
    let mut payload_set = subject_set;
    payload_set.extend(body_set);

    Ok((email, payload_set, encoded_words, data_mode))
}

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
    length_mode: Option<SentenceLengthMode>,
    k_min_override: Option<usize>,
    k_max_override: Option<usize>,
) -> Result<(String, HashSet<String>, Vec<String>, DataMode), PipelineError> {
    // Resolve "default" wordlist to actual name (e.g., "bip39" for English).
    let wordlist = resolve_wordlist_name(language, wordlist);

    // Prose-in-email composition: route to two-pass pipeline.
    // cs/base64/email uses the existing CS grammar path (unchanged).
    if is_prose_email_dialect(dialect) && language != "cs" {
        return compose_email(
            input, language, wordlist, dialect, forced_data_mode,
            seed, verbose, cover_override, length_mode,
            k_min_override, k_max_override,
        );
    }

    // Auto-detect subject prefix from input (Re: / Fwd:).
    // When dialect is "subject", check if input starts with a known prefix,
    // strip it, and route to the appropriate dialect variant (subject_re / subject_fwd).
    // This mirrors how CS grammar routes to pgp/nip04 based on explicit dialect choice,
    // but here the "choice" is inferred from the input content.
    let (input, dialect) = if dialect == "subject" {
        let trimmed = input.trim_start();
        if trimmed.to_lowercase().starts_with("re:") {
            (&trimmed[3..].trim_start() as &str, "subject_re")
        } else if trimmed.to_lowercase().starts_with("fwd:") {
            (&trimmed[4..].trim_start() as &str, "subject_fwd")
        } else {
            (input, dialect)
        }
    } else {
        (input, dialect)
    };

    // Stage 1: shared — input → payload tokens.
    let (payload_toks, encoded_words, data_mode, wordlist_set) =
        prepare_payload(input, language, wordlist, dialect, forced_data_mode)?;

    // Stage 2: full payload → prose.
    let (text, payload_set) = generate_prose_for_zone(
        &payload_toks,
        0..payload_toks.len(),
        language,
        wordlist,
        dialect,
        &wordlist_set,
        seed,
        verbose,
        cover_override,
        length_mode,
        k_min_override,
        k_max_override,
    )?;

    Ok((text, payload_set, encoded_words, data_mode))
}

/// Best-of-N generation: sample `n_candidates` full encodings at consecutive
/// seeds and return the best under a lexicographic objective — highest payload
/// density first, and among candidates within `DENSITY_TOL` of that density, the
/// highest semantic coherence.
///
/// This is the "grammar proposes, objective disposes" design: each candidate is
/// a complete, grammatical, correctly-ordered encoding; we merely select among
/// them. Because every candidate preserves the payload words in order, selecting
/// never affects decoding — it only trades cover-word choices. Density is never
/// sacrificed beyond `DENSITY_TOL`, honoring density-primacy.
///
/// Falls back to a single encode when `n_candidates <= 1` or the language has no
/// semantic model (nothing to rank coherence by).
#[allow(clippy::too_many_arguments)]
pub fn encode_into_language_best_of(
    input: &str,
    language: &str,
    wordlist: &str,
    dialect: &str,
    forced_data_mode: Option<DataMode>,
    base_seed: u64,
    verbose: bool,
    cover_override: Option<&str>,
    length_mode: Option<SentenceLengthMode>,
    k_min_override: Option<usize>,
    k_max_override: Option<usize>,
    n_candidates: usize,
) -> Result<(String, HashSet<String>, Vec<String>, DataMode), PipelineError> {
    // At most this much payload density (fraction of total words) may be given up
    // in exchange for higher coherence. Small: density stays primary.
    const DENSITY_TOL: f64 = 0.02;

    let n = n_candidates.max(1);
    let model = crate::generator::data::load_semantics(language);
    if n == 1 || model.is_none() {
        return encode_into_language(
            input, language, wordlist, dialect, forced_data_mode, base_seed, verbose,
            cover_override, length_mode, k_min_override, k_max_override,
        );
    }
    let model = model.unwrap();

    let mut candidates: Vec<(f64, f64, (String, HashSet<String>, Vec<String>, DataMode))> =
        Vec::with_capacity(n);
    for k in 0..n {
        let seed = base_seed.wrapping_add(k as u64);
        let res = encode_into_language(
            input, language, wordlist, dialect, forced_data_mode, seed, verbose,
            cover_override, length_mode, k_min_override, k_max_override,
        )?;
        let total = res.0.split_whitespace().count().max(1);
        let density = res.2.len() as f64 / total as f64;
        let coherence = model.coherence_score(&res.0);
        candidates.push((density, coherence, res));
    }

    let max_density = candidates.iter().map(|c| c.0).fold(f64::MIN, f64::max);
    let best = candidates
        .into_iter()
        .filter(|c| c.0 >= max_density - DENSITY_TOL)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .expect("at least one candidate generated");
    Ok(best.2)
}

/// Strip RFC 5322 header label names from text to prevent header/payload collisions.
///
/// When prose-in-email content is decoded, the email header labels (like "Subject:")
/// get trimmed to bare words (e.g., "subject") which may collide with payload words.
/// This function detects email headers and replaces each header line with just its
/// value, preserving any payload words embedded in header values (e.g., the Subject line).
///
/// Activation heuristic: the text must start with a recognized RFC 5322 header
/// (From:, To:, Date:, Received:). Regular prose never matches this pattern.
fn strip_email_header_labels(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("");
    let first_lower = first_line.to_lowercase();
    // Only activate for text that looks like an email (starts with a standard header).
    if !first_lower.starts_with("from:")
        && !first_lower.starts_with("to:")
        && !first_lower.starts_with("date:")
        && !first_lower.starts_with("received:")
    {
        return text.to_string();
    }

    let mut result = String::new();
    let mut in_headers = true;

    for line in text.lines() {
        if in_headers {
            if line.trim().is_empty() {
                // Blank line separates headers from body.
                in_headers = false;
                result.push('\n');
            } else if line.starts_with(' ') || line.starts_with('\t') {
                // Folded header continuation — keep value as-is.
                result.push_str(line.trim_start());
                result.push('\n');
            } else if let Some(colon_pos) = line.find(':') {
                // Header line: strip the label, keep the value.
                let value = line[colon_pos + 1..].trim_start();
                if !value.is_empty() {
                    result.push_str(value);
                    result.push('\n');
                }
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Render decoded bytes into a textual representation.
///
/// `Some(mode)` forces the representation requested by the pipeline target:
/// `Hex` → lowercase hex, `Base64` → base64, `Ascii7`/`Bytes8` → UTF-8 (lossy).
/// `None` keeps the back-compatible auto behavior: the UTF-8 string when the
/// bytes are valid UTF-8, otherwise lowercase hex.
fn render_decoded_bytes(bytes: &[u8], format: Option<DataMode>) -> String {
    match format {
        Some(DataMode::Hex) => codec::hex_encode(bytes),
        Some(DataMode::Base64) => codec::base64_encode(bytes),
        Some(DataMode::Ascii7) | Some(DataMode::Bytes8) => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        None => String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| codec::hex_encode(bytes)),
    }
}

/// Decode language prose back to the original data string.
///
/// Prose → extract payload words (filter out cover words) → binary → string.
pub fn decode_from_language(
    text: &str,
    language: &str,
    wordlist: &str,
    verbose: bool,
) -> Result<String, PipelineError> {
    decode_from_language_with_payload_lang(text, language, language, wordlist, verbose, "body", None)
}

/// Decode extracted payload words to bytes, resolving the bare-vs-framed ambiguity.
///
/// Framed Glossia prose carries a leading padding-count word and is interspersed
/// with cover/function words. When cover words are present (`payload_words <
/// total_tokens`) the input is unambiguously framed and decodes through the
/// grammar's declared codec (`bitpack` for power-of-two wordlists).
///
/// When *every* input token is a payload word (`payload_words == total_tokens`)
/// the input is ambiguous: it may be a cover-hidden Glossia payload sequence —
/// whose leading padding word still makes `bitpack` valid — or a *bare* mnemonic
/// typed directly, which has no padding word. Crucially these are distinguishable
/// at decode time: `bitpack` reads the first word as a padding count and rejects
/// it unless it is `< bits_per_word` AND leaves byte-aligned data (see
/// `decode_bitpack`). A bare mnemonic's first word is real data, so it almost
/// always fails one of those checks. So we *try* `bitpack` first and fall back to
/// `base_n` only when it genuinely can't decode — recovering cover-hidden payload
/// sequences without breaking bare mnemonics (issue #23).
fn decode_extracted_words(
    extracted: &[String],
    tree: &WordlistTree,
    grammar_codec: &str,
    all_payload: bool,
) -> Result<Vec<u8>, codec::DecodeError> {
    if grammar_codec == "bitpack" && all_payload && !extracted.is_empty() {
        codec::decode_base_n(extracted, tree, "bitpack")
            .or_else(|_| codec::decode_base_n(extracted, tree, "base_n"))
    } else {
        codec::decode_base_n(extracted, tree, grammar_codec)
    }
}

/// Count whitespace-delimited input tokens that survive alphanumeric trimming.
///
/// Used to detect all-payload input: when this equals the number of extracted
/// payload words, the input carries no cover words (see
/// [`decode_extracted_words`]).
fn alnum_token_count(text: &str) -> usize {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .count()
}

/// Internal decode with separate grammar language and payload language.
///
/// `grammar_lang` determines which grammar.yaml to load (for payload_separator).
/// `grammar_dialect` determines which dialect to load (for dialect-specific overrides).
/// `payload_lang` determines which directory to load payload wordlist files from.
fn decode_from_language_with_payload_lang(
    text: &str,
    grammar_lang: &str,
    payload_lang: &str,
    wordlist: &str,
    verbose: bool,
    grammar_dialect: &str,
    format: Option<DataMode>,
) -> Result<String, PipelineError> {
    // Strip email header labels if the input looks like an RFC 5322 message.
    // This prevents header names like "Subject" from colliding with payload words.
    let text = strip_email_header_labels(text);
    let text = text.as_str();

    // Resolve "default" wordlist to actual name (e.g., "bip39" for English).
    let wordlist = if wordlist == "default" {
        let dw = default_wordlist(payload_lang);
        if dw == "default" { wordlist } else { dw }
    } else {
        wordlist
    };

    // 1. Load payload wordlist (cached — see cached_payload_tree).
    let payload_tree = cached_payload_tree(payload_lang, wordlist)?;

    // 2. Grammar tells us how payload words are separated.
    let grammar = Grammar::from_language_dialect(grammar_lang, grammar_dialect)
        .map_err(|e| PipelineError::DecodeError(format!("Grammar error: {}", e)))?;
    let payload_separator = grammar.payload_separator();

    // For CS-style grammars with word-level payload (e.g., sig_bip39), strip
    // ASCII armor header/footer lines before extracting payload. These lines
    // (starting with "-----") contain structural words like "BEGIN"/"END" that
    // may collide with payload words.
    let text = if !grammar.dot_is_punctuation() && !payload_separator.is_empty() {
        let stripped: String = text.lines()
            .filter(|line| !line.trim_start().starts_with("-----"))
            .collect::<Vec<_>>()
            .join("\n");
        std::borrow::Cow::Owned(stripped)
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    let text: &str = &text;

    // 3. Extract payload words from prose.
    let extracted: Vec<String> = if payload_separator.is_empty() {
        // Concatenated payload (CS grammar): chars from pure-payload blocks.
        let payload_set: HashSet<String> = payload_tree.words().iter()
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

    // 4. Payload words → binary → data string (base-N decoding).
    // For the grammar bitpack codec, an all-payload input (no cover words) may be
    // a cover-hidden Glossia sequence (valid bitpack) or a bare mnemonic (no
    // padding word); decode_extracted_words tries bitpack then falls back to
    // base_n (issue #23).
    let all_payload = !payload_separator.is_empty() && alnum_token_count(text) == extracted.len();
    let bytes = decode_extracted_words(&extracted, &*payload_tree, grammar.codec(), all_payload)
        .map_err(|e| PipelineError::DecodeError(format!("{}", e)))?;
    Ok(render_decoded_bytes(&bytes, format))
}

/// Decode language prose, returning both the decoded text and extracted payload words.
pub fn decode_from_language_rich(
    text: &str,
    language: &str,
    wordlist: &str,
    verbose: bool,
) -> Result<(String, Vec<String>), PipelineError> {
    decode_from_language_rich_with_payload_lang(text, language, language, wordlist, verbose, "body", None)
}

/// Internal rich decode with separate grammar language and payload language.
fn decode_from_language_rich_with_payload_lang(
    text: &str,
    grammar_lang: &str,
    payload_lang: &str,
    wordlist: &str,
    verbose: bool,
    grammar_dialect: &str,
    format: Option<DataMode>,
) -> Result<(String, Vec<String>), PipelineError> {
    // Strip email header labels if the input looks like an RFC 5322 message.
    let text = strip_email_header_labels(text);
    let text = text.as_str();

    // Resolve "default" wordlist to actual name.
    let wordlist = if wordlist == "default" {
        let dw = default_wordlist(payload_lang);
        if dw == "default" { wordlist } else { dw }
    } else {
        wordlist
    };

    // 1. Load payload wordlist (cached — see cached_payload_tree).
    let payload_tree = cached_payload_tree(payload_lang, wordlist)?;

    // 2. Grammar tells us how payload words are separated.
    let grammar = Grammar::from_language_dialect(grammar_lang, grammar_dialect)
        .map_err(|e| PipelineError::DecodeError(format!("Grammar error: {}", e)))?;
    let payload_separator = grammar.payload_separator();

    // For CS-style grammars with word-level payload, strip ASCII armor lines.
    let text = if !grammar.dot_is_punctuation() && !payload_separator.is_empty() {
        let stripped: String = text.lines()
            .filter(|line| !line.trim_start().starts_with("-----"))
            .collect::<Vec<_>>()
            .join("\n");
        std::borrow::Cow::Owned(stripped)
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    let text: &str = &text;

    // 3. Extract payload words from prose.
    let extracted: Vec<String> = if payload_separator.is_empty() {
        let payload_set: HashSet<String> = payload_tree.words().iter()
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

    // 4. Payload words → binary → data string (base-N decoding).
    // For the grammar bitpack codec, an all-payload input (no cover words) may be
    // a cover-hidden Glossia sequence (valid bitpack) or a bare mnemonic (no
    // padding word); decode_extracted_words tries bitpack then falls back to
    // base_n (issue #23).
    let all_payload = !payload_separator.is_empty() && alnum_token_count(text) == extracted.len();
    let bytes = decode_extracted_words(&extracted, &*payload_tree, grammar.codec(), all_payload)
        .map_err(|e| PipelineError::DecodeError(format!("{}", e)))?;
    let decoded = render_decoded_bytes(&bytes, format);

    Ok((decoded, extracted))
}

// ═══════════════════════════════════════════════════════════════════════
// Crypto encode/decode helpers
// ═══════════════════════════════════════════════════════════════════════

/// The bech32 character set (index → char).
const BECH32_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// Map a bech32 character to its 5-bit value.
fn bech32_char_to_value(c: char) -> Option<u8> {
    let lower = c.to_ascii_lowercase() as u8;
    BECH32_CHARSET.iter().position(|&b| b == lower).map(|i| i as u8)
}

/// Map a 5-bit value to a bech32 character.
fn bech32_value_to_char(v: u8) -> Option<char> {
    BECH32_CHARSET.get(v as usize).map(|&b| b as char)
}

/// Pack a slice of 5-bit values into a byte vector.
/// Format: [count_byte] [packed_data_bytes...]
/// The count byte stores the number of 5-bit symbols so the round-trip
/// can reproduce the exact character count (handles non-byte-aligned data).
fn pack_5bit_values(values: &[u8]) -> Vec<u8> {
    let total_bits = values.len() * 5;
    let num_data_bytes = (total_bits + 7) / 8;
    let mut packed = vec![0u8; 1 + num_data_bytes];
    packed[0] = values.len() as u8;

    let mut bit_pos = 0usize;
    for &v in values {
        for bit_offset in (0..5).rev() {
            let byte_idx = 1 + bit_pos / 8;
            let bit_idx = 7 - (bit_pos % 8);
            if (v >> bit_offset) & 1 == 1 {
                packed[byte_idx] |= 1 << bit_idx;
            }
            bit_pos += 1;
        }
    }
    packed
}

/// Unpack a byte vector (from pack_5bit_values) into 5-bit values.
fn unpack_5bit_values(packed: &[u8]) -> Result<Vec<u8>, PipelineError> {
    if packed.is_empty() {
        return Err(PipelineError::EncodeError("empty packed data".into()));
    }
    let num_chars = packed[0] as usize;
    let data = &packed[1..];

    let mut values = Vec::with_capacity(num_chars);
    let mut bit_pos = 0usize;
    for _ in 0..num_chars {
        let mut value: u8 = 0;
        for bit_offset in (0..5).rev() {
            let byte_idx = bit_pos / 8;
            let bit_idx = 7 - (bit_pos % 8);
            if byte_idx < data.len() && (data[byte_idx] >> bit_idx) & 1 == 1 {
                value |= 1 << bit_offset;
            }
            bit_pos += 1;
        }
        values.push(value);
    }
    Ok(values)
}

/// Decode a cryptocurrency address/token to a hex data string.
///
/// For bech32 dialects: strips prefix, converts 5-bit bech32 chars to packed
/// bytes (with a length prefix for exact round-trip), returns hex.
/// For base58/base64url: strips prefix and returns the data as-is.
fn decode_crypto(
    input: &str,
    language: &str,
    dialect: &str,
    verbose: bool,
) -> Result<String, PipelineError> {
    // Strip prefix to get data portion.
    let data_part = if let Some((_, _, data)) = detect_crypto_format(input) {
        data
    } else {
        input
    };

    if data_part.is_empty() {
        return Err(PipelineError::DecodeError(
            "empty data portion after prefix removal".to_string(),
        ));
    }

    let wl = crypto_payload_type(language, dialect);

    if verbose {
        eprintln!("Crypto decode: {} data chars from {}/{}", data_part.len(), language, dialect);
    }

    match wl {
        "bech32" => {
            // Bech32: convert characters → 5-bit values → packed bytes → hex.
            let values: Vec<u8> = data_part.to_lowercase().chars()
                .map(|c| bech32_char_to_value(c)
                    .ok_or_else(|| PipelineError::DecodeError(
                        format!("invalid bech32 char: {}", c))))
                .collect::<Result<_, _>>()?;
            let packed = pack_5bit_values(&values);
            Ok(packed.iter().map(|b| format!("{:02x}", b)).collect())
        }
        "base58" | "base64url" => {
            // Treat as ASCII string — the codec auto-detects on the encode side.
            Ok(data_part.to_string())
        }
        _ => Ok(data_part.to_string()),
    }
}

/// Encode a data string into a cryptocurrency address/token.
///
/// For bech32 dialects: hex → bytes → unpack 5-bit values → bech32 chars → prefix.
/// For base58/base64url: prefix + data string.
fn encode_crypto(
    input: &str,
    language: &str,
    dialect: &str,
    verbose: bool,
) -> Result<String, PipelineError> {
    let wl = crypto_payload_type(language, dialect);
    let prefix = crypto_dialect_prefix(language, dialect);

    if verbose {
        eprintln!("Crypto encode: input {} chars into {}/{}", input.len(), language, dialect);
    }

    match wl {
        "bech32" => {
            // Hex → bytes → unpack 5-bit values → bech32 characters.
            let packed = codec::hex_decode(input)
                .ok_or_else(|| PipelineError::EncodeError(
                    "crypto encode: expected hex input for bech32 dialect".into()))?;
            let values = unpack_5bit_values(&packed)?;
            let chars: String = values.iter()
                .map(|&v| bech32_value_to_char(v)
                    .ok_or_else(|| PipelineError::EncodeError(
                        format!("invalid bech32 5-bit value: {}", v))))
                .collect::<Result<_, _>>()?;
            Ok(format!("{}{}", prefix, chars))
        }
        "base58" | "base64url" => {
            Ok(format!("{}{}", prefix, input))
        }
        _ => Ok(format!("{}{}", prefix, input)),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── cached_payload_tree() ────────────────────────────────────────

    #[test]
    fn test_cached_payload_tree_returns_shared_instance() {
        let wl = crate::generator::data::default_wordlist("english");
        let a = cached_payload_tree("english", wl).unwrap();
        let b = cached_payload_tree("english", wl).unwrap();
        // Second call must hit the cache and hand back the same Arc.
        assert!(Arc::ptr_eq(&a, &b));
    }

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
    fn test_parse_spells_resolves_hp_wordlist() {
        // "encode into latin spells" should resolve payload_wordlist: "hp" from grammar.yaml
        let p = Pipeline::from_meta("encode into latin spells").unwrap();
        assert_eq!(p.target, Endpoint::language_full("latin", "hp", "spells"),
            "spells dialect should resolve payload_wordlist to 'hp' from grammar.yaml");
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
    fn test_cover_hidden_payload_round_trip() {
        // The API Key demo encodes via the body dialect (bitpack, padding word).
        // Its cover-hidden payload-only sequence is all payload words (like a bare
        // mnemonic) but carries a leading padding word, so decode must try bitpack
        // before base_n. Verify both full prose and payload-only decode back, and
        // that a leading-0x00 key survives (the reason we keep the padding word).
        let enc = Pipeline::from_meta("encode into english naturally").unwrap();
        let dec = Pipeline::from_meta("transcode from english/bip39/body into ascii7").unwrap();
        let dec_hex = Pipeline::from_meta("transcode from english/bip39/body into hex").unwrap();
        let tree = cached_payload_tree("english", "bip39").unwrap();
        let strip_cover = |prose: &str| -> String {
            prose.split_whitespace()
                .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
                .filter(|w| !w.is_empty() && tree.contains(&w.to_lowercase()))
                .collect::<Vec<_>>()
                .join(" ")
        };

        let key = "sk-1SYMaH9HEJ5CHFMDQd5GoBtjRQzwOPQK";
        let prose = enc.execute(key).unwrap();
        assert_eq!(dec.execute(&prose).unwrap(), key, "full prose must round-trip");
        assert_eq!(dec.execute(&strip_cover(&prose)).unwrap(), key, "cover-hidden must round-trip");

        // A hex key with a leading zero byte: bitpack preserves it exactly.
        let hex = "00abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456700";
        let enc_hex = Pipeline::from_meta("transcode from hex into english/bip39/body").unwrap();
        let hprose = enc_hex.execute(hex).unwrap();
        assert_eq!(dec_hex.execute(&hprose).unwrap(), hex, "leading-zero hex must round-trip");
        assert_eq!(dec_hex.execute(&strip_cover(&hprose)).unwrap(), hex, "cover-hidden leading-zero hex");
    }

    #[test]
    fn test_bip39_dialect_arbitrary_key_round_trip() {
        // The API Key demo encodes arbitrary-length keys through the bip39 dialect
        // (base_n, no padding word) so the cover-hidden payload-only sequence is a
        // clean, round-trippable data sequence. Verify both the full prose and the
        // payload-only word list decode back to the original key.
        let key = "sk-1SYMaH9HEJ5CHFMDQd5GoBtjRQzwOPQK";
        let enc = Pipeline::from_meta("encode into english bip39").unwrap();
        let prose = enc.execute(key).unwrap();

        let dec = Pipeline::from_meta("transcode from english/bip39/bip39 into ascii7").unwrap();
        assert_eq!(dec.execute(&prose).unwrap(), key, "full prose must round-trip");

        // Strip cover words → the cover-hidden payload-only view.
        let tree = cached_payload_tree("english", "bip39").unwrap();
        let bare: String = prose
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty() && tree.contains(&w.to_lowercase()))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(dec.execute(&bare).unwrap(), key, "cover-hidden payload must round-trip");
    }

    #[test]
    fn test_encode_into_language_treats_payload_words_as_text() {
        // #29: `encode into <lang>` always treats its input as raw data, never
        // guessing that payload-word input is a seed/prose to transcode. So an
        // ASCII phrase whose words happen to be payload words — and even a full
        // bare BIP39 mnemonic — is encoded as text and round-trips verbatim,
        // instead of being silently transcoded to a (shorter) byte value.
        let enc = Pipeline::from_meta("encode into english naturally").unwrap();
        let dec = Pipeline::from_meta("decode from english").unwrap();
        let cases = [
            "absorb absurd access",
            "zoo zone zero",
            // A genuine, valid-checksum 12-word mnemonic is still just text here.
            "abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon about",
        ];
        for text in cases {
            let rich = enc.execute_rich(text).unwrap();
            assert!(
                matches!(rich.resolved_source, Some(Endpoint::Format(_))),
                "input must resolve to a data Format, not a seed/prose source: {:?}",
                rich.resolved_source
            );
            let prose = enc.execute(text).unwrap();
            let back = dec.execute(&prose).unwrap();
            assert_eq!(back, text, "payload-word input must round-trip as text, got {:?}", back);
        }
    }

    #[test]
    fn test_explicit_raw_source_still_transcodes_seed() {
        // Removing the encode-into carve-out does not touch the explicit path the
        // demo uses: naming a `raw` source re-encodes a bare mnemonic as a seed,
        // compacting it into a denser language (here English → Latin → English).
        let mnemonic = "abandon abandon abandon abandon abandon abandon \
                        abandon abandon abandon abandon abandon about";
        let to_latin = Pipeline::from_meta("transcode from english/bip39/raw into latin/default/raw").unwrap();
        let back = Pipeline::from_meta("transcode from latin/default/raw into english/bip39/raw").unwrap();
        let latin = to_latin.execute(mnemonic).unwrap();
        assert_eq!(
            back.execute(&latin).unwrap(),
            mnemonic,
            "explicit raw transcode must round-trip the seed words"
        );
    }

    #[test]
    fn test_verb_parsed_from_meta() {
        assert_eq!(Pipeline::from_meta("encode into latin").unwrap().verb, Verb::Encode);
        assert_eq!(Pipeline::from_meta("transcode from english into latin").unwrap().verb, Verb::Transcode);
        assert_eq!(Pipeline::from_meta("translate from english into latin").unwrap().verb, Verb::Transcode);
        assert_eq!(Pipeline::from_meta("decode from english").unwrap().verb, Verb::Decode);
        // Shorthand with no verb stays Unspecified (endpoint-inferred direction).
        assert_eq!(Pipeline::from_meta("latin").unwrap().verb, Verb::Unspecified);
    }

    #[test]
    fn test_transcode_verb_auto_detects_source() {
        // The verb is authoritative: `transcode into <lang>` with no explicit
        // `from` treats the input as an existing encoding, auto-detects its source
        // dialect, and re-encodes. A bare BIP39 mnemonic is detected as
        // english/bip39 and compacted into Latin, then round-trips back to the
        // seed words. This is the seed-compaction convenience, now gated by the
        // verb the user typed rather than by guessing at the input's content.
        let mnemonic = "abandon abandon abandon abandon abandon abandon \
                        abandon abandon abandon abandon abandon about";
        let to_latin = Pipeline::from_meta("transcode into latin").unwrap();
        let latin = to_latin.execute(mnemonic).unwrap();
        let back = Pipeline::from_meta("transcode from latin into english/bip39/raw").unwrap();
        assert_eq!(
            back.execute(&latin).unwrap(),
            mnemonic,
            "transcode-verb auto-detect must round-trip the seed words"
        );
    }

    #[test]
    fn test_transcode_verb_errors_without_detectable_source() {
        // `transcode into <lang>` declares "the input is an existing encoding".
        // If none is detected, error and ask for an explicit source rather than
        // silently encoding the input as text.
        let pipe = Pipeline::from_meta("transcode into english").unwrap();
        let err = pipe.execute("xyzzy plugh frobnicate").unwrap_err();
        assert!(
            matches!(err, PipelineError::InvalidPipeline(ref m) if m.contains("could not detect")),
            "expected a 'could not detect source' error, got {:?}",
            err
        );
    }

    #[test]
    fn test_raw_encode_decode_round_trip() {
        // Regression: the "raw" dialect must use the SAME codec on encode and
        // decode. Previously raw-encode used bitpack (prepending a padding-count
        // word) while raw-decode used base_n, so the bytes did not survive the
        // round-trip and the output carried a spurious leading word.
        let input = "cafe"; // hex → bytes 0xca 0xfe

        let encode = Pipeline::from_params(
            Endpoint::Format(DataMode::Hex),
            Endpoint::language_full("english", "bip39", "raw"),
        );
        let bare_words = encode.execute(input).unwrap();

        // base-N of 0xcafe is exactly 2 words; bitpack would add a 3rd padding
        // word. Asserting the count locks in the "no header" property.
        assert_eq!(
            bare_words.split_whitespace().count(),
            2,
            "raw output must be bare data words with no padding header, got: {bare_words:?}"
        );

        let decode = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "raw"),
            Endpoint::Format(DataMode::Hex),
        );
        let decoded = decode.execute(&bare_words).unwrap();
        assert_eq!(decoded, input, "raw encode/decode must round-trip");
    }

    #[test]
    fn test_raw_decode_rich_returns_stats() {
        // Regression: do_decode_rich had no `raw` branch, so a rich decode of bare
        // payload words fell through to the prose decoder. It now decodes via
        // base-N and reports all-payload stats, agreeing with the plain do_decode.
        let bare = "add garlic"; // base-N of 0xcafe in english/bip39

        let pipeline = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "raw"),
            Endpoint::Format(DataMode::Hex),
        );

        let plain = pipeline.execute(bare).unwrap();
        let rich = pipeline.execute_rich(bare).unwrap();

        assert_eq!(rich.output, plain, "rich and plain raw decode must agree");
        assert_eq!(rich.output, "cafe");
        assert_eq!(rich.payload_words, vec!["add".to_string(), "garlic".to_string()]);
        let stats = rich.stats.expect("raw decode should report stats");
        assert_eq!(stats.payload_count, 2);
        assert_eq!(stats.cover_count, 0);
        assert_eq!(stats.total_words, 2);
        assert_eq!(stats.ratio, 1.0);
    }

    #[test]
    fn test_english_bip39_dialect_preserves_seed_words() {
        // The english "bip39" dialect uses base_n, so encoding a BIP39 seed weaves
        // the exact seed words into prose (with cover words) instead of re-chunking
        // them via bitpack. Verify every seed word survives and the seed round-trips.
        let seed = "abandon ability able about above absent absorb access accident account accuse achieve";

        let encode = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "raw"),
            Endpoint::language_full("english", "bip39", "bip39"),
        )
        .with_seed(42);
        let prose = encode.execute(seed).unwrap();

        // Every seed word appears verbatim in the prose (interspersed with cover).
        for w in seed.split_whitespace() {
            assert!(
                prose.split_whitespace().any(|t| {
                    t.trim_matches(|c: char| !c.is_alphanumeric())
                        .eq_ignore_ascii_case(w)
                }),
                "seed word {w:?} missing from english bip39 prose: {prose:?}"
            );
        }

        // Round-trips back to the exact seed (prose → bytes → raw seed words).
        let recovered = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "bip39"),
            Endpoint::language_full("english", "bip39", "raw"),
        )
        .execute(&prose)
        .unwrap();
        assert_eq!(
            recovered.split_whitespace().collect::<Vec<_>>(),
            seed.split_whitespace().collect::<Vec<_>>(),
            "english bip39 dialect must round-trip to the original seed"
        );
    }

    #[test]
    fn test_decode_honors_target_format() {
        // Regression for #28: decode into a Format endpoint must render the
        // recovered bytes in that format, not always UTF-8-else-hex.
        let key = "sk-test-ABC123xyz";

        let prose = Pipeline::from_params(
            Endpoint::Auto,
            Endpoint::language_full("english", "bip39", "body"),
        )
        .with_seed(42)
        .execute(key)
        .unwrap();

        let decode = |fmt: DataMode| {
            Pipeline::from_params(
                Endpoint::language_full("english", "bip39", "body"),
                Endpoint::Format(fmt),
            )
            .execute(&prose)
            .unwrap()
        };

        // hex target -> hex of the bytes (and decodes back to the key)
        let hex = decode(DataMode::Hex);
        assert_eq!(
            codec::hex_decode(&hex).expect("valid hex"),
            key.as_bytes(),
            "into hex must yield hex of the bytes, got {hex:?}"
        );

        // base64 target -> base64 of the bytes (and decodes back to the key)
        let b64 = decode(DataMode::Base64);
        assert_eq!(
            codec::base64_decode(&b64).expect("valid base64"),
            key.as_bytes(),
            "into base64 must yield base64 of the bytes, got {b64:?}"
        );

        // ascii target -> the original string
        assert_eq!(decode(DataMode::Ascii7), key, "into ascii must yield the string");

        // Auto (no explicit format) keeps the UTF-8-else-hex behavior.
        let auto = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "body"),
            Endpoint::Auto,
        )
        .execute(&prose)
        .unwrap();
        assert_eq!(auto, key, "Auto decode must be unchanged");
    }

    #[test]
    fn test_transcode_bare_mnemonic_english_to_latin() {
        // Regression for issue #23: bare payload words (a raw BIP39 mnemonic, not
        // Glossia-encoded prose) carry no bitpack padding header, so transcoding
        // them must not fail with "data_bits not byte-aligned". They decode as a
        // plain base-N integer, matching the `raw` dialect path.
        let bare = "abandon ability able about";

        let transcode = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "body"),
            Endpoint::language("latin"),
        )
        .with_seed(42);
        let latin = transcode
            .execute(bare)
            .expect("bare-word transcode should succeed, not error on byte alignment");
        assert!(!latin.is_empty(), "Latin prose should not be empty");

        // Byte-lossless: the Latin output must decode to the same bytes the bare
        // English words decode to directly.
        let from_english = Pipeline::from_params(
            Endpoint::language_full("english", "bip39", "body"),
            Endpoint::Auto,
        )
        .execute(bare)
        .unwrap();
        let from_latin = Pipeline::from_params(
            Endpoint::language("latin"),
            Endpoint::Auto,
        )
        .execute(&latin)
        .unwrap();
        assert_eq!(
            from_english, from_latin,
            "bare-word transcode must be byte-lossless"
        );
    }

    #[test]
    fn test_btc_encode_decode_rich_round_trip() {
        // Encode a bc1 address into English prose, then transcode back via execute_rich.
        let btc_addr = "bc1q29zsu7g0auf4f40avkcych9zzycd4ep3dvdxdgz";

        // Step 1: Encode bc1q → English prose (using execute_rich, same as WASM)
        let encode_pipeline = Pipeline::from_meta("encode into english naturally")
            .unwrap()
            .with_seed(42);
        let encode_result = encode_pipeline.execute_rich(btc_addr).unwrap();
        let prose = &encode_result.output;
        assert!(!prose.is_empty());

        // Verify resolved_source is crypto/btc
        let resolved = encode_result.resolved_source.as_ref().unwrap();
        assert!(format!("{}", resolved).starts_with("crypto/btc"),
            "resolved_source should be crypto/btc, got: {}", resolved);

        // Step 2: Transcode English prose → bc1q address (via execute_rich)
        let decode_pipeline = Pipeline::from_meta(
            "translate from english naturally into crypto/btc/bech32/bip173"
        ).unwrap().with_seed(42);
        let decode_result = decode_pipeline.execute_rich(prose).unwrap();

        assert_eq!(decode_result.output, btc_addr,
            "Round-trip should recover original bc1q address");
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
        assert_eq!(p.target, Endpoint::language_full("cs", "base58", "pgp"));
    }

    #[test]
    fn test_parse_html_flag_order_independent() {
        // "html" can appear before or after the language word
        let p = Pipeline::from_meta("encode into html pgp").unwrap();
        assert!(p.html, "html flag should be true regardless of order");
        assert_eq!(p.target, Endpoint::language_full("cs", "base58", "pgp"));
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
        assert_eq!(p.target, Endpoint::language_full("cs", "base58", "pgp"),
            "pgp should route to cs language with pgp dialect and base58 wordlist");
    }

    #[test]
    fn test_nostr_routes_to_cs_nip04_dialect() {
        let p = Pipeline::from_meta("encode into nostr").unwrap();
        assert_eq!(p.target, Endpoint::language_full("cs", "base64", "nip04"),
            "nostr should route to cs language with nip04 dialect and base64 wordlist");
    }

    // ── Adverb modifiers ────────────────────────────────────────────

    #[test]
    fn test_adverb_compactly() {
        let p = Pipeline::from_meta("encode hex into english compactly").unwrap();
        assert_eq!(p.length_mode, Some(SentenceLengthMode::Compact),
            "compactly should set length_mode to Compact");
    }

    #[test]
    fn test_adverb_efficiently() {
        let p = Pipeline::from_meta("translate from hex into latin efficiently").unwrap();
        assert_eq!(p.length_mode, Some(SentenceLengthMode::Compact),
            "efficiently should set length_mode to Compact");
        assert_eq!(p.k_max, Some(20),
            "efficiently should set k_max to 20");
    }

    #[test]
    fn test_adverb_naturally() {
        let p = Pipeline::from_meta("encode hex into english naturally").unwrap();
        assert_eq!(p.length_mode, Some(SentenceLengthMode::Natural),
            "naturally should set length_mode to Natural");
    }

    #[test]
    fn test_adverb_minimally() {
        let p = Pipeline::from_meta("encode into latin minimally").unwrap();
        assert_eq!(p.length_mode, Some(SentenceLengthMode::Compact),
            "minimally should set length_mode to Compact");
        assert_eq!(p.k_min, Some(2),
            "minimally should set k_min to 2");
        assert_eq!(p.k_max, Some(12),
            "minimally should set k_max to 12");
    }

    #[test]
    fn test_adverb_optimally() {
        let p = Pipeline::from_meta("encode hex into english optimally").unwrap();
        assert_eq!(p.variations, Some(50),
            "optimally should set variations to 50");
    }

    #[test]
    fn test_adverb_thoroughly() {
        let p = Pipeline::from_meta("translate from hex into latin thoroughly").unwrap();
        assert_eq!(p.variations, Some(100),
            "thoroughly should set variations to 100");
    }

    #[test]
    fn test_multiple_adverbs() {
        let p = Pipeline::from_meta("encode hex into english compactly optimally").unwrap();
        assert_eq!(p.length_mode, Some(SentenceLengthMode::Compact),
            "compactly should set length_mode");
        assert_eq!(p.variations, Some(50),
            "optimally should set variations");
    }

    #[test]
    fn test_adverb_with_dialect_modifier() {
        let p = Pipeline::from_meta("encode into latin prose compactly").unwrap();
        assert_eq!(p.target, Endpoint::language_with_dialect("latin", "prose"),
            "prose dialect should be preserved");
        assert_eq!(p.length_mode, Some(SentenceLengthMode::Compact),
            "compactly should set length_mode");
    }

    // ── Image language pipeline parsing ───────────────────────────────

    #[test]
    fn test_image_default_dialect_is_voronoi() {
        let p = Pipeline::from_meta("encode into image").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "viridis", "voronoi"),
            "image should default to voronoi dialect with viridis wordlist");
    }

    #[test]
    fn test_image_grid_dialect() {
        let p = Pipeline::from_meta("encode into image grid").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "viridis", "grid"),
            "grid dialect modifier should override default voronoi");
    }

    #[test]
    fn test_image_mosaic_dialect() {
        let p = Pipeline::from_meta("encode into image mosaic").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "viridis", "mosaic"));
    }

    #[test]
    fn test_image_constellation_dialect() {
        let p = Pipeline::from_meta("encode into image constellation").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "viridis", "constellation"));
    }

    #[test]
    fn test_image_raw_dialect() {
        let p = Pipeline::from_meta("encode into image raw").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "viridis", "raw"));
    }

    #[test]
    fn test_image_patches_dialect() {
        let p = Pipeline::from_meta("encode into image patches").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "viridis", "patches"));
    }

    #[test]
    fn test_image_transcode_from_english() {
        let p = Pipeline::from_meta("translate from english into image voronoi").unwrap();
        assert_eq!(p.source, Endpoint::language("english"));
        assert_eq!(p.target, Endpoint::language_full("image", "viridis", "voronoi"));
    }

    // ── Image palette (wordlist) selection ──────────────────────────

    #[test]
    fn test_image_palette_plasma_voronoi() {
        let p = Pipeline::from_meta("encode into image plasma voronoi").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "plasma", "voronoi"));
    }

    #[test]
    fn test_image_palette_rocket_grid() {
        let p = Pipeline::from_meta("encode into image rocket grid").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "rocket", "grid"));
    }

    #[test]
    fn test_image_palette_without_dialect() {
        let p = Pipeline::from_meta("encode into image magma").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "magma", "voronoi"),
            "palette without dialect should default to voronoi");
    }

    #[test]
    fn test_image_palette_order_dialect_first() {
        // "voronoi plasma" — dialect before palette
        let p = Pipeline::from_meta("encode into image voronoi plasma").unwrap();
        assert_eq!(p.target, Endpoint::language_full("image", "plasma", "voronoi"));
    }

    // ── Crypto language pipeline parsing ─────────────────────────────

    #[test]
    fn test_btc_routes_to_crypto_btc_bip173() {
        let p = Pipeline::from_meta("encode into btc").unwrap();
        assert_eq!(p.target, Endpoint::language_with_dialect("crypto/btc", "bip173"),
            "btc should route to crypto/btc language with bip173 dialect");
    }

    #[test]
    fn test_bitcoin_routes_to_crypto_btc_bip173() {
        let p = Pipeline::from_meta("encode into bitcoin").unwrap();
        assert_eq!(p.target, Endpoint::language_with_dialect("crypto/btc", "bip173"));
    }

    #[test]
    fn test_npub_routes_to_crypto_nostr_pub() {
        let p = Pipeline::from_meta("encode into npub").unwrap();
        assert_eq!(p.target, Endpoint::language_with_dialect("crypto/nostr", "pub"));
    }

    #[test]
    fn test_lightning_routes_to_crypto_ln_main() {
        let p = Pipeline::from_meta("encode into lightning").unwrap();
        assert_eq!(p.target, Endpoint::language_with_dialect("crypto/ln", "main"));
    }

    #[test]
    fn test_crypto_detect_btc_bip173() {
        // bc1q prefix → bip173 (SegWit v0), data starts after 4-char prefix
        assert_eq!(
            detect_crypto_format("bc1q29zsu7g0auf4f40avkcych9zzycd4ep3dvdxdgz"),
            Some(("crypto/btc", "bip173", "29zsu7g0auf4f40avkcych9zzycd4ep3dvdxdgz"))
        );
    }

    #[test]
    fn test_crypto_detect_btc_bip350() {
        // bc1p prefix → bip350 (Taproot)
        assert_eq!(
            detect_crypto_format("bc1p29zsu7g0auf4f40avkcych9zzycd4ep3dvdxdgz"),
            Some(("crypto/btc", "bip350", "29zsu7g0auf4f40avkcych9zzycd4ep3dvdxdgz"))
        );
    }

    #[test]
    fn test_crypto_detect_npub() {
        let npub = "npub1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqsutdnz";
        assert_eq!(
            detect_crypto_format(npub).map(|(lang, _, _)| lang),
            Some("crypto/nostr")
        );
    }

    #[test]
    fn test_crypto_detect_sha256_no_longer_matched() {
        // SHA-256 detection has been removed — 64-char hex is no longer auto-detected.
        let hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(detect_crypto_format(hash).is_none());
    }

    #[test]
    fn test_crypto_payload_type_mapping() {
        assert_eq!(crypto_payload_type("crypto/btc", "bip173"), "bech32");
        assert_eq!(crypto_payload_type("crypto/btc", "bip350"), "bech32");
        assert_eq!(crypto_payload_type("crypto/nostr", "pub"), "bech32");
        assert_eq!(crypto_payload_type("crypto/btc", "p2pkh"), "base58");
        assert_eq!(crypto_payload_type("crypto/btc", "p2pkh-test"), "base58");
        assert_eq!(crypto_payload_type("crypto/cashu", "a"), "base64url");
        assert_eq!(crypto_payload_type("crypto/ln", "main"), "bech32");
    }

    #[test]
    fn test_crypto_dialect_prefix_mapping() {
        assert_eq!(crypto_dialect_prefix("crypto/btc", "bip173"), "bc1q");
        assert_eq!(crypto_dialect_prefix("crypto/btc", "bip173-test"), "tb1q");
        assert_eq!(crypto_dialect_prefix("crypto/btc", "bip350"), "bc1p");
        assert_eq!(crypto_dialect_prefix("crypto/btc", "bip350-test"), "tb1p");
        assert_eq!(crypto_dialect_prefix("crypto/btc", "p2pkh"), "");
        assert_eq!(crypto_dialect_prefix("crypto/nostr", "pub"), "npub1");
        assert_eq!(crypto_dialect_prefix("crypto/cashu", "a"), "cashuA");
    }

    #[test]
    fn test_crypto_auto_detect_btc_into_english() {
        // When encoding a Bitcoin address into English, the pipeline should
        // auto-detect crypto format and create a transcode (crypto/btc → english).
        let p = Pipeline::from_meta("encode into english").unwrap();
        let input = "bc1q29zsu7g0auf4f40avkcych9zzycd4ep3dvdxdgz";
        let source = p.resolve_source(input).unwrap();
        assert!(matches!(source, Endpoint::Language { ref language, ref dialect, .. }
            if language == "crypto/btc" && dialect == "bip173"),
            "Bitcoin address should auto-detect as crypto/btc/bip173, got {:?}", source);
    }

    // ── Crypto round-trip tests ─────────────────────────────────────

    #[test]
    fn test_crypto_btc_address_round_trip() {
        // Bitcoin address → packed hex → Bitcoin address
        let btc_address = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        let hex_data = decode_crypto(btc_address, "crypto/btc", "bip173", false).unwrap();
        let re_encoded = encode_crypto(&hex_data, "crypto/btc", "bip173", false).unwrap();
        assert_eq!(re_encoded, btc_address.to_lowercase(),
            "Bech32 decode → encode should recover original address");
    }

    #[test]
    fn test_crypto_npub_round_trip() {
        // Nostr npub → packed hex → npub
        let npub = "npub1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqsutdnz";
        let hex_data = decode_crypto(npub, "crypto/nostr", "pub", false).unwrap();
        let re_encoded = encode_crypto(&hex_data, "crypto/nostr", "pub", false).unwrap();
        assert_eq!(re_encoded, npub.to_lowercase());
    }

    #[test]
    fn test_crypto_bech32_packing() {
        // Verify 5-bit packing is correct
        let values = vec![0, 14, 20, 15, 7, 13, 26, 0]; // qw508d6q
        let packed = pack_5bit_values(&values);
        let unpacked = unpack_5bit_values(&packed).unwrap();
        assert_eq!(values, unpacked, "Pack/unpack should be identity");
    }

    #[test]
    fn test_crypto_btc_address_decode_strips_prefix() {
        let btc = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        let hex_data = decode_crypto(btc, "crypto/btc", "bip173", false).unwrap();
        // The packed hex should start with count byte, not 'bc1q' characters
        assert!(!hex_data.starts_with("bc"), "Prefix should be stripped");
    }
}
