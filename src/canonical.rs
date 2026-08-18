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

use crate::align::{align, Alignment};
use crate::codec::{
    checksum_seed, crc32, decode_base_n, decode_base_n_fixed, encode_base_n, payload_tokens,
};
use crate::merkle::WordlistTree;
use crate::pipeline::{cached_payload_tree, prepare_words_encode, resolve_wordlist_name};
use crate::rs::Interleaved;

/// The version new canonical artifacts are written at.
pub const CANONICAL_VERSION: u8 = 3;

/// How much Reed–Solomon parity a version spends, as a rate rather than a count.
///
/// Pinned to the version, like everything else the renderer reads. What a
/// version must fix is that an artifact's word count follows from its payload
/// length and from nothing else — not that it follows by a constant. A formula
/// satisfies that as fully as a number does, and a constant does not survive
/// arbitrary length: four words of protection is generous for a 19-word address
/// and negligible for a 1000-word transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParityRules {
    /// Parity never falls below this, however short the payload.
    pub floor: usize,
    /// At least one parity symbol per this many message symbols.
    pub divisor: usize,
}

/// v3's parity: one symbol per eight, never fewer than four.
///
/// Pinned to the version rather than offered as a parameter. A parameter would
/// have to be declared somewhere the decoder can read before it has decoded —
/// costing a symbol, the very thing parity is counted in — and it would make an
/// artifact's word count depend on a caller's choice rather than on its payload.
///
/// **The rate.** One in eight gives, via `2·errors + erasures ≤ parity`, about
/// 12% of the words repairable when [`crate::align`] locates them and 6% when
/// the decoder must find them. Payloads here are not bounded — a whole
/// transaction, a mail body — and a fixed count would thin to nothing across
/// that range: four words is generous protection for a 19-word address and
/// negligible for a 1000-word transaction. A rate holds its meaning at every
/// length, and 1/8 is the rate of RS(255,223), which is well-trodden ground.
///
/// **The floor.** Below 32 message words the floor binds, so an address, a
/// hash160 or a witness program costs exactly the four words it would have cost
/// under a fixed budget — the short artifacts this format was first sized for
/// are unaffected by the rate.
///
/// Measured in English: a 20-byte hash160 runs 19 → 23 words, a 32-byte program
/// 27 → 31, a 128-byte payload 97 → 110, a 1 KB payload 745 → 839. Latin packs
/// 15 bits to a word rather than 11, so it needs fewer words for the same bytes
/// and pays the same rate on a smaller base.
pub const V3_PARITY: ParityRules = ParityRules { floor: 4, divisor: 8 };

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
/// The `(framing, parity)` combinations a decoder tries, most recent first.
///
/// Both halves have to be guessed before the version that states them can be
/// read, and both are caught by the same thing: v2 and later carry a crc32, so a
/// wrong guess fails it with probability 1 − 2⁻³² rather than producing a
/// plausible payload. v1 has no checksum, which is exactly why it is tried last.
///
/// v3 and v2 share a framing and differ only in parity, so the pair is what
/// distinguishes them — the envelope alone no longer identifies a version.
const FRAMINGS: &[(Envelope, Option<ParityRules>)] = &[
    (Envelope::PayloadLeading, Some(V3_PARITY)),
    (Envelope::PayloadLeading, None),
    (Envelope::VersionLeading, None),
];

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
    /// [`Envelope`] and [`FRAMINGS`].
    pub envelope: Envelope,
    /// Reed–Solomon parity symbols appended to the packed words, or 0 for a
    /// version that carries none.
    ///
    /// This sits beside `envelope` rather than inside it because it is not a
    /// byte framing at all: v3 seals exactly the bytes v2 does, and the parity
    /// is added a layer later, over the WORDS those bytes packed into. That is
    /// the whole point of the choice of field — a symbol is a word, so one
    /// mistranscribed word is one wrong symbol rather than the two or three
    /// byte faults it would be if parity were computed before packing.
    ///
    /// Like `envelope`, the decoder must guess this before it can read the
    /// version that states it, so it too is part of a framing candidate.
    pub parity: Option<ParityRules>,
}

