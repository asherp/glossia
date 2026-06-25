#!/usr/bin/env node
/**
 * Test script to demonstrate dialect detection logic
 *
 * This simulates what happens in the browser when auto-detection runs.
 */

import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Import the WASM module (requires Node 18+)
const wasmPath = join(__dirname, 'glossia_bg.wasm');
const jsPath = join(__dirname, 'glossia.js');

console.log('🧪 Glossia Dialect Detection Test\n');

// Since we're in Node, we need to polyfill the browser environment
async function runTest() {
  try {
    // Import the WASM module
    const { encode, decode, get_languages, get_wordlists } = await import('./glossia.js');

    console.log('✅ WASM module loaded successfully\n');

    // Get available languages
    const languages = JSON.parse(get_languages());
    console.log('📚 Available languages:', languages);

    // Get wordlists for each language
    console.log('\n📖 Available wordlists:');
    for (const lang of languages) {
      const wordlists = JSON.parse(get_wordlists(lang));
      console.log(`  ${lang}:`, wordlists.join(', '));
    }

    console.log('\n' + '='.repeat(60));
    console.log('TEST 1: Encode with english/bip39, detect it back');
    console.log('='.repeat(60));

    // Test encoding
    const input = "Hello Glossia dialect detection!";
    const lang = "english";
    const wl = "bip39";
    const grammar = "body";
    const seed = BigInt(42);

    console.log(`\n📝 Input: "${input}"`);
    console.log(`🔧 Config: ${lang}/${wl}, grammar=${grammar}, seed=${seed}`);

    const encodeResult = JSON.parse(encode(input, lang, wl, grammar, seed));

    if (encodeResult.error) {
      console.error('❌ Encode error:', encodeResult.error);
      return;
    }

    const encodedText = encodeResult.encoded_text;
    const payloadWords = encodeResult.payload_words || [];
    const stats = encodeResult.stats || {};

    console.log(`\n✨ Encoded: "${encodedText}"`);
    console.log(`📊 Stats: ${stats.payload_count} payload words, ${stats.cover_count} cover words`);
    console.log(`📦 Payload words:`, payloadWords.join(', '));

    // Now simulate auto-detection
    console.log('\n🔍 Running dialect detection (trying all combinations)...\n');

    let bestLang = null;
    let bestWl = null;
    let bestScore = 0;
    let attempts = 0;

    for (const testLang of languages) {
      const wordlists = JSON.parse(get_wordlists(testLang));
      for (const testWl of wordlists) {
        attempts++;
        try {
          const result = JSON.parse(decode(encodedText, testLang, testWl));
          const payloadCount = (result.payload_words || []).length;

          console.log(`  Try ${attempts}: ${testLang}/${testWl} → ${payloadCount} words`);

          if (payloadCount > bestScore) {
            bestScore = payloadCount;
            bestLang = testLang;
            bestWl = testWl;
          }
        } catch (e) {
          // Ignore errors
          console.log(`  Try ${attempts}: ${testLang}/${testWl} → ERROR`);
        }
      }
    }

    console.log(`\n🎯 Detection result:`);
    console.log(`   Best match: ${bestLang}/${bestWl} (${bestScore} words)`);
    console.log(`   Correct? ${bestLang === lang && bestWl === wl ? '✅ YES' : '❌ NO'}`);

    // Decode with detected dialect
    console.log(`\n🔓 Decoding with detected dialect (${bestLang}/${bestWl})...`);
    const decodeResult = JSON.parse(decode(encodedText, bestLang, bestWl));

    console.log(`   Decoded: "${decodeResult.decoded_text}"`);
    console.log(`   Match original? ${decodeResult.decoded_text === input ? '✅ YES' : '❌ NO'}`);

    console.log('\n' + '='.repeat(60));
    console.log('TEST COMPLETE ✅');
    console.log('='.repeat(60));

    console.log(`\n💡 In the browser, this would:`);
    console.log(`   1. Automatically update language dropdown to "${bestLang}"`);
    console.log(`   2. Automatically update wordlist dropdown to "${bestWl}"`);
    console.log(`   3. Show green indicator "✓ ${bestLang}/${bestWl}" for 2.5 seconds`);
    console.log(`   4. Display decoded output: "${decodeResult.decoded_text}"`);

  } catch (error) {
    console.error('❌ Test failed:', error.message);
    console.error('\n💡 Note: This test requires Node.js 18+ with WASM support');
    console.error('   To test in browser, run: python3 -m http.server -d web 8080');
  }
}

runTest();
