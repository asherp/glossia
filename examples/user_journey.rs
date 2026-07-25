//! What a person actually experiences handling a prose-encoded Bitcoin address (#76).
//!
//! Each scenario is a real encode/decode round trip, not a mock-up: the prose,
//! the verdict, and the recovered address are all computed.
//!
//! Run: cargo run --release --example user_journey

use glossia::codec::{checksum_seed, crc32, hex_encode, payload_tokens_with_markup, Markup};
use glossia::generator::core::{Placement, Role};
use glossia::generator::data::load_payload_words_for_wordlist;
use glossia::pipeline::encode_words_into_language_traced;
use std::collections::HashSet;

/// No counter sweep: verification reproduces a fixed artifact rather than
/// generating a new one, so the encoder's fluency search would become a cost the
/// verifier and every repair candidate must repeat.
const COUNTER_RANGE: u64 = 1;
/// Opcode glyphs from the Book of Bitcoin notation (btc-prose.js OPCODE_SYMBOLS).
/// U+24EA and U+2460 are Unicode category No — *alphanumeric* — so they survive
/// the decoder's token trim. They are safe only because they are declared here.
const SIGILS: [char; 7] = [
    '\u{29C9}', // ⧉ OP_DUP
    '\u{2316}', // ⌖ OP_HASH160
    '\u{2261}', // ≡ OP_EQUALVERIFY
    '\u{2207}', // ∇ OP_CHECKSIG
    '=',         // OP_EQUAL
    '\u{24EA}', // ⓪ OP_0
    '\u{2460}', // ① OP_1
];

struct Codec {
    wl: Vec<String>,
    set: HashSet<String>,
    bpw: usize,
    markup: Markup,
}

impl Codec {
    fn new() -> Self {
        let wl = load_payload_words_for_wordlist("english", "bip39").unwrap();
        let set = wl.iter().map(|w| w.to_lowercase()).collect();
        let bpw = wl.len().trailing_zeros() as usize;
        let markup = Markup::new(SIGILS, &wl).expect("sigils validate");
        Self { wl, set, bpw, markup }
    }

    fn header_bits(&self, program_len: usize) -> usize {
        let words = (program_len * 8 + self.bpw - 1) / self.bpw;
        let mut hb = words * self.bpw - program_len * 8;
        if hb < 5 { hb += self.bpw; }
        hb
    }

    fn pack(&self, program: &[u8]) -> Vec<String> {
        let hb = self.header_bits(program.len());
        let header = ((self.bpw as u32) << (hb - 4)) | 1;
        let db = program.len() * 8;
        let bit = |i: usize| -> usize {
            if i < db { ((program[i / 8] >> (7 - (i % 8))) & 1) as usize }
            else { ((header >> (hb - 1 - (i - db))) & 1) as usize }
        };
        (0..(db + hb) / self.bpw)
            .map(|w| (0..self.bpw).fold(0, |a, b| (a << 1) | bit(w * self.bpw + b)))
            .map(|i| self.wl[i].clone())
            .collect()
    }

    fn unpack(&self, words: &[String], n_bytes: usize) -> Option<Vec<u8>> {
        let hb = self.header_bits(n_bytes);
        if words.len() != (n_bytes * 8 + hb) / self.bpw { return None; }
        let mut bits = Vec::new();
        for w in words {
            let i = self.wl.iter().position(|x| x.to_lowercase() == w.to_lowercase())?;
            for b in (0..self.bpw).rev() { bits.push((i >> b) & 1); }
        }
        Some((0..n_bytes).map(|i| (0..8).fold(0u8, |a, b| (a << 1) | bits[i * 8 + b] as u8)).collect())
    }

    fn seed(&self, program: &[u8]) -> u64 {
        let mut c = program.to_vec();
        c.extend_from_slice(&[self.bpw as u8, 1u8]);
        checksum_seed(&c, 0)
    }

    fn render(&self, program: &[u8], sigil: char) -> (String, Vec<Placement>) {
        let words = self.pack(program);
        let (t, _c, p) = encode_words_into_language_traced(
            &words, "english", "default", "body", self.seed(program), COUNTER_RANGE as usize,
        ).expect("encode");
        (format!("{sigil} {t}"), p)
    }

