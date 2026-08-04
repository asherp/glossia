use crate::merkle::WordlistTree;
use std::fmt;
use num_bigint::BigUint;
use num_traits::Zero;

// ═══════════════════════════════════════════════════════════════════════
// Data Mode
// ═══════════════════════════════════════════════════════════════════════

/// Data encoding mode, used by `detect_mode` to classify input strings
/// for pre-decoding before base-N encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataMode {
    /// Raw bytes, 8 bits per unit. Default and backward-compatible.
    Bytes8 = 0,
    /// 7-bit ASCII. Each byte is packed as 7 bits (high bit dropped).
    /// Auto-detected when all input bytes are < 128. Saves 12.5%.
    Ascii7 = 1,
    /// Pre-decoded base64. The input string is base64-decoded to raw bytes
    /// before encoding at 8 bits/unit. Saves ~25%.
    Base64 = 2,
    /// Pre-decoded hex. The input string is hex-decoded to raw bytes
    /// before encoding at 8 bits/unit. Saves 50%.
    Hex = 3,
}

impl DataMode {}

impl fmt::Display for DataMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataMode::Bytes8 => write!(f, "bytes8"),
            DataMode::Ascii7 => write!(f, "ascii7"),
            DataMode::Base64 => write!(f, "base64"),
            DataMode::Hex => write!(f, "hex"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Errors
// ═══════════════════════════════════════════════════════════════════════

/// Errors that can occur during encoding or decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// A word in the input is not in the wordlist.
    UnknownWord(String),
    /// The input word sequence is empty.
    EmptyInput,
    /// The wordlist is empty.
    EmptyWordlist,
    /// Decoded bytes are not valid UTF-8.
    InvalidUtf8,
    /// The encoded payload is structurally invalid (e.g. inconsistent padding, misaligned bitstream).
    MalformedPayload(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnknownWord(w) => write!(f, "unknown word: {}", w),
            DecodeError::EmptyInput => write!(f, "empty input"),
            DecodeError::EmptyWordlist => write!(f, "empty wordlist"),
            DecodeError::InvalidUtf8 => write!(f, "decoded bytes are not valid UTF-8"),
            DecodeError::MalformedPayload(msg) => write!(f, "malformed payload: {}", msg),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Hex utilities
// ═══════════════════════════════════════════════════════════════════════

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Decode a hex string to bytes. Returns None if odd length or invalid chars.
pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut result = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        result.push((hi << 4) | lo);
    }
    Some(result)
}

/// Encode bytes as lowercase hex string.
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        result.push(HEX[(b >> 4) as usize] as char);
        result.push(HEX[(b & 0x0F) as usize] as char);
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════
// Decode-side tokenization
// ═══════════════════════════════════════════════════════════════════════

/// Normalize a received token the way decoding does: strip non-alphanumeric
/// characters from both ends, then lowercase.
///
/// This is Unicode-aware, because `char::is_alphanumeric` follows Unicode
/// properties. Symbols drawn from *alphanumeric* categories — circled digits
/// (`①`), Greek letters (`β`), Roman numerals (`Ⅻ`), superscripts — are therefore
/// NOT stripped. Whitespace-separated they are harmless (nothing matches them in
/// a payload wordlist), but placed directly against a word with no separating
/// space they stop the trim before it reaches the word, so `①absorb` normalizes
/// to `①absorb` and the payload word is silently lost. Decorative markup around
/// prose must come from non-alphanumeric symbols.
pub fn normalize_token(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Harvest payload words from received text: split on whitespace, normalize each
/// token, and keep the ones `is_payload` accepts.
///
/// This is the canonical decode-side tokenizer — the inverse of embedding payload
/// words in prose. Cover words are dropped simply by failing the predicate, which
/// is why payload and cover wordlists must stay disjoint.
pub fn payload_tokens<F: Fn(&str) -> bool>(text: &str, is_payload: F) -> Vec<String> {
    text.split_whitespace()
        .map(|w| normalize_token(w))
        .filter(|w| !w.is_empty() && is_payload(w))
        .collect()
}

/// Count whitespace-separated tokens with any alphanumeric content.
pub fn alnum_token_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| !normalize_token(w).is_empty())
        .count()
}

/// A declared set of markup characters — opcode sigils, delimiters, any decoration
/// a format places around prose — that decoding must strip rather than read.
///
/// [`normalize_token`] can only strip what is non-alphanumeric, which is a guess:
/// `β` is Unicode category `Ll`, indistinguishable from a payload letter, so
/// `βabsorb` normalizes to itself and the payload word vanishes. Declaring the
/// markup removes the guess — a known symbol is stripped whether or not Unicode
/// considers it a letter.
///
/// The safety condition is not "non-alphanumeric" but "not a character any payload
/// word uses", which [`Markup::new`] checks against the wordlist. Both shipped
/// payload lists are plain `a`–`z`, so symbols, Greek letters and numerals are all
/// admissible; declaring `e` is not.
#[derive(Debug, Clone, Default)]
pub struct Markup {
    chars: std::collections::HashSet<char>,
}

impl Markup {
    /// Declare a markup set, validated against the payload wordlist.
    ///
    /// Returns `Err` with the offending characters if any of them appears inside a
    /// payload word, since stripping such a character would corrupt decoding.
    pub fn new<I, S>(chars: I, payload_words: &[S]) -> Result<Self, Vec<char>>
    where
        I: IntoIterator<Item = char>,
        S: AsRef<str>,
    {
        let chars: std::collections::HashSet<char> = chars.into_iter().collect();
        let alphabet: std::collections::HashSet<char> = payload_words
            .iter()
            .flat_map(|w| w.as_ref().to_lowercase().chars().collect::<Vec<_>>())
            .collect();
        let mut bad: Vec<char> = chars
            .iter()
            .copied()
            .filter(|c| {
                alphabet.contains(c) || c.to_lowercase().any(|lc| alphabet.contains(&lc))
            })
            .collect();
        if !bad.is_empty() {
            bad.sort_unstable();
            return Err(bad);
        }
        Ok(Self { chars })
    }

