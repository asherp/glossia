//! Bits per syllable: what a payload costs to *say*.
//!
//! Density is normally quoted per word — payload words over total words. That is
//! the right unit for the generator, which spends its budget in slots, but it is
//! the wrong unit for a human. Speech runs at a roughly constant syllable rate
//! (~4–6 syllables/second in English), and a transcriber's error rate scales with
//! syllables too. So the honest question for a *speakable* encoding is not how
//! many words it costs but how many syllables — and there, an 11-bit word that
//! happens to be "act" is three times the bargain that "ability" is.
//!
//! Measured here:
//!   1. calibration — the vowel-group heuristic against CMUdict, so the
//!      heuristic-only languages below carry a known error bar
//!   2. English dialects — prose and every verse meter, real CMUdict counts
//!   2b. the wordlist frontier — bits/syllable against wordlist size
//!   3. cross-language body prose — english / czech / german / latin
//!   4. spoken character encodings — hex, base32, base58, base64, bech32,
//!      decimal, computed from letter names rather than measured
//!
//! Run: cargo run --release --example bits_per_syllable [samples] [best_of]
//!
//! Release matters twice over: debug is ~40x slower for sequence enumeration,
//! and debug builds embed English only, so czech/german/latin are missing.

use glossia::generator::data::{load_payload_words_for_wordlist, load_prosody_cached};
use glossia::generator::prosody::{scans_text, ProsodyModel};
use glossia::grammar::DialectConfig;
use glossia::pipeline::encode_words_into_language;
use glossia::{canonical_encode, CANONICAL_VERSION};

// ─── syllable counting ───────────────────────────────────────────────────────

/// Which vowel letters, and which letter pairs fuse into one nucleus.
///
/// A syllable is one vowel nucleus, so counting them is counting maximal vowel
/// runs — with the language's digraphs and diphthongs collapsed, since those are
/// spelled as two letters but heard as one. Everything language-specific lives
/// here rather than in the counter.
struct Phonotactics {
    vowels: &'static str,
    /// Two-letter sequences that are one nucleus, not two.
    digraphs: &'static [&'static str],
    /// A final `e` is written but not heard (English `stone`, `able`).
    silent_final_e: bool,
    /// `r`/`l` may be a nucleus on their own between consonants (Czech `vlk`).
    syllabic_liquids: bool,
}

fn phonotactics(language: &str) -> Phonotactics {
    match language {
        // Long vowels are written with acute accents and are one nucleus each;
        // `ou/au/eu` are the three diphthongs. `vlk`, `krk`, `smrt` have no
        // vowel at all — the liquid carries the syllable.
        "czech" => Phonotactics {
            vowels: "aáeéěiíoóuúůyý",
            digraphs: &["ou", "au", "eu"],
            silent_final_e: false,
            syllabic_liquids: true,
        },
        // Final `e` is pronounced (`Ende`, `Reise`), which is most of why the
        // English rule cannot simply be reused.
        "german" => Phonotactics {
            vowels: "aeiouäöüy",
            digraphs: &["ie", "ei", "eu", "äu", "au", "aa", "ee", "oo", "ai", "oi"],
            silent_final_e: false,
            syllabic_liquids: false,
        },
        // Classical Latin's diphthongs. `i`/`u` also act as glides between
        // vowels, which this does not model — the calibration below is the
        // honest statement of what that costs.
        "latin" => Phonotactics {
            vowels: "aeiouy",
            digraphs: &["ae", "oe", "au", "eu", "ei", "ui"],
            silent_final_e: false,
            syllabic_liquids: false,
        },
        _ => Phonotactics {
            vowels: "aeiouy",
            digraphs: &[],
            silent_final_e: true,
            syllabic_liquids: false,
        },
    }
}

/// Syllable count by vowel-group counting. Never returns zero for a word with
/// letters in it — a word you can say has at least one syllable.
fn heuristic_syllables(word: &str, p: &Phonotactics) -> usize {
    let w: Vec<char> = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    if w.is_empty() {
        return 0;
    }
    let is_vowel = |c: char| p.vowels.contains(c);

    let mut n = 0usize;
    let mut i = 0usize;
    while i < w.len() {
        if !is_vowel(w[i]) {
            i += 1;
            continue;
        }
        n += 1;
        // Consume the rest of this nucleus: a declared digraph takes both
        // letters, and a run of like vowels (`ii`) is one nucleus anyway.
        let start = i;
        i += 1;
        while i < w.len() && is_vowel(w[i]) {
            let pair: String = [w[i - 1], w[i]].iter().collect();
            let fused = p.digraphs.contains(&pair.as_str()) || w[i] == w[i - 1] || i > start + 1;
            if !fused && !p.digraphs.is_empty() {
                break; // two adjacent vowels that are not a diphthong = hiatus
            }
            i += 1;
        }
    }

    if p.silent_final_e && n > 1 && w[w.len() - 1] == 'e' {
        // `able`, `little`: the `e` is silent but `le` still carries a syllable.
        let l_before = w.len() >= 2 && matches!(w[w.len() - 2], 'l' | 'r');
        let cluster = w.len() >= 3 && !is_vowel(w[w.len() - 3]);
        if !(l_before && cluster) {
            n -= 1;
        }
    }

    if p.syllabic_liquids && n == 0 {
        n = w.iter().filter(|c| matches!(c, 'r' | 'l')).count();
    }
    n.max(1)
}