static V1: VersionRules = VersionRules {
    best_of: 4,
    dialect: "body",
    // English's semantics.yaml is load-bearing for v1 renderings and is
    // therefore frozen: regenerating it requires a new canonical version.
    semantics_languages: &["english"],
    envelope: Envelope::VersionLeading,
    parity: None,
};

static V2: VersionRules = VersionRules {
    best_of: 4,
    dialect: "body",
    semantics_languages: &["english"],
    envelope: Envelope::PayloadLeading,
    parity: None,
};

/// v3: v2's bytes exactly, with Reed–Solomon parity over the packed words.
///
/// Nothing about the byte framing or the rendering rules moves — the same
/// envelope, the same fluency budget, the same dialect, the same semantics. The
/// only difference is [`V3_PARITY`] parity words riding after the payload, which
/// is why a v2 artifact and a v3 artifact of the same payload share their
/// opening words and diverge only in length.
static V3: VersionRules = VersionRules {
    best_of: 4,
    dialect: "body",
    semantics_languages: &["english"],
    envelope: Envelope::PayloadLeading,
    parity: Some(V3_PARITY),
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
        3 => Some(&V3),
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

/// How far the received prose confirmed the payload it decoded to.
///
/// The third state #76 names — outright failure — is the `Err` half of the
/// decode result, not a variant here: a decode that could not name a version or
/// whose checksum did not hold produces no payload to render a verdict on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The wording is the canonical rendering of the payload. Because the cover
    /// seed is a checksum of the encoded bytes, a wrong payload re-renders the
    /// whole paragraph differently — so matching wording is a check on the
    /// bytes, not merely on the prose.
    Verified,
    /// The payload decoded and its checksum held, but the wording is not the
    /// canonical rendering. The cover took transcription damage, or the text
    /// was produced some other way. The payload still stands on its crc32.
    Unverified,
}

/// Result of [`canonical_decode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDecoded {
    /// The version byte the artifact carries.
    pub version: u8,
    /// The payload bytes (version byte stripped).
    pub payload: Vec<u8>,
    /// Whether the wording confirms the payload, and if not, how far it fell
    /// short. Reaching this type at all means the envelope's checksum held, so
    /// [`Verdict::Unverified`] is "the bytes are right and the prose carrying
    /// them is damaged", not "the bytes are doubtful".
    pub verdict: Verdict,
    /// Where the received wording and the canonical rendering diverge, with the
    /// damage mapped onto payload slots. Empty of damage exactly when
    /// [`Verdict::Verified`].
    ///
    /// This is what a checker UI marks up, and what an error-correcting decoder
    /// takes its erasure positions from: [`Alignment::payload_slots`] is the
    /// received symbols in the rendering's own coordinates, so a dropped or
    /// spurious word does not shift everything after it.
    pub alignment: Alignment,
    /// Word positions Reed–Solomon parity repaired on the way to this payload,
    /// ascending. Empty under a version carrying no parity, and empty under one
    /// that does when the prose arrived intact.
    ///
    /// Non-empty means the payload below is a *correction*, not a transcription:
    /// these words did not say what the artifact was written with. A caller
    /// surfacing a repair should show it rather than apply it silently — the
    /// correction is backed by the envelope's crc32, but a reader who mis-copied
    /// a word is better told which one.
    pub repaired: Vec<usize>,
    /// Whether the received wording matches the canonical re-render for this
    /// payload and version, compared as normalized alphanumeric tokens — so
    /// punctuation spacing and surrounding markup are formatting, not damage.
    /// `false` means the payload decoded but the prose is not the canonical
    /// rendering of it: transcription damage, or text produced some other way.
    ///
    /// Retained as the flat form of [`Verdict`]; `verified == (verdict ==
    /// Verdict::Verified)`.
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

// ═══════════════════════════════════════════════════════════════════════
// Parity, over the words
// ═══════════════════════════════════════════════════════════════════════
//
// A payload word's index in the wordlist IS a field element, because every
// shipped payload wordlist is a power of two. So the parity is computed over
// the words rather than over the bytes they packed from, and one mistranscribed
// word costs exactly one symbol. Computed before packing, an 11-bit word
// straddles two or three byte boundaries and a single fault would cost two or
// three symbols of parity to repair.

