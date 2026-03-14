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
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnknownWord(w) => write!(f, "unknown word: {}", w),
            DecodeError::EmptyInput => write!(f, "empty input"),
            DecodeError::EmptyWordlist => write!(f, "empty wordlist"),
            DecodeError::InvalidUtf8 => write!(f, "decoded bytes are not valid UTF-8"),
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
/// - `"bitpack"`: fixed-size bit chunks (uniform distribution, requires power-of-2 wordlist)
/// - `"base_n"` (or anything else): big-integer base-N encoding
pub fn encode_base_n(data: &[u8], wordlist: &WordlistTree, codec: &str) -> Result<Vec<String>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    if data.is_empty() {
        return Ok(Vec::new());
    }
    if codec == "bitpack" && wordlist.len() > 1 && wordlist.len().is_power_of_two() {
        encode_bitpack(data, wordlist)
    } else {
        encode_base_n_bigint(data, wordlist)
    }
}

/// Decode wordlist words back to bytes.
///
/// The `codec` parameter controls decoding strategy:
/// - `"bitpack"`: fixed-size bit chunks (requires power-of-2 wordlist)
/// - `"base_n"` (or anything else): big-integer base-N decoding
pub fn decode_base_n(words: &[String], wordlist: &WordlistTree, codec: &str) -> Result<Vec<u8>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    if words.is_empty() {
        return Ok(Vec::new());
    }
    if codec == "bitpack" && wordlist.len() > 1 && wordlist.len().is_power_of_two() {
        decode_bitpack(words, wordlist)
    } else {
        decode_base_n_bigint(words, wordlist)
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
