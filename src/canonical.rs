//! Canonical, versioned encoding.
//!
//! [`canonical_encode`] produces exactly one prose form for a given
//! `(payload, language, wordlist)`: the payload is wrapped in a version byte
//! and a checksum, packed into payload words, and rendered with the cover
//! seeded from a checksum of the encoded bytes. Because the rendering rules are
//! frozen per version in [`rules_for`], an artifact produced under version *n*
//! renders — and therefore verifies — identically under every later library
//! release: [`canonical_decode`] reads the version byte from the artifact
//! itself and re-renders under that version's rules, never the current ones.
//!
//! # The envelope
//!
//! ```text
//!   bytes:  [ payload ][ version:1 ][ crc32:4 ]
//!   words:  [ data_0 ] ... [ data_N-1 ][ padding ]      (canonical_encode)
//!           [ data_0 ] ... [ data_N-1 ]                 (canonical_encode_fixed)
//! ```
//!
//! Both envelope fields ride BEHIND the payload, and that placement is the
//! whole point of the layout. Every byte at a fixed offset from the START of
//! the packing pins the words a reader meets first: with the version byte
//! leading, the opening word of every English artifact was one of eight, and
//! with `bitpack`'s padding word leading it was a single constant word per
//! field size — every 32-byte hash opened "abandon". Behind the payload, the
//! opening words are payload entropy, and the checksum's four bytes stand
//! between the version byte and the end of the paragraph so the closing words
//! vary too.
//!
//! The trailing crc32 covers `payload || version`. It is a transcription check,
//! not a cryptographic one, and it is deliberately redundant with
//! [`CanonicalDecoded::verified`]: verification re-renders the whole paragraph
//! and catches strictly more damage, but it costs a generation pass, so
//! [`canonical_decode_raw`] — the entry point for callers decoding many
//! candidates — leans on the checksum alone.
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
//! # Two framings, both live
//!
//! Version 1 put the version byte in FRONT of the payload, and that position was
//! meant to be eternal: the byte has to be readable before the rules are known,
//! so moving it appears to break the very mechanism that keeps artifacts
//! readable. Version 2 moves it anyway, and the checksum is what pays for the
//! move. A decoder no longer has to be TOLD which framing it is holding — it
//! tries [`Envelope::PayloadLeading`], and a wrong guess fails its crc32 with
//! probability 1 − 2⁻³². So it falls back to [`Envelope::VersionLeading`]
//! without guesswork, and v1 artifacts keep decoding and keep verifying under
//! v1's own rules.
//!
//! [`Envelope`] is therefore part of [`VersionRules`], and a version vouches
//! only for the framing it declares — a v1 attempt is rejected unless the byte
//! it finds names a version that actually says `VersionLeading`. This is what
//! `rules_for` is for; dropping an old version would make the whole apparatus
//! decorative.
//!
//! One caveat no version rule can repair: v1 artifacts in LATIN and GERMAN do
//! not decode under 0.4.0. Those wordlists were one word short of a power of two
//! until this release, so restoring the words renumbered every index and moved
//! both languages from base-N to bitpack. That is a wordlist change rather than
//! a format change, and it sits outside what a canonical version freezes.
//! English and Czech v1 artifacts are unaffected.
//!
//! Callers that want more flexibility — their own seeds, bit packing, headers,
//! best-of budgets — should use [`crate::pipeline::encode_words_into_language`]
//! and friends; those entry points are unversioned by design.

use std::collections::HashSet;
use std::fmt;

use crate::codec::{
    checksum_seed, crc32, decode_base_n, decode_base_n_fixed, encode_base_n, normalize_token,
    payload_tokens,
};
use crate::pipeline::{cached_payload_tree, prepare_words_encode, resolve_wordlist_name};

/// The version new canonical artifacts are written at.
pub const CANONICAL_VERSION: u8 = 2;

/// The byte↔word packing for the length-parameterized entry points. The caller
/// states the payload's byte count, so no padding word is needed at all. Only
/// [`Envelope::PayloadLeading`] versions offer it; v1 never had a fixed form.
const FIXED_ENVELOPE_CODEC: &str = "bitpack_fixed";

/// Width of the trailing crc32 over `payload || version`.
const CHECKSUM_LEN: usize = 4;

/// How a version frames its payload, and therefore how its bytes pack into
/// words.
///
/// This is the one part of a version's format that the decoder must work out
/// WITHOUT knowing the version — the layout tells it where the version byte is.
/// v1 solved that by putting the byte first and declaring the framing eternal;
/// v2 moves it behind the payload and pays for the move with a checksum, which
/// is what lets a decoder tell the two apart by trying and checking rather than
/// by being told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envelope {
    /// v1: `version || payload`, packed with `bitpack` (leading padding word).
    /// No checksum — the wording re-render was the only integrity check.
    VersionLeading,
    /// v2: `payload || version || crc32`, packed with `canonical_bitpack`
    /// (trailing padding word) or `bitpack_fixed` (none).
    PayloadLeading,
}