    /// Decode + verify. Returns (recovered program, verdict, similarity).
    fn read(&self, artifact: &str, n_bytes: usize) -> (Option<Vec<u8>>, &'static str, f64) {
        let words = payload_tokens_with_markup(artifact, &self.markup, |w| self.set.contains(w));
        let expected = (n_bytes * 8 + self.header_bits(n_bytes)) / self.bpw;
        if words.len() != expected {
            return (None, "UNREADABLE — wrong number of address words", 0.0);
        }
        let Some(program) = self.unpack(&words, n_bytes) else {
            return (None, "UNREADABLE — word not in the address vocabulary", 0.0);
        };
        // Re-render and compare: this is the checksum.
        let mut best = 0.0f64;
        let recv: Vec<&str> = artifact.split_whitespace().skip(1).collect();
        for c in 0..COUNTER_RANGE {
            let w = self.pack(&program);
            let mut cc = program.clone();
            cc.extend_from_slice(&[self.bpw as u8, 1u8]);
            let s = checksum_seed(&cc, 0).wrapping_add(c);
            if let Ok((t, _, _)) = encode_words_into_language_traced(
                &w, "english", "default", "body", s, 1) {
                let got: Vec<&str> = t.split_whitespace().collect();
                if got == recv { return (Some(program), "VERIFIED", 1.0); }
                let n = recv.len().max(got.len()).max(1);
                best = best.max(recv.iter().zip(got.iter()).filter(|(a, b)| a == b).count() as f64 / n as f64);
            }
        }
        if best > 0.75 {
            (Some(program), "READABLE, NOT VERIFIED — wording differs slightly; address itself looks intact", best)
        } else {
            (Some(program), "MISMATCH — this is not the address it was written as", best)
        }
    }
}

fn roles(p: &[Placement]) -> String {
    p.iter().filter_map(|x| match x.role {
        Some(Role::Subject) => Some(format!("{} (subject)", x.word)),
        Some(Role::Object) => Some(format!("{} (object)", x.word)),
        None => None,
    }).collect::<Vec<_>>().join(", ")
}

fn hr(title: &str) {
    println!("\n{}\n{title}\n{}", "─".repeat(76), "─".repeat(76));
}

