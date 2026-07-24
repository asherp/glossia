//! Prototype: Bitcoin locking scripts as self-verifying Glossia prose (glossia#76).
//!
//! Payload layout (proposed v1):
//!     [0]     version      profile id: grammar + semantics + RNG + cover mode
//!     [1]     script_type  1=P2PKH 2=P2SH 3=P2WPKH 4=P2WSH 5=P2TR
//!     [2..4]  wordlist_len u16 BE, payload wordlist size (allows extension)
//!     [4..]   program      hash160 / witness program (opcodes implied by type)
//!
//! The cover realization is seeded from a CRC-32 of the payload, so the choice of
//! prose *is* the checksum. Verification re-encodes the decoded payload and
//! compares: a correct payload reproduces the text, a wrong one renders differently.
//!
//! Run: cargo run --release --example prose_address_demo

use glossia::codec::{decode_base_n, hex_decode, hex_encode};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::merkle::WordlistTree;

const COUNTER_RANGE: u64 = 16;
/// Must match the grammar's declared codec (english/grammar.yaml: `codec: bitpack`).
const CODEC: &str = "bitpack";

/// CRC-32 (IEEE 802.3), computed bitwise — no dependency, and the polynomial is
/// the spec, not an implementation detail.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// splitmix64 finalizer — scatters consecutive counters across u64 so adjacent
/// counter values map to unrelated prose (mirrors `mix64` in glossia-msg.js).
fn mix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn seed_for(payload: &[u8], counter: u64) -> u64 {
    mix64((crc32(payload) as u64) << 32 | counter)
}

fn encode_at(hex: &str, seed: u64) -> Result<(String, Vec<String>), String> {
    glossia::pipeline::encode_into_language(
        hex, "english", "default", "body", None, seed, false, None, None, None, None,
    )
    .map(|(text, _cover, payload_words, _mode)| (text, payload_words))
    .map_err(|e| format!("{e:?}"))
}

/// Encode, sweeping the fluency counter and keeping the densest rendering.
/// Returns (prose, payload_words, winning_counter).
fn encode_checksum_seeded(payload: &[u8]) -> Result<(String, Vec<String>, u64), String> {
    let hex = hex_encode(payload);
    let mut best: Option<(f64, String, Vec<String>, u64)> = None;
    for c in 0..COUNTER_RANGE {
        let (text, words) = encode_at(&hex, seed_for(payload, c))?;
        let density = words.len() as f64 / text.split_whitespace().count().max(1) as f64;
        if best.as_ref().map_or(true, |(d, ..)| density > *d) {
            best = Some((density, text, words, c));
        }
    }
    let (_, text, words, c) = best.unwrap();
    Ok((text, words, c))
}

/// Harvest payload words from received text, exactly as the decoder does.
fn payload_tokens(text: &str, tree: &WordlistTree) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty() && tree.contains(w))
        .collect()
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Verified { counter: u64 },
    DecodedUnverified,
    Failed(String),
}

/// Decode + verify by re-render. Fixed length means the byte count is known,
/// so a dropped or spurious payload word shows up before any re-encode.
#[allow(unused_variables)]
fn decode_verified(text: &str, byte_count: usize, tree: &WordlistTree) -> (Option<Vec<u8>>, Verdict) {
    let words = payload_tokens(text, tree);
    // Fixed length => word count is a function of byte_count alone, so derive it
    // from the codec. English's grammar.yaml declares `codec: bitpack`, which adds
    // a padding word; the raw path (`encode_raw_base_n`) uses `bitpack_fixed` and
    // is one word shorter. Whichever the format picks, it must match end to end.
    let expected_words = glossia::codec::encode_base_n(&vec![0u8; byte_count], tree, CODEC)
        .map(|w| w.len())
        .unwrap_or(0);
    if words.len() != expected_words {
        return (
            None,
            Verdict::Failed(format!(
                "word count {} != expected {expected_words}",
                words.len()
            )),
        );
    }
    let bytes = match decode_base_n(&words, tree, CODEC) {
        Ok(b) => b,
        Err(e) => return (None, Verdict::Failed(format!("{e:?}"))),
    };
    // Re-render under every candidate counter; a match confirms the payload.
    for c in 0..COUNTER_RANGE {
        if let Ok((rendered, _)) = encode_at(&hex_encode(&bytes), seed_for(&bytes, c)) {
            if rendered.split_whitespace().eq(text.split_whitespace()) {
                return (Some(bytes), Verdict::Verified { counter: c });
            }
        }
    }
    (Some(bytes), Verdict::DecodedUnverified)
}

