//! Scratch probe for canonical goldens: prints the canonical rendering of a few
//! fixed payloads per language so exact strings can be pinned in tests.

fn main() {
    let payloads: Vec<(&str, Vec<u8>)> = vec![
        ("20B hash160", (0u8..20).collect()),
        ("zeros", vec![0u8; 8]),
        ("one byte", vec![0xAB]),
        ("32B", (100u8..132).collect()),
    ];
    for (language, wordlist) in [("english", "bip39"), ("latin", "default"), ("czech", "default")] {
        for (name, p) in &payloads {
            match glossia::canonical_encode(p, language, wordlist) {
                Ok(text) => {
                    let d = glossia::canonical_decode(&text, language, wordlist).unwrap();
                    assert_eq!(&d.payload, p, "{language} {name} round trip");
                    assert!(d.verified, "{language} {name} verified");
                    println!("=== {language}/{wordlist} {name} (v{}) ===\n{text}\n", d.version);
                }
                Err(e) => println!("=== {language}/{wordlist} {name} ERROR: {e} ===\n"),
            }
        }
    }
}