fn main() {
    let c = Codec::new();
    // P2WPKH, bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4
    let program = glossia::codec::hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6").unwrap();
    let (artifact, places) = c.render(&program, '\u{24EA}');  // OP_0

    hr("1. SOMEONE SENDS YOU AN ADDRESS");
    println!("\n  {artifact}\n");
    println!("  This is a pay-to-witness-public-key-hash address. \u{24EA} is OP_0, the");
    println!("  witness version; the sentences carry the 20-byte program. {} of the", places.len());
    println!("  words are address data, the rest is connective prose.\n");
    println!("  hash160  {}", hex_encode(&program));
    println!("  checksum {:08x}", crc32(&program));

    hr("2. YOU PASTE IT IN AND IT CHECKS OUT");
    let (got, verdict, _) = c.read(&artifact, program.len());
    println!("\n  {verdict}");
    println!("  recovered: {}", hex_encode(got.as_deref().unwrap_or(&[])));
    println!("  matches the original: {}", got.as_deref() == Some(&program[..]));
    println!("\n  Verification re-renders the address from the bytes you decoded and");
    println!("  compares the prose. Only the true bytes reproduce this exact wording.");

    hr("3. YOU RETYPE IT AND FUMBLE A CONNECTIVE WORD");
    let sloppy = artifact.replacen(" may ", " might ", 1).replacen(" per ", " for ", 1);
    println!("\n  {sloppy}\n");
    let (got, verdict, sim) = c.read(&sloppy, program.len());
    println!("  {verdict}");
    println!("  address recovered correctly: {}", got.as_deref() == Some(&program[..]));
    println!("  wording match: {:.0}%", sim * 100.0);
    println!("\n  The address words were untouched, so the money still goes to the right");
    println!("  place — but the wording no longer matches, so it cannot self-verify.");
    println!("  A wallet should say so rather than claim success.");

    hr("4. YOU MISTYPE AN ADDRESS WORD INTO ANOTHER REAL WORD");
    let victim = &places[places.len() / 2].word;
    let typo = artifact.replacen(victim.as_str(), "kitten", 1);
    println!("\n  '{victim}' misread as 'kitten':\n");
    println!("  {typo}\n");
    let (_g, verdict, sim) = c.read(&typo, program.len());
    println!("  {verdict}  (wording match {:.0}%)", sim * 100.0);
    println!("\n  This is the dangerous case: 'kitten' is a valid address word, so the");
    println!("  text still decodes — to a DIFFERENT address. The wording is what catches");
    println!("  it. Funds sent here would be unrecoverable, so the check matters.");

    hr("5. A WORD GETS LOST IN COPY-PASTE");
    let mut toks: Vec<&str> = artifact.split_whitespace().collect();
    let drop_at = places[2].token_index + 1;
    let dropped = toks.remove(drop_at);
    println!("\n  '{dropped}' lost:\n");
    println!("  {}\n", toks.join(" "));
    let (_g, verdict, _) = c.read(&toks.join(" "), program.len());
    println!("  {verdict}");
    println!("\n  A fixed-length address has a fixed word count, so a dropped address word");
    println!("  is caught before anything is decoded at all.");

    hr("6. A CONNECTIVE WORD TURNS INTO AN ADDRESS WORD");
    // 'son' is connective prose; 'sun' is an address word. One character apart.
    let inserted = artifact.replacen(" son ", " sun ", 1);
    println!("\n  'son' misread as 'sun':\n");
    println!("  {inserted}\n");
    let (_g, verdict, _) = c.read(&inserted, program.len());
    println!("  {verdict}");
    println!("\n  This is the mirror of scenario 5. Nothing was lost — a word was GAINED,");
    println!("  because a connective word landed in the address vocabulary. The fixed");
    println!("  word count catches it the same way.");

    hr("7. TWO ERRORS THAT HIDE EACH OTHER");
    // A dropped address word and a gained one cancel out in the count.
    let compound = artifact.replacen(" son ", " sun ", 1).replacen("ring", "map", 1);
    println!("\n  'son' -> 'sun' (gained) and 'ring' -> 'map' (lost), together:\n");
    println!("  {compound}\n");
    let (g, verdict, sim) = c.read(&compound, program.len());
    println!("  {verdict}  (wording match {:.0}%)", sim * 100.0);
    println!("  decodes to: {}", hex_encode(g.as_deref().unwrap_or(&[])));
    println!("  is that the right address? {}", g.as_deref() == Some(&program[..]));
    println!("\n  The word count is back to normal, so counting alone would pass this.");
    println!("  Only the wording check catches it — which is the reason the address");
    println!("  determines its own prose rather than being wrapped in arbitrary text.");

    hr("8. TWO ADDRESSES THAT DIFFER BY ONE BIT");
    let mut near = program.clone();
    near[19] ^= 1;
    let (other, _p2) = c.render(&near, '\u{24EA}');
    println!("\n  yours:  {artifact}\n");
    println!("  theirs: {other}\n");
    println!("  The two hash160s differ in a single bit, and share 14 of 15 address");
    println!("  words. You are not meant to spot that. You are meant to see that these");
    println!("  are two different paragraphs — which is why the wording is derived from");
    println!("  the address rather than chosen freely.");

    hr("9. READING IT ALOUD");
    println!("\n  If you dictate this, the listener needs the address words in order:\n");
    println!("  {}\n", places.iter().map(|p| p.word.as_str()).collect::<Vec<_>>().join(" · "));
    println!("  The grammar is what makes that speakable — and the roles are a second");
    println!("  handle on it:\n");
    println!("  {}", roles(&places));
    println!("\n  Sentence by sentence:");
    let mut cur = usize::MAX;
    for p in &places {
        if p.sentence != cur { cur = p.sentence; print!("\n    sentence {}: ", cur + 1); }
        print!("{} ", p.word);
    }
    println!("\n");
}