impl Envelope {
    /// The self-describing codec this framing packs with.
    fn codec(self) -> &'static str {
        match self {
            Envelope::VersionLeading => "bitpack",
            Envelope::PayloadLeading => "canonical_bitpack",
        }
    }

    /// Bytes this framing adds around the payload.
    fn overhead(self) -> usize {
        match self {
            Envelope::VersionLeading => 1,
            Envelope::PayloadLeading => 1 + CHECKSUM_LEN,
        }
    }

    /// Wrap a payload for this framing.
    fn seal(self, payload: &[u8], version: u8) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(payload.len() + self.overhead());
        match self {
            Envelope::VersionLeading => {
                bytes.push(version);
                bytes.extend_from_slice(payload);
            }
            Envelope::PayloadLeading => {
                bytes.extend_from_slice(payload);
                bytes.push(version);
                bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
            }
        }
        bytes
    }

    /// Unwrap into `(version, payload)`, checking the checksum where there is
    /// one. The version is NOT checked against the registry here — that is the
    /// caller's business.
    fn open(self, bytes: &[u8]) -> Result<(u8, Vec<u8>), CanonicalError> {
        if bytes.len() < 1 + self.overhead() {
            return Err(CanonicalError::Decode(format!(
                "decoded to {} bytes, fewer than the {} a {:?} envelope needs",
                bytes.len(),
                1 + self.overhead(),
                self
            )));
        }
        match self {
            Envelope::VersionLeading => Ok((bytes[0], bytes[1..].to_vec())),
            Envelope::PayloadLeading => {
                let (body, checksum) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
                let found =
                    u32::from_be_bytes(checksum.try_into().expect("split at CHECKSUM_LEN"));
                let expected = crc32(body);
                if found != expected {
                    return Err(CanonicalError::ChecksumMismatch { expected, found });
                }
                Ok((body[body.len() - 1], body[..body.len() - 1].to_vec()))
            }
        }
    }
}

/// The framings a decoder tries, newest first.
///
/// Order matters only for cost, not correctness: `PayloadLeading` carries a
/// crc32, so a wrong guess is caught with probability 1 − 2⁻³², and
/// `VersionLeading` is only accepted when the byte it finds names rules that
/// actually declare that framing. A v2 artifact read as v1 yields a version
/// byte from the middle of a payload, which all but never lands on a
/// registered `VersionLeading` version.
const ENVELOPES: &[Envelope] = &[Envelope::PayloadLeading, Envelope::VersionLeading];

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
    /// How this version frames its payload. Unlike the other fields, the
    /// decoder must determine this BEFORE it knows the version — see
    /// [`Envelope`] and [`ENVELOPES`].
    pub envelope: Envelope,
}

static V1: VersionRules = VersionRules {
    best_of: 4,
    dialect: "body",
    // English's semantics.yaml is load-bearing for v1 renderings and is
    // therefore frozen: regenerating it requires a new canonical version.
    semantics_languages: &["english"],
    envelope: Envelope::VersionLeading,
};

static V2: VersionRules = VersionRules {
    best_of: 4,
    dialect: "body",
    semantics_languages: &["english"],
    envelope: Envelope::PayloadLeading,
};

/// The frozen rules for a canonical version, or `None` if this library release
/// does not know the version (i.e. the artifact was written by a newer one).
///
/// Version 1 stays registered even though v2 reframed the envelope. Keeping it
/// is the whole point of versioning rules: an artifact written by 0.3.0 still
/// decodes and still verifies, because `rules_for` hands the decoder v1's
/// framing and v1's rendering rules rather than the current ones. What made
/// that possible is v2's checksum — a decoder can try the new framing, see it
/// fail its crc32, and fall back to the old one without guessing.
///
/// One caveat that no version rule can repair: v1 artifacts in LATIN and GERMAN
/// do not decode under this release. Those two payload wordlists were one word
/// short of a power of two until 0.4.0 (YAML resolved `false:` and `null:` to
/// non-string scalars and the loader dropped them), so restoring the words
/// renumbered every index and switched the languages from base-N to bitpack.
/// That is a wordlist change, not a format change, and it is outside what a
/// canonical version freezes. English and Czech v1 artifacts are unaffected.
pub fn rules_for(version: u8) -> Option<&'static VersionRules> {
    match version {
        1 => Some(&V1),
        2 => Some(&V2),
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
    /// The trailing crc32 does not match the payload and version it covers:
    /// the words decoded, but not to the bytes that were encoded.
    ChecksumMismatch { expected: u32, found: u32 },
    /// The version has no length-parameterized form. Only versions framed
    /// `payload || version || checksum` offer one; v1's leading version byte
    /// predates the fixed packing.
    NoFixedForm(u8),
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
            CanonicalError::ChecksumMismatch { expected, found } => write!(
                f,
                "checksum mismatch: the words carry {:08x}, the payload they decode to checks as {:08x}",
                found, expected
            ),
            CanonicalError::NoFixedForm(v) => write!(
                f,
                "canonical version {} has no fixed-length form (its version byte leads)",
                v
            ),
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
/// (envelope included), so any change to the payload re-renders the whole
/// paragraph, which is what makes damage legible to a reader.
///
/// This is the self-describing form: the word list carries its own padding
/// count, so [`canonical_decode`] needs nothing but the prose. A caller that
/// already knows the payload's byte count should prefer
/// [`canonical_encode_fixed`], which spends no word on saying it.
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
    encode_canonical_with(payload, language, wordlist, version, rules.envelope.codec())
}

