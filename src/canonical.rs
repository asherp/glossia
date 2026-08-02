//! Canonical, versioned encoding.
//!
//! [`canonical_encode`] produces exactly one prose form for a given
//! `(payload, language, wordlist)`: the payload is prefixed with a version
//! byte, packed into payload words, and rendered with the cover seeded from a
//! checksum of the encoded bytes. Because the rendering rules are frozen per
//! version in [`rules_for`], an artifact produced under version *n* renders —
//! and therefore verifies — identically under every later library release:
//! [`canonical_decode`] reads the version byte from the artifact itself and
//! re-renders under that version's rules, never the current ones.
//!
//! This is what makes it safe to improve generation later. Shipping a
//! `semantics.yaml` for a language, changing the best-of budget, or any other
//! change to how prose is rendered gets a new version entry; existing
//! artifacts keep verifying because their version byte selects the old rules.
//! The flip side is a freeze: everything a version's rendering reads — the
//! grammar, dialect and cover data for its languages, plus the semantic model
//! for languages listed in its rules — must not change in place once the
//! version has shipped. Wordlists are already append-only; appends are safe
//! for decoding but DO change cover selection, so a wordlist append also
//! requires a new canonical version if canonical artifacts exist for that
//! language. The golden tests in `tests/canonical.rs` enforce the freeze.
//!
//! Callers that want more flexibility — their own seeds, bit packing, headers,
//! best-of budgets — should use [`crate::pipeline::encode_words_into_language`]
//! and friends; those entry points are unversioned by design.

use std::collections::HashSet;
use std::fmt;

use crate::codec::{checksum_seed, decode_base_n, encode_base_n, normalize_token, payload_tokens};
use crate::pipeline::{cached_payload_tree, prepare_words_encode, resolve_wordlist_name};

/// The version new canonical artifacts are written at.
pub const CANONICAL_VERSION: u8 = 1;

/// The byte↔word packing used to carry the version byte. Deliberately NOT part
/// of [`VersionRules`]: the version byte must be readable before the version is
/// known, so the packing itself can never be versioned. (`bitpack` self-describes
/// its padding with a leading pad word; non-power-of-two wordlists fall back to
/// base-N big-integer packing inside `encode_base_n`, deterministically.)
const ENVELOPE_CODEC: &str = "bitpack";

/// The rendering rules a canonical version freezes.
///
/// Editing an existing version's rules is a format break — every artifact
/// written under it stops verifying. To change how canonical prose is
/// rendered, add a new version and bump [`CANONICAL_VERSION`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRules {
    /// Fluency budget: the densest of this many consecutive seeds is the
    /// canonical rendering. A verifier repeats the same selection.
    pub best_of: usize,
    /// Grammar dialect the rendering uses, regardless of the language's
    /// default.
    pub dialect: &'static str,
    /// Languages whose semantic model participates in rendering under this
    /// version. A language absent here renders with POS-only planning even if
    /// a later release ships a `semantics.yaml` for it — that is the mechanism
    /// that lets semantics arrive for a language without re-rendering its
    /// existing artifacts.
    pub semantics_languages: &'static [&'static str],
}

static V1: VersionRules = VersionRules {
    best_of: 4,
    dialect: "body",
    // English's semantics.yaml is load-bearing for v1 renderings and is
    // therefore frozen: regenerating it requires a new canonical version.
    semantics_languages: &["english"],
};

/// The frozen rules for a canonical version, or `None` if this library release
/// does not know the version (i.e. the artifact was written by a newer one).
pub fn rules_for(version: u8) -> Option<&'static VersionRules> {
    match version {
        1 => Some(&V1),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    /// Canonical artifacts always carry at least the version byte plus one
    /// payload byte.
    EmptyPayload,
    /// The version byte names rules this library release does not have —
    /// the artifact comes from a newer release (or the byte is corrupted).
    UnsupportedVersion(u8),
    /// No payload words were found in the text.
    NoPayloadWords,
    Encode(String),
    Decode(String),
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanonicalError::EmptyPayload => write!(f, "payload is empty"),
            CanonicalError::UnsupportedVersion(v) => {
                write!(f, "canonical version {} is not supported by this build (max {})", v, CANONICAL_VERSION)
            }
            CanonicalError::NoPayloadWords => write!(f, "no payload words found in text"),
            CanonicalError::Encode(e) => write!(f, "encode error: {}", e),
            CanonicalError::Decode(e) => write!(f, "decode error: {}", e),
        }
    }
}

