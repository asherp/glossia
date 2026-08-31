//! How many syllables does it take to say one Bitcoin address out loud?
//!
//! The companion to `bits_per_syllable`, which reports *rates*. This reports a
//! single concrete artifact, because a rate divided into a payload size is easy
//! to get wrong and a reader can check a count.
//!
//! The subject is the BIP173 P2WPKH test vector — one 20-byte witness program,
//! rendered every way anyone actually renders it:
//!
//!   bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4
//!
//! Character encodings are counted exactly, character by character, under two
//! naming protocols: plain letter names ("bee") and the NATO/ICAO spelling
//! alphabet ("bravo"). Glossia rows are measured by encoding the same 20 bytes
//! and counting the rendered text with CMUdict syllables.
//!
//! Run: cargo run --release --example address_syllables [samples]

use glossia::codec::encode_base_n;
use glossia::generator::data::load_prosody_cached;
use glossia::generator::prosody::ProsodyModel;
use glossia::pipeline::{cached_payload_tree, encode_words_into_language};
use glossia::{canonical_encode, CANONICAL_VERSION};

/// The BIP173 P2WPKH example: witness program, and the strings that encode it.
const PROGRAM: [u8; 20] = [
    0x75, 0x1e, 0x76, 0xe8, 0x19, 0x91, 0x96, 0xd4, 0x54, 0x94, 0x1c, 0x45, 0xd1, 0xb3, 0xa3, 0x23,
    0xf1, 0x43, 0x3b, 0xd6,
];
const BECH32: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
/// The same hash160 as a legacy P2PKH address — base58check, mixed case.
const BASE58: &str = "1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH";
const HEX: &str = "751e76e8199196d454941c45d1b3a323f1433bd6";
const BASE32: &str = "OUPHN2AZSGLNIVEUDRC5DM5DEPYUGO6W";
const BASE64: &str = "dR526BmRltRUlBxF0bOjI/FDO9Y=";

/// Syllables in the English name of one character, said plainly.
fn plain(c: char) -> usize {
    match c.to_ascii_lowercase() {
        'w' => 3,          // "double-u"
        'a'..='z' => 1,
        '0' | '7' => 2,    // "zero", "seven"
        '1'..='9' => 1,
        '+' | '/' | '-' => 1,
        '_' => 3,
        '=' => 1, // base64 padding: "equals"
        _ => 1,
    }
}

/// NATO/ICAO spelling alphabet. Digits keep their ordinary names — the ICAO
/// respellings ("niner", "tree") are a radio convention, not a general one, and
/// using them would only move the digit rows by a rounding error.
fn nato(c: char) -> usize {
    const N: [usize; 26] = [
        2, 2, 2, 2, 2, 2, 1, 2, 3, 3, 2, 2, 1, 3, 2, 2, 2, 3, 3, 2, 3, 2, 2, 2, 2, 2,
    ];
    match c.to_ascii_lowercase() {
        l @ 'a'..='z' => N[(l as u8 - b'a') as usize],
        _ => plain(c),
    }
}

/// Mean syllables per character over an alphabet, under one naming protocol.
///
/// A concrete address is one draw: this hex string is 60% digits, which are
/// cheap, so it undercounts a typical one. The expectation is what a *random*
/// program of the same size costs, and the two together bracket the answer.
fn mean_char(alphabet: &str, name: fn(char) -> usize, case_sensitive: bool) -> f64 {
    let n = alphabet.chars().count() as f64;
    alphabet
        .chars()
        .map(|c| (name(c) + usize::from(case_sensitive && c.is_ascii_uppercase())) as f64)
        .sum::<f64>()
        / n
}

/// Cost of saying a whole string, one character at a time.
///
/// A case-sensitive alphabet needs the case said too, or the listener cannot
/// write it down. One syllable ("cap") is the friendly reading; "capital" would
/// be three.
fn say(s: &str, name: fn(char) -> usize, case_sensitive: bool) -> usize {
    s.chars()
        .map(|c| name(c) + usize::from(case_sensitive && c.is_ascii_uppercase()))
        .sum()
}

fn word_syllables(text: &str, model: &ProsodyModel) -> usize {
    text.split_whitespace()
        .map(|t| {
            let w = t
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
                .to_lowercase();
            if w.is_empty() {
                0
            } else {
                model.syllables(&w).unwrap_or(1)
            }
        })
        .sum()
}

/// One row. `expected` is the mean over random payloads of the same size —
/// `None` for the word rows, which are measured over samples already.
fn row(form: &str, carries: &str, units: String, syl: usize, expected: Option<f64>, baseline: usize) {
    println!(
        "   {:<26} {:<22} {:>8} {:>6} {:>10} {:>7.2}x",
        form,
        carries,
        units,
        syl,
        expected.map_or_else(|| "—".to_string(), |e| format!("{e:.0}")),
        syl as f64 / baseline as f64,
    );
}

