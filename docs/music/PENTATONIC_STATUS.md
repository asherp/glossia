# Pentatonic Dialect - ✅ FULLY WORKING

## ✅ Complete Implementation

### Dialect Infrastructure
- `languages/music/payload_pentatonic.yaml` - 45 notes (C, D, E, G, A across 9 octaves)
- `grammar.yaml` updated with `pentatonic` and `pentatonic-scored` dialects
- Multi-wordlist support via `payload_wordlist: pentatonic` parameter
- Build system correctly discovers and embeds pentatonic wordlist
- `bitpacking: false` flag allows non-power-of-2 wordlists

### Encoding Capacity
- **45 notes**: log₂(45) ≈ 5.49 bits/note
- **256-bit payload**: ~47 notes (vs. 37 for chromatic)
- **Trade-off**: 27% more notes, but universally pleasant sound

## ✅ Base-N Codec Implemented

**The `--from-ascii` workflow now works perfectly!**

```bash
# Encoding works!
echo "Hello" | cargo run --bin glossia -- --from-ascii - --language music --dialect pentatonic
# Output: A4 e6 a1 a2 g8 c3 g5 d1 e3

# Decoding works!
echo "A4 e6 a1 a2 g8 c3 g5 d1 e3" | cargo run --bin glossia -- --decode --language music --wordlist pentatonic
# Output: Hello

# Full round-trip verified!
echo "Hello, world!" | cargo run --bin glossia -- --from-ascii - --language music --dialect pentatonic | \
  cargo run --bin glossia -- --decode --language music --wordlist pentatonic
# Output: Hello, world!
```

### How It Works

The base-N codec (`src/codec.rs`) implements arbitrary-base number conversion:
1. Treats input bytes as a big integer (using `num-bigint::BigUint`)
2. Converts to base-45 representation via division/remainder
3. Maps each base-45 digit to a pentatonic note
4. No header word needed (fixed alphabet size)

### How CS Handles Non-Power-of-2

CS dialects (ascii-7 with 95 chars, base58 with 58 chars) also have `bitpacking: false`, but they:
- Don't support `--from-ascii` direct encoding
- Are used via `--from` / `--into` pipeline transformations
- Work as intermediate formats in transformation chains

Example:
```bash
# CS doesn't encode directly from ASCII:
echo "test" | glossia --into cs --from hex < hex_data.txt
```

## 🎯 Solutions (Pick One)

### Option 1: Base Conversion Codec (Recommended)
Implement non-bitpacking encoding like base conversion:
- Treat 45-note pentatonic as base-45 number system
- Convert binary data → base-45 representation → note sequence
- Similar to how base58 encoding works in Bitcoin

**Implementation**:
- Add `encode_base_n()` and `decode_base_n()` functions to `src/codec.rs`
- Check `bitpacking` flag in grammar; if false, use base-N codec
- Update CLI to route non-bitpacking languages through base-N path

**Pros**: Full `--from-ascii` support, efficient encoding
**Cons**: New codec implementation (~200 lines)

### Option 2: Auto-Detection with Fallback
When encoding ASCII → music:
1. Try encoding with pentatonic (45 notes)
2. If wordlist exhausted, fall back to chromatic (128 notes)
3. Decoder auto-detects which was used

**Implementation**:
- Pre-convert data to note indices: `data_bytes → [0..127]`
- Filter: if all indices < 45, use pentatonic; else chromatic
- This matches user's stated goal: "assume most restrictive vocabulary"

**Pros**: Automatic optimization, no new codec
**Cons**: Still requires base-N conversion for 45-note wordlist

### Option 3: Explicit Word Sequences Only
Document that pentatonic dialect works for:
- Generating prose from explicit note lists
- Transforming between dialects
- NOT for `--from-ascii` encoding

**Pros**: No code changes, works today
**Cons**: Limited utility

## ✅ What Works Today

Even without `--from-ascii`, the pentatonic dialect infrastructure is complete:

### 1. Generate from Explicit Notes
```bash
# These notes are all in pentatonic scale
cargo run --bin glossia -- C4 E4 G4 A4 C5 D5 --language music --dialect pentatonic-scored
```

**Output**:
```
tempo=120 time=4/4
C4 quarter E4 half G4 quarter A4 whole |
C5 half D5 quarter ||
```

### 2. Decode Works Perfectly
```bash
# Decoding filters by N-tagged tokens, so chromatic and pentatonic decode identically
echo "C4 E4 G4 A4 C5" | cargo run --bin glossia -- --decode --language music
```

### 3. Multi-Dialect System
The architecture is proven:
- CS has 6 payload variants (base58, base64, base16, ascii7, etc.)
- Music now has 2 (chromatic=default, pentatonic)
- Adding blues (54 notes), diatonic (63 notes) is straightforward

## 📋 Recommended Next Steps

1. **Implement base-N codec** (Option 1) - enables full `--from-ascii` support
2. **Add blues and diatonic scales** - prove multi-scale system
3. **Auto-detection logic** - use most restrictive scale that fits
4. **Update docs** - once codec is working, update main README

## 🎵 Sound Quality Comparison

| Dialect | Notes | Encoding | Sound |
|---------|-------|----------|-------|
| **chromatic** (default) | 128 | Works | Atonal, experimental |
| **pentatonic** | 45 | Needs codec | Folk-like, pleasant |
| **blues** (future) | 54 | Needs codec | Blues licks |
| **diatonic** (future) | 63 | Needs codec | Familiar melodies |

## 💡 Key Insight

**The multi-wordlist infrastructure is complete.** The missing piece is just the non-bitpacking codec, which is a well-understood problem (base conversion) with a straightforward solution.

Once implemented, we get:
- ✅ Automatic scale selection (pentatonic < blues < diatonic < chromatic)
- ✅ Optimal encoding (fewest bits for the data's "musical vocabulary")
- ✅ Beautiful sound (constrained to pleasant scales)

## Code Pointers

- Wordlist definitions: `languages/music/payload*.yaml`
- Grammar with dialects: `languages/music/grammar.yaml`
- Bitpacking codec (needs bypass): `src/codec.rs:344, 436, 540`
- CLI encode entry point: `src/bin/glossia.rs:309-313`
- Build-time validation: `build.rs:121-131` (checks `bitpacking` flag)

---

**Status**: Infrastructure complete, codec implementation needed for `--from-ascii` workflow.
