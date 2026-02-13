use crate::merkle::WordlistTree;
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════
// Data Mode
// ═══════════════════════════════════════════════════════════════════════

/// Encoding mode embedded in the wire format header word.
///
/// The header word's wordlist index encodes both the mode and padding:
///   `header_index = mode_value * b + padding`
/// where `b = log2(wordlist_size)` and `padding` is the number of
/// zero-padded trailing bits (0 to b-1).
///
/// **Backward compatibility**: mode 0 produces header indices 0..b-1,
/// identical to the original format. Old decoders reject mode ≥ 1 with
/// `InvalidPadding`, so they fail loudly rather than silently corrupting.
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

impl DataMode {
    /// Bits per input unit for this mode.
    pub fn bits_per_unit(&self) -> usize {
        match self {
            DataMode::Ascii7 => 7,
            _ => 8,
        }
    }

    /// Reconstruct from numeric mode value (as stored in header word).
    pub fn from_mode_val(v: usize) -> Option<Self> {
        match v {
            0 => Some(DataMode::Bytes8),
            1 => Some(DataMode::Ascii7),
            2 => Some(DataMode::Base64),
            3 => Some(DataMode::Hex),
            _ => None,
        }
    }

    /// Whether this mode's header indices fit within a wordlist of `2^b` entries.
    ///
    /// Requires `(mode_val + 1) * b <= 2^b`. For b=11 (BIP39), all 4 modes
    /// fit easily (max header = 43 < 2048). For b < 4, some modes may not fit.
    pub fn fits_in_wordlist(self, b: usize) -> bool {
        if b == 0 {
            return self as usize == 0; // Only Bytes8 for degenerate 1-word list
        }
        let mode_val = self as usize;
        (mode_val + 1).checked_mul(b).map_or(false, |max| max <= (1usize << b))
    }
}

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
    /// The wordlist size is not a power of two.
    NonPowerOfTwo(usize),
    /// The header word encodes an unsupported mode (mode_val ≥ 4).
    InvalidPadding(usize),
    /// After removing padding bits the total is not a multiple of `bits_per_unit`.
    DataNotByteAligned { total_bits: usize, padding: usize },
    /// Decoded bytes are not valid UTF-8 (only from `decode_str`).
    InvalidUtf8,
    /// The requested DataMode does not fit in this wordlist's header space.
    UnsupportedMode { mode: DataMode, wordlist_bits: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnknownWord(w) => write!(f, "unknown word: {}", w),
            DecodeError::EmptyInput => write!(f, "empty input"),
            DecodeError::EmptyWordlist => write!(f, "empty wordlist"),
            DecodeError::NonPowerOfTwo(n) => {
                write!(f, "wordlist size {} is not a power of two", n)
            }
            DecodeError::InvalidPadding(p) => write!(f, "invalid padding value: {}", p),
            DecodeError::DataNotByteAligned {
                total_bits,
                padding,
            } => write!(
                f,
                "data not byte-aligned: {} total bits - {} padding",
                total_bits, padding
            ),
            DecodeError::InvalidUtf8 => write!(f, "decoded bytes are not valid UTF-8"),
            DecodeError::UnsupportedMode { mode, wordlist_bits } => write!(
                f,
                "mode {} does not fit in {}-bit wordlist header space",
                mode, wordlist_bits
            ),
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
// Internal helpers
// ═══════════════════════════════════════════════════════════════════════

/// Return `log2(n)` if `n` is a power of two, otherwise `None`.
fn log2_exact(n: usize) -> Option<usize> {
    if n == 0 || (n & (n - 1)) != 0 {
        return None;
    }
    Some(n.trailing_zeros() as usize)
}

// ═══════════════════════════════════════════════════════════════════════
// Encoding
// ═══════════════════════════════════════════════════════════════════════

/// Encode data with an explicit [`DataMode`].
///
/// Wire format: `[header_word] [data_word_0] ... [data_word_{N-1}]`
///
/// The header word's wordlist index encodes both mode and padding:
///   `header_index = mode_value * b + padding`
///
/// For modes 2/3, `data` should be the **pre-decoded** bytes (e.g., output
/// of `hex_decode`). For mode 1, `data` should be ASCII bytes (high bit
/// is masked off during packing). For mode 0, `data` is raw bytes.
pub fn encode_with_mode(
    data: &[u8],
    wordlist: &WordlistTree,
    mode: DataMode,
) -> Result<Vec<String>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    let b = log2_exact(wordlist.len()).ok_or(DecodeError::NonPowerOfTwo(wordlist.len()))?;

    if b == 0 {
        // 1-word wordlist: can only represent empty data in Bytes8 mode
        if data.is_empty() && mode == DataMode::Bytes8 {
            return Ok(vec![wordlist.get(0).unwrap().clone()]);
        }
        return Err(DecodeError::UnsupportedMode { mode, wordlist_bits: 0 });
    }

    if !mode.fits_in_wordlist(b) {
        return Err(DecodeError::UnsupportedMode { mode, wordlist_bits: b });
    }

    let mode_val = mode as usize;
    let bpu = mode.bits_per_unit();

    // Empty data
    if data.is_empty() {
        let header_index = mode_val * b; // padding = 0
        return Ok(vec![wordlist.get(header_index).unwrap().clone()]);
    }

    let total_bits = data.len() * bpu;
    let n_words = (total_bits + b - 1) / b;
    let padding = n_words * b - total_bits;
    let header_index = mode_val * b + padding;

    let mut result = Vec::with_capacity(1 + n_words);
    result.push(wordlist.get(header_index).unwrap().clone());

    // Bit-pack data into b-bit word indices
    let mut bit_buffer: u64 = 0;
    let mut bits_in_buffer: usize = 0;
    let mut byte_idx = 0;

    for _ in 0..n_words {
        while bits_in_buffer < b && byte_idx < data.len() {
            let unit = if bpu == 7 {
                (data[byte_idx] & 0x7F) as u64
            } else {
                data[byte_idx] as u64
            };
            bit_buffer = (bit_buffer << bpu) | unit;
            bits_in_buffer += bpu;
            byte_idx += 1;
        }

        if bits_in_buffer >= b {
            let shift = bits_in_buffer - b;
            let index = ((bit_buffer >> shift) & ((1u64 << b) - 1)) as usize;
            bit_buffer &= (1u64 << shift) - 1;
            bits_in_buffer -= b;
            result.push(wordlist.get(index).unwrap().clone());
        } else {
            let index =
                ((bit_buffer << (b - bits_in_buffer)) & ((1u64 << b) - 1)) as usize;
            bits_in_buffer = 0;
            bit_buffer = 0;
            result.push(wordlist.get(index).unwrap().clone());
        }
    }

    Ok(result)
}

