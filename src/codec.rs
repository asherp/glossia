use crate::merkle::WordlistTree;
use std::fmt;

/// Errors that can occur during decoding.
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
    /// The padding word index is out of range (must be 0..b where b = log2(wordlist_size)).
    InvalidPadding(usize),
    /// After removing padding bits the total is not a multiple of 8.
    DataNotByteAligned { total_bits: usize, padding: usize },
    /// Decoded bytes are not valid UTF-8 (only from `decode_str`).
    InvalidUtf8,
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
                "data not byte-aligned: {} total bits - {} padding = {} data bits",
                total_bits,
                padding,
                total_bits - padding
            ),
            DecodeError::InvalidUtf8 => write!(f, "decoded bytes are not valid UTF-8"),
        }
    }
}

/// Return `log2(n)` if `n` is a power of two, otherwise `None`.
fn log2_exact(n: usize) -> Option<usize> {
    if n == 0 || (n & (n - 1)) != 0 {
        return None;
    }
    Some(n.trailing_zeros() as usize)
}

/// Encode arbitrary bytes into payload words.
///
/// Wire format: `[padding_word] [data_word_0] ... [data_word_{N-1}]`
///
/// - The padding word's wordlist index equals the number of zero-padded
///   trailing bits in the last data word (0 to `b-1`).
/// - Data words are bit-packed from the input bytes, `b` bits per word.
///
/// The wordlist must be a non-empty power-of-two in size.
pub fn encode(data: &[u8], wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    let b = log2_exact(wordlist.len()).ok_or(DecodeError::NonPowerOfTwo(wordlist.len()))?;

    // Special case: empty data => padding=0, no data words
    if data.is_empty() {
        return Ok(vec![wordlist.get(0).unwrap().clone()]);
    }

    let total_bits = data.len() * 8;
    let n_words = (total_bits + b - 1) / b; // ceil(total_bits / b)
    let padding = n_words * b - total_bits;

    let mut result = Vec::with_capacity(1 + n_words);

    // Padding word
    result.push(wordlist.get(padding).unwrap().clone());

    // Bit-pack data into b-bit indices
    let mut bit_buffer: u64 = 0;
    let mut bits_in_buffer: usize = 0;
    let mut byte_idx = 0;

    for _ in 0..n_words {
        // Fill the buffer until we have at least b bits
        while bits_in_buffer < b && byte_idx < data.len() {
            bit_buffer = (bit_buffer << 8) | (data[byte_idx] as u64);
            bits_in_buffer += 8;
            byte_idx += 1;
        }

        // Extract the top b bits (zero-pad if needed for the last word)
        if bits_in_buffer >= b {
            let shift = bits_in_buffer - b;
            let index = ((bit_buffer >> shift) & ((1u64 << b) - 1)) as usize;
            bit_buffer &= (1u64 << shift) - 1;
            bits_in_buffer -= b;
            result.push(wordlist.get(index).unwrap().clone());
        } else {
            // Last word: fewer than b bits remain, left-shift to fill
            let index = ((bit_buffer << (b - bits_in_buffer)) & ((1u64 << b) - 1)) as usize;
            bits_in_buffer = 0;
            bit_buffer = 0;
            result.push(wordlist.get(index).unwrap().clone());
        }
    }

    Ok(result)
}

/// Decode payload words back into bytes.
///
/// Expects format: `[padding_word] [data_word_0] ... [data_word_{N-1}]`
pub fn decode(words: &[String], wordlist: &WordlistTree) -> Result<Vec<u8>, DecodeError> {
    if words.is_empty() {
        return Err(DecodeError::EmptyInput);
    }
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    let b = log2_exact(wordlist.len()).ok_or(DecodeError::NonPowerOfTwo(wordlist.len()))?;

    // Read padding from first word
    let padding = wordlist
        .position(&words[0])
        .ok_or_else(|| DecodeError::UnknownWord(words[0].clone()))?;

    if padding >= b {
        return Err(DecodeError::InvalidPadding(padding));
    }

    let data_words = &words[1..];

    // Special case: no data words means empty payload
    if data_words.is_empty() {
        if padding != 0 {
            return Err(DecodeError::InvalidPadding(padding));
        }
        return Ok(Vec::new());
    }

    let total_bits = data_words.len() * b;
    let data_bits = total_bits - padding;

    if data_bits % 8 != 0 {
        return Err(DecodeError::DataNotByteAligned {
            total_bits,
            padding,
        });
    }

    let n_bytes = data_bits / 8;

    // Convert word indices to a bitstream and extract bytes
    let mut bit_buffer: u64 = 0;
    let mut bits_in_buffer: usize = 0;
    let mut result = Vec::with_capacity(n_bytes);

    for word in data_words {
        let index = wordlist
            .position(word)
            .ok_or_else(|| DecodeError::UnknownWord(word.clone()))?;

        bit_buffer = (bit_buffer << b) | (index as u64);
        bits_in_buffer += b;

        // Extract complete bytes
        while bits_in_buffer >= 8 && result.len() < n_bytes {
            let shift = bits_in_buffer - 8;
            let byte = ((bit_buffer >> shift) & 0xFF) as u8;
            bit_buffer &= (1u64 << shift) - 1;
            bits_in_buffer -= 8;
            result.push(byte);
        }
    }

    Ok(result)
}

/// Convenience: encode a UTF-8 string.
pub fn encode_str(s: &str, wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    encode(s.as_bytes(), wordlist)
}

/// Convenience: decode words back into a UTF-8 string.
pub fn decode_str(words: &[String], wordlist: &WordlistTree) -> Result<String, DecodeError> {
    let bytes = decode(words, wordlist)?;
    String::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)
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
        // Construct a word sequence where the padding word has too large an index
        let wl = make_wordlist(3); // b=3, valid padding 0..2
        // Use word at index 3 as padding (invalid: 3 >= 3)
        let mut words = vec![wl.get(3).unwrap().clone()];
        words.push(wl.get(0).unwrap().clone());
        assert_eq!(decode(&words, &wl), Err(DecodeError::InvalidPadding(3)));
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
            // And its index should be < b
            let pad_idx = wl.position(&encoded[0]).unwrap();
            assert!(
                pad_idx < 11,
                "padding index {} out of range for len={}",
                pad_idx,
                len
            );
        }
    }
}
