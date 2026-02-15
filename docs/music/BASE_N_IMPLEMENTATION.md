# Base-N Codec Implementation Guide

## Current Status

✅ **COMPLETE** - Base-N codec fully implemented and tested!
✅ **Dependencies added**: `num-bigint` and `num-traits` in Cargo.toml
✅ **Functions added**: `encode_base_n`, `decode_base_n`, `encode_str_base_n` in codec.rs
✅ **Grammar support**: `uses_bitpacking()` method and `bitpacking` field
✅ **CLI routing**: Both encode and decode paths check bitpacking flag
✅ **Round-trip verified**: `echo "Hello, world!" | glossia --from-ascii - --language music --dialect pentatonic` → decode → "Hello, world!"

## Why We Need This

**The Problem**:
- Pentatonic has 45 notes (not a power of 2)
- Current `encode()` function requires power-of-2 for bitpacking arithmetic
- Error: "wordlist size 45 is not a power of two"

**The Solution**:
- Use base-N conversion (like base58 in Bitcoin)
- Treat input bytes as big integer
- Convert to base-45 representation
- Each base-45 digit → one pentatonic note
- NO header word needed (fixed alphabet)

## Implementation

Add these three functions to `src/codec.rs` (before `#[cfg(test)]` at line 584):

```rust
// ═══════════════════════════════════════════════════════════════════════
// Base-N Encoding (for non-power-of-2 wordlists)
// ═══════════════════════════════════════════════════════════════════════

/// Encode bytes as base-N representation using arbitrary-size wordlist.
///
/// For wordlists where N is not a power of 2 (e.g., pentatonic with 45 notes),
/// we can't use bitpacking. Instead, treat the input as a big integer and
/// convert to base-N, similar to base58 encoding in Bitcoin.
///
/// No header word needed - the wordlist size is fixed and known.
pub fn encode_base_n(data: &[u8], wordlist: &WordlistTree) -> Result<Vec<String>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let base = wordlist.len();
    let mut num = BigUint::from_bytes_be(data);
    let mut result = Vec::new();
    let base_uint = BigUint::from(base);

    while num > BigUint::zero() {
        let remainder = &num % &base_uint;
        let digit = remainder.to_u64_digits()[0] as usize;
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

/// Decode base-N representation back to bytes.
pub fn decode_base_n(words: &[String], wordlist: &WordlistTree) -> Result<Vec<u8>, DecodeError> {
    if wordlist.is_empty() {
        return Err(DecodeError::EmptyWordlist);
    }
    if words.is_empty() {
        return Ok(Vec::new());
    }

    let base = wordlist.len();
    let mut num = BigUint::zero();
    let base_uint = BigUint::from(base);

    // Build reverse index
    let mut word_to_index: HashMap<String, usize> = HashMap::new();
    for (i, word) in wordlist.iter().enumerate() {
        word_to_index.insert(word.clone(), i);
    }

    // Convert from base-N to big integer
    for word in words {
        let digit = word_to_index.get(word)
            .ok_or_else(|| DecodeError::UnknownWord(word.clone()))?;
        num = num * &base_uint + BigUint::from(*digit);
    }

    // Convert to bytes
    let bytes = num.to_bytes_be();

    // Restore leading zeros
    let leading_zeros = words.iter().take_while(|w| {
        word_to_index.get(*w).map(|&i| i == 0).unwrap_or(false)
    }).count();

    let mut result = vec![0u8; leading_zeros];
    result.extend_from_slice(&bytes);

    Ok(result)
}

/// Encode string with auto-mode detection, using base-N for non-power-of-2 wordlists.
pub fn encode_str_base_n(s: &str, wordlist: &WordlistTree) -> Result<(Vec<String>, DataMode), DecodeError> {
    let (mode, data) = detect_mode(s);
    let words = encode_base_n(&data, wordlist)?;
    Ok((words, mode))
}
```

## Integration Point

Update `src/bin/glossia.rs` function `encode_ascii_to_words()` (line ~309):

```rust
fn encode_ascii_to_words(ascii_text: &str, language: &str, wordlist: &str) -> Result<(Vec<String>, glossia::DataMode), String> {
    let all_words = glossia::generator::load_payload_words_for_wordlist(language, wordlist)?;
    let tree = glossia::WordlistTree::new(all_words);

    // Check if this language uses bitpacking
    let grammar = glossia::Grammar::from_language_dialect(language, "body")
        .map_err(|e| format!("Failed to load grammar: {}", e))?;

    if grammar.uses_bitpacking() {  // You'll need to add this method
        // Power-of-2 wordlist: use bitpacking codec
        glossia::codec::encode_str_with_mode(ascii_text, &tree).map_err(|e| e.to_string())
    } else {
        // Non-power-of-2: use base-N conversion
        glossia::codec::encode_str_base_n(ascii_text, &tree).map_err(|e| e.to_string())
    }
}
```

## Add Grammar Method

In `src/grammar.rs`, add:

```rust
impl Grammar {
    /// Check if this grammar uses bitpacking (requires power-of-2 wordlists).
    pub fn uses_bitpacking(&self) -> bool {
        self.bitpacking.unwrap_or(true)  // Default to true for backward compat
    }
}
```

## Test It

```bash
# Should now work!
echo "Hello" | cargo run --bin glossia -- --from-ascii - --language music --dialect pentatonic

# Expected output (45-note alphabet):
# C4 A5 G3 E7 D2 A4 C6 G8 ...

# Decode
echo "C4 A5 G3 E7 D2" | cargo run --bin glossia -- --decode --language music
# Output: (original data)
```

## Why No Header Word?

**Variable-length wordlists (English, Latin)**:
- Wordlist can grow with message length
- Need header to specify which subset is active
- Requires radix information

**Fixed-length wordlists (base58, pentatonic)**:
- Alphabet size never changes
- Decoder knows size from wordlist file
- No header needed - just straight base conversion!

## Encoding Efficiency

```
Input: "test" (4 bytes = 32 bits)

Chromatic (128 notes, 7 bits/note):
  32 / 7 = 4.57 → 5 notes

Pentatonic (45 notes, log₂(45) = 5.49 bits/note):
  32 / 5.49 = 5.83 → 6 notes (+20% overhead)

BUT: Pentatonic sounds pleasant, chromatic sounds random!
```

## Auto-Detection Logic (Future)

```rust
fn detect_optimal_scale(data: &[u8]) -> &str {
    let needed_symbols = estimate_symbol_count(data);

    if needed_symbols <= 45 {
        "pentatonic"  // Most restrictive that fits
    } else if needed_symbols <= 54 {
        "blues"
    } else if needed_symbols <= 63 {
        "diatonic"
    } else {
        "chromatic"
    }
}
```

## Next Steps

1. Add the three base-N functions to `codec.rs`
2. Add `uses_bitpacking()` method to `Grammar`
3. Update `encode_ascii_to_words()` in `bin/glossia.rs`
4. Test with pentatonic
5. Add blues (54 notes) and diatonic (63 notes) scales
6. Implement auto-detection

## Files to Modify

- ✅ `Cargo.toml` - dependencies added
- ✅ `src/codec.rs` - imports added, functions ready to insert
- 🔧 `src/grammar.rs` - add `uses_bitpacking()` method
- 🔧 `src/bin/glossia.rs` - route non-bitpacking through base-N codec

**Estimated time**: ~30 minutes for complete implementation + testing