/// Encode arbitrary bytes as mode 0 (raw 8-bit). Backward-compatible.
///
/// See [`encode_with_mode`] for the full API.
pub fn encode(data: &[u8], wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    encode_with_mode(data, wordlist, DataMode::Bytes8)
}

// ═══════════════════════════════════════════════════════════════════════
// Decoding
// ═══════════════════════════════════════════════════════════════════════

/// Decode payload words, returning the [`DataMode`] and raw bytes.
///
/// The mode is read from the header word. For modes 2/3 (base64/hex),
/// the returned bytes are the **pre-decoded** form; use [`decode_str`]
/// to reconstruct the original string.
pub fn decode_with_mode(
    words: &[String],
    wordlist: &WordlistTree,
) -> Result<(DataMode, Vec<u8>), DecodeError> {
    if words.is_empty() {
        return Err(DecodeError::EmptyInput);
    }
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    let b = log2_exact(wordlist.len()).ok_or(DecodeError::NonPowerOfTwo(wordlist.len()))?;

    // Guard against b=0 (1-word wordlist, 0 bits per word)
    if b == 0 {
        if words.len() == 1 {
            return Ok((DataMode::Bytes8, Vec::new()));
        }
        return Err(DecodeError::DataNotByteAligned { total_bits: 0, padding: 0 });
    }

    let header = wordlist
        .position(&words[0])
        .ok_or_else(|| DecodeError::UnknownWord(words[0].clone()))?;

    let mode_val = header / b;
    let padding = header % b;

    let mode =
        DataMode::from_mode_val(mode_val).ok_or(DecodeError::InvalidPadding(header))?;
    let bpu = mode.bits_per_unit();

    let data_words = &words[1..];

    // Empty payload
    if data_words.is_empty() {
        if padding != 0 {
            return Err(DecodeError::InvalidPadding(header));
        }
        return Ok((mode, Vec::new()));
    }

    let total_bits = data_words.len() * b;
    if total_bits < padding {
        return Err(DecodeError::DataNotByteAligned { total_bits, padding });
    }
    let data_bits = total_bits - padding;
    if data_bits % bpu != 0 {
        return Err(DecodeError::DataNotByteAligned { total_bits, padding });
    }

    let n_units = data_bits / bpu;

    let mut bit_buffer: u64 = 0;
    let mut bits_in_buffer: usize = 0;
    let mut result = Vec::with_capacity(n_units);

    for word in data_words {
        let index = wordlist
            .position(word)
            .ok_or_else(|| DecodeError::UnknownWord(word.clone()))?;

        bit_buffer = (bit_buffer << b) | (index as u64);
        bits_in_buffer += b;

        while bits_in_buffer >= bpu && result.len() < n_units {
            let shift = bits_in_buffer - bpu;
            let unit = ((bit_buffer >> shift) & ((1u64 << bpu) - 1)) as u8;
            bit_buffer &= (1u64 << shift) - 1;
            bits_in_buffer -= bpu;
            result.push(unit);
        }
    }

    Ok((mode, result))
}

