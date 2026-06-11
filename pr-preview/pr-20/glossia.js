/* @ts-self-types="./glossia.d.ts" */

/**
 * Decode encoded text back to original input.
 *
 * Returns JSON: `{ "payload_words": [...], "decoded_text": "..." }`
 * @param {string} text
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function decode(text, language, wordlist) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.decode(ptr0, len0, ptr1, len1, ptr2, len2);
        deferred4_0 = ret[0];
        deferred4_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Decode a banner from hex colors extracted from SVG cells.
 *
 * Takes a JSON array of hex color strings (one per Voronoi cell, in scan order)
 * and decodes the embedded payload bytes.
 *
 * Returns JSON: `{ "payload_hex": "...", "n_palette": N, "epsilon": E, "success": true }`
 * or `{ "error": "..." }` on failure.
 * @param {string} hex_colors_json
 * @param {number} nsym
 * @returns {string}
 */
export function decode_image_banner(hex_colors_json, nsym) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(hex_colors_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.decode_image_banner(ptr0, len0, nsym);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Decode an image from extracted hex colors back to original data.
 *
 * Takes a JSON array of CSS hex colors (e.g., `["#440255", "#2a788e", ...]`)
 * extracted from SVG fill attributes, plus the palette name (e.g., "viridis").
 *
 * Each hex color is matched to the nearest CIELAB payload word in the palette
 * wordlist. The matched words are then decoded back to the original payload.
 *
 * Returns JSON: `{ "decoded_text": "...", "payload_words": [...],
 *   "color_words": [...], "n_colors": N, "bits_per_color": B }`
 * or `{ "error": "..." }` on failure.
 * @param {string} hex_colors_json
 * @param {string} palette
 * @returns {string}
 */
export function decode_image_from_colors(hex_colors_json, palette) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(hex_colors_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(palette, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.decode_image_from_colors(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Decode base-N encoded text back to raw bytes (hex output).
 *
 * Works with both bare payload words and prose-wrapped text —
 * cover words are automatically filtered out.
 *
 * `expected_byte_count`: the known payload size in bytes (e.g. 32 for a pubkey).
 * Pass 0 to infer from word count (exact when bits_per_word divides payload evenly).
 *
 * Returns JSON: `{ "decoded_hex": "...", "payload_words": [...] }`
 * @param {string} text
 * @param {string} language
 * @param {string} wordlist
 * @param {number} expected_byte_count
 * @returns {string}
 */
export function decode_raw_base_n(text, language, wordlist, expected_byte_count) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.decode_raw_base_n(ptr0, len0, ptr1, len1, ptr2, len2, expected_byte_count);
        deferred4_0 = ret[0];
        deferred4_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Auto-detect which dialect (language + wordlist) best matches the given text.
 *
 * Uses compile-time precomputed word indices for fast O(log n) binary search.
 * Returns all matches sorted by score (best first).
 *
 * Returns JSON array: `[{ "language": "english", "wordlist": "bip39", "dialects": ["body", "subject"],
 *                         "hits": 10, "total": 12, "hit_rate": 0.83, "wordlist_size": 2048 }, ...]`
 * @param {string} text
 * @returns {string}
 */
export function detect_dialect_from_text(text) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.detect_dialect_from_text(ptr0, len0);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Encode input text into natural language prose.
 *
 * Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... } }`
 * @param {string} input
 * @param {string} language
 * @param {string} wordlist
 * @param {string} grammar_dialect
 * @param {bigint} seed
 * @returns {string}
 */
export function encode(input, language, wordlist, grammar_dialect, seed) {
    let deferred5_0;
    let deferred5_1;
    try {
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(grammar_dialect, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.encode(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed);
        deferred5_0 = ret[0];
        deferred5_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Encode pre-formatted data (hex, base58, base64) using character-level encoding.
 *
 * This bypasses the codec layer and uses each character directly as a payload word.
 * Designed for CS (cryptographic signature) dialects that use payload_separator: "".
 *
 * Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... } }`
 * @param {string} input
 * @param {string} language
 * @param {string} wordlist
 * @param {string} grammar_dialect
 * @param {bigint} seed
 * @returns {string}
 */
export function encode_characters(input, language, wordlist, grammar_dialect, seed) {
    let deferred5_0;
    let deferred5_1;
    try {
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(grammar_dialect, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.encode_characters(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed);
        deferred5_0 = ret[0];
        deferred5_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Encode a hex payload into a banner SVG.
 *
 * Creates a Voronoi banner with RS error correction encoding the payload
 * bytes. The SVG contains self-describing header + payload cells.
 *
 * Args:
 *   - `payload_hex`: hex-encoded payload bytes (e.g., "deadbeef...")
 *   - `width`, `height`: banner dimensions
 *   - `seed`: random seed for Voronoi layout
 *
 * Returns SVG string on success, or JSON `{"error":"..."}` on failure.
 * @param {string} payload_hex
 * @param {number} width
 * @param {number} height
 * @param {bigint} seed
 * @returns {string}
 */
export function encode_image_banner(payload_hex, width, height, seed) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(payload_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.encode_image_banner(ptr0, len0, width, height, seed);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Generate random words and encode them directly as word indices.
 *
 * This bypasses the codec layer (no hex/base64/ascii detection) and directly
 * uses the random words as payload. Much more efficient for BIP39-style use cases.
 *
 * Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... }, "data_mode": "words" }`
 * @param {number} count
 * @param {string} language
 * @param {string} wordlist
 * @param {string} grammar_dialect
 * @param {bigint} seed
 * @returns {string}
 */
export function encode_random_words(count, language, wordlist, grammar_dialect, seed) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(grammar_dialect, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.encode_random_words(count, ptr0, len0, ptr1, len1, ptr2, len2, seed);
        deferred4_0 = ret[0];
        deferred4_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Encode raw bytes (hex or base64 input) into base-N payload words.
 *
 * If `dialect` is empty, returns space-joined bare payload words.
 * If `dialect` is provided (e.g., "body"), wraps the payload words in prose.
 *
 * Returns JSON: `{ "encoded_text": "...", "payload_words": [...], "stats": { ... } }`
 * @param {string} input
 * @param {string} language
 * @param {string} wordlist
 * @param {string} dialect
 * @param {bigint} seed
 * @returns {string}
 */
export function encode_raw_base_n(input, language, wordlist, dialect, seed) {
    let deferred5_0;
    let deferred5_1;
    try {
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(dialect, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.encode_raw_base_n(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed);
        deferred5_0 = ret[0];
        deferred5_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Get all available dialects across all languages with full metadata.
 *
 * Returns a hierarchical structure for building a dialect selector UI.
 *
 * Returns JSON:
 * ```json
 * [
 *   {
 *     "language": "english",
 *     "language_display": "English",
 *     "dialects": [
 *       {
 *         "dialect": "body",
 *         "display_name": "BIP39 (Natural Body)",
 *         "full_id": "english-bip39-body",
 *         "payload_wordlist": "bip39",
 *         "cover_wordlist": "default",
 *         "wordlist_size": 2048,
 *         "bits_per_word": 11.0,
 *         "is_character_level": false,
 *         "description": "Natural multi-sentence prose"
 *       },
 *       ...
 *     ]
 *   },
 *   ...
 * ]
 * ```
 * @returns {string}
 */
export function get_all_dialects() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.get_all_dialects();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Get the bits per word for a language/wordlist combination.
 *
 * For power-of-two wordlists (BIP39, etc.): returns exact integer bits
 * For non-power-of-two wordlists (base58, base64): returns fractional bits
 *
 * Returns JSON: `{ "bits_per_word": 11.0, "is_power_of_two": true }` or `{ "error": "..." }`
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function get_bits_per_word(language, wordlist) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.get_bits_per_word(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Return the default wordlist profile name for a language.
 *
 * Uses the grammar-declared `default_wordlist` if present, otherwise
 * falls back to the first available profile.
 * @param {string} language
 * @returns {string}
 */
export function get_default_wordlist(language) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.get_default_wordlist(ptr0, len0);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Return JSON array of available language names.
 * @returns {string}
 */
export function get_languages() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.get_languages();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Get the exact wordlist size for a language/wordlist combination.
 *
 * Returns JSON: `{ "size": 2048 }` or `{ "error": "..." }`
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function get_wordlist_size(language, wordlist) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.get_wordlist_size(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Return JSON array of available wordlist profiles for a language.
 * @param {string} language
 * @returns {string}
 */
export function get_wordlists(language) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.get_wordlists(ptr0, len0);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Execute a pipeline with explicit source and target endpoints.
 *
 * Source/target follow the same format as `--from`/`--into` CLI flags:
 * - Language: `"english"`, `"latin"`, `"english/bip39/body"`
 * - Format: `"hex"`, `"base64"`, `"ascii7"`, `"bytes"`
 * - Auto: `"auto"` — auto-detect from input content
 *
 * Returns JSON: `{ "output": "...", "source": "...", "target": "..." }` or `{ "error": "..." }`
 * @param {string} input
 * @param {string} source
 * @param {string} target
 * @param {bigint} seed
 * @returns {string}
 */
export function pipeline_execute(input, source, target, seed) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(target, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.pipeline_execute(ptr0, len0, ptr1, len1, ptr2, len2, seed);
        deferred4_0 = ret[0];
        deferred4_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Generate random payload words.
 *
 * Returns JSON array of random words from the specified wordlist.
 * @param {number} count
 * @param {string} language
 * @param {string} wordlist
 * @param {bigint} seed
 * @returns {string}
 */
export function random_words(count, language, wordlist, seed) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.random_words(count, ptr0, len0, ptr1, len1, seed);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Render text notation (containing color tokens) as an SVG string.
 *
 * Mirrors the CLI `render_text_to_svg` logic: extracts hex colors from the
 * text notation, maps the dialect name to a layout, and returns SVG markup.
 *
 * `dialect` selects the layout: "voronoi" (default), "grid", "constellation", "patches".
 * `circular` enables circular (disk) clipping on the canvas.
 *
 * Returns the raw SVG string on success, or JSON `{"error":"..."}` on failure.
 * @param {string} text
 * @param {string} dialect
 * @param {number} width
 * @param {number} height
 * @param {bigint} seed
 * @param {boolean} circular
 * @returns {string}
 */
export function render_image_svg(text, dialect, width, height, seed, circular) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(dialect, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.render_image_svg(ptr0, len0, ptr1, len1, width, height, seed, circular);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Transcode text from one language to another via Pipeline.
 *
 * The meta instruction is a natural-language pipeline specification:
 * - `"translate from english into latin"` — transcode English prose to Latin
 * - `"encode into english"` — encode raw data into English prose
 * - `"decode from english"` — decode English prose back to raw data
 *
 * Returns JSON: `{ "output": "...", "source": "...", "target": "..." }` or `{ "error": "..." }`
 * @param {string} input
 * @param {string} meta_instruction
 * @param {bigint} seed
 * @returns {string}
 */
export function transcode(input, meta_instruction, seed) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(meta_instruction, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.transcode(ptr0, len0, ptr1, len1, seed);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_bbadd78c1bac3a77: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./glossia_bg.js": import0,
    };
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('glossia_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
