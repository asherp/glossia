//! The WASM boundary for v3's repair path.
//!
//! `tests/canonical.rs` proves the library repairs damage. What is proved here
//! is that a browser can *reach* that: the exports speak JSON, and the shape one
//! returns has to be the shape the next one accepts. The round trip these tests
//! walk — align, read the slots, hand them back — is the one a checker UI walks,
//! so a mismatch between `alignment.payload_slots` and what
//! `canonical_decode_slots_fixed` parses would strand the repair path with every
//! underlying algorithm still passing its own tests.
//!
//! These call the exports as plain Rust. `#[wasm_bindgen]` leaves the function
//! itself alone off-wasm, so the JSON shaping under test is the same code the
//! browser gets.
//!
//! `src/wasm.rs` is behind the `wasm` feature, which CI's `cargo test
//! --workspace` does not enable, so the whole file compiles away without it.
//! Run these with `cargo test --features wasm`.

#![cfg(feature = "wasm")]

use glossia::wasm::{
    align_prose, canonical_decode_fixed, canonical_decode_fixed_repaired,
    canonical_decode_slots_fixed,
};
use glossia::{canonical_encode_fixed, cached_payload_tree};
use serde_json::Value;

const LANG: &str = "english";
const WL: &str = "bip39";

fn json(s: &str) -> Value {
    serde_json::from_str(s).expect("every export must return parseable JSON")
}

/// The decode harvest's own predicate, for staging damage the way the library
/// sees it.
fn is_payload(word: &str) -> bool {
    let tree = cached_payload_tree(LANG, WL).expect("payload tree");
    tree.words().iter().any(|w| w.to_lowercase() == word)
}

/// A payload word occurring exactly once and whitespace-delimited, with its
/// slot — so a replacement lands where the test says it does.
fn a_lone_payload_word(text: &str) -> (usize, String) {
    let slots = glossia::codec::payload_tokens(text, &is_payload);
    slots
        .iter()
        .enumerate()
        .find(|(_, w)| {
            slots.iter().filter(|x| x == w).count() == 1 && text.contains(&format!(" {w} "))
        })
        .map(|(k, w)| (k, w.clone()))
        .expect("the rendering must contain a lone whitespace-delimited payload word")
}

fn hash160() -> Vec<u8> {
    (0u8..20).collect()
}

#[test]
fn a_clean_decode_reports_no_repair_and_a_clean_alignment() {
    let payload = hash160();
    let text = canonical_encode_fixed(&payload, LANG, WL).unwrap();

    let d = json(&canonical_decode_fixed(&text, LANG, WL, payload.len()));

    assert_eq!(d["version"], 3);
    assert_eq!(d["verified"], true);
    assert_eq!(
        d["repaired"].as_array().unwrap().len(),
        0,
        "intact prose must report no correction"
    );
    assert_eq!(d["alignment"]["clean"], true);
    assert_eq!(
        d["alignment"]["erasures"].as_array().unwrap().len(),
        0,
        "a clean alignment has no holes"
    );
}

#[test]
fn repaired_names_the_damaged_word_through_the_json_boundary() {
    // The field that tells a reader which word they mis-copied. It exists on
    // the Rust type; this is the assertion that it survives serialization.
    let payload = hash160();
    let text = canonical_encode_fixed(&payload, LANG, WL).unwrap();
    let (slot, word) = a_lone_payload_word(&text);

    // Swap it for another payload word: damage the decoder can find unaided,
    // since the substitute is still on the wordlist.
    let tree = cached_payload_tree(LANG, WL).unwrap();
    let present: Vec<String> = text
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .collect();
    let intruder = tree
        .words()
        .iter()
        .map(|w| w.to_lowercase())
        .find(|w| !present.contains(w))
        .expect("the wordlist must hold a word this rendering did not use");

    let damaged = text.replacen(&format!(" {word} "), &format!(" {intruder} "), 1);
    assert_ne!(damaged, text, "the swap must actually land");

    let d = json(&canonical_decode_fixed(&damaged, LANG, WL, payload.len()));

    assert_eq!(
        d["payload_hex"].as_str().unwrap(),
        glossia::codec::hex_encode(&payload),
        "the payload must come back intact"
    );
    assert_eq!(
        d["repaired"], serde_json::json!([slot]),
        "the repair must name the damaged word, not merely happen"
    );
}