/// Alphabets, for the expectation. The fixed part of each string (a bech32 HRP,
/// a base58 version prefix, base64 padding) is not a free draw, so it is scored
/// exactly and only the variable characters get the mean.
const BECH32_ALPHABET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BASE58_ALPHABET: &str =
    "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BASE32_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const BASE64_ALPHABET: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const HEX_ALPHABET: &str = "0123456789abcdef";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let samples: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(50);

    let model = load_prosody_cached("english").expect("english ships prosody.yaml");
    let tree = cached_payload_tree("english", "bip39").expect("english bip39 wordlist");

    // The payload words the pipeline itself would pack these 20 bytes into.
    let words = encode_base_n(&PROGRAM, &tree, "bitpack").expect("pack 20 bytes");
    let bare_syl: usize = words.iter().map(|w| model.syllables(w).unwrap_or(1)).sum();

    // Body prose varies with the cover seed, so average rather than reporting
    // whichever draw seed 0 happened to give.
    let (mut prose_syl, mut prose_words) = (0usize, 0usize);
    let mut sample_prose = String::new();
    for i in 0..samples {
        let (text, _) = encode_words_into_language(
            &words, "english", "bip39", "body", (i as u64).wrapping_mul(7919), 4,
        )
        .expect("encode body");
        prose_syl += word_syllables(&text, &model);
        prose_words += text.split_whitespace().count();
        if i == 0 {
            sample_prose = text;
        }
    }
    let prose_syl_avg = (prose_syl as f64 / samples as f64).round() as usize;
    let prose_words_avg = (prose_words as f64 / samples as f64).round() as usize;

    let canonical = canonical_encode(&PROGRAM, "english", "bip39").expect("canonical encode");
    let canon_syl = word_syllables(&canonical, &model);
    let canon_words = canonical.split_whitespace().count();

    // Quoted against saying the bech32 address itself, since that is the thing
    // a person is actually holding.
    let base = say(BECH32, plain, false);

    println!("\n   address: {BECH32}");
    println!("   program: {} bytes / 160 bits\n", PROGRAM.len());
    println!(
        "   {:<26} {:<22} {:>8} {:>6} {:>10} {:>8}",
        "rendering", "carries", "units", "syl", "expected", "vs bech32"
    );
    println!("   {}", "─".repeat(86));

    for (label, name) in [
        ("plain letter names", plain as fn(char) -> usize),
        ("NATO spelling alphabet", nato as fn(char) -> usize),
    ] {
        println!("   ── said one character at a time, {label}");
        // "bc1q" is fixed (HRP, separator, witness version 0); 38 chars vary.
        row("bech32 address", "160b + 30b checksum", format!("{} ch", BECH32.len()),
            say(BECH32, name, false),
            Some(say("bc1q", name, false) as f64 + 38.0 * mean_char(BECH32_ALPHABET, name, false)),
            base);
        // Leading "1" is the version byte 0x00; 33 chars vary.
        row("base58check (legacy)", "160b + 32b checksum", format!("{} ch", BASE58.len()),
            say(BASE58, name, true),
            Some(1.0 + 33.0 * mean_char(BASE58_ALPHABET, name, true)),
            base);
        row("hex", "160b, no checksum", format!("{} ch", HEX.len()),
            say(HEX, name, false),
            Some(40.0 * mean_char(HEX_ALPHABET, name, false)),
            base);
        row("base32", "160b, no checksum", format!("{} ch", BASE32.len()),
            say(BASE32, name, false),
            Some(32.0 * mean_char(BASE32_ALPHABET, name, false)),
            base);
        // 27 data characters plus one "=" of padding.
        row("base64", "160b, no checksum", format!("{} ch", BASE64.len()),
            say(BASE64, name, true),
            Some(1.0 + 27.0 * mean_char(BASE64_ALPHABET, name, true)),
            base);
    }

    println!("   ── said as words (glossia rows averaged over {samples} cover seeds)");
    row("bip39 words (bare)", "160b, no checksum", format!("{} w", words.len()),
        bare_syl, None, base);
    row("glossia body prose", "160b, no checksum", format!("{prose_words_avg} w"),
        prose_syl_avg, None, base);
    row(
        &format!("glossia canonical v{CANONICAL_VERSION}"),
        "160b + crc32 + parity",
        format!("{canon_words} w"),
        canon_syl,
        None,
        base,
    );

    println!("\n   words: {}", words.join(" "));
    println!("\n   prose: {sample_prose}");
    println!("\n   canonical: {canonical}");
    println!();
}