    /// A markup set with no validation available (no wordlist to check against).
    pub fn unchecked<I: IntoIterator<Item = char>>(chars: I) -> Self {
        Self { chars: chars.into_iter().collect() }
    }

    pub fn contains(&self, c: char) -> bool {
        self.chars.contains(&c)
    }

    /// Normalize a token, stripping declared markup as well as non-alphanumerics.
    pub fn normalize_token(&self, word: &str) -> String {
        word.trim_matches(|c: char| !c.is_alphanumeric() || self.chars.contains(&c))
            .to_lowercase()
    }
}

/// Like [`payload_tokens`], but strips a declared [`Markup`] set from token edges
/// first, so format decoration cannot hide an adjacent payload word.
pub fn payload_tokens_with_markup<F: Fn(&str) -> bool>(
    text: &str,
    markup: &Markup,
    is_payload: F,
) -> Vec<String> {
    text.split_whitespace()
        .map(|w| markup.normalize_token(w))
        .filter(|w| !w.is_empty() && is_payload(w))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Payload integrity
// ═══════════════════════════════════════════════════════════════════════

/// CRC-32 (IEEE 802.3, reflected, polynomial 0xEDB88320).
///
/// Computed bitwise so the polynomial is visible as the specification rather
/// than baked into a lookup table. Used to derive a cover seed from a payload,
/// so that which prose was chosen carries the payload's checksum (see #76).
/// This detects transcription damage; it is not a cryptographic digest and
/// resists no adversary.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// splitmix64 finalizer: scatter a small counter across the whole u64 range so
/// consecutive counter values map to unrelated cover realizations. Mirrors
/// `mix64` in `web/glossia-msg.js`.
pub fn mix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Derive a cover seed from a payload checksum and a fluency counter, so the
/// choice of cover realization carries the checksum at no length cost.
pub fn checksum_seed(payload: &[u8], counter: u64) -> u64 {
    mix64(((crc32(payload) as u64) << 32) | (counter & 0xFFFF_FFFF))
}

// ═══════════════════════════════════════════════════════════════════════
// Base64 utilities
// ═══════════════════════════════════════════════════════════════════════

const B64_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode a standard base64 string. Returns None on invalid input.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 {
            return None;
        }
        let data_len = 4 - pad;
        for &c in &chunk[..data_len] {
            if b64_val(c).is_none() {
                return None;
            }
        }
        for &c in &chunk[data_len..] {
            if c != b'=' {
                return None;
            }
        }
        let v0 = b64_val(chunk[0]).unwrap_or(0) as u32;
        let v1 = b64_val(chunk[1]).unwrap_or(0) as u32;
        let v2 = if pad < 2 { b64_val(chunk[2]).unwrap_or(0) as u32 } else { 0 };
        let v3 = if pad < 1 { b64_val(chunk[3]).unwrap_or(0) as u32 } else { 0 };
        let n = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        result.push((n >> 16) as u8);
        if pad < 2 {
            result.push((n >> 8) as u8);
        }
        if pad < 1 {
            result.push(n as u8);
        }
    }
    Some(result)
}