/// Trailing punctuation is rendering, not word — same rule the prosody model uses.
fn bare(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
        .to_lowercase()
}

/// Syllables for one rendered token, preferring measured data over the heuristic.
///
/// Returns `(syllables, was_measured)` so a run can report how much of its
/// answer rests on CMUdict and how much on spelling.
fn syllables(token: &str, model: Option<&ProsodyModel>, p: &Phonotactics) -> (usize, bool) {
    let w = bare(token);
    if w.is_empty() {
        return (0, true); // a bare "." costs nothing to say
    }
    match model.and_then(|m| m.syllables(&w)) {
        Some(n) => (n, true),
        None => (heuristic_syllables(&w, p), false),
    }
}

// ─── payload generation ──────────────────────────────────────────────────────

/// xorshift over the wordlist — the same throwaway payload source the other
/// prosody rigs use, so their numbers and these line up.
fn payload(n: usize, nonce: u64, wl: &[String]) -> Vec<String> {
    let mut x = nonce.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            wl[(x as usize) % wl.len()].clone()
        })
        .collect()
}

/// What one measured configuration comes back with.
struct Run {
    payload_words: usize,
    total_words: usize,
    syllables: usize,
    guessed: usize,
    scanned: usize,
    samples: usize,
}

impl Run {
    fn bits_per_syllable(&self, bits_per_word: usize) -> f64 {
        (self.payload_words * bits_per_word) as f64 / self.syllables as f64
    }
    fn density(&self) -> f64 {
        self.payload_words as f64 / self.total_words as f64
    }
    fn syl_per_word(&self) -> f64 {
        self.syllables as f64 / self.total_words as f64
    }
}

#[allow(clippy::too_many_arguments)]
fn measure(
    language: &str,
    wordlist: &str,
    dialect: &str,
    words_per_payload: usize,
    samples: usize,
    best_of: usize,
    wl: &[String],
    model: Option<&ProsodyModel>,
) -> Run {
    let p = phonotactics(language);
    let spec = DialectConfig::from_language_dialect_cached(language, dialect)
        .ok()
        .and_then(|d| d.meter().cloned());
    let mut run = Run {
        payload_words: 0,
        total_words: 0,
        syllables: 0,
        guessed: 0,
        scanned: 0,
        samples,
    };
    for i in 0..samples {
        let pw = payload(words_per_payload, i as u64, wl);
        let (text, _) = encode_words_into_language(
            &pw,
            language,
            wordlist,
            dialect,
            (i as u64).wrapping_mul(7919),
            best_of,
        )
        .unwrap_or_else(|e| panic!("{language}/{dialect} encode: {e}"));
        let toks: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
        for t in &toks {
            let (n, measured) = syllables(t, model, &p);
            run.syllables += n;
            if !measured {
                run.guessed += 1;
            }
        }
        run.payload_words += words_per_payload;
        run.total_words += toks.len();
        if let (Some(spec), Some(m)) = (spec.as_ref(), model) {
            if scans_text(&toks, m, spec) {
                run.scanned += 1;
            }
        }
    }
    run
}

/// The ceiling for a wordlist: the payload words alone, no cover, no grammar.
/// This is what a bare mnemonic costs to read out.
fn bare_wordlist(language: &str, wl: &[String], model: Option<&ProsodyModel>) -> (f64, f64) {
    let p = phonotactics(language);
    let (mut syl, mut guessed) = (0usize, 0usize);
    for w in wl {
        let (n, measured) = syllables(w, model, &p);
        syl += n;
        if !measured {
            guessed += 1;
        }
    }
    (
        syl as f64 / wl.len() as f64,
        guessed as f64 / wl.len() as f64,
    )
}

// ─── spoken character encodings ──────────────────────────────────────────────

