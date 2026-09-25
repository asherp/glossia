/* @ts-self-types="./glossia.d.ts" */

/**
 * Align received prose against the rendering it should have been, without
 * decoding either.
 *
 * This is the half that locates damage. Decoding filters prose against the
 * wordlist, so damage does not stay put: a payload word mangled off the
 * wordlist never arrives, and a cover word mangled onto it arrives as a symbol
 * nobody sent. Alignment puts both back in the rendering's own coordinates,
 * producing a codeword of known length with its holes marked.
 *
 * It cannot bootstrap: `rendered` must be a candidate rendering to compare
 * against, which a caller gets from its own (often memoized) encode of a
 * payload it already has. That is the checking case — confirming a
 * transcription of something on the page — not the recovering-from-nothing
 * case.
 *
 * Returns JSON `{ tokens, matched, payload_slots, erasures, spurious, clean,
 * payload_intact }` or `{ error }`.
 * @param {string} received
 * @param {string} rendered
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function align_prose(received, rendered, language, wordlist) {
    let deferred5_0;
    let deferred5_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(received, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(rendered, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.align_prose(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Decode canonical prose and verify it by re-rendering under the rules of the
 * version byte the artifact carries — not the current version, so artifacts
 * from older canonical versions keep verifying. `canonical_text` is the
 * reference rendering the verification compared against, so a checker can
 * diff wording without generating again.
 *
 * `repaired` names the word positions Reed–Solomon parity corrected on the way
 * to this payload — empty under a version carrying none, and empty under v3
 * when the prose arrived intact. Non-empty means the payload is a *correction*
 * rather than a transcription, so a UI should show it rather than apply it
 * silently: the bytes are backed by the envelope's crc32, but a reader who
 * mis-copied a word is better told which one.
 *
 * Returns JSON
 * `{ version, payload_hex, verified, canonical_text, repaired, alignment }` or
 * `{ error }`.
 * @param {string} text
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function canonical_decode(text, language, wordlist) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_decode(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Decode prose written by `canonical_encode_fixed` and verify it by
 * re-rendering. `payload_len` is the payload's byte count, the envelope's own
 * bytes excluded.
 *
 * See `canonical_decode` for what `repaired` means. This entry finds damage on
 * its own, so it repairs within `2·errors ≤ parity` — half what it manages when
 * told where the damage is. `canonical_decode_fixed_repaired` and
 * `canonical_decode_slots_fixed` are the entries that get told.
 *
 * Returns JSON
 * `{ version, payload_hex, verified, canonical_text, repaired, alignment }` or
 * `{ error }`.
 * @param {string} text
 * @param {string} language
 * @param {string} wordlist
 * @param {number} payload_len
 * @returns {string}
 */
export function canonical_decode_fixed(text, language, wordlist, payload_len) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_decode_fixed(retptr, ptr0, len0, ptr1, len1, ptr2, len2, payload_len);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * `canonical_decode_fixed`, told which payload-word positions are already known
 * bad.
 *
 * A located fault costs parity half what an unlocated one does — `2·errors +
 * erasures ≤ parity` — so this repairs twice the damage the plain entry can.
 * `erasures_json` is a JSON array of positions into the harvested payload-word
 * sequence, parity words included, which is the coordinate system
 * `alignment.payload_slots` is already in. An empty array is always valid.
 *
 * A word mangled *off* the wordlist never reaches the harvest at all, so the
 * text alone cannot carry its position — that damage needs
 * `canonical_decode_slots_fixed`, which takes the slots rather than the prose.
 *
 * Returns the same JSON as `canonical_decode_fixed`, or `{ error }`.
 * @param {string} text
 * @param {string} language
 * @param {string} wordlist
 * @param {number} payload_len
 * @param {string} erasures_json
 * @returns {string}
 */
export function canonical_decode_fixed_repaired(text, language, wordlist, payload_len, erasures_json) {
    let deferred5_0;
    let deferred5_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(erasures_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.canonical_decode_fixed_repaired(retptr, ptr0, len0, ptr1, len1, ptr2, len2, payload_len, ptr3, len3);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
    }
}

/**
 * The decode half alone: payload words → version + payload, no verification
 * re-render. For repair searches that decode many candidates and render each
 * through their own (memoized) `canonical_encode` call.
 *
 * Returns JSON `{ version, payload_hex }` or `{ error }`.
 * @param {string} text
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function canonical_decode_raw(text, language, wordlist) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_decode_raw(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * The fixed decode half alone: no verification re-render, checksum still
 * checked.
 *
 * Returns JSON `{ version, payload_hex }` or `{ error }`.
 * @param {string} text
 * @param {string} language
 * @param {string} wordlist
 * @param {number} payload_len
 * @returns {string}
 */
