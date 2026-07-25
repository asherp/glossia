//! Do payload words change grammatical ROLE when payload bits change? (#76)
//!
//! Role change is a coarser signal than a word-by-word diff: "insect was the
//! subject, now it's an object" is something a reader can hold in mind. Because
//! the cover is reseeded from the payload checksum, a changed payload reshuffles
//! which slot each word lands in — even though the words themselves are 14/15
//! identical after a single bit flip.

use glossia::codec::{checksum_seed, hex_decode};
use glossia::generator::core::{Placement, Role};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language_traced;
use glossia::Pos;

fn pack(program: &[u8], bpw: usize, header: u32, hb: usize) -> Vec<usize> {
    let db = program.len() * 8;
    let bit = |i: usize| -> usize {
        if i < db { ((program[i / 8] >> (7 - (i % 8))) & 1) as usize }
        else { ((header >> (hb - 1 - (i - db))) & 1) as usize }
    };
    (0..(db + hb) / bpw).map(|w| (0..bpw).fold(0, |a, b| (a << 1) | bit(w * bpw + b))).collect()
}

fn render(program: &[u8], header: u32, hb: usize, wl: &[String], bpw: usize) -> (String, Vec<Placement>) {
    let words: Vec<String> = pack(program, bpw, header, hb).iter().map(|&i| wl[i].clone()).collect();
    let mut checked = program.to_vec();
    checked.extend_from_slice(&[bpw as u8, 1u8]);
    let (t, _c, p) = encode_words_into_language_traced(
        &words, "english", "default", "body", checksum_seed(&checked, 0), 1).expect("encode");
    (t, p)
}

fn describe(p: &Placement) -> String {
    let r = match p.role { Some(Role::Subject) => "subj", Some(Role::Object) => "obj", None => "—" };
    format!("{}({:?},{})", p.word, p.pos, r)
}

fn main() {
    let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
    let bpw = wl.len().trailing_zeros() as usize;
    let program = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6").unwrap();
    let hb = 15 * bpw - program.len() * 8;
    let header = ((bpw as u32) << (hb - 4)) | 1;

    let (base_text, base_p) = render(&program, header, hb, &wl, bpw);
    println!("BASELINE\n  {base_text}\n");
    println!("  {}\n", base_p.iter().map(describe).collect::<Vec<_>>().join("  "));

    let mut role_changed = vec![0usize; base_p.len()];
    let mut pos_changed = vec![0usize; base_p.len()];
    let mut sentence_moved = vec![0usize; base_p.len()];
    let mut any_role = 0usize;
    let mut examples: Vec<(usize, String, String)> = Vec::new();
    let n = program.len() * 8;

    for b in 0..n {
        let mut q = program.clone();
        q[b / 8] ^= 1 << (7 - (b % 8));
        let (_t, p) = render(&q, header, hb, &wl, bpw);
        let mut changed_here = 0;
        for (i, (a, c)) in base_p.iter().zip(p.iter()).enumerate() {
            if a.word == c.word {
                if a.role != c.role { role_changed[i] += 1; changed_here += 1; }
                if a.pos != c.pos { pos_changed[i] += 1; }
                if a.sentence != c.sentence { sentence_moved[i] += 1; }
            }
        }
        if changed_here > 0 { any_role += 1; }
        if examples.len() < 2 && changed_here >= 3 {
            examples.push((b, p.iter().map(describe).collect::<Vec<_>>().join("  "), _t));
        }
    }

    println!("{}", "═".repeat(78));
    println!("\n{n} single-bit flips — per-word change rates (unchanged words only)\n");
    println!("  {:<12} {:>10} {:>10} {:>12}", "word", "role", "POS", "sentence");
    for (i, p) in base_p.iter().enumerate() {
        println!("  {:<12} {:>9.0}% {:>9.0}% {:>11.0}%", p.word,
            role_changed[i] as f64 / n as f64 * 100.0,
            pos_changed[i] as f64 / n as f64 * 100.0,
            sentence_moved[i] as f64 / n as f64 * 100.0);
    }
    println!("\n  flips changing at least one word's role: {any_role} of {n} ({:.0}%)",
             any_role as f64 / n as f64 * 100.0);

    println!("\n{}", "═".repeat(78));
    println!("\nEXAMPLES (3+ role changes):\n");
    for (b, roles, text) in &examples {
        println!("  bit {b}:\n  {text}\n  {roles}\n");
    }
    let _ = Pos::N;
}