/// Syllables in the English name of a character, said one character at a time.
///
/// `w` is the only three-syllable letter name and `zero`/`seven` the only
/// two-syllable digits, which is the whole reason a base-64 alphabet does not
/// cost a flat one syllable per symbol.
fn letter_name_syllables(c: char) -> usize {
    match c {
        'w' | 'W' => 3,
        'a'..='z' => 1,
        'A'..='Z' => 1,
        '0' => 2,
        '7' => 2,
        '1'..='9' => 1,
        '+' | '/' | '-' => 1, // plus, slash, dash
        '_' => 3,             // underscore
        '=' => 1,             // equals
        _ => 1,
    }
}

/// NATO/ICAO spelling alphabet, the protocol anyone actually uses when the
/// channel is noisy and the string matters.
fn nato_syllables(c: char) -> usize {
    const NATO: [usize; 26] = [
        2, 2, 2, 2, 2, 2, 1, 2, 3, 3, 2, 2, 1, 3, 2, 2, 2, 3, 3, 2, 3, 2, 2, 2, 2, 2,
    ];
    match c.to_ascii_lowercase() {
        'a'..='z' => NATO[(c.to_ascii_lowercase() as u8 - b'a') as usize],
        _ => letter_name_syllables(c),
    }
}

struct Alphabet {
    name: &'static str,
    chars: &'static str,
    /// Whether the alphabet is case-sensitive, i.e. whether saying it aloud
    /// needs a case marker on top of the character name.
    mixed_case: bool,
}

/// One syllable for "cap" — the shortest case marker anyone would accept. A
/// spoken "capital" would be three, so this is the friendly reading.
const CASE_MARKER: f64 = 1.0;

fn alphabet_cost(a: &Alphabet, name: fn(char) -> usize) -> (f64, f64) {
    let n = a.chars.chars().count();
    let bits = (n as f64).log2();
    let total: f64 = a
        .chars
        .chars()
        .map(|c| {
            let mut s = name(c) as f64;
            if a.mixed_case && c.is_ascii_uppercase() {
                s += CASE_MARKER;
            }
            s
        })
        .sum();
    let syl = total / n as f64;
    (bits, bits / syl)
}