impl std::error::Error for CanonicalError {}

/// Result of [`canonical_decode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDecoded {
    /// The version byte the artifact carries.
    pub version: u8,
    /// The payload bytes (version byte stripped).
    pub payload: Vec<u8>,
    /// Whether the received wording matches the canonical re-render for this
    /// payload and version, compared as normalized alphanumeric tokens — so
    /// punctuation spacing and surrounding markup are formatting, not damage.
    /// `false` means the payload decoded but the prose is not the canonical
    /// rendering of it: transcription damage, or text produced some other way.
    pub verified: bool,
    /// The canonical rendering of the decoded payload — the reference the
    /// verification compared against. Verification computes it anyway, and a
    /// checker UI diffing received wording against expected wording needs it,
    /// so returning it saves that caller a duplicate generation.
    pub canonical_text: String,
}

/// Encode `payload` as canonical prose at [`CANONICAL_VERSION`].
///
/// Deterministic: one payload has exactly one canonical rendering per
/// `(language, wordlist)`. The cover seed is a checksum of the encoded bytes
/// (version byte included), so any change to the payload re-renders the whole
/// paragraph, which is what makes damage legible to a reader.
pub fn canonical_encode(
    payload: &[u8],
    language: &str,
    wordlist: &str,
) -> Result<String, CanonicalError> {
    canonical_encode_at(payload, language, wordlist, CANONICAL_VERSION)
}

/// Encode at an explicit version — the re-render half of verification, and the
/// escape hatch for writing artifacts compatible with an older release.
pub fn canonical_encode_at(
    payload: &[u8],
    language: &str,
    wordlist: &str,
    version: u8,
) -> Result<String, CanonicalError> {
    let rules = rules_for(version).ok_or(CanonicalError::UnsupportedVersion(version))?;
    if payload.is_empty() {
        return Err(CanonicalError::EmptyPayload);
    }

    let mut bytes = Vec::with_capacity(1 + payload.len());
    bytes.push(version);
    bytes.extend_from_slice(payload);

    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;
    let words = encode_base_n(&bytes, &tree, ENVELOPE_CODEC)
        .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;

    render_canonical(&words, &bytes, language, wl, rules)
}

/// Decode canonical prose: harvest the payload words, unpack the version byte,
/// and verify by re-rendering under that version's rules.
pub fn canonical_decode(
    text: &str,
    language: &str,
    wordlist: &str,
) -> Result<CanonicalDecoded, CanonicalError> {
    let (version, payload) = canonical_decode_raw(text, language, wordlist)?;
    let reference = canonical_encode_at(&payload, language, wordlist, version)?;
    let verified = same_wording(text, &reference);
    Ok(CanonicalDecoded { version, payload, verified, canonical_text: reference })
}

/// The decode half alone: payload words → version byte + payload bytes, with
/// the version checked against the registry but the wording NOT verified.
///
/// This is for callers doing many decodes per verification — a repair search
/// proposing typo corrections decodes every candidate but only needs each
/// candidate's rendering once, from its own (often memoized) encode call.
/// [`canonical_decode`] is this plus the verify re-render.
pub fn canonical_decode_raw(
    text: &str,
    language: &str,
    wordlist: &str,
) -> Result<(u8, Vec<u8>), CanonicalError> {
    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Decode(format!("{:?}", e)))?;
    let payload_set: HashSet<String> =
        tree.words().iter().map(|w| w.to_lowercase()).collect();

    let words = payload_tokens(text, |w| payload_set.contains(w));
    if words.is_empty() {
        return Err(CanonicalError::NoPayloadWords);
    }

    let bytes = decode_base_n(&words, &tree, ENVELOPE_CODEC)
        .map_err(|e| CanonicalError::Decode(format!("{:?}", e)))?;
    if bytes.len() < 2 {
        return Err(CanonicalError::Decode("decoded to fewer than 2 bytes".to_string()));
    }
    let version = bytes[0];
    let payload = bytes[1..].to_vec();
    // Unknown version: the payload words still decode, but there is no way to
    // verify the wording without the version's rules. Refusing outright is the
    // honest answer — "unverifiable" from a canonical decoder would be
    // indistinguishable from damage.
    rules_for(version).ok_or(CanonicalError::UnsupportedVersion(version))?;
    Ok((version, payload))
}