/// Encode `payload` as canonical prose with NO padding word.
///
/// Identical bytes and identical rules to [`canonical_encode`] — same envelope,
/// same version, same cover seed — but packed with `bitpack_fixed`, so the word
/// list is pure payload. The reader of the prose cannot tell how long the
/// payload is, and neither can the decoder: [`canonical_decode_fixed`] must be
/// told the byte count. That is the right trade wherever the count is already
/// known and stated (a 32-byte hash, a 20-byte hash160, a 65-byte key), which is
/// most structured data.
pub fn canonical_encode_fixed(
    payload: &[u8],
    language: &str,
    wordlist: &str,
) -> Result<String, CanonicalError> {
    canonical_encode_fixed_at(payload, language, wordlist, CANONICAL_VERSION)
}

/// [`canonical_encode_fixed`] at an explicit version — the re-render half of
/// [`canonical_decode_fixed`]'s verification.
pub fn canonical_encode_fixed_at(
    payload: &[u8],
    language: &str,
    wordlist: &str,
    version: u8,
) -> Result<String, CanonicalError> {
    let rules = rules_for(version).ok_or(CanonicalError::UnsupportedVersion(version))?;
    if rules.envelope != Envelope::PayloadLeading {
        return Err(CanonicalError::NoFixedForm(version));
    }
    encode_canonical_with(payload, language, wordlist, version, FIXED_ENVELOPE_CODEC)
}

/// The shared encode body: seal the envelope, pack it with `codec`, render.
fn encode_canonical_with(
    payload: &[u8],
    language: &str,
    wordlist: &str,
    version: u8,
    codec: &str,
) -> Result<String, CanonicalError> {
    let rules = rules_for(version).ok_or(CanonicalError::UnsupportedVersion(version))?;
    if payload.is_empty() {
        return Err(CanonicalError::EmptyPayload);
    }

    let bytes = rules.envelope.seal(payload, version);
    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;
    let words = encode_base_n(&bytes, &tree, codec)
        .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;

    render_canonical(&words, &bytes, language, wl, rules)
}

/// Decode canonical prose: harvest the payload words, unpack the envelope, and
/// verify by re-rendering under that version's rules.
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

/// Decode prose written by [`canonical_encode_fixed`], given the payload's byte
/// count, and verify by re-rendering under that version's rules.
///
/// `payload_len` is the payload alone — the envelope's own bytes are this
/// function's business, not the caller's.
pub fn canonical_decode_fixed(
    text: &str,
    language: &str,
    wordlist: &str,
    payload_len: usize,
) -> Result<CanonicalDecoded, CanonicalError> {
    let (version, payload) = canonical_decode_raw_fixed(text, language, wordlist, payload_len)?;
    let reference = canonical_encode_fixed_at(&payload, language, wordlist, version)?;
    let verified = same_wording(text, &reference);
    Ok(CanonicalDecoded { version, payload, verified, canonical_text: reference })
}

/// The decode half alone: payload words → version byte + payload bytes, with the
/// checksum and the version registry checked but the wording NOT verified.
///
/// This is for callers doing many decodes per verification — a repair search
/// proposing typo corrections decodes every candidate but only needs each
/// candidate's rendering once, from its own (often memoized) encode call. The
/// trailing checksum is what makes that cheap pass worth anything on its own.
/// [`canonical_decode`] is this plus the verify re-render.
pub fn canonical_decode_raw(
    text: &str,
    language: &str,
    wordlist: &str,
) -> Result<(u8, Vec<u8>), CanonicalError> {
    decode_canonical_with(text, language, wordlist, None)
}