/// Fraction of token positions that agree between the received text and the
/// closest re-render. This is the alignment oracle's signal: damaged *cover*
/// leaves nearly every token in place, whereas a wrong *payload* changes the
/// checksum, hence the seed, hence essentially the whole rendering.
fn best_similarity(text: &str, tree: &WordlistTree) -> f64 {
    let words = payload_tokens(text, tree);
    let bytes = match decode_base_n(&words, tree, CODEC) {
        Ok(b) => b,
        Err(_) => return 0.0,
    };
    let recv: Vec<&str> = text.split_whitespace().collect();
    let mut best = 0.0f64;
    for c in 0..COUNTER_RANGE {
        if let Ok((rendered, _)) = encode_at(&hex_encode(&bytes), seed_for(&bytes, c)) {
            let got: Vec<&str> = rendered.split_whitespace().collect();
            let n = recv.len().max(got.len()).max(1);
            let agree = recv.iter().zip(got.iter()).filter(|(a, b)| a == b).count();
            best = best.max(agree as f64 / n as f64);
        }
    }
    best
}

const SCRIPT_TYPES: [&str; 6] = ["?", "P2PKH", "P2SH", "P2WPKH", "P2WSH", "P2TR"];

fn main() {
    let cases: Vec<(String, String)> = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| (c[0].clone(), c[1].clone()))
        .collect();

    let payload_words = load_payload_words_for_wordlist("english", "bip39").expect("wordlist");
    let tree = WordlistTree::new(payload_words.clone());
    println!("payload wordlist: english/bip39, {} words\n", payload_words.len());

    for (addr, hex) in &cases {
        let payload = hex_decode(hex).expect("hex");
        let (prose, words, counter) = encode_checksum_seeded(&payload).expect("encode");

        println!("{}", "─".repeat(74));
        println!("{addr}");
        println!(
            "  {} · {} payload bytes · crc32 {:08x} · counter {counter}",
            SCRIPT_TYPES[payload[1] as usize], payload.len(), crc32(&payload)
        );
        println!("\n{prose}\n");
        println!(
            "  {} payload words / {} total   seed {:#018x}",
            words.len(),
            prose.split_whitespace().count(),
            seed_for(&payload, counter)
        );

        // Round-trip + verification
        let (decoded, verdict) = decode_verified(&prose, payload.len(), &tree);
        let ok = decoded.as_deref() == Some(payload.as_slice());
        println!("  round-trip: {}   verdict: {verdict:?}", if ok { "exact" } else { "MISMATCH" });

        // Negative test: swap one payload word for another valid wordlist word.
        let victim = &words[words.len() / 2];
        let replacement = if victim == &payload_words[0] { &payload_words[1] } else { &payload_words[0] };
        let tampered = prose
            .split_whitespace()
            .map(|w| {
                let bare = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                if &bare == victim { replacement.clone() } else { w.to_string() }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let (_, tv) = decode_verified(&tampered, payload.len(), &tree);
        println!(
            "  payload word swapped ('{victim}' -> '{replacement}'): {tv:?}, {:.0}% token match",
            best_similarity(&tampered, &tree) * 100.0
        );

        // Contrast: damage a COVER word instead. The payload is untouched, so the
        // address still decodes correctly — only the checksum channel is dented.
        // Mangle by suffixing, so the damaged token cannot land back on the payload
        // wordlist (a replacement that IS a payload word would be an insertion, not
        // cover damage — a different failure mode entirely).
        let mut damaged_count = 0;
        let cover_damaged = prose
            .split_whitespace()
            .map(|w| {
                let bare = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                if !tree.contains(&bare) && bare.len() >= 3 && damaged_count < 2 {
                    damaged_count += 1;
                    format!("{bare}x")
                } else {
                    w.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let (cd, cv) = decode_verified(&cover_damaged, payload.len(), &tree);
        println!(
            "  {damaged_count} cover words damaged: {cv:?}, {:.0}% token match, payload still {}",
            best_similarity(&cover_damaged, &tree) * 100.0,
            if cd.as_deref() == Some(payload.as_slice()) { "exact" } else { "WRONG" }
        );
    }
}
