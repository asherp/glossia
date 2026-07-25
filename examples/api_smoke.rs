//! Smoke test for the composed encode API (glossia#76).
//!
//! Demonstrates the two stages composing: pack bits into payload words yourself,
//! then hand those words to the generator for cover insertion. The format header
//! rides in the bit-packing slack, so it costs no words.
//!
//! Run: cargo run --release --example api_smoke

use glossia::codec::{checksum_seed, crc32, hex_decode, payload_tokens, payload_tokens_with_markup, Markup};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language;
use std::collections::HashSet;

/// Pack program bytes followed by a header into fixed-width word indices,
/// filling the slack exactly.
fn pack(program: &[u8], bits_per_word: usize, header: u32, header_bits: usize) -> Vec<usize> {
    let data_bits = program.len() * 8;
    let n_words = (data_bits + header_bits) / bits_per_word;
    assert_eq!(n_words * bits_per_word, data_bits + header_bits, "must fill exactly");

    let bit = |i: usize| -> usize {
        if i < data_bits {
            ((program[i / 8] >> (7 - (i % 8))) & 1) as usize
        } else {
            let h = i - data_bits;
            ((header >> (header_bits - 1 - h)) & 1) as usize
        }
    };
    (0..n_words)
        .map(|w| (0..bits_per_word).fold(0, |acc, b| (acc << 1) | bit(w * bits_per_word + b)))
        .collect()
}

fn unpack(
    indices: &[usize],
    bits_per_word: usize,
    n_bytes: usize,
    header_bits: usize,
) -> (Vec<u8>, u32) {
    let mut bits = Vec::with_capacity(indices.len() * bits_per_word);
    for &i in indices {
        for b in (0..bits_per_word).rev() {
            bits.push((i >> b) & 1);
        }
    }
    let program = (0..n_bytes)
        .map(|i| (0..8).fold(0u8, |acc, b| (acc << 1) | bits[i * 8 + b] as u8))
        .collect();
    let header = (0..header_bits).fold(0u32, |acc, b| (acc << 1) | bits[n_bytes * 8 + b] as u32);
    (program, header)
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let bits_per_word = wl.len().trailing_zeros() as usize; // 2048 -> 11
    let log2_wl = bits_per_word as u32;
    let version = 1u32;
    println!("wordlist {} words, {bits_per_word} bits/word (log2 = {log2_wl})\n", wl.len());

    // P2WPKH witness program (BIP173 vector): 20 bytes.
    let program = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6").unwrap();
    let header_bits = 15 * bits_per_word - program.len() * 8; // 165 - 160 = 5
    let header = (log2_wl << (header_bits - 4)) | (version & ((1 << (header_bits - 4)) - 1));

    let indices = pack(&program, bits_per_word, header, header_bits);
    let words: Vec<String> = indices.iter().map(|&i| wl[i].clone()).collect();

    // Checksum covers program + header, i.e. exactly the encoded bits.
    let mut checked = program.clone();
    checked.extend_from_slice(&[log2_wl as u8, version as u8]);
    let seed = checksum_seed(&checked, 0);

    let (text, counter) =
        encode_words_into_language(&words, "english", "default", "body", seed, 4).unwrap();
    let artifact = format!("\u{25BD} {text}");

    println!("crc32   {:08x}", crc32(&checked));
    println!("seed    {seed:#018x}   winning counter {counter}");
    println!(
        "header  {header_bits} bits, free (160 + {header_bits} = {} = {} words)",
        160 + header_bits,
        (160 + header_bits) / bits_per_word
    );
    println!(
        "size    {} payload / {} total words\n",
        words.len(),
        text.split_whitespace().count()
    );
    println!("{artifact}\n");

    // Declare the format's opcode sigils, validated against the payload alphabet.
    // Declaring them means decoding strips them by name rather than guessing from
    // Unicode category -- so even a letter-like sigil is safe, and one that would
    // collide with payload text is rejected up front.
    let sigils = ['\u{29C9}', '\u{2317}', '\u{225F}', '\u{2713}', '\u{29B5}',
                  '\u{25BD}', '\u{25B3}', '\u{03B2}'];
    let markup = Markup::new(sigils, &wl).expect("sigils must not collide with payload alphabet");
    match Markup::new(['e'], &wl) {
        Err(bad) => println!("rejected as markup (payload letters): {bad:?}"),
        Ok(_) => println!("WARNING: 'e' wrongly accepted as markup"),
    }
    // Adjacent, no space -- the case plain normalization loses.
    let flush = format!("\u{03B2}{}", artifact.trim_start_matches("\u{25BD} "));
    let set: HashSet<String> = wl.iter().map(|w| w.to_lowercase()).collect();
    println!(
        "flush sigil: plain harvest {} words, declared-markup harvest {} words",
        payload_tokens(&flush, |w| set.contains(w)).len(),
        payload_tokens_with_markup(&flush, &markup, |w| set.contains(w)).len()
    );

    let harvested = payload_tokens_with_markup(&artifact, &markup, |w| set.contains(w));
    let idx: Vec<usize> = harvested
        .iter()
        .map(|w| wl.iter().position(|x| x.to_lowercase() == *w).unwrap())
        .collect();
    let (prog2, hdr2) = unpack(&idx, bits_per_word, program.len(), header_bits);

    println!("harvested {} words, order preserved: {}", harvested.len(), harvested == words);
    println!("program round-trip: {}", if prog2 == program { "exact" } else { "MISMATCH" });
    println!(
        "header  log2={} version={}",
        hdr2 >> (header_bits - 4),
        hdr2 & ((1 << (header_bits - 4)) - 1)
    );

    // Verification by re-render: the winning counter reproduces the rendering.
    let (again, _) = encode_words_into_language(
        &words, "english", "default", "body", seed.wrapping_add(counter), 1,
    )
    .unwrap();
    println!("re-render from seed+counter matches: {}", again == text);
}
