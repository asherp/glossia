// Example: Generate 12 random BIP39 words and merkleize them

use glossia::merkle::{merkleize, parse_merkleized, verify_merkleized};
use glossia::generator::data::{load_payload_tree, load_cover_tree};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load wordlists
    let payload_tree = load_payload_tree("english")?;
    let cover_tree = load_cover_tree("english");
    
    // Generate 12 random BIP39 words
    let mut rng = StdRng::seed_from_u64(42);
    let all_payload_words = payload_tree.words();
    let mut payload_words = Vec::new();
    
    for _ in 0..12 {
        let idx = rng.gen_range(0..all_payload_words.len());
        payload_words.push(all_payload_words[idx].clone());
    }
    
    println!("Original payload (12 words):");
    println!("  {}\n", payload_words.join(" "));
    
    // Merkleize the payload
    let merkle_result = merkleize(&payload_words, &cover_tree)?;
    
    println!("Merkleized sequence ({} words total, {} Merkle nodes):", 
             merkle_result.sequence.len(), 
             merkle_result.n_merkle);
    println!("  {}\n", merkle_result.sequence.join(" "));
    
    // Parse it back
    let parsed = parse_merkleized(&merkle_result.sequence, &payload_tree)?;
    
    println!("Parsed back to payload:");
    println!("  {}\n", parsed.join(" "));
    
    // Verify round-trip
    assert_eq!(payload_words, parsed, "Round-trip failed!");
    
    // Verify the merkleized sequence
    let is_valid = verify_merkleized(&merkle_result.sequence, &cover_tree, &payload_tree)?;
    println!("Verification: {}", if is_valid { "✓ Valid" } else { "✗ Invalid" });
    
    Ok(())
}
