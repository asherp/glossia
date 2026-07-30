//! Guard the prose-address sigil set against every shipped payload wordlist (#76).
//!
//! A format that decorates prose with symbols must not use a character any payload
//! word uses. Wordlists are append-only, so a sigil that is safe today can be
//! invalidated by a later addition — checking against every embedded wordlist turns
//! that into a test failure rather than a mis-decoded address.

use glossia::codec::{payload_tokens_with_markup, Markup};
use glossia::generator::data::{
    all_payload_alphabets, load_payload_words_for_wordlist, markup_collisions, payload_alphabet,
};

/// The opcode glyphs the prose address format uses, taken from the Book of
/// Bitcoin notation (`btc-prose.js` OPCODE_SYMBOLS) rather than invented here.
///
/// Two of them are the reason `Markup` exists. U+24EA and U+2460 are Unicode
/// category `No` — alphanumeric — so `normalize_token` does NOT strip them, and
/// flush against a payload word they would hide it. They are safe only because
/// the format declares them.
const SIGILS: [char; 7] = [
    '\u{29C9}', // ⧉ OP_DUP
    '\u{2316}', // ⌖ OP_HASH160
    '\u{2261}', // ≡ OP_EQUALVERIFY
    '\u{2207}', // ∇ OP_CHECKSIG
    '=',         // OP_EQUAL
    '\u{24EA}', // ⓪ OP_0
    '\u{2460}', // ① OP_1
];

#[test]
fn sigils_are_valid_markup_for_the_wordlist_the_format_uses() {
    // The address format encodes against english/bip39, whose alphabet is a-z.
    // Every glyph must be outside it, or decoding would strip payload content.
    let wl = load_payload_words_for_wordlist("english", "bip39").expect("wordlist");
    Markup::new(SIGILS, &wl).expect("BoB notation must validate against bip39");
}

#[test]
fn the_notation_is_not_portable_to_every_wordlist() {
    // Not a defect — a boundary worth pinning. OP_EQUAL is '=', which is a PAYLOAD
    // character in cs/base64 and cs/ascii7 (the latter uses all 95 printable
    // ASCII). So this notation is safe for the wordlist this format uses, and is
    // NOT automatically safe elsewhere. Reusing it against a CS wordlist needs a
    // different glyph for OP_EQUAL.
    let collisions = markup_collisions(SIGILS);
    let names: Vec<&str> = collisions.keys().map(|k| k.as_str()).collect();
    assert!(
        names.iter().any(|n| n.starts_with("cs/")),
        "expected the ASCII '=' glyph to collide with a cs wordlist, got {names:?}"
    );
    for (name, chars) in &collisions {
        assert_eq!(
            chars,
            &vec!['='],
            "{name}: only OP_EQUAL's '=' should collide; a new clash means the \
             notation or a wordlist changed"
        );
    }
}

#[test]
fn ascii_punctuation_is_not_safe_as_markup() {
    // cs/ascii7 uses all 95 printable ASCII characters and cs/base64 includes '=',
    // so "non-alphanumeric" is not a sufficient safety condition — the reason
    // Markup validates against the wordlist instead of against Unicode category.
    for c in ['=', '-', '.', '_', '/', '+'] {
        assert!(
            !markup_collisions([c]).is_empty(),
            "{c:?} should be reported as colliding with some payload wordlist"
        );
    }
}

#[test]
fn every_shipped_wordlist_reports_an_alphabet() {
    let alphabets = all_payload_alphabets();
    assert!(alphabets.len() >= 10, "expected many wordlists, got {}", alphabets.len());
    for (name, alphabet) in &alphabets {
        assert!(!alphabet.is_empty(), "{name} has an empty alphabet");
    }
}

#[test]
fn bip39_alphabet_is_plain_lowercase_latin() {
    let alphabet = payload_alphabet("english", "bip39").expect("bip39 alphabet");
    let expected: std::collections::BTreeSet<char> = ('a'..='z').collect();
    assert_eq!(alphabet, expected);
}

#[test]
fn declared_sigils_survive_a_real_encode_flush_against_a_payload_word() {
    let wl = load_payload_words_for_wordlist("english", "bip39").expect("wordlist");
    let markup = Markup::new(SIGILS, &wl).expect("sigils validate against bip39");
    let set: std::collections::HashSet<String> = wl.iter().map(|w| w.to_lowercase()).collect();

    // Every sigil placed directly against a payload word, no separating space.
    for sigil in SIGILS {
        let text = format!("{sigil}absorb {sigil}victory");
        let got = payload_tokens_with_markup(&text, &markup, |w| set.contains(w));
        assert_eq!(got, vec!["absorb", "victory"], "sigil {sigil:?} hid a payload word");
    }
}