// ─── report ──────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let samples: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(50);
    let best_of: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);

    let en_model = load_prosody_cached("english");
    let en_wl = load_payload_words_for_wordlist("english", "bip39").unwrap();

    // ── 1. how far can we trust the heuristic ──────────────────────────────
    //
    // English is the one language with both a measured count and a guessed one,
    // so it is the only place the guess can be scored. Whatever error it shows
    // here is the error bar on the latin/czech/german rows below.
    let p_en = phonotactics("english");
    let model = en_model.as_ref().expect("english ships prosody.yaml");
    let (mut exact, mut off_by_one, mut n, mut abs_err) = (0usize, 0usize, 0usize, 0i64);
    for w in &en_wl {
        if let Some(truth) = model.syllables(w) {
            let guess = heuristic_syllables(w, &p_en);
            n += 1;
            abs_err += (guess as i64 - truth as i64).abs();
            if guess == truth {
                exact += 1;
            } else if (guess as i64 - truth as i64).abs() == 1 {
                off_by_one += 1;
            }
        }
    }
    println!("\n── vowel-group heuristic vs CMUdict ({n} english payload words)");
    println!(
        "   exact {:.1}%   within 1 {:.1}%   mean abs error {:.3} syllables",
        100.0 * exact as f64 / n as f64,
        100.0 * (exact + off_by_one) as f64 / n as f64,
        abs_err as f64 / n as f64,
    );
    println!("   (english rows below use CMUdict; czech, german and latin have no");
    println!("    prosody data and rest entirely on the heuristic — read them ±that)");

    // ── 2. English dialects ────────────────────────────────────────────────
    const EN_BITS: usize = 11;
    let (bare_syl, _) = bare_wordlist("english", &en_wl, en_model.as_deref());
    println!("\n── english, 13-word payloads x {samples} samples, best_of={best_of}");
    println!(
        "   {:>9} {:>7} {:>8} {:>8} {:>9} {:>8} {:>7}",
        "dialect", "words", "density", "syl/word", "syllables", "bits/syl", "scans"
    );
    println!(
        "   {:>9} {:>7.1} {:>8.3} {:>8.3} {:>9.1} {:>8.2} {:>7}",
        "(none)",
        13.0,
        1.0,
        bare_syl,
        13.0 * bare_syl,
        EN_BITS as f64 / bare_syl,
        "—",
    );
    let mut en_body_bps = 0.0;
    for dialect in ["body", "syllabic", "haiku", "iambic", "anapest", "dactyl"] {
        let r = measure(
            "english",
            "bip39",
            dialect,
            13,
            samples,
            best_of,
            &en_wl,
            en_model.as_deref(),
        );
        let bps = r.bits_per_syllable(EN_BITS);
        if dialect == "body" {
            en_body_bps = bps;
        }
        let scans = if dialect == "body" {
            "—".to_string()
        } else {
            format!("{:.0}%", 100.0 * r.scanned as f64 / r.samples as f64)
        };
        println!(
            "   {:>9} {:>7.1} {:>8.3} {:>8.3} {:>9.1} {:>8.2} {:>7}",
            dialect,
            r.total_words as f64 / r.samples as f64,
            r.density(),
            r.syl_per_word(),
            r.syllables as f64 / r.samples as f64,
            bps,
            scans,
        );
    }

    // ── 2b. the wordlist frontier ──────────────────────────────────────────
    //
    // A wordlist of 2^m words carries m bits per word, so bits/word is free to
    // grow — but the words you add to reach 2^m are, on average, longer than the
    // ones already there. Per *syllable* the two effects fight, and the fight
    // has a winner: take the k shortest words of a list and ask what m/mean(k)
    // comes to. Nothing here is a proposal to change BIP39, which is fixed by a
    // standard outside this project and append-only inside it. It is the number
    // to look at when sizing a *new* payload wordlist for a speech-first dialect.
    println!("\n── wordlist frontier: the 2^m shortest words of each list");
    println!(
        "   {:>8} {:>4} {:>7} {:>9} {:>9}",
        "language", "m", "words", "syl/word", "bits/syl"
    );
    for (language, wordlist) in [("english", "bip39"), ("latin", "default")] {
        let Ok(wl) = load_payload_words_for_wordlist(language, wordlist) else {
            continue;
        };
        let model = if language == "english" { en_model.as_deref() } else { None };
        let p = phonotactics(language);
        let mut by_len: Vec<usize> = wl.iter().map(|w| syllables(w, model, &p).0).collect();
        by_len.sort_unstable();
        let full = wl.len().trailing_zeros() as usize;
        let mut best = (0usize, 0.0f64);
        for m in 8..=full {
            let k = 1usize << m;
            if k > by_len.len() {
                break;
            }
            let mean = by_len[..k].iter().sum::<usize>() as f64 / k as f64;
            let bps = m as f64 / mean;
            if bps > best.1 {
                best = (m, bps);
            }
            println!("   {language:>8} {m:>4} {k:>7} {mean:>9.3} {bps:>9.2}");
        }
        println!(
            "   {:>8} {:>4} {:>7} {:>9} {:>9}   <- peak at 2^{}",
            "", "", "", "", "", best.0
        );
    }

    // ── 3. cross-language ──────────────────────────────────────────────────
    //
    // bits/word is set by the wordlist size (every shipped payload list is a
    // power of two), so latin's 32768 words carry 15 bits each against
    // english/czech/german's 11. Whether that survives contact with longer
    // words is exactly the bits/syllable question.
    let mut prose_bps: Vec<(&str, f64)> = Vec::new();
    println!("\n── body prose across languages, {samples} samples, best_of={best_of}");
    println!(
        "   {:>8} {:>6} {:>8} {:>8} {:>9} {:>9} {:>7}",
        "language", "b/word", "density", "syl/word", "bare b/syl", "prose b/syl", "guess"
    );
    for (language, wordlist) in [
        ("english", "bip39"),
        ("czech", "default"),
        ("german", "default"),
        ("latin", "default"),
    ] {
        let wl = match load_payload_words_for_wordlist(language, wordlist) {
            Ok(w) => w,
            Err(e) => {
                println!("   {language:>8}  unavailable ({e}) — release build?");
                continue;
            }
        };
        let model = if language == "english" {
            en_model.as_deref()
        } else {
            None
        };
        let bits = wl.len().trailing_zeros() as usize;
        let (bare_syl, bare_guess) = bare_wordlist(language, &wl, model);
        let r = measure(
            language, wordlist, "body", 13, samples, best_of, &wl, model,
        );
        prose_bps.push((language, r.bits_per_syllable(bits)));
        println!(
            "   {:>8} {:>6} {:>8.3} {:>8.3} {:>9.2} {:>9.2} {:>6.0}%",
            language,
            bits,
            r.density(),
            r.syl_per_word(),
            bits as f64 / bare_syl,
            r.bits_per_syllable(bits),
            100.0 * bare_guess.max(r.guessed as f64 / r.total_words as f64),
        );
    }

    // ── 4. spoken character encodings ──────────────────────────────────────
    //
    // Not measured — computed. A character encoding's spoken cost is fully
    // determined by its alphabet and the naming protocol, with no grammar in
    // between to sample.
    const ALPHABETS: &[Alphabet] = &[
        Alphabet { name: "decimal", chars: "0123456789", mixed_case: false },
        Alphabet { name: "hex", chars: "0123456789abcdef", mixed_case: false },
        Alphabet { name: "bech32", chars: "qpzry9x8gf2tvdw0s3jn54khce6mua7l", mixed_case: false },
        Alphabet {
            name: "base32",
            chars: "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
            mixed_case: false,
        },
        Alphabet {
            name: "base58",
            chars: "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz",
            mixed_case: true,
        },
        Alphabet {
            name: "base64",
            chars: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
            mixed_case: true,
        },
    ];
    println!("\n── character encodings, said one character at a time");
    println!(
        "   {:>8} {:>7} {:>9} {:>9} {:>9} {:>9}",
        "encoding", "b/char", "syl/char", "bits/syl", "nato syl", "nato b/syl"
    );
    for a in ALPHABETS {
        let (bits, bps) = alphabet_cost(a, letter_name_syllables);
        let (_, nato_bps) = alphabet_cost(a, nato_syllables);
        let syl = bits / bps;
        let nato_syl = bits / nato_bps;
        println!(
            "   {:>8} {:>7.3} {:>9.3} {:>9.2} {:>9.3} {:>9.2}",
            a.name, bits, syl, bps, nato_syl, nato_bps
        );
    }

    // ── 5. what an actual artifact costs ───────────────────────────────────
    //
    // Sections 2-4 measure the encoding. This measures the *product*: the
    // canonical envelope charges a version byte and a crc32, and v3 charges
    // Reed-Solomon parity on top, so the payload bits a user cares about are
    // fewer than the words carry. Bits here are the payload's own, which is
    // why these sit below the section-2 numbers and should.
    println!(
        "\n── canonical v{CANONICAL_VERSION} artifacts (envelope + crc32 + RS parity, all in)"
    );
    println!(
        "   {:>8} {:>7} {:>7} {:>9} {:>9} {:>9}",
        "language", "bytes", "words", "syllables", "bits/syl", "vs prose"
    );
    for (language, wordlist) in [
        ("english", "bip39"),
        ("czech", "default"),
        ("latin", "default"),
    ] {
        let model = if language == "english" {
            en_model.as_deref()
        } else {
            None
        };
        let p = phonotactics(language);
        for bytes in [16usize, 20, 32] {
            // Canonical encoding is deterministic per payload, but the cover
            // seed is a checksum of the encoded bytes, so word count still
            // varies payload to payload. Average over a batch rather than
            // reporting whichever draw one arbitrary payload happened to get.
            let batch = samples.min(20).max(1);
            let (mut syl, mut words, mut ok) = (0usize, 0usize, 0usize);
            for k in 0..batch {
                let data: Vec<u8> = (0..bytes)
                    .map(|i| (i as u8).wrapping_mul(37).wrapping_add(11).wrapping_add(k as u8))
                    .collect();
                let text = match canonical_encode(&data, language, wordlist) {
                    Ok(t) => t,
                    Err(e) => {
                        println!("   {language:>8} {bytes:>7}  unavailable ({e})");
                        break;
                    }
                };
                for t in text.split_whitespace() {
                    syl += syllables(t, model, &p).0;
                    words += 1;
                }
                ok += 1;
            }
            if ok == 0 {
                continue;
            }
            let bps = (bytes * 8 * ok) as f64 / syl as f64;
            println!(
                "   {:>8} {:>7} {:>7.1} {:>9.1} {:>9.2} {:>8.0}%",
                language,
                bytes,
                words as f64 / ok as f64,
                syl as f64 / ok as f64,
                bps,
                100.0 * bps / prose_bps.iter().find(|(l, _)| *l == language).map_or(bps, |(_, v)| *v),
            );
        }
    }

    println!(
        "\n   english body prose: {en_body_bps:.2} bits/syllable — {:.0}% of hex's naive \
         {:.2}, {:.1}x its NATO {:.2}",
        100.0 * en_body_bps / alphabet_cost(&ALPHABETS[1], letter_name_syllables).1,
        alphabet_cost(&ALPHABETS[1], letter_name_syllables).1,
        en_body_bps / alphabet_cost(&ALPHABETS[1], nato_syllables).1,
        alphabet_cost(&ALPHABETS[1], nato_syllables).1,
    );
    println!();
}