/// Decode payload words back into bytes.
///
/// Mode-aware: reads the [`DataMode`] from the header word. For mode 0,
/// behavior is identical to the original codec. Use [`decode_str`] to
/// reconstruct the original string including re-encoding for hex/base64.
pub fn decode(words: &[String], wordlist: &WordlistTree) -> Result<Vec<u8>, DecodeError> {
    decode_with_mode(words, wordlist).map(|(_, bytes)| bytes)
}

// ═══════════════════════════════════════════════════════════════════════
// String convenience API (with auto-detection)
// ═══════════════════════════════════════════════════════════════════════

/// Encode a string with automatic format detection.
///
/// Detects hex, base64, and ASCII inputs and uses the most compact
/// encoding mode. Falls back to wider modes if the optimal mode
/// doesn't fit in the wordlist header space.
///
/// **Hex detection**: even length, all hex chars, ≥ 2 chars. Case is
/// normalized to lowercase on decode.
///
/// **Base64 detection**: length divisible by 4, valid base64 alphabet,
/// must contain at least one of `+`, `/`, `=`.
///
/// **ASCII detection**: all bytes < 128.
pub fn encode_str(s: &str, wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    let b = log2_exact(wordlist.len()).ok_or(DecodeError::NonPowerOfTwo(wordlist.len()))?;

    let (detected_mode, detected_data) = detect_mode(s);

    // Try detected mode; fall back to wider modes if header doesn't fit
    if detected_mode.fits_in_wordlist(b) {
        return encode_with_mode(&detected_data, wordlist, detected_mode);
    }

    // Fallback: try Ascii7 → Bytes8
    let bytes = s.as_bytes();
    if detected_mode != DataMode::Ascii7
        && bytes.iter().all(|&byte| byte < 128)
        && DataMode::Ascii7.fits_in_wordlist(b)
    {
        return encode_with_mode(bytes, wordlist, DataMode::Ascii7);
    }

    encode_with_mode(bytes, wordlist, DataMode::Bytes8)
}