/// Word indices as field elements.
fn symbols_of(words: &[String], tree: &WordlistTree) -> Result<Vec<u16>, CanonicalError> {
    words
        .iter()
        .map(|w| {
            let i = tree
                .position(w)
                .ok_or_else(|| CanonicalError::Decode(format!("{w:?} is not a payload word")))?;
            u16::try_from(i).map_err(|_| {
                CanonicalError::Decode(format!("word index {i} exceeds the symbol width"))
            })
        })
        .collect()
}

/// Field elements back to words.
fn words_of(symbols: &[u16], tree: &WordlistTree) -> Result<Vec<String>, CanonicalError> {
    let words = tree.words();
    symbols
        .iter()
        .map(|&s| {
            words
                .get(s as usize)
                .cloned()
                .ok_or_else(|| CanonicalError::Decode(format!("symbol {s} is not a word")))
        })
        .collect()
}

/// Append this version's parity words. A version spending none is a no-op, so
/// v1 and v2 pass through untouched and their renderings cannot move.
fn append_parity(
    words: Vec<String>,
    tree: &WordlistTree,
    parity: Option<ParityRules>,
) -> Result<Vec<String>, CanonicalError> {
    let Some(rules) = parity else {
        return Ok(words);
    };
    let il = Interleaved::for_wordlist_len(tree.len(), rules.floor, rules.divisor)
        .map_err(|e| CanonicalError::Encode(e.to_string()))?;
    let symbols = symbols_of(&words, tree)?;
    let codeword = il
        .encode(&symbols)
        .map_err(|e| CanonicalError::Encode(e.to_string()))?;
    words_of(&codeword, tree)
}

/// Repair what parity allows, then strip it, returning the message words and
/// the positions that were repaired.
///
/// `erasures` names positions already known bad — from [`crate::align`], which
/// recovers them by comparing against a candidate's re-render. Supplying them
/// halves their cost: `2·errors + erasures ≤ parity`. Passing none is valid and
/// leaves the code correcting up to `parity / 2` faults it must locate itself.
fn take_parity(
    words: &[String],
    tree: &WordlistTree,
    parity: Option<ParityRules>,
    erasures: &[usize],
) -> Result<(Vec<String>, Vec<usize>), CanonicalError> {
    let Some(rules) = parity else {
        return Ok((words.to_vec(), Vec::new()));
    };
    let il = Interleaved::for_wordlist_len(tree.len(), rules.floor, rules.divisor)
        .map_err(|e| CanonicalError::Decode(e.to_string()))?;
    let symbols = symbols_of(words, tree)?;
    let corrected = il
        .decode(&symbols, erasures)
        .map_err(|e| CanonicalError::Decode(e.to_string()))?;
    let message = il
        .message_of(&corrected.codeword)
        .ok_or_else(|| CanonicalError::Decode("word count belongs to no message".into()))?;
    let message = words_of(message, tree)?;
    let mut repaired = corrected.errors;
    repaired.extend(corrected.erasures);
    repaired.sort_unstable();
    repaired.dedup();
    Ok((message, repaired))
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
    let words = append_parity(words, &tree, rules.parity)?;

    render_canonical(&words, &bytes, language, wl, rules)
}

/// Decode canonical prose: harvest the payload words, unpack the envelope, and
/// verify by re-rendering under that version's rules.
pub fn canonical_decode(
    text: &str,
    language: &str,
    wordlist: &str,
) -> Result<CanonicalDecoded, CanonicalError> {
    let (version, payload, repaired) =
        decode_canonical_with(text, language, wordlist, None, &[])?;
    let reference = canonical_encode_at(&payload, language, wordlist, version)?;
    judge(text, reference, version, payload, repaired, language, wordlist)
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
    canonical_decode_fixed_repaired(text, language, wordlist, payload_len, &[])
}

/// [`canonical_decode_fixed`], told which word positions are already known bad.
///
/// This is where the two halves meet. [`crate::align`] recovers positions by
/// comparing received prose against a candidate's re-render, and a located fault
/// costs parity half what an unlocated one does — so `2·errors + erasures ≤
/// parity` lets [`V3_PARITY`] repair four words it is told about where it could
/// only find two on its own.
///
/// Positions index the harvested payload-word sequence, parity words included,
/// which is the coordinate system [`Alignment::payload_slots`] is already in.
/// Where a word was mangled off the wordlist entirely it is absent from the
/// harvest, so the text alone cannot carry the position — use
/// [`canonical_decode_slots_fixed`] with the alignment's slots instead.
pub fn canonical_decode_fixed_repaired(
    text: &str,
    language: &str,
    wordlist: &str,
    payload_len: usize,
    erasures: &[usize],
) -> Result<CanonicalDecoded, CanonicalError> {
    let (version, payload, repaired) =
        decode_canonical_with(text, language, wordlist, Some(payload_len), erasures)?;
    let reference = canonical_encode_fixed_at(&payload, language, wordlist, version)?;
    judge(text, reference, version, payload, repaired, language, wordlist)
}