/// Encode bytes as standard base64 string.
pub fn base64_encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(B64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(B64_CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════
// Format detection
// ═══════════════════════════════════════════════════════════════════════

/// Auto-detect the most compact encoding for a string.
///
/// Detection cascade (narrowest first):
/// 1. **Hex**: even length, all hex chars, length ≥ 2
/// 2. **Base64**: length % 4 == 0, valid base64 alphabet, contains at
///    least one of `+`, `/`, `=` (to avoid false positives on plain text)
/// 3. **Ascii7**: all bytes < 128
/// 4. **Bytes8**: everything else (full UTF-8)
///
/// Returns `(mode, data)` where `data` is the bytes to encode:
/// - Hex/Base64: pre-decoded bytes (fewer!)
/// - Ascii7/Bytes8: original string bytes
pub fn detect_mode(s: &str) -> (DataMode, Vec<u8>) {
    let bytes = s.as_bytes();

    // Empty string: use Bytes8 for backward compat (no savings either way)
    if bytes.is_empty() {
        return (DataMode::Bytes8, Vec::new());
    }

    // 1. Hex: even length, all hex chars, ≥ 2 chars
    if bytes.len() >= 2
        && bytes.len() % 2 == 0
        && bytes.iter().all(|&b| hex_nibble(b).is_some())
    {
        if let Some(decoded) = hex_decode(s) {
            return (DataMode::Hex, decoded);
        }
    }

    // 2. Base64: length % 4 == 0, valid chars, contains +/=/
    if bytes.len() >= 4
        && bytes.len() % 4 == 0
        && bytes.iter().all(|&b| b64_val(b).is_some() || b == b'=')
        && bytes.iter().any(|&b| b == b'+' || b == b'/' || b == b'=')
    {
        if let Some(decoded) = base64_decode(s) {
            return (DataMode::Base64, decoded);
        }
    }

    // 3. ASCII: all bytes < 128
    if bytes.iter().all(|&b| b < 128) {
        return (DataMode::Ascii7, bytes.to_vec());
    }

    // 4. Full UTF-8
    (DataMode::Bytes8, bytes.to_vec())
}

// ═══════════════════════════════════════════════════════════════════════
// Codec dispatch: grammar-controlled via `codec` parameter
// ═══════════════════════════════════════════════════════════════════════

/// Encode bytes into wordlist words.
///
/// The `codec` parameter controls encoding strategy:
/// - `"bitpack"`: fixed-size bit chunks with padding word (uniform distribution, requires power-of-2 wordlist)
/// - `"canonical_bitpack"`: as `"bitpack"`, but the padding word rides last
/// - `"bitpack_fixed"`: fixed-size bit chunks without padding word (for known-length payloads)
/// - `"base_n"` (or anything else): big-integer base-N encoding
pub fn encode_base_n(data: &[u8], wordlist: &WordlistTree, codec: &str) -> Result<Vec<String>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    if data.is_empty() {
        return Ok(Vec::new());
    }
    if wordlist.len() > 1 && wordlist.len().is_power_of_two() {
        match codec {
            "bitpack" => return encode_bitpack(data, wordlist),
            "canonical_bitpack" => return encode_canonical_bitpack(data, wordlist),
            "bitpack_fixed" => return encode_bitpack_fixed(data, wordlist),
            _ => {}
        }
    }
    encode_base_n_bigint(data, wordlist)
}

/// Decode wordlist words back to bytes.
///
/// The `codec` parameter controls decoding strategy:
/// - `"bitpack"`: fixed-size bit chunks with padding word (requires power-of-2 wordlist)
/// - `"canonical_bitpack"`: as `"bitpack"`, but the padding word rides last
/// - `"bitpack_fixed"`: fixed-size bit chunks without padding word (requires `expected_bytes`)
/// - `"base_n"` (or anything else): big-integer base-N decoding
///
/// For `"bitpack_fixed"`, use `decode_base_n_fixed` instead to supply the expected byte count.
pub fn decode_base_n(words: &[String], wordlist: &WordlistTree, codec: &str) -> Result<Vec<u8>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    if words.is_empty() {
        return Ok(Vec::new());
    }
    if wordlist.len() > 1 && wordlist.len().is_power_of_two() {
        match codec {
            "bitpack" => return decode_bitpack(words, wordlist),
            "canonical_bitpack" => return decode_canonical_bitpack(words, wordlist),
            "bitpack_fixed" => {
                // Without expected_bytes, infer from word count (exact when aligned)
                let bits_per_word = wordlist.len().trailing_zeros() as usize;
                let total_bits = words.len() * bits_per_word;
                return decode_bitpack_fixed(words, wordlist, total_bits / 8);
            }
            _ => {}
        }
    }
    decode_base_n_bigint(words, wordlist)
}

