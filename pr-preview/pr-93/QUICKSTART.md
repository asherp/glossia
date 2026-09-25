# Glossia Dialect Detection - Quick Start Guide

## 🚀 Start the Server

```bash
cd /Users/asherp/git/glossia/web
python3 -m http.server 8080
```

Then open: **http://localhost:8080/index.html**

## ✨ Try the Auto-Detection

### Step-by-Step Demo

1. **Open the landing page**
   - You'll see the Glossia interface with Encode mode active
   - Default: english/bip39 wordlist

2. **Generate sample data**
   - Click "Random 12" button
   - You'll see 12 random BIP39 words appear

3. **Encode the words**
   - The page auto-encodes as you type
   - Payload words are highlighted in purple
   - Notice the stats: payload/cover word ratio

4. **Copy the encoded text**
   - Click the "Copy" button in the output panel
   - Or manually select and copy the generated prose

5. **Switch to Decode mode**
   - Click the swap button (⇄) in the center
   - Interface switches to Decode mode
   - Input placeholder changes to "Paste encoded text here..."

6. **Paste and watch the magic! ✨**
   - Paste the encoded text you copied
   - **AUTOMATICALLY HAPPENS:**
     - Language/wordlist dropdowns update to "english/bip39"
     - Green checkmark appears: "✓ english/bip39"
     - Text is decoded back to original 12 words
     - Indicator fades after 2.5 seconds

## 🧪 Advanced Testing

### Test Cross-Dialect Detection

1. In Encode mode, select "cs" language and "base16" wordlist
2. Type some text (e.g., "Hello World")
3. Copy the encoded output
4. Switch to Decode mode
5. Change language to "english" (wrong on purpose)
6. Paste the cs/base16 text
7. **Watch it auto-correct to cs/base16**

### Test Manual Override

1. In Decode mode with text pasted
2. Manually change language dropdown to "latin"
3. Notice it stays on "latin" (respects your choice)
4. Type more text in the input
5. Now detection runs again on the new input

### Run Automated Tests

Open: **http://localhost:8080/test.html**

Click the buttons to run:
- **Test 1:** Full encode → detect → decode cycle
- **Test 2:** Cross-dialect accuracy (tests 5 combinations)
- **Test 3:** Performance benchmark

## 🎯 What to Look For

### Visual Indicators

✅ **Successful Detection:**
- Mode label changes to "✓ english/bip39" (or detected dialect)
- Text color turns green
- Reverts to "Decode" after 2.5 seconds

❌ **No Detection:**
- Input too short (<10 characters)
- No payload words found in text
- User manually changed controls

### Browser Console

Press F12 and check console for:
```javascript
// Check detection cache
dialectDetectionCache.size  // Number of cached results

// Current mode
mode  // 'encode' or 'decode'

// Skip flag state
skipAutoDetect  // Should be false normally
```

## 📊 Performance Expectations

Typical detection times (depends on device):
- **Fast:** < 100ms (modern desktop)
- **Normal:** 100-500ms (average laptop)
- **Slow:** > 500ms (older devices, many dialects)

The page tries all language/wordlist combinations:
- English: 3 wordlists
- CS: 3 wordlists
- Latin: 1 wordlist
- **Total:** ~7 combinations tested per detection

## 🐛 Troubleshooting

### Detection not running?
- Ensure you're in **Decode mode** (not Encode)
- Input must be **10+ characters**
- Try clearing the cache: refresh the page

### Wrong dialect detected?
- Some texts may match multiple dialects
- Detection picks the one with **most payload words**
- You can always manually override

### Detection too slow?
- This is expected with many dialects
- Results are **cached** for repeated pastes
- Consider reducing debounce delay in code

### Server not accessible?
```bash
# Kill existing server
lsof -ti:8080 | xargs kill -9

# Restart
cd /Users/asherp/git/glossia/web
python3 -m http.server 8080
```

## 📁 Test Files Created

- `index.html` - Main landing page with auto-detection
- `test.html` - Automated test suite
- `TEST_DIALECT_DETECTION.md` - Detailed test plan
- `test_detection.mjs` - Node.js simulation
- `QUICKSTART.md` - This file

## 🎓 Understanding the Code

Key functions in `index.html`:

```javascript
// Main detection function (line ~636)
function autoDetectDialect() {
  // Loops through all language/wordlist pairs
  // Decodes with each, counts payload words
  // Selects best match
}

// Apply detected settings (line ~688)
function applyDetectedDialect(lang, wl, score) {
  // Updates dropdowns
  // Shows indicator
}

// Visual feedback (line ~703)
function showDetectionIndicator(lang, wl, count) {
  // Green checkmark for 2.5s
}
```

## 🚀 Next Steps

1. ✅ **Test the basic flow** (above)
2. ✅ **Try cross-dialect detection**
3. ✅ **Run automated tests**
4. ✅ **Check performance**
5. 📝 Provide feedback for improvements

## 💡 Pro Tips

- **Cache warmup:** First paste may be slower, subsequent pastes use cache
- **Manual override:** Change dropdown after paste to override detection
- **Clear cache:** Refresh page to clear detection cache
- **Test different dialects:** Try encoding with cs/base64 or latin/hp

---

**Glossia Dialect Detection** - Automatically detects which language/wordlist was used to encode text.
Built with ❤️ using WASM + vanilla JavaScript.