export function canonical_decode_raw_fixed(text, language, wordlist, payload_len) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_decode_raw_fixed(retptr, ptr0, len0, ptr1, len1, ptr2, len2, payload_len);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Decode from aligned payload slots rather than from prose.
 *
 * `slots_json` is a JSON array holding, per payload word the rendering
 * expected, the word that arrived there or `null` where nothing usable did —
 * exactly the `alignment.payload_slots` this module's decode entries and
 * `align_prose` return. Every `null` becomes an erasure.
 *
 * Taking slots rather than text is what makes a word mangled *off* the
 * wordlist repairable. Such a word never arrives in the harvest, so the
 * sequence comes up one short and every later word slides into the wrong slot;
 * the position survives only in the alignment. Passing prose would lose it.
 *
 * Returns the same JSON as `canonical_decode_fixed`, or `{ error }`.
 * @param {string} slots_json
 * @param {string} language
 * @param {string} wordlist
 * @param {number} payload_len
 * @returns {string}
 */
export function canonical_decode_slots_fixed(slots_json, language, wordlist, payload_len) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(slots_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_decode_slots_fixed(retptr, ptr0, len0, ptr1, len1, ptr2, len2, payload_len);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Canonical, versioned encode: exactly one prose form per payload, rendered
 * under the frozen rules of the current canonical version (see
 * `src/canonical.rs`). `payload_hex` is the payload bytes as hex.
 *
 * Returns JSON `{ encoded_text, version }` or `{ error }`.
 * @param {string} payload_hex
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function canonical_encode(payload_hex, language, wordlist) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(payload_hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_encode(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Canonical encode at an EXPLICIT format version, rather than the current one.
 *
 * The framing follows the version's own rules, so this writes a version-1
 * artifact (version byte leading, no checksum) as faithfully as a current one.
 * It is the re-render half of verification — a JS host checking an old artifact
 * needs to reproduce it under the rules it was written with — and the escape
 * hatch for emitting prose an older release can still read.
 *
 * Returns JSON `{ encoded_text, version }` or `{ error, kind }`.
 * @param {string} payload_hex
 * @param {string} language
 * @param {string} wordlist
 * @param {number} version
 * @returns {string}
 */
export function canonical_encode_at(payload_hex, language, wordlist, version) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(payload_hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_encode_at(retptr, ptr0, len0, ptr1, len1, ptr2, len2, version);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Canonical encode with NO padding word, for a payload whose byte count the
 * caller already knows and can restate when decoding. Same envelope, same
 * version, same rules as `canonical_encode` — one word shorter, and the prose
 * opens on payload rather than on a constant the payload's length fixed.
 *
 * Returns JSON `{ encoded_text, version }` or `{ error }`.
 * @param {string} payload_hex
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function canonical_encode_fixed(payload_hex, language, wordlist) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(payload_hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_encode_fixed(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * `canonical_encode_fixed` with placements, for UIs that annotate the prose.
 *
 * Returns JSON `{ encoded_text, version, placements: [...] }` or `{ error }`.
 * @param {string} payload_hex
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function canonical_encode_fixed_traced(payload_hex, language, wordlist) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(payload_hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_encode_fixed_traced(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Canonical encode with placements — the same text as `canonical_encode`, plus
 * where each payload word landed, for UIs that annotate the prose.
 *
 * Returns JSON `{ encoded_text, version, placements: [{ word, payload_index,
 * pos, token_index, sentence, role }] }` or `{ error }`.
 * @param {string} payload_hex
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function canonical_encode_traced(payload_hex, language, wordlist) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(payload_hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.canonical_encode_traced(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Derive a cover seed from a payload checksum, so the choice of prose carries the
 * checksum. `hex` is the exact byte string the checksum covers.
 *
 * Exposed so browsers and the Rust core agree bit-for-bit rather than
 * reimplementing CRC-32 and splitmix64 in JavaScript.
 * @param {string} hex
 * @param {bigint} counter
 * @returns {bigint}
 */
export function checksum_seed_for(hex, counter) {
    const ptr0 = passStringToWasm0(hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.checksum_seed_for(ptr0, len0, counter);
    return BigInt.asUintN(64, ret);
}

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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.decode(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(hex_colors_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.decode_image_banner(retptr, ptr0, len0, nsym);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(hex_colors_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(palette, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.decode_image_from_colors(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.decode_raw_base_n(retptr, ptr0, len0, ptr1, len1, ptr2, len2, expected_byte_count);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.detect_dialect_from_text(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(grammar_dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.encode(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Like `encode`, but samples `best_of` candidate encodings and returns the
 * densest / most semantically coherent one (English only; falls back to a single
 * encoding for other languages or `best_of <= 1`). Payload words are preserved
 * in order in every candidate, so decoding is unaffected.
 * @param {string} input
 * @param {string} language
 * @param {string} wordlist
 * @param {string} grammar_dialect
 * @param {bigint} seed
 * @param {number} best_of
 * @returns {string}
 */
export function encode_best_of(input, language, wordlist, grammar_dialect, seed, best_of) {
    let deferred5_0;
    let deferred5_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(grammar_dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.encode_best_of(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed, best_of);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(grammar_dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.encode_characters(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(payload_hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.encode_image_banner(retptr, ptr0, len0, width, height, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(grammar_dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.encode_random_words(retptr, count, ptr0, len0, ptr1, len1, ptr2, len2, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
    }
}

/**
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.encode_raw_base_n(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Like `encode_raw_base_n`, but samples `best_of` candidates and returns the
 * densest / most semantically coherent one (English only; single encoding
 * otherwise). Payload words stay in order in every candidate, so decoding is
 * unaffected. This is the entry the web board uses for reader-facing prose.
 * @param {string} input
 * @param {string} language
 * @param {string} wordlist
 * @param {string} dialect
 * @param {bigint} seed
 * @param {number} best_of
 * @returns {string}
 */
export function encode_raw_base_n_best_of(input, language, wordlist, dialect, seed, best_of) {
    let deferred5_0;
    let deferred5_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.encode_raw_base_n_best_of(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed, best_of);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Wrap caller-supplied payload words in cover prose, reporting where each landed.
 *
 * For formats that pack their own bits — a fixed-length address that carries a
 * header in the bit-packing slack, say — where the byte-oriented entries cannot
 * express the packing. `words_json` is a JSON array of payload words, embedded in
 * the order given.
 *
 * Returns JSON `{ encoded_text, counter, placements: [{ word, payload_index, pos,
 * token_index, sentence, role }] }`, or `{ error }` if any word is not in the
 * payload wordlist (which would otherwise be silently dropped).
 * @param {string} words_json
 * @param {string} language
 * @param {string} wordlist
 * @param {string} dialect
 * @param {bigint} seed
 * @param {number} best_of
 * @returns {string}
 */
export function encode_words(words_json, language, wordlist, dialect, seed, best_of) {
    let deferred5_0;
    let deferred5_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(words_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len3 = WASM_VECTOR_LEN;
        wasm.encode_words(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, seed, best_of);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.get_all_dialects(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.get_bits_per_word(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
    }
}

/**
 * The cover vocabulary for a language/dialect, as a flat JSON array.
 *
 * The complement of `get_payload_words`, and what a verifier needs to tell a
 * misspelling from a word that was never payload. Locating a damaged payload
 * word means searching tokens that are not in the payload wordlist — but the
 * connective prose is not in it either, so without this list every cover word
 * is a candidate site and the search does an order of magnitude more work than
 * the question requires.
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function get_cover_words(language, wordlist) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.get_cover_words(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.get_default_wordlist(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.get_languages(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Encode raw bytes (hex or base64 input) into base-N payload words.
 *
 * The payload wordlist itself, as a JSON array in index order.
 *
 * Needed by callers that pack their own bits: the index of a word IS its value,
 * so a format doing its own bit-packing needs the mapping. Returns `{ error }` on
 * an unknown language/wordlist.
 * @param {string} language
 * @param {string} wordlist
 * @returns {string}
 */
export function get_payload_words(language, wordlist) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.get_payload_words(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.get_wordlist_size(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.get_wordlists(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
    }
}

/**
 * CRC-32 of a hex byte string, for display alongside an artifact.
 * @param {string} hex
 * @returns {number}
 */
export function payload_crc32(hex) {
    const ptr0 = passStringToWasm0(hex, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.payload_crc32(ptr0, len0);
    return ret >>> 0;
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(source, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(target, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.pipeline_execute(retptr, ptr0, len0, ptr1, len1, ptr2, len2, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred4_0 = r0;
        deferred4_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(wordlist, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.random_words(retptr, count, ptr0, len0, ptr1, len1, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(dialect, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.render_image_svg(retptr, ptr0, len0, ptr1, len1, width, height, seed, circular);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(meta_instruction, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.transcode(retptr, ptr0, len0, ptr1, len1, seed);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred3_0 = r0;
        deferred3_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_bb96b2010945f0bc: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
    };
    return {
        __proto__: null,
        "./glossia_bg.js": import0,
    };
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
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
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

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