#[test]
fn align_prose_slots_feed_straight_into_the_slot_decoder() {
    // The contract that makes the repair path reachable from a browser, and the
    // one place two exports have to agree. A payload word mangled OFF the
    // wordlist never reaches the harvest, so the prose alone cannot say a word
    // is missing — the sequence is one shorter and every later word has slid up
    // a slot. Only the alignment holds the position, so `payload_slots` must
    // cross the boundary and come back parseable, nulls and all.
    let payload = hash160();
    let text = canonical_encode_fixed(&payload, LANG, WL).unwrap();
    let (slot, word) = a_lone_payload_word(&text);

    let damaged = text.replacen(&format!(" {word} "), &format!(" {word}zz "), 1);
    assert_ne!(damaged, text);

    let a = json(&align_prose(&damaged, &text, LANG, WL));
    assert_eq!(
        a["erasures"], serde_json::json!([slot]),
        "alignment must locate the hole"
    );
    assert_eq!(a["clean"], false);
    assert!(
        a["payload_slots"][slot].is_null(),
        "the damaged slot must arrive as null, which is what becomes the erasure"
    );

    // Hand the slots back exactly as they came, with no reshaping — the whole
    // point of the two shapes matching.
    let slots_json = serde_json::to_string(&a["payload_slots"]).unwrap();
    let d = json(&canonical_decode_slots_fixed(
        &slots_json,
        LANG,
        WL,
        payload.len(),
    ));

    assert_eq!(
        d["payload_hex"].as_str().unwrap(),
        glossia::codec::hex_encode(&payload),
        "a located hole must be repairable"
    );
    assert_eq!(d["repaired"], serde_json::json!([slot]));
}

#[test]
fn telling_the_decoder_where_the_damage_is_doubles_what_it_repairs() {
    // `2·errors + erasures ≤ parity`. Three swaps is past the unlocated bound
    // of two, so the plain entry must refuse; naming the same three positions
    // brings them inside the located bound of four.
    let payload = hash160();
    let text = canonical_encode_fixed(&payload, LANG, WL).unwrap();
    let slots = glossia::codec::payload_tokens(&text, &is_payload);

    let tree = cached_payload_tree(LANG, WL).unwrap();
    let spare: Vec<String> = tree
        .words()
        .iter()
        .map(|w| w.to_lowercase())
        .filter(|w| !slots.contains(w))
        .take(3)
        .collect();
    assert_eq!(spare.len(), 3, "need three words the rendering did not use");

    let mut damaged = text.clone();
    let mut hit = Vec::new();
    for (k, w) in slots.iter().enumerate() {
        if hit.len() == 3 {
            break;
        }
        let delimited = format!(" {w} ");
        if slots.iter().filter(|x| *x == w).count() == 1 && damaged.contains(&delimited) {
            damaged = damaged.replacen(&delimited, &format!(" {} ", spare[hit.len()]), 1);
            hit.push(k);
        }
    }
    assert_eq!(hit.len(), 3, "the test needs three lone payload words");

    let unlocated = json(&canonical_decode_fixed(&damaged, LANG, WL, payload.len()));
    assert!(
        unlocated.get("error").is_some(),
        "three unlocated faults are past 2e ≤ 4 and must fail loudly, not guess"
    );

    let erasures = serde_json::to_string(&hit).unwrap();
    let located = json(&canonical_decode_fixed_repaired(
        &damaged,
        LANG,
        WL,
        payload.len(),
        &erasures,
    ));
    assert_eq!(
        located["payload_hex"].as_str().unwrap(),
        glossia::codec::hex_encode(&payload),
        "the same damage, located, is inside the budget"
    );
    assert_eq!(located["repaired"], serde_json::json!(hit));
}

#[test]
fn damage_past_the_bound_returns_an_error_rather_than_a_plausible_payload() {
    // The property the whole format leans on: a decoder that hands back its
    // best guess is how a burst beyond the bound becomes a valid-looking wrong
    // answer. Every payload word replaced is far past any budget.
    let payload = hash160();
    let text = canonical_encode_fixed(&payload, LANG, WL).unwrap();
    let slots = glossia::codec::payload_tokens(&text, &is_payload);
    let filler = cached_payload_tree(LANG, WL).unwrap().words()[0].to_lowercase();

    // Every slot present but wrong: no erasures, maximum errors.
    let wrecked: Vec<Option<String>> = slots.iter().map(|_| Some(filler.clone())).collect();
    let d = json(&canonical_decode_slots_fixed(
        &serde_json::to_string(&wrecked).unwrap(),
        LANG,
        WL,
        payload.len(),
    ));

    assert!(
        d.get("error").is_some(),
        "damage past the bound must be reported, never silently 'corrected'"
    );
    assert!(d.get("payload_hex").is_none());
}

#[test]
fn malformed_json_is_an_error_not_a_panic() {
    // These arguments cross from JavaScript, where nothing checks their shape.
    // A panic in wasm aborts the module for the whole page, so the parse has to
    // fail into the same `{ error }` every other export returns.
    let bad_slots = json(&canonical_decode_slots_fixed("not json at all", LANG, WL, 20));
    assert_eq!(bad_slots["kind"], "bad_slots");
    assert!(bad_slots["error"].as_str().unwrap().contains("slots"));

    // Right JSON type, wrong element type — an array of numbers where words or
    // nulls belong.
    let wrong_element = json(&canonical_decode_slots_fixed("[1, 2, 3]", LANG, WL, 20));
    assert_eq!(wrong_element["kind"], "bad_slots");

    let bad_erasures = json(&canonical_decode_fixed_repaired(
        "some prose", LANG, WL, 20, "{}",
    ));
    assert_eq!(bad_erasures["kind"], "bad_erasures");
}