/// [`canonical_decode_raw`] for the fixed packing, given the payload byte count.
pub fn canonical_decode_raw_fixed(
    text: &str,
    language: &str,
    wordlist: &str,
    payload_len: usize,
) -> Result<(u8, Vec<u8>), CanonicalError> {
    decode_canonical_with(text, language, wordlist, Some(payload_len))
}

/// The shared decode body. `payload_len` present selects the fixed packing and
/// states how many bytes to expect; absent selects the self-describing one.
///
/// The fixed packing exists only under [`Envelope::PayloadLeading`], so a stated
/// length pins the framing and there is nothing to search. Without one, each
/// framing in [`ENVELOPES`] is tried in turn and the first that both unpacks and
/// names a version declaring that same framing wins. The error reported on total
/// failure is the FIRST framing's, since that is the one a current artifact was
/// meant to be.
fn decode_canonical_with(
    text: &str,
    language: &str,
    wordlist: &str,
    payload_len: Option<usize>,
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

    if let Some(n) = payload_len {
        let envelope = Envelope::PayloadLeading;
        let bytes = decode_base_n_fixed(
            &words,
            &tree,
            FIXED_ENVELOPE_CODEC,
            n + envelope.overhead(),
        )
        .map_err(|e| CanonicalError::Decode(format!("{:?}", e)))?;
        // The checksum runs before the version lookup on purpose: damaged words
        // are far likelier than an artifact from the future, so a mangled
        // paragraph should report itself as mangled rather than as an
        // unsupported version it never claimed.
        let (version, payload) = envelope.open(&bytes)?;
        let rules = rules_for(version).ok_or(CanonicalError::UnsupportedVersion(version))?;
        if rules.envelope != envelope {
            return Err(CanonicalError::NoFixedForm(version));
        }
        return Ok((version, payload));
    }

    let mut first_error = None;
    for &envelope in ENVELOPES {
        let attempt = decode_base_n(&words, &tree, envelope.codec())
            .map_err(|e| CanonicalError::Decode(format!("{:?}", e)))
            .and_then(|bytes| envelope.open(&bytes))
            .and_then(|(version, payload)| {
                let rules =
                    rules_for(version).ok_or(CanonicalError::UnsupportedVersion(version))?;
                // A version only vouches for the framing it declares. Without
                // this the v1 attempt would accept any artifact whose seventh
                // byte happened to read 1 or 2.
                if rules.envelope != envelope {
                    return Err(CanonicalError::UnsupportedVersion(version));
                }
                Ok((version, payload))
            });
        match attempt {
            Ok(found) => return Ok(found),
            Err(e) => first_error.get_or_insert(e),
        };
    }
    // Unknown version, or nothing that framed cleanly: the payload words may
    // still unpack, but there is no way to verify the wording without a
    // version's rules. Refusing outright is the honest answer —
    // "unverifiable" from a canonical decoder would be indistinguishable from
    // damage.
    Err(first_error.expect("ENVELOPES is never empty"))
}

/// Canonical encode with placements: the current-version rendering plus where
/// each payload word landed (POS, sentence, subject/object role), for UIs that
/// annotate the prose. The text is identical to [`canonical_encode`]'s.
pub fn canonical_encode_traced(
    payload: &[u8],
    language: &str,
    wordlist: &str,
) -> Result<(String, Vec<crate::generator::core::Placement>), CanonicalError> {
    let rules = rules_for(CANONICAL_VERSION).expect("current version must be registered");
    encode_traced_with(payload, language, wordlist, rules.envelope.codec())
}

/// [`canonical_encode_fixed`] with placements. Text identical to
/// [`canonical_encode_fixed`]'s, and one placement fewer than
/// [`canonical_encode_traced`] returns, there being no padding word to place.
pub fn canonical_encode_fixed_traced(
    payload: &[u8],
    language: &str,
    wordlist: &str,
) -> Result<(String, Vec<crate::generator::core::Placement>), CanonicalError> {
    encode_traced_with(payload, language, wordlist, FIXED_ENVELOPE_CODEC)
}

fn encode_traced_with(
    payload: &[u8],
    language: &str,
    wordlist: &str,
    codec: &str,
) -> Result<(String, Vec<crate::generator::core::Placement>), CanonicalError> {
    let version = CANONICAL_VERSION;
    let rules = rules_for(version).expect("current version must be registered");
    if payload.is_empty() {
        return Err(CanonicalError::EmptyPayload);
    }

    let bytes = rules.envelope.seal(payload, version);
    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Encode(format!("{:?}", e)))?;
    let words = encode_base_n(&bytes, &tree, codec)
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