/// Decode payload words back into the original string.
///
/// Reads the [`DataMode`] from the header word and reconstructs the
/// original string:
/// - **Bytes8/Ascii7**: interprets bytes as UTF-8
/// - **Base64**: re-encodes bytes as standard base64
/// - **Hex**: re-encodes bytes as lowercase hex
pub fn decode_str(words: &[String], wordlist: &WordlistTree) -> Result<String, DecodeError> {
    let (mode, bytes) = decode_with_mode(words, wordlist)?;
    match mode {
        DataMode::Bytes8 | DataMode::Ascii7 => {
            String::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)
        }
        DataMode::Base64 => Ok(base64_encode(&bytes)),
        DataMode::Hex => Ok(hex_encode(&bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a toy wordlist of size 2^b for testing.
    fn make_wordlist(b: usize) -> WordlistTree {
        let n = 1usize << b;
        let words: Vec<String> = (0..n).map(|i| format!("w{}", i)).collect();
        WordlistTree::new(words)
    }

    // ── Round-trip tests ──────────────────────────────────────────────

    #[test]
    fn round_trip_empty() {
        let wl = make_wordlist(4); // 16 words
        let encoded = encode(&[], &wl).unwrap();
        assert_eq!(encoded.len(), 1, "empty data => only the padding word");
        let decoded = decode(&encoded, &wl).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn round_trip_one_byte() {
        let wl = make_wordlist(4); // 16 words, 4 bits/word
        let data = vec![0xAB];
        let encoded = encode(&data, &wl).unwrap();
        let decoded = decode(&encoded, &wl).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn round_trip_various_sizes() {
        let wl = make_wordlist(11); // 2048 words (BIP39 size)
        for len in 1..=64 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 13) as u8).collect();
            let encoded = encode(&data, &wl).unwrap();
            let decoded = decode(&encoded, &wl).unwrap();
            assert_eq!(decoded, data, "failed round-trip for {} bytes", len);
        }
    }

    #[test]
    fn round_trip_exact_fit() {
        // 8 bits per word, 1 byte per word => padding should be 0
        let wl = make_wordlist(8); // 256 words
        let data = vec![0x00, 0xFF, 0x42];
        let encoded = encode(&data, &wl).unwrap();
        // Padding word should have index 0 (no padding)
        assert_eq!(wl.position(&encoded[0]), Some(0));
        let decoded = decode(&encoded, &wl).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn round_trip_all_zeros() {
        let wl = make_wordlist(11);
        let data = vec![0u8; 32];
        let encoded = encode(&data, &wl).unwrap();
        let decoded = decode(&encoded, &wl).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn round_trip_all_ones() {
        let wl = make_wordlist(11);
        let data = vec![0xFFu8; 32];
        let encoded = encode(&data, &wl).unwrap();
        let decoded = decode(&encoded, &wl).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn round_trip_str() {
        let wl = make_wordlist(11);
        let msg = "Hello, Glossia!";
        let encoded = encode_str(msg, &wl).unwrap();
        let decoded = decode_str(&encoded, &wl).unwrap();
        assert_eq!(decoded, msg);
    }

    // ── Known-value tests ─────────────────────────────────────────────

    #[test]
    fn known_value_single_byte() {
        // 4-bit words (16-word list), encode 0xFF
        // 0xFF = 1111_1111 => two 4-bit words: 0b1111=15, 0b1111=15
        // total_bits=8, n_words=2, padding=0
        let wl = make_wordlist(4);
        let encoded = encode(&[0xFF], &wl).unwrap();
        assert_eq!(encoded.len(), 3); // padding + 2 data words
        assert_eq!(wl.position(&encoded[0]), Some(0)); // padding=0
        assert_eq!(wl.position(&encoded[1]), Some(15)); // 0b1111
        assert_eq!(wl.position(&encoded[2]), Some(15)); // 0b1111
    }

    #[test]
    fn known_value_with_padding() {
        // 3-bit words (8-word list), encode 0xFF
        // 0xFF = 1111_1111 = 8 bits
        // n_words = ceil(8/3) = 3, total = 9 bits, padding = 1
        // bits: 111 111 110 (padded with 0)
        // indices: 7, 7, 6
        let wl = make_wordlist(3);
        let encoded = encode(&[0xFF], &wl).unwrap();
        assert_eq!(encoded.len(), 4); // padding + 3 data words
        assert_eq!(wl.position(&encoded[0]), Some(1)); // padding=1
        assert_eq!(wl.position(&encoded[1]), Some(7)); // 0b111
        assert_eq!(wl.position(&encoded[2]), Some(7)); // 0b111
        assert_eq!(wl.position(&encoded[3]), Some(6)); // 0b110
    }

    // ── Error tests ───────────────────────────────────────────────────

    #[test]
    fn error_empty_input() {
        let wl = make_wordlist(4);
        let empty: Vec<String> = vec![];
        assert_eq!(decode(&empty, &wl), Err(DecodeError::EmptyInput));
    }

    #[test]
    fn error_unknown_word() {
        let wl = make_wordlist(4);
        let words = vec!["not_in_list".to_string()];
        assert_eq!(
            decode(&words, &wl),
            Err(DecodeError::UnknownWord("not_in_list".to_string()))
        );
    }

    #[test]
    fn error_empty_wordlist() {
        let wl = WordlistTree::new(vec![]);
        assert_eq!(encode(&[1, 2, 3], &wl), Err(DecodeError::EmptyWordlist));
        assert_eq!(
            decode(&["x".to_string()], &wl),
            Err(DecodeError::EmptyWordlist)
        );
    }

    #[test]
    fn error_non_power_of_two() {
        let words: Vec<String> = (0..3).map(|i| format!("w{}", i)).collect();
        let wl = WordlistTree::new(words);
        assert_eq!(encode(&[1], &wl), Err(DecodeError::NonPowerOfTwo(3)));
    }

    #[test]
    fn error_invalid_padding() {
        // In the mode-aware format, header_index = mode * b + padding.
        // For b=5 (32-word list), index 20 → mode_val = 20/5 = 4,
        // which exceeds defined modes (0-3). This returns InvalidPadding.
        let wl = make_wordlist(5); // 32 words, b=5
        let mut words = vec![wl.get(20).unwrap().clone()];
        words.push(wl.get(0).unwrap().clone());
        assert_eq!(decode(&words, &wl), Err(DecodeError::InvalidPadding(20)));
    }

    // ── Edge cases ────────────────────────────────────────────────────

    #[test]
    fn round_trip_single_bit_words() {
        // b=1, wordlist of size 2
        let wl = make_wordlist(1);
        let data = vec![0b10110011u8];
        let encoded = encode(&data, &wl).unwrap();
        // 8 bits => 8 words + 1 padding word = 9
        assert_eq!(encoded.len(), 9);
        let decoded = decode(&encoded, &wl).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn padding_word_is_always_first() {
        let wl = make_wordlist(11);
        for len in 0..=20 {
            let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let encoded = encode(&data, &wl).unwrap();
            // First word should always be a valid wordlist member
            assert!(
                wl.position(&encoded[0]).is_some(),
                "padding word missing for len={}",
                len
            );
            // encode() uses mode 0, so index should be < b
            let pad_idx = wl.position(&encoded[0]).unwrap();
            assert!(
                pad_idx < 11,
                "padding index {} out of range for len={}",
                pad_idx,
                len
            );
        }
    }

    // ── DataMode tests ──────────────────────────────────────────────

    #[test]
    fn mode_fits_in_wordlist() {
        // b=11 (BIP39): all 4 modes fit. max header = 3*11+10 = 43 < 2048
        assert!(DataMode::Bytes8.fits_in_wordlist(11));
        assert!(DataMode::Ascii7.fits_in_wordlist(11));
        assert!(DataMode::Base64.fits_in_wordlist(11));
        assert!(DataMode::Hex.fits_in_wordlist(11));

        // b=4 (base16): all 4 modes fit. (3+1)*4 = 16 = 2^4
        assert!(DataMode::Hex.fits_in_wordlist(4));

        // b=3: modes 0-1 fit, modes 2-3 don't. (2+1)*3 = 9 > 8
        assert!(DataMode::Bytes8.fits_in_wordlist(3));
        assert!(DataMode::Ascii7.fits_in_wordlist(3));
        assert!(!DataMode::Base64.fits_in_wordlist(3));
        assert!(!DataMode::Hex.fits_in_wordlist(3));

        // b=1: modes 0-1 fit. (1+1)*1 = 2 = 2^1
        assert!(DataMode::Bytes8.fits_in_wordlist(1));
        assert!(DataMode::Ascii7.fits_in_wordlist(1));
        assert!(!DataMode::Base64.fits_in_wordlist(1));
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
        // Contains '=' → triggers base64 detection
        let (mode, data) = detect_mode("SGVsbG8=");
        assert_eq!(mode, DataMode::Base64);
        assert_eq!(data, b"Hello".to_vec());
    }

    #[test]
    fn detect_base64_with_plus() {
        // Contains '+' → triggers base64 detection
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
        // "abc" is odd length → not hex → ASCII
        let (mode, _) = detect_mode("abc");
        assert_eq!(mode, DataMode::Ascii7);
    }

    #[test]
    fn detect_no_special_base64_chars_falls_to_hex_or_ascii() {
        // "ABCD" is valid hex (even length, all hex) → detected as hex
        let (mode, _) = detect_mode("ABCD");
        assert_eq!(mode, DataMode::Hex);

        // "Hello World" has spaces, not hex → ASCII
        let (mode, _) = detect_mode("Hello World");
        assert_eq!(mode, DataMode::Ascii7);
    }

    // ── Mode-aware encode/decode round-trips ────────────────────────

    #[test]
    fn round_trip_hex_str() {
        let wl = make_wordlist(11);
        let hex = "deadbeef01234567";
        let encoded = encode_str(hex, &wl).unwrap();
        let decoded = decode_str(&encoded, &wl).unwrap();
        assert_eq!(decoded, hex.to_lowercase());

        // Verify it used fewer words than mode 0
        let encoded_mode0 = encode(hex.as_bytes(), &wl).unwrap();
        assert!(
            encoded.len() < encoded_mode0.len(),
            "hex mode ({} words) should use fewer words than mode 0 ({} words)",
            encoded.len(),
            encoded_mode0.len()
        );
    }

    #[test]
    fn round_trip_base64_str() {
        let wl = make_wordlist(11);
        let b64 = "SGVsbG8gV29ybGQ="; // "Hello World"
        let encoded = encode_str(b64, &wl).unwrap();
        let decoded = decode_str(&encoded, &wl).unwrap();
        assert_eq!(decoded, b64);

        // Verify it used fewer words than mode 0
        let encoded_mode0 = encode(b64.as_bytes(), &wl).unwrap();
        assert!(
            encoded.len() < encoded_mode0.len(),
            "base64 mode ({} words) should use fewer words than mode 0 ({} words)",
            encoded.len(),
            encoded_mode0.len()
        );
    }

    #[test]
    fn round_trip_ascii_str() {
        let wl = make_wordlist(11);
        let text = "Hello, Glossia! This is a test.";
        let encoded = encode_str(text, &wl).unwrap();
        let decoded = decode_str(&encoded, &wl).unwrap();
        assert_eq!(decoded, text);

        // Verify it used fewer words than mode 0
        let encoded_mode0 = encode(text.as_bytes(), &wl).unwrap();
        assert!(
            encoded.len() < encoded_mode0.len(),
            "ascii7 mode ({} words) should use fewer words than mode 0 ({} words)",
            encoded.len(),
            encoded_mode0.len()
        );
    }

    #[test]
    fn round_trip_utf8_str() {
        let wl = make_wordlist(11);
        let text = "café résumé";
        let encoded = encode_str(text, &wl).unwrap();
        let decoded = decode_str(&encoded, &wl).unwrap();
        assert_eq!(decoded, text);
    }

    #[test]
    fn round_trip_empty_str() {
        let wl = make_wordlist(11);
        let encoded = encode_str("", &wl).unwrap();
        let decoded = decode_str(&encoded, &wl).unwrap();
        assert_eq!(decoded, "");
    }

    // ── Header word verification ────────────────────────────────────

    #[test]
    fn header_encodes_mode_and_padding() {
        let wl = make_wordlist(11); // b=11

        // Mode 0 (encode): header = 0*11 + padding
        let enc0 = encode(b"hello", &wl).unwrap();
        let hdr0 = wl.position(&enc0[0]).unwrap();
        assert!(hdr0 < 11, "mode 0 header {} should be < 11", hdr0);

        // Mode 1 (ASCII): header = 1*11 + padding = 11..21
        let enc1 = encode_str("hello", &wl).unwrap();
        let hdr1 = wl.position(&enc1[0]).unwrap();
        assert!(
            hdr1 >= 11 && hdr1 < 22,
            "mode 1 header {} should be in 11..21",
            hdr1
        );

        // Mode 3 (hex): header = 3*11 + padding = 33..43
        let enc3 = encode_str("deadbeef", &wl).unwrap();
        let hdr3 = wl.position(&enc3[0]).unwrap();
        assert!(
            hdr3 >= 33 && hdr3 < 44,
            "mode 3 header {} should be in 33..43",
            hdr3
        );
    }

    // ── Backward compatibility ──────────────────────────────────────

    #[test]
    fn mode0_backward_compatible() {
        // encode() and decode() are mode 0 only — verify identical behavior
        let wl = make_wordlist(11);
        for len in 1..=32 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 13) as u8).collect();
            let encoded = encode(&data, &wl).unwrap();

            // decode returns same bytes
            let decoded = decode(&encoded, &wl).unwrap();
            assert_eq!(decoded, data);

            // decode_with_mode returns Bytes8 mode
            let (mode, decoded2) = decode_with_mode(&encoded, &wl).unwrap();
            assert_eq!(mode, DataMode::Bytes8);
            assert_eq!(decoded2, data);
        }
    }

    #[test]
    fn old_decoder_rejects_new_modes() {
        // Simulate old decoder: header >= b is invalid
        let wl = make_wordlist(11);
        let encoded = encode_str("hello", &wl).unwrap(); // mode 1
        let header_idx = wl.position(&encoded[0]).unwrap();
        assert!(
            header_idx >= 11,
            "mode 1 header should be >= b=11, got {}",
            header_idx
        );
        // Old decoder would check: if header >= b { InvalidPadding }
    }

    // ── Word count savings ──────────────────────────────────────────

    #[test]
    fn hex_saves_50_percent_words() {
        let wl = make_wordlist(11);
        // 64-char hex string (SHA-256 hash)
        let hex = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
        let smart = encode_str(hex, &wl).unwrap();
        let naive = encode(hex.as_bytes(), &wl).unwrap();
        let savings = (naive.len() as f64 - smart.len() as f64) / naive.len() as f64;
        assert!(
            savings > 0.45,
            "hex should save ~50%%, got {:.1}%",
            savings * 100.0
        );
    }

    #[test]
    fn ascii_saves_about_12_percent_words() {
        let wl = make_wordlist(11);
        let text = "The quick brown fox jumps over the lazy dog and some more text here.";
        let smart = encode_str(text, &wl).unwrap();
        let naive = encode(text.as_bytes(), &wl).unwrap();
        let savings = (naive.len() as f64 - smart.len() as f64) / naive.len() as f64;
        assert!(
            savings > 0.05,
            "ascii7 should save ~12.5%%, got {:.1}%",
            savings * 100.0
        );
    }

    // ── Cross-mode decode ───────────────────────────────────────────

    #[test]
    fn decode_returns_raw_bytes_for_hex() {
        let wl = make_wordlist(11);
        let encoded = encode_str("deadbeef", &wl).unwrap();

        // decode() returns pre-decoded bytes
        let raw = decode(&encoded, &wl).unwrap();
        assert_eq!(raw, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        // decode_str() returns hex string
        let text = decode_str(&encoded, &wl).unwrap();
        assert_eq!(text, "deadbeef");
    }

    #[test]
    fn decode_returns_raw_bytes_for_base64() {
        let wl = make_wordlist(11);
        let encoded = encode_str("SGVsbG8=", &wl).unwrap();

        // decode() returns pre-decoded bytes
        let raw = decode(&encoded, &wl).unwrap();
        assert_eq!(raw, b"Hello".to_vec());

        // decode_str() returns base64 string
        let text = decode_str(&encoded, &wl).unwrap();
        assert_eq!(text, "SGVsbG8=");
    }

    // ── encode_with_mode direct tests ───────────────────────────────

    #[test]
    fn encode_with_mode_rejects_unsupported() {
        let wl = make_wordlist(3); // b=3: modes 0-1 only
        assert!(matches!(
            encode_with_mode(b"test", &wl, DataMode::Hex),
            Err(DecodeError::UnsupportedMode { .. })
        ));
    }

    #[test]
    fn round_trip_mode1_explicit() {
        let wl = make_wordlist(11);
        let data = b"Hello, World!";
        let encoded = encode_with_mode(data, &wl, DataMode::Ascii7).unwrap();
        let (mode, decoded) = decode_with_mode(&encoded, &wl).unwrap();
        assert_eq!(mode, DataMode::Ascii7);
        assert_eq!(decoded, data.to_vec());
    }

    #[test]
    fn round_trip_mode3_explicit() {
        let wl = make_wordlist(11);
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let encoded = encode_with_mode(&data, &wl, DataMode::Hex).unwrap();
        let (mode, decoded) = decode_with_mode(&encoded, &wl).unwrap();
        assert_eq!(mode, DataMode::Hex);
        assert_eq!(decoded, data);
    }

    // ── Small wordlist fallback ─────────────────────────────────────

    #[test]
    fn encode_str_falls_back_for_small_wordlist() {
        // b=3: hex mode doesn't fit, should fall back to ascii7
        let wl = make_wordlist(3);
        let hex = "deadbeef";
        let encoded = encode_str(hex, &wl).unwrap();
        let (mode, _) = decode_with_mode(&encoded, &wl).unwrap();
        // Should have fallen back to Ascii7 (mode 1) since Hex (mode 3) doesn't fit
        assert_eq!(mode, DataMode::Ascii7);
        // Still round-trips as a string (just less efficiently)
        let decoded = decode_str(&encoded, &wl).unwrap();
        assert_eq!(decoded, hex);
    }
}