/// Decode wordlist words back to bytes with a known payload byte count.
///
/// Use this for `"bitpack_fixed"` codec where the caller knows the original data length.
pub fn decode_base_n_fixed(words: &[String], wordlist: &WordlistTree, codec: &str, expected_bytes: usize) -> Result<Vec<u8>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    if words.is_empty() {
        return Ok(Vec::new());
    }
    if codec == "bitpack_fixed" && wordlist.len() > 1 && wordlist.len().is_power_of_two() {
        decode_bitpack_fixed(words, wordlist, expected_bytes)
    } else {
        decode_base_n(words, wordlist, codec)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Bitpack encoding (for power-of-2 wordlists)
// ═══════════════════════════════════════════════════════════════════════

/// Encode bytes as fixed-size bit chunks, like BIP-39.
///
/// Each chunk independently indexes the full wordlist, giving uniform
/// distribution across all word positions.
///
/// Format: [padding_word] [data_word_0] [data_word_1] ... [data_word_N-1]
///
/// The first word's index encodes the number of padding bits (0..B-1)
/// in the last data word, resolving byte-count ambiguity when
/// bits_per_word > 8.
fn encode_bitpack(data: &[u8], wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    let bits_per_word = wordlist.len().trailing_zeros() as usize;
    let total_data_bits = data.len() * 8;
    let n_data_words = (total_data_bits + bits_per_word - 1) / bits_per_word;
    let pad_bits = n_data_words * bits_per_word - total_data_bits;

    let mut result = Vec::with_capacity(1 + n_data_words);

    // First word: padding count
    result.push(wordlist.get(pad_bits).unwrap().clone());

    // Data words: extract bits_per_word bits at a time, MSB-first
    for i in 0..n_data_words {
        let bit_offset = i * bits_per_word;
        let mut index: usize = 0;
        for b in 0..bits_per_word {
            let global_bit = bit_offset + b;
            let bit_val = if global_bit < total_data_bits {
                // Extract bit from data (MSB-first within each byte)
                let byte_idx = global_bit / 8;
                let bit_idx = 7 - (global_bit % 8);
                ((data[byte_idx] >> bit_idx) & 1) as usize
            } else {
                0 // zero-pad on the right
            };
            index = (index << 1) | bit_val;
        }
        result.push(wordlist.get(index).unwrap().clone());
    }

    Ok(result)
}

/// Decode bitpacked words back to bytes.
///
/// First word index = padding bit count.
/// Remaining words: concatenate their bits, strip padding, extract bytes.
fn decode_bitpack(words: &[String], wordlist: &WordlistTree) -> Result<Vec<u8>, DecodeError> {
    use std::collections::HashMap;

    let bits_per_word = wordlist.len().trailing_zeros() as usize;

    // Build reverse index (case-insensitive)
    let mut word_to_index: HashMap<String, usize> = HashMap::new();
    for (i, word) in wordlist.words().iter().enumerate() {
        word_to_index.insert(word.to_lowercase(), i);
    }

    // First word: padding count
    let pad_bits = *word_to_index.get(&words[0].to_lowercase())
        .ok_or_else(|| DecodeError::UnknownWord(words[0].clone()))? as usize;

    if pad_bits >= bits_per_word {
        return Err(DecodeError::UnknownWord(format!(
            "invalid padding count {} (max {})", pad_bits, bits_per_word - 1
        )));
    }

    let data_words = &words[1..];
    if data_words.is_empty() {
        // pad_bits must be 0 for empty data
        return Ok(Vec::new());
    }

    let total_bits = data_words.len() * bits_per_word;
    let data_bits = total_bits - pad_bits;
    if data_bits % 8 != 0 {
        return Err(DecodeError::MalformedPayload(format!(
            "decode_bitpack: data_bits={} not byte-aligned (pad_bits={}, total_bits={})",
            data_bits, pad_bits, total_bits
        )));
    }
    let n_bytes = data_bits / 8;

    // Collect all indices
    let mut indices = Vec::with_capacity(data_words.len());
    for word in data_words {
        let idx = word_to_index.get(&word.to_lowercase())
            .ok_or_else(|| DecodeError::UnknownWord(word.clone()))?;
        indices.push(*idx);
    }

    // Reconstruct bytes from the bitstream
    let mut result = vec![0u8; n_bytes];
    for (word_i, &idx) in indices.iter().enumerate() {
        for b in 0..bits_per_word {
            let global_bit = word_i * bits_per_word + b;
            if global_bit >= data_bits {
                break;
            }
            let bit_val = (idx >> (bits_per_word - 1 - b)) & 1;
            if bit_val == 1 {
                let byte_idx = global_bit / 8;
                let bit_idx = 7 - (global_bit % 8);
                result[byte_idx] |= 1 << bit_idx;
            }
        }
    }

    Ok(result)
}

// ═══════════════════════════════════════════════════════════════════════
// Canonical bitpack encoding (padding word last)
// ═══════════════════════════════════════════════════════════════════════

/// Encode bytes exactly as [`encode_bitpack`] does, but with the padding word
/// riding LAST instead of first.
///
/// Format: `[data_word_0] ... [data_word_N-1] [padding_word]`
///
/// Same bits, same padding count, same round trip -- only the position of the
/// envelope word differs, and the reason is what a reader sees. The padding
/// count depends solely on the payload's LENGTH, so for any fixed-size field it
/// is a constant, and at the front of the list it becomes a constant FIRST word
/// of the prose: every 32-byte hash opened with the wordlist's word 0. Moving it
/// to the tail puts payload entropy in the opening words, where a reader starts,
/// and leaves the fixed word where a paragraph is already trailing off.
fn encode_canonical_bitpack(data: &[u8], wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    let mut words = encode_bitpack(data, wordlist)?;
    if !words.is_empty() {
        let pad = words.remove(0);
        words.push(pad);
    }
    Ok(words)
}

/// Decode [`encode_canonical_bitpack`] output: rotate the trailing padding word
/// back to the front and hand the rest to the shared bitpack decoder, so the two
/// layouts can never drift apart in their bit handling.
fn decode_canonical_bitpack(words: &[String], wordlist: &WordlistTree) -> Result<Vec<u8>, DecodeError> {
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let mut rotated = Vec::with_capacity(words.len());
    rotated.push(words[words.len() - 1].clone());
    rotated.extend_from_slice(&words[..words.len() - 1]);
    decode_bitpack(&rotated, wordlist)
}

// ═══════════════════════════════════════════════════════════════════════
// Bitpack fixed encoding (no padding word, for known-length payloads)
// ═══════════════════════════════════════════════════════════════════════

/// Encode bytes as fixed-size bit chunks without a padding word.
///
/// For known-length payloads (e.g. 32-byte pubkeys), the caller already knows
/// the byte count, so the padding word is unnecessary overhead.
///
/// Format: [data_word_0] [data_word_1] ... [data_word_N-1]
fn encode_bitpack_fixed(data: &[u8], wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    let bits_per_word = wordlist.len().trailing_zeros() as usize;
    let total_data_bits = data.len() * 8;
    let n_data_words = (total_data_bits + bits_per_word - 1) / bits_per_word;

    let mut result = Vec::with_capacity(n_data_words);

    for i in 0..n_data_words {
        let bit_offset = i * bits_per_word;
        let mut index: usize = 0;
        for b in 0..bits_per_word {
            let global_bit = bit_offset + b;
            let bit_val = if global_bit < total_data_bits {
                let byte_idx = global_bit / 8;
                let bit_idx = 7 - (global_bit % 8);
                ((data[byte_idx] >> bit_idx) & 1) as usize
            } else {
                0
            };
            index = (index << 1) | bit_val;
        }
        result.push(wordlist.get(index).unwrap().clone());
    }

    Ok(result)
}

/// Decode bitpack-fixed words back to bytes.
///
/// The caller must supply `expected_bytes` — the original payload byte count.
/// This resolves the bit-alignment ambiguity without needing a padding word.
fn decode_bitpack_fixed(words: &[String], wordlist: &WordlistTree, expected_bytes: usize) -> Result<Vec<u8>, DecodeError> {
    use std::collections::HashMap;

    let bits_per_word = wordlist.len().trailing_zeros() as usize;
    let data_bits = expected_bytes * 8;

    let mut word_to_index: HashMap<String, usize> = HashMap::new();
    for (i, word) in wordlist.words().iter().enumerate() {
        word_to_index.insert(word.to_lowercase(), i);
    }

    let mut indices = Vec::with_capacity(words.len());
    for word in words {
        let idx = word_to_index.get(&word.to_lowercase())
            .ok_or_else(|| DecodeError::UnknownWord(word.clone()))?;
        indices.push(*idx);
    }

    let mut result = vec![0u8; expected_bytes];
    for (word_i, &idx) in indices.iter().enumerate() {
        for b in 0..bits_per_word {
            let global_bit = word_i * bits_per_word + b;
            if global_bit >= data_bits {
                break;
            }
            let bit_val = (idx >> (bits_per_word - 1 - b)) & 1;
            if bit_val == 1 {
                let byte_idx = global_bit / 8;
                let bit_idx = 7 - (global_bit % 8);
                result[byte_idx] |= 1 << bit_idx;
            }
        }
    }

    Ok(result)
}

// ═══════════════════════════════════════════════════════════════════════
// Base-N Encoding (for non-power-of-2 wordlists)
// ═══════════════════════════════════════════════════════════════════════

/// Encode bytes as base-N big-integer representation.
///
/// Used for non-power-of-2 wordlists where bitpacking doesn't apply.
fn encode_base_n_bigint(data: &[u8], wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    let base = wordlist.len();
    let mut num = BigUint::from_bytes_be(data);
    let mut result = Vec::new();
    let base_uint = BigUint::from(base);

    while num > BigUint::zero() {
        let remainder = &num % &base_uint;
        let digit = if remainder.is_zero() {
            0
        } else {
            remainder.to_u64_digits()[0] as usize
        };
        result.push(wordlist.get(digit).unwrap().clone());
        num /= &base_uint;
    }

    result.reverse();

    // Preserve leading zero bytes (important for round-trip)
    let leading_zeros = data.iter().take_while(|&&b| b == 0).count();
    for _ in 0..leading_zeros {
        result.insert(0, wordlist.get(0).unwrap().clone());
    }

    Ok(result)
}

/// Decode base-N big-integer representation back to bytes.
fn decode_base_n_bigint(words: &[String], wordlist: &WordlistTree) -> Result<Vec<u8>, DecodeError> {
    use std::collections::HashMap;

    let base = wordlist.len();
    let mut num = BigUint::zero();
    let base_uint = BigUint::from(base);

    // Build reverse index (case-insensitive to match WordlistTree behavior)
    let mut word_to_index: HashMap<String, usize> = HashMap::new();
    for (i, word) in wordlist.words().iter().enumerate() {
        word_to_index.insert(word.to_lowercase(), i);
    }

    // Convert from base-N to big integer
    for word in words {
        let digit = word_to_index.get(&word.to_lowercase())
            .ok_or_else(|| DecodeError::UnknownWord(word.clone()))?;
        num = num * &base_uint + BigUint::from(*digit);
    }

    // Convert to bytes
    let bytes = num.to_bytes_be();

    // Restore leading zeros
    let leading_zeros = words.iter().take_while(|w| {
        word_to_index.get(&w.to_lowercase()).map(|&i| i == 0).unwrap_or(false)
    }).count();

    let mut result = vec![0u8; leading_zeros];
    result.extend_from_slice(&bytes);

    Ok(result)
}

/// Encode string with auto-mode detection.
///
/// The `codec` parameter controls encoding strategy ("bitpack" or "base_n").
pub fn encode_str_base_n(s: &str, wordlist: &WordlistTree, codec: &str) -> Result<(Vec<String>, DataMode), DecodeError> {
    let (mode, data) = detect_mode(s);
    let words = encode_base_n(&data, wordlist, codec)?;
    Ok((words, mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vector() {
        // The canonical CRC-32/ISO-HDLC check value for "123456789".
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn crc32_detects_single_bit_flip() {
        let a = [0x75u8, 0x1e, 0x76, 0xe8];
        let mut b = a;
        b[2] ^= 0x01;
        assert_ne!(crc32(&a), crc32(&b));
    }

    #[test]
    fn checksum_seed_varies_with_payload_and_counter() {
        let p = b"locking script";
        assert_ne!(checksum_seed(p, 0), checksum_seed(p, 1));
        assert_ne!(checksum_seed(p, 0), checksum_seed(b"locking scripts", 0));
        // Deterministic.
        assert_eq!(checksum_seed(p, 7), checksum_seed(p, 7));
    }

    #[test]
    fn normalize_token_strips_and_lowercases() {
        assert_eq!(normalize_token("Absorb."), "absorb");
        assert_eq!(normalize_token("\"quote,\""), "quote");
        assert_eq!(normalize_token("---"), "");
    }

    #[test]
    fn normalize_token_strips_non_alphanumeric_symbols_even_when_adjacent() {
        // Opcode sigils must come from non-alphanumeric Unicode categories.
        for sym in ["\u{29C9}", "\u{2317}", "\u{225F}", "\u{2713}", "\u{25BD}", "\u{25B3}"] {
            assert_eq!(normalize_token(&format!("{sym}absorb")), "absorb", "sym {sym}");
            assert_eq!(normalize_token(sym), "", "sym {sym} alone");
        }
    }

    #[test]
    fn alphanumeric_symbols_swallow_an_adjacent_payload_word() {
        // Circled digits, Greek letters and Roman numerals are alphanumeric under
        // Unicode, so the trim stops at them. Separated by a space they are
        // harmless; touching a word they hide it. This is why the address format
        // restricts opcode symbols to non-alphanumeric characters.
        let is_payload = |w: &str| w == "absorb";
        for sym in ["\u{2460}", "\u{03B2}", "\u{216B}"] {
            assert_eq!(payload_tokens(&format!("{sym} absorb"), is_payload), vec!["absorb"]);
            assert!(
                payload_tokens(&format!("{sym}absorb"), is_payload).is_empty(),
                "adjacent {sym} should hide the payload word"
            );
        }
    }

    #[test]
    fn declared_markup_rescues_an_adjacent_alphanumeric_symbol() {
        // The case normalize_token cannot handle: beta is Unicode Ll, so trimming
        // by category leaves it attached and the payload word is lost. Declaring
        // it as markup strips it.
        let wl = ["absorb".to_string(), "victory".to_string()];
        let markup = Markup::new(['\u{03B2}', '\u{2460}', '\u{25BD}'], &wl).unwrap();
        let is_payload = |w: &str| w == "absorb";

        assert!(payload_tokens("\u{03B2}absorb", is_payload).is_empty(), "undeclared: lost");
        assert_eq!(
            payload_tokens_with_markup("\u{03B2}absorb", &markup, is_payload),
            vec!["absorb"],
            "declared: recovered"
        );
        assert_eq!(
            payload_tokens_with_markup("\u{2460}absorb \u{25BD} victory", &markup, is_payload),
            vec!["absorb"]
        );
    }

    #[test]
    fn markup_rejects_characters_payload_words_use() {
        let wl = ["absorb".to_string(), "victory".to_string()];
        // 'e' does not occur, 'a' and 'b' do.
        assert!(Markup::new(['e'], &wl).is_ok());
        assert_eq!(Markup::new(['a', 'b', '\u{25BD}'], &wl).unwrap_err(), vec!['a', 'b']);
        // Case-folded: declaring 'A' would still strip 'a'.
        assert_eq!(Markup::new(['A'], &wl).unwrap_err(), vec!['A']);
    }

    #[test]
    fn markup_leaves_undeclared_text_alone() {
        let wl = ["absorb".to_string()];
        let markup = Markup::new(['\u{25BD}'], &wl).unwrap();
        assert_eq!(markup.normalize_token("Absorb."), "absorb");
        assert_eq!(markup.normalize_token("\u{25BD}absorb"), "absorb");
        // A markup char inside a word is not an edge, so the token stays corrupt
        // and is dropped rather than silently mis-parsed.
        assert_eq!(markup.normalize_token("ab\u{25BD}sorb"), "ab\u{25BD}sorb");
    }

    #[test]
    fn payload_tokens_drops_cover_and_keeps_order() {
        let payload: std::collections::HashSet<&str> =
            ["insect", "victory", "ring"].into_iter().collect();
        assert_eq!(
            payload_tokens("Insect may see victory to ring.", |w| payload.contains(w)),
            vec!["insect", "victory", "ring"]
        );
    }

    #[test]
    fn alnum_token_count_ignores_pure_punctuation() {
        assert_eq!(alnum_token_count("a b c"), 3);
        assert_eq!(alnum_token_count("\u{25BD} a b"), 2);
    }

    use super::*;

    /// Build a toy wordlist of arbitrary size for testing base-N.
    fn make_wordlist(n: usize) -> WordlistTree {
        let words: Vec<String> = (0..n).map(|i| format!("w{}", i)).collect();
        WordlistTree::new(words)
    }

    // ── Base-N round-trip tests ─────────────────────────────────────

    #[test]
    fn base_n_round_trip_empty() {
        let wl = make_wordlist(100);
        let encoded = encode_base_n(&[], &wl, "base_n").unwrap();
        assert!(encoded.is_empty(), "empty data => no words");
        let decoded = decode_base_n(&encoded, &wl, "base_n").unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn base_n_round_trip_one_byte() {
        let wl = make_wordlist(45); // non-power-of-2
        let data = vec![0xAB];
        let encoded = encode_base_n(&data, &wl, "base_n").unwrap();
        let decoded = decode_base_n(&encoded, &wl, "base_n").unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base_n_round_trip_various_sizes() {
        let wl = make_wordlist(2048); // BIP39 size — force base_n here
        for len in 1..=64 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 13) as u8).collect();
            let encoded = encode_base_n(&data, &wl, "base_n").unwrap();
            let decoded = decode_base_n(&encoded, &wl, "base_n").unwrap();
            assert_eq!(decoded, data, "failed round-trip for {} bytes", len);
        }
    }

    #[test]
    fn base_n_round_trip_leading_zeros() {
        let wl = make_wordlist(100); // non-power-of-2
        let data = vec![0x00, 0x00, 0x01, 0xFF];
        let encoded = encode_base_n(&data, &wl, "base_n").unwrap();
        let decoded = decode_base_n(&encoded, &wl, "base_n").unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base_n_round_trip_non_power_of_two() {
        // Non-power-of-2 sizes only — these use base-N bigint
        for n in &[3, 17, 45, 100, 1000] {
            let wl = make_wordlist(*n);
            let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x23];
            let encoded = encode_base_n(&data, &wl, "base_n").unwrap();
            let decoded = decode_base_n(&encoded, &wl, "base_n").unwrap();
            assert_eq!(decoded, data, "failed for wordlist size {}", n);
        }
    }

    // ── Bitpack-specific tests ────────────────────────────────────────

    #[test]
    fn bitpack_round_trip_various_wordlist_sizes() {
        // All power-of-2 sizes that matter
        for bits in &[2, 3, 4, 5, 6, 7, 11, 13, 15, 17] {
            let n: usize = 1 << bits;
            let wl = make_wordlist(n);
            for data_len in 1..=32 {
                let data: Vec<u8> = (0..data_len).map(|i| (i * 37 + 13) as u8).collect();
                let encoded = encode_base_n(&data, &wl, "bitpack").unwrap();
                let decoded = decode_base_n(&encoded, &wl, "bitpack").unwrap();
                assert_eq!(decoded, data,
                    "failed round-trip for {} bytes with wordlist size {}", data_len, n);
            }
        }
    }

    #[test]
    fn bitpack_round_trip_leading_zeros() {
        let wl = make_wordlist(2048);
        let data = vec![0x00, 0x00, 0x01, 0xFF];
        let encoded = encode_base_n(&data, &wl, "bitpack").unwrap();
        let decoded = decode_base_n(&encoded, &wl, "bitpack").unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bitpack_round_trip_all_zeros() {
        let wl = make_wordlist(32768);
        let data = vec![0x00; 16];
        let encoded = encode_base_n(&data, &wl, "bitpack").unwrap();
        let decoded = decode_base_n(&encoded, &wl, "bitpack").unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bitpack_round_trip_all_ones() {
        let wl = make_wordlist(2048);
        let data = vec![0xFF; 16];
        let encoded = encode_base_n(&data, &wl, "bitpack").unwrap();
        let decoded = decode_base_n(&encoded, &wl, "bitpack").unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bitpack_round_trip_no_padding_case() {
        // 8 bits / 4 bits_per_word = 2 words, 0 padding
        let wl = make_wordlist(16); // 4 bits per word
        let data = vec![0xAB]; // 8 bits = 2 words * 4 bits, pad=0
        let encoded = encode_base_n(&data, &wl, "bitpack").unwrap();
        // First word is padding word (index 0), then 2 data words
        assert_eq!(encoded.len(), 3);
        let decoded = decode_base_n(&encoded, &wl, "bitpack").unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bitpack_padding_word_present() {
        // Verify the first word is the padding word
        let wl = make_wordlist(2048); // 11 bits per word
        let data = vec![0xAB]; // 8 bits, ceil(8/11) = 1 word, pad = 11-8 = 3
        let encoded = encode_base_n(&data, &wl, "bitpack").unwrap();
        assert_eq!(encoded.len(), 2); // 1 padding word + 1 data word
        assert_eq!(encoded[0], "w3"); // pad_bits = 3
    }

    #[test]
    fn bitpack_uniform_distribution() {
        // With bitpacking, the first DATA word should span the full range,
        // not just 0-255 like base-N
        let wl = make_wordlist(32768); // 15 bits per word
        let mut seen_high = false;
        for i in 0u8..=255 {
            let data = vec![i, 0x00];
            let encoded = encode_base_n(&data, &wl, "bitpack").unwrap();
            // Second word (first data word) should have the high bits from the data
            // For data [i, 0x00], first 15 bits = i<<7, index = i*128
            // When i >= 2, index >= 256, which base-N would never produce for the first word
            let word = &encoded[1]; // skip padding word
            let idx: usize = word.strip_prefix("w").unwrap().parse().unwrap();
            if idx >= 256 {
                seen_high = true;
                break;
            }
        }
        assert!(seen_high, "bitpack first data word should span full wordlist range");
    }

    // ── Bitpack-fixed tests ───────────────────────────────────────────

    #[test]
    fn bitpack_fixed_no_padding_word() {
        let wl = make_wordlist(65536); // 16 bits per word
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF]; // 32 bits = 2 words exactly
        let encoded = encode_base_n(&data, &wl, "bitpack_fixed").unwrap();
        assert_eq!(encoded.len(), 2, "no padding word, just 2 data words");
        // Compare with bitpack which adds a padding word
        let encoded_padded = encode_base_n(&data, &wl, "bitpack").unwrap();
        assert_eq!(encoded_padded.len(), 3, "bitpack has padding word + 2 data words");
    }

    #[test]
    fn bitpack_fixed_round_trip_aligned() {
        // 32 bytes with 16-bit words: 256/16 = 16 words, no remainder
        let wl = make_wordlist(65536);
        let data: Vec<u8> = (0..32).map(|i| (i * 37 + 13) as u8).collect();
        let encoded = encode_base_n(&data, &wl, "bitpack_fixed").unwrap();
        assert_eq!(encoded.len(), 16);
        let decoded = decode_base_n_fixed(&encoded, &wl, "bitpack_fixed", 32).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bitpack_fixed_round_trip_unaligned() {
        // 32 bytes with 15-bit words: ceil(256/15) = 18 words, 14 padding bits
        let wl = make_wordlist(32768); // 15 bits per word
        let data: Vec<u8> = (0..32).map(|i| (i * 37 + 13) as u8).collect();
        let encoded = encode_base_n(&data, &wl, "bitpack_fixed").unwrap();
        assert_eq!(encoded.len(), 18);
        let decoded = decode_base_n_fixed(&encoded, &wl, "bitpack_fixed", 32).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bitpack_fixed_constant_word_count() {
        // The whole point: all 32-byte payloads produce the same number of words
        let wl = make_wordlist(65536);
        let mut word_counts = std::collections::HashSet::new();
        for seed in 0u8..=255 {
            let mut data = vec![0u8; 32];
            data[0] = seed;
            let encoded = encode_base_n(&data, &wl, "bitpack_fixed").unwrap();
            word_counts.insert(encoded.len());
        }
        assert_eq!(word_counts.len(), 1, "all 32-byte payloads must produce same word count");
        assert!(word_counts.contains(&16));
    }

    #[test]
    fn bitpack_fixed_round_trip_leading_zeros() {
        let wl = make_wordlist(65536);
        let data = vec![0x00, 0x00, 0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
        let encoded = encode_base_n(&data, &wl, "bitpack_fixed").unwrap();
        assert_eq!(encoded.len(), 4); // 64 bits / 16 = 4 words
        let decoded = decode_base_n_fixed(&encoded, &wl, "bitpack_fixed", 8).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bitpack_fixed_round_trip_all_zeros() {
        let wl = make_wordlist(65536);
        let data = vec![0x00; 32];
        let encoded = encode_base_n(&data, &wl, "bitpack_fixed").unwrap();
        assert_eq!(encoded.len(), 16);
        let decoded = decode_base_n_fixed(&encoded, &wl, "bitpack_fixed", 32).unwrap();
        assert_eq!(decoded, data);
    }

    // ── Base-N error tests ──────────────────────────────────────────

    #[test]
    fn base_n_error_empty_wordlist() {
        let wl = WordlistTree::new(vec![]);
        assert_eq!(encode_base_n(&[1, 2, 3], &wl, "base_n"), Err(DecodeError::EmptyWordlist));
        assert_eq!(
            decode_base_n(&["x".to_string()], &wl, "base_n"),
            Err(DecodeError::EmptyWordlist)
        );
    }

    #[test]
    fn base_n_error_unknown_word() {
        let wl = make_wordlist(16);
        let words = vec!["not_in_list".to_string()];
        assert_eq!(
            decode_base_n(&words, &wl, "base_n"),
            Err(DecodeError::UnknownWord("not_in_list".to_string()))
        );
    }

    // ── encode_str_base_n tests ─────────────────────────────────────

    #[test]
    fn base_n_str_round_trip_hex() {
        let wl = make_wordlist(2048);
        let hex = "deadbeef01234567";
        let (encoded, mode) = encode_str_base_n(hex, &wl, "bitpack").unwrap();
        assert_eq!(mode, DataMode::Hex);
        let decoded = decode_base_n(&encoded, &wl, "bitpack").unwrap();
        assert_eq!(hex_encode(&decoded), hex.to_lowercase());
    }

    #[test]
    fn base_n_str_round_trip_ascii() {
        let wl = make_wordlist(2048);
        let text = "Hello, Glossia!";
        let (encoded, mode) = encode_str_base_n(text, &wl, "bitpack").unwrap();
        assert_eq!(mode, DataMode::Ascii7);
        let decoded = decode_base_n(&encoded, &wl, "bitpack").unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), text);
    }

    // ── Hex utilities ───────────────────────────────────────────────

    #[test]
    fn hex_round_trip() {
        assert_eq!(hex_decode("deadbeef"), Some(vec![0xDE, 0xAD, 0xBE, 0xEF]));
        assert_eq!(hex_encode(&[0xDE, 0xAD, 0xBE, 0xEF]), "deadbeef");
        assert_eq!(hex_decode("00ff42"), Some(vec![0x00, 0xFF, 0x42]));
        assert_eq!(hex_encode(&[0x00, 0xFF, 0x42]), "00ff42");
        // Odd length
        assert_eq!(hex_decode("abc"), None);
        // Invalid chars
        assert_eq!(hex_decode("zz"), None);
        // Empty
        assert_eq!(hex_decode(""), Some(vec![]));
        assert_eq!(hex_encode(&[]), "");
    }

    // ── Base64 utilities ────────────────────────────────────────────

    #[test]
    fn base64_round_trip() {
        // "Hello" → "SGVsbG8="
        let encoded = base64_encode(b"Hello");
        assert_eq!(encoded, "SGVsbG8=");
        assert_eq!(base64_decode(&encoded), Some(b"Hello".to_vec()));

        // Padding variants
        assert_eq!(base64_encode(b"He"), "SGU=");
        assert_eq!(base64_decode("SGU="), Some(b"He".to_vec()));
        assert_eq!(base64_encode(b"Hel"), "SGVs");
        assert_eq!(base64_decode("SGVs"), Some(b"Hel".to_vec()));

        // Empty
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_decode(""), Some(vec![]));

        // Invalid
        assert_eq!(base64_decode("!!!"), None);
        assert_eq!(base64_decode("AB"), None); // not multiple of 4
    }

    // ── Format detection ────────────────────────────────────────────

    #[test]
    fn detect_hex() {
        let (mode, data) = detect_mode("deadbeef");
        assert_eq!(mode, DataMode::Hex);
        assert_eq!(data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn detect_hex_uppercase() {
        let (mode, data) = detect_mode("DEADBEEF");
        assert_eq!(mode, DataMode::Hex);
        assert_eq!(data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn detect_hex_mixed_case() {
        let (mode, data) = detect_mode("DeAdBeEf");
        assert_eq!(mode, DataMode::Hex);
        assert_eq!(data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn detect_base64() {
        let (mode, data) = detect_mode("SGVsbG8=");
        assert_eq!(mode, DataMode::Base64);
        assert_eq!(data, b"Hello".to_vec());
    }

    #[test]
    fn detect_base64_with_plus() {
        let (mode, _data) = detect_mode("abc+defg");
        assert_eq!(mode, DataMode::Base64);
    }

    #[test]
    fn detect_ascii() {
        let (mode, data) = detect_mode("Hello, World!");
        assert_eq!(mode, DataMode::Ascii7);
        assert_eq!(data, b"Hello, World!".to_vec());
    }

    #[test]
    fn detect_utf8() {
        let (mode, data) = detect_mode("café");
        assert_eq!(mode, DataMode::Bytes8);
        assert_eq!(data, "café".as_bytes().to_vec());
    }

    #[test]
    fn detect_empty() {
        let (mode, data) = detect_mode("");
        assert_eq!(mode, DataMode::Bytes8);
        assert!(data.is_empty());
    }

    #[test]
    fn detect_odd_hex_falls_to_ascii() {
        let (mode, _) = detect_mode("abc");
        assert_eq!(mode, DataMode::Ascii7);
    }

    #[test]
    fn detect_no_special_base64_chars_falls_to_hex_or_ascii() {
        let (mode, _) = detect_mode("ABCD");
        assert_eq!(mode, DataMode::Hex);

        let (mode, _) = detect_mode("Hello World");
        assert_eq!(mode, DataMode::Ascii7);
    }
}
