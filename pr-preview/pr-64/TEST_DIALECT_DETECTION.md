# Dialect Detection Test Plan

## Overview
The landing page now automatically detects which Glossia dialect (language/wordlist) was used to encode text when you paste it in decode mode.

## Test Scenarios

### Scenario 1: Basic Auto-Detection
**Steps:**
1. Open http://localhost:8080 in your browser
2. Default mode is "Encode" with "english" language selected
3. Type or use "Random 12" to generate sample words
4. Click encode (or it auto-encodes)
5. Copy the output text
6. Click the swap button (⇄) to switch to "Decode" mode
7. Paste the copied text into the input area
8. **Expected:** The language/wordlist dropdowns automatically update to "english/bip39" (or whatever was used)
9. **Expected:** You see a green checkmark indicator "✓ english/bip39" for 2.5 seconds
10. **Expected:** The decoded output appears correctly

### Scenario 2: Cross-Language Detection
**Steps:**
1. In Encode mode, select "cs" language and "base16" wordlist
2. Enter some text (e.g., "Hello World")
3. Copy the encoded output
4. Switch to Decode mode
5. Change language to "english" (intentionally wrong)
6. Paste the cs/base16 encoded text
7. **Expected:** Language automatically switches back to "cs/base16"
8. **Expected:** Text decodes correctly

### Scenario 3: Manual Override
**Steps:**
1. In Decode mode with some encoded text pasted
2. Manually change the language dropdown to a different language
3. **Expected:** The selection stays on your manual choice
4. **Expected:** Auto-detection does NOT override your manual selection
5. Type more text into the input
6. **Expected:** NOW auto-detection runs again and may update the selection

### Scenario 4: Cache Performance
**Steps:**
1. Paste some encoded text in Decode mode
2. Observe the language/wordlist update (detection runs)
3. Delete a few characters from the input
4. Re-type those same characters
5. **Expected:** Detection should use cached results (faster, no flicker)

### Scenario 5: Short Input Handling
**Steps:**
1. In Decode mode, paste just "hello" (5 chars)
2. **Expected:** No auto-detection runs (minimum is 10 characters)
3. Paste more text to reach 10+ characters
4. **Expected:** Auto-detection kicks in

## Visual Indicators

### Success Indicator
When auto-detection finds a match:
- Mode label changes from "Decode" to "✓ english/bip39" (or detected dialect)
- Text color changes to green (`--success` color)
- Reverts back to "Decode" after 2.5 seconds

### No Indicator Cases
- When pasted text has no payload words
- When current selection is already correct
- When input is too short (<10 chars)
- When user manually changed controls (respects user choice)

## Technical Implementation Details

### Detection Algorithm
```javascript
1. Take input text (first 100 chars as cache key)
2. Loop through all available languages
3. For each language, loop through all wordlists
4. Try decoding with each combination
5. Count how many payload words are extracted
6. Select the combination with highest count
7. Cache result for performance
8. Update UI if better match found
```

### Performance Optimizations
- **Caching:** Results cached by first 100 chars of input (max 20 entries)
- **Debouncing:** Input changes debounced by 300ms before detection runs
- **Conditional execution:** Only runs in decode mode, only on input changes
- **Error handling:** Failed decode attempts silently ignored

### Smart Behaviors
- ✅ Only runs when **input text** changes
- ✅ Skipped when **user manually selects** language/wordlist
- ✅ Cache cleared when switching encode/decode modes
- ✅ Minimum 10 character input required
- ✅ Only updates if score > 0 (at least some words matched)

## Browser Console Testing

Open browser DevTools console and try:

```javascript
// Check cache
dialectDetectionCache.size  // Should show number of cached entries

// Check current mode
mode  // Should be 'encode' or 'decode'

// Check skip flag
skipAutoDetect  // Should be false normally

// Manually trigger detection
autoDetectDialect()  // Runs detection on current input
```

## Known Limitations

1. **Performance:** Detection tries ALL language/wordlist combinations - can be slow with many dialects
2. **Ambiguity:** If two dialects extract same number of words, picks first found
3. **No confidence score:** Doesn't show how confident the detection is
4. **No fallback UI:** If no dialect matches, dropdowns stay as-is (no "unknown" indicator)

## Future Enhancements

- [ ] Show confidence percentage
- [ ] Early exit when perfect match found (optimization)
- [ ] Show "top 3" candidates if close
- [ ] Add manual "re-detect" button
- [ ] Tooltip explaining what auto-detection does
- [ ] Loading spinner during detection for slow cases
