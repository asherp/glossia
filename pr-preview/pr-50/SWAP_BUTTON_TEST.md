# Swap Button Test Guide

## Automated Test

**URL:** http://localhost:8080/test_swap.html

1. Open the URL in your browser
2. Click "Run Test"
3. Watch the automated round-trip test:
   - Generates 12 random BIP39 words
   - Encodes them (gets ~35 words with cover)
   - Swaps → Decodes back to 12 words
   - Swaps → Re-encodes
   - Verifies perfect round-trip ✓

**Expected Result:** Green "✅ Swap button workflow verified!" message

## Manual Test - Landing Page

**URL:** http://localhost:8080/index.html

### Test 1: Basic Swap (Encode → Decode)

1. Open http://localhost:8080/index.html
2. Click **"Random 12"**
   - Input shows: 12 BIP39 words (e.g., "abandon ability able...")
   - Output shows: ~35 words with payload highlighted
3. Click the **swap button (⇄)** in the center
4. Observe:
   - ✅ Input now shows the ~35 word encoded text
   - ✅ Left panel header changes from "Input: word indices (direct)" to "Input: encoded text"
   - ✅ Right panel header changes from "Encoding with: english/BIP39" to "Decoding with: english/BIP39"
   - ✅ Output shows the original 12 BIP39 words
5. Verify round-trip:
   - Compare output to the original 12 words from step 2
   - Should match perfectly ✓

### Test 2: Double Swap (Encode → Decode → Encode)

1. Continue from Test 1
2. Click the **swap button (⇄)** again
3. Observe:
   - ✅ Input shows the 12 BIP39 words again
   - ✅ Mode switches back to Encode
   - ✅ Output shows ~35 words (possibly different cover words)
4. Click **swap (⇄)** again
5. Verify it decodes back to the 12 words ✓

### Test 3: Hex Detection Swap

1. Clear input and type: **cafe1234**
2. Observe:
   - Input label shows "Input: hex"
   - Output shows encoded text
3. Click **swap (⇄)**
4. Observe:
   - Input shows the encoded text
   - Mode switches to Decode
   - Output shows: **cafe1234** ✓
5. Click **swap (⇄)** again
6. Verify:
   - Input shows "cafe1234"
   - Re-encodes as hex

### Test 4: Plain Text Swap

1. Clear input and type: **Hello, World!**
2. Observe:
   - Input label shows "Input: 7-bit ASCII"
   - Output shows encoded text
3. Click **swap (⇄)**
4. Observe:
   - Input shows the encoded text
   - Output shows: **Hello, World!** ✓

### Test 5: Cover Words Toggle + Swap

1. Click **"Random 12"**
2. Click the **☑ Cover** button to turn it off
   - Output shrinks to just 12 words (payload only)
3. Click **swap (⇄)**
   - Input should have the 12 payload words (not the full text)
   - Decodes correctly ✓
4. Click **☑ Cover** to turn it back on
5. Click **swap (⇄)** again
   - Re-encodes with cover words

## What to Look For

### ✅ Success Indicators

- Content flows from right panel → left panel on swap
- Mode switches (Encode ↔ Decode)
- Panel headers update correctly
- Round-trip is lossless (encode → decode → same data)
- Input dialect label updates appropriately
- Stats update correctly

### ❌ Failure Indicators

- Content doesn't swap
- Mode switches but content stays the same
- Decoded output differs from original input
- JavaScript errors in console
- Swap button doesn't respond

## Edge Cases to Test

1. **Empty output:** Swap when there's no output (should do nothing gracefully)
2. **Empty input:** Start with empty input, encode, swap (should work)
3. **Manual text edit:** Type in input, encode, manually edit output in textarea (can't actually edit output-area div)
4. **Different wordlists:** Encode with bip39, change to base16, swap (should still decode with bip39)
5. **Rapid swapping:** Click swap multiple times quickly (should handle gracefully)

## Known Behavior

- Swap uses `outputArea.dataset.plainText` to get the clean output text (no HTML markup)
- If dataset.plainText is not set, falls back to `textContent.trim()`
- In decode mode, the input dialect label shows "Input: encoded text" (generic, since we don't know the source format)
- Cover word toggle affects what gets swapped (with cover ON, full text; OFF, payload only)

## Troubleshooting

**Swap doesn't work:**
- Check browser console for JavaScript errors
- Verify WASM loaded successfully
- Try refreshing the page

**Round-trip fails:**
- Check which wordlist is selected
- Verify the output was fully generated before swapping
- Check for any encoding/decoding errors in the UI

**Content gets corrupted:**
- This shouldn't happen - file a bug if it does
- Check if special characters are being handled correctly
- Verify UTF-8 encoding is preserved