/// Canonical encode with placements: the current-version rendering plus where
/// each payload word landed (POS, sentence, subject/object role), for UIs that
/// annotate the prose. The text is identical to [`canonical_encode`]'s.
pub fn canonical_encode_traced(
    payload: &[u8],
    language: &str,
    wordlist: &str,
) -> Result<(String, Vec<crate::generator::core::Placement>), CanonicalError> {
    let version = CANONICAL_VERSION;
    let rules = rules_for(version).expect("current version must be registered");
    if payload.is_empty() {
        return Err(CanonicalError::EmptyPayload);
    }

    let mut bytes = Vec::with_capacity(1 + payload.len());
    bytes.push(version);
    bytes.extend_from_slice(payload);

    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;
    let words = encode_base_n(&bytes, &tree, ENVELOPE_CODEC)
        .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;

    let (toks, payload_word_set, gen) = prepare_render(&words, language, wl, rules)?;
    let seed = checksum_seed(&bytes, 0);
    let (text, _set, k) = crate::generator::core::generate_text_best_of_indexed(
        seed, rules.best_of, &gen.lex, &toks, Some(&payload_word_set), false,
        gen.mode, language, Some(rules.dialect), gen.k_min, gen.k_max, gen.length_mode, " ",
    );
    let mut rng = <crate::CoverRng as rand::SeedableRng>::seed_from_u64(seed.wrapping_add(k));
    let (traced_text, _s, placements) = crate::generator::core::generate_text_traced(
        &mut rng, &gen.lex, &toks, Some(&payload_word_set), false, gen.mode, language,
        Some(rules.dialect), gen.k_min, gen.k_max, gen.length_mode, " ",
    );
    debug_assert_eq!(traced_text, text, "traced re-render must match the selected candidate");
    Ok((text, placements))
}

/// Shared render setup under a version's frozen rules. Env-independent:
/// semantics is attached or detached from the rules alone, so
/// `GLOSSIA_DISABLE_SEMANTICS` cannot change what a canonical artifact looks
/// like.
fn prepare_render(
    words: &[String],
    language: &str,
    wordlist: &str,
    rules: &VersionRules,
) -> Result<
    (
        Vec<crate::generator::types::PayloadTok>,
        HashSet<String>,
        crate::pipeline::ZoneGenerator,
    ),
    CanonicalError,
> {
    let (toks, payload_word_set, mut gen) =
        prepare_words_encode(words, language, wordlist, rules.dialect)
            .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;

    if rules.semantics_languages.contains(&language) {
        let model = crate::generator::data::load_semantics_cached_ignore_env(language)
            .ok_or_else(|| {
                CanonicalError::Encode(format!(
                    "canonical rules expect a semantic model for {} but none is embedded",
                    language
                ))
            })?;
        gen.lex = gen.lex.with_semantics(model);
    } else {
        gen.lex = gen.lex.without_semantics();
    }
    Ok((toks, payload_word_set, gen))
}

/// Render payload words under a version's frozen rules.
fn render_canonical(
    words: &[String],
    bytes: &[u8],
    language: &str,
    wordlist: &str,
    rules: &VersionRules,
) -> Result<String, CanonicalError> {
    let (toks, payload_word_set, gen) = prepare_render(words, language, wordlist, rules)?;
    let seed = checksum_seed(bytes, 0);
    let (text, _set, _k) = crate::generator::core::generate_text_best_of_indexed(
        seed, rules.best_of, &gen.lex, &toks, Some(&payload_word_set), false,
        gen.mode, language, Some(rules.dialect), gen.k_min, gen.k_max, gen.length_mode, " ",
    );
    Ok(text)
}

/// Wording equality over normalized alphanumeric tokens: word-exact,
/// punctuation- and markup-insensitive.
fn same_wording(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(normalize_token)
            .filter(|t| !t.is_empty())
            .collect()
    };
    norm(a) == norm(b)
}