/// Decode from aligned payload slots rather than from prose.
///
/// `slots` is [`Alignment::payload_slots`]: one entry per payload word the
/// rendering expected, holding what arrived there or `None` where nothing
/// usable did. Taking slots rather than text is what makes a word mangled OFF
/// the wordlist repairable — such a word never reaches the harvest, so its
/// position exists only in the alignment, and passing prose would lose it.
///
/// Every `None` becomes an erasure. The placeholder written into the gap is
/// never read: the decoder zeroes erased positions before computing syndromes.
pub fn canonical_decode_slots_fixed(
    slots: &[Option<String>],
    language: &str,
    wordlist: &str,
    payload_len: usize,
) -> Result<CanonicalDecoded, CanonicalError> {
    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Decode(format!("{:?}", e)))?;
    let filler = tree
        .words()
        .first()
        .ok_or_else(|| CanonicalError::Decode("empty wordlist".into()))?;

    let erasures: Vec<usize> = slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_none())
        .map(|(i, _)| i)
        .collect();
    let text = slots
        .iter()
        .map(|s| s.as_deref().unwrap_or(filler.as_str()))
        .collect::<Vec<_>>()
        .join(" ");

    canonical_decode_fixed_repaired(&text, language, wordlist, payload_len, &erasures)
}

/// Align received text against its canonical re-render and assemble the verdict.
///
/// The alignment subsumes the old boolean comparison: the wording matches
/// exactly when every position aligned as [`crate::align::Op::Same`], so
/// `verified` is read off the alignment rather than computed a second way. That
/// keeps one answer where there were two, and it means an unverified result
/// arrives with the reason attached instead of only the fact.
fn judge(
    text: &str,
    reference: String,
    version: u8,
    payload: Vec<u8>,
    repaired: Vec<usize>,
    language: &str,
    wordlist: &str,
) -> Result<CanonicalDecoded, CanonicalError> {
    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Decode(format!("{:?}", e)))?;
    let payload_set: HashSet<String> = tree.words().iter().map(|w| w.to_lowercase()).collect();

    let alignment = align(text, &reference, None, |w| payload_set.contains(w));
    let verified = alignment.is_clean();
    let verdict = if verified { Verdict::Verified } else { Verdict::Unverified };

    Ok(CanonicalDecoded {
        version,
        payload,
        verdict,
        alignment,
        repaired,
        verified,
        canonical_text: reference,
    })
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
    decode_canonical_with(text, language, wordlist, None, &[]).map(|(v, p, _)| (v, p))
}

/// [`canonical_decode_raw`] for the fixed packing, given the payload byte count.
pub fn canonical_decode_raw_fixed(
    text: &str,
    language: &str,
    wordlist: &str,
    payload_len: usize,
) -> Result<(u8, Vec<u8>), CanonicalError> {
    decode_canonical_with(text, language, wordlist, Some(payload_len), &[])
        .map(|(v, p, _)| (v, p))
}

