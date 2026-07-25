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

/// The opcode sigils used by the prose address format.
const SIGILS: [char; 8] = [
    '\u{29C9}', // ⧉  OP_DUP
    '\u{2317}', // ⌗  OP_HASH160
    '\u{225F}', // ≟  OP_EQUALVERIFY
    '\u{2713}', // ✓  OP_CHECKSIG
    '\u{2A75}', // ⩵  OP_EQUAL
    '\u{25BD}', // ▽  witness v0
    '\u{25B3}', // △  witness v1
    '\u{03B2}', // β  difficulty mark (letter-like: safe only because declared)
];

#[test]
fn address_sigils_collide_with_no_shipped_payload_wordlist() {
    let collisions = markup_collisions(SIGILS);
    assert!(
        collisions.is_empty(),
        "prose-address sigils collide with payload wordlists: {collisions:?}"
    );
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