/// The shared decode body. `payload_len` present selects the fixed packing and
/// states how many bytes to expect; absent selects the self-describing one.
///
/// The fixed packing exists only under [`Envelope::PayloadLeading`], so a stated
/// length rules that framing in — but not the parity, which still has to be
/// guessed. Each candidate in [`FRAMINGS`] is tried in turn and the first that
/// unpacks and names a version declaring that same framing AND that same parity
/// wins. The error reported on total failure is the FIRST candidate's, since
/// that is the one a current artifact was meant to be.
///
/// `erasures` names word positions already known bad, which lets parity repair
/// twice as many of them; see [`take_parity`]. Empty is always valid.
///
/// Returns the version, the payload, and which word positions parity repaired.
fn decode_canonical_with(
    text: &str,
    language: &str,
    wordlist: &str,
    payload_len: Option<usize>,
    erasures: &[usize],
) -> Result<(u8, Vec<u8>, Vec<usize>), CanonicalError> {
    let wl = resolve_wordlist_name(language, wordlist);
    let tree = cached_payload_tree(language, wl)
        .map_err(|e| CanonicalError::Decode(format!("{:?}", e)))?;
    let payload_set: HashSet<String> =
        tree.words().iter().map(|w| w.to_lowercase()).collect();

    let words = payload_tokens(text, |w| payload_set.contains(w));
    if words.is_empty() {
        return Err(CanonicalError::NoPayloadWords);
    }

    // How far a framing candidate got before it failed. A later stage is a
    // better diagnosis, because reaching it means everything before it held.
    const STAGE_PARITY: u8 = 0;
    const STAGE_UNPACK: u8 = 1;
    const STAGE_CHECKSUM: u8 = 2;
    const STAGE_VERSION: u8 = 3;

    let mut first_error: Option<(u8, CanonicalError)> = None;
    for &(envelope, parity) in FRAMINGS {
        // The fixed packing exists only under `PayloadLeading`, so a stated
        // length rules out the other framing entirely — but not the other
        // parity, which still has to be tried.
        if payload_len.is_some() && envelope != Envelope::PayloadLeading {
            continue;
        }

        let attempt = (|| {
            // Parity comes off first: it was added last, over the words, so
            // everything below this line sees exactly the word sequence a
            // parity-less version would have produced.
            let (message, repaired) =
                take_parity(&words, &tree, parity, erasures).map_err(|e| (STAGE_PARITY, e))?;
            let bytes = match payload_len {
                Some(n) => decode_base_n_fixed(
                    &message,
                    &tree,
                    FIXED_ENVELOPE_CODEC,
                    n + envelope.overhead(),
                ),
                None => decode_base_n(&message, &tree, envelope.codec()),
            }
            .map_err(|e| (STAGE_UNPACK, CanonicalError::Decode(format!("{:?}", e))))?;

            // The checksum runs before the version lookup on purpose: damaged
            // words are far likelier than an artifact from the future, so a
            // mangled paragraph should report itself as mangled rather than as
            // an unsupported version it never claimed.
            let (version, payload) = envelope.open(&bytes).map_err(|e| (STAGE_CHECKSUM, e))?;
            let rules = rules_for(version)
                .ok_or((STAGE_VERSION, CanonicalError::UnsupportedVersion(version)))?;
            // A version only vouches for the framing it declares, parity
            // included. Without this the v1 attempt would accept any artifact
            // whose seventh byte happened to read 1 or 2, and the v2 attempt
            // would accept a v3 artifact whose parity it had silently eaten.
            if rules.envelope != envelope {
                return Err((
                    STAGE_VERSION,
                    if payload_len.is_some() {
                        CanonicalError::NoFixedForm(version)
                    } else {
                        CanonicalError::UnsupportedVersion(version)
                    },
                ));
            }
            if rules.parity != parity {
                return Err((STAGE_VERSION, CanonicalError::UnsupportedVersion(version)));
            }
            Ok((version, payload, repaired))
        })();

        match attempt {
            Ok(found) => return Ok(found),
            Err((stage, e)) => {
                // Keep the error that got FURTHEST, not the one that came first.
                //
                // With one framing this distinction did not exist. With three,
                // the newest is tried first and fails earliest on an older
                // artifact, so reporting the first would bury the diagnosis: a
                // paragraph claiming an unknown version would be described as
                // one whose parity did not check, which is both less true and
                // less useful. Getting past the crc32 is real evidence about
                // what the bytes are; failing before it is not. Ties go to the
                // earlier candidate, which is the more current version.
                if first_error.as_ref().is_none_or(|&(s, _)| stage > s) {
                    first_error = Some((stage, e));
                }
            }
        };
    }
    // Unknown version, or nothing that framed cleanly: the payload words may
    // still unpack, but there is no way to verify the wording without a
    // version's rules. Refusing outright is the honest answer —
    // "unverifiable" from a canonical decoder would be indistinguishable from
    // damage.
    Err(first_error.expect("FRAMINGS is never empty").1)
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
    let words = append_parity(words, &tree, rules.parity)?;

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

