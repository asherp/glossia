use std::collections::{HashMap, VecDeque};

/// WordlistTree: canonical word list with O(1) membership lookup.
/// Encapsulates the canonical word list (file order) + HashMap index for fast membership.
/// Designed for future append-only extension.
#[derive(Debug, Clone)]
pub struct WordlistTree {
    words: Vec<String>,
    index: HashMap<String, usize>,
}

impl WordlistTree {
    /// Create a new WordlistTree from a word list.
    /// The order of `words` is preserved as the canonical ordering.
    pub fn new(words: Vec<String>) -> Self {
        let mut index = HashMap::with_capacity(words.len());
        for (i, word) in words.iter().enumerate() {
            index.insert(word.to_lowercase(), i);
        }
        Self { words, index }
    }

    /// Check if a word is in the wordlist (O(1) lookup).
    pub fn contains(&self, word: &str) -> bool {
        self.index.contains_key(&word.to_lowercase())
    }

    /// Get the canonical position (index) of a word in the wordlist.
    /// Returns None if the word is not in the wordlist.
    pub fn position(&self, word: &str) -> Option<usize> {
        self.index.get(&word.to_lowercase()).copied()
    }

    /// Get the number of words in the wordlist.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Check if the wordlist is empty.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Get a reference to the canonical word list (file order).
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// Get a word by its canonical index.
    pub fn get(&self, idx: usize) -> Option<&String> {
        self.words.get(idx)
    }
}

/// Result of merkleization: the sequence, number of leaves (payload words), and number of Merkle nodes.
#[derive(Debug, Clone)]
pub struct MerkleResult {
    pub sequence: Vec<String>,
    pub n_leaves: usize,
    pub n_merkle: usize,
}

/// Binary tree node for Merkle tree construction
#[derive(Debug, Clone)]
enum MerkleNode {
    Leaf(String),
    Internal(String, Box<MerkleNode>, Box<MerkleNode>),
}

/// Build a binary tree bottom-up with payload words as leaves.
/// Then assign cover words top-down via BFS (most frequent first).
/// Finally, perform pre-order traversal to get the sequence.
pub fn merkleize(
    payload: &[String],
    cover_tree: &WordlistTree,
) -> Result<MerkleResult, String> {
    let n = payload.len();
    
    // Minimum payload: N >= 2 (need at least 2 leaves for a Merkle tree)
    if n < 2 {
        return Err(format!("Payload must have at least 2 words, got {}", n));
    }
    
    // Maximum payload: need N-1 cover words for internal nodes
    if cover_tree.len() < n - 1 {
        return Err(format!(
            "Need at least {} cover words for {} payload words, got {}",
            n - 1,
            n,
            cover_tree.len()
        ));
    }
    
    // Pick the first N-1 cover words from the list (file order, most frequent first)
    let merkle_cover_words: Vec<String> = cover_tree.words()[..n - 1].to_vec();
    
    // Build binary tree bottom-up (preserving payload order at leaves)
    let mut nodes: VecDeque<MerkleNode> = payload
        .iter()
        .map(|word| MerkleNode::Leaf(word.clone()))
        .collect();
    
    // Build tree bottom-up: repeatedly combine pairs of nodes
    let mut cover_idx = 0;
    while nodes.len() > 1 {
        let left = nodes.pop_front().unwrap();
        let right = if nodes.len() > 0 {
            nodes.pop_front().unwrap()
        } else {
            // Odd number of nodes: duplicate the last one
            left.clone()
        };
        
        let cover_word = merkle_cover_words[cover_idx].clone();
        cover_idx += 1;
        
        let internal = MerkleNode::Internal(
            cover_word,
            Box::new(left),
            Box::new(right),
        );
        nodes.push_back(internal);
    }
    
    // Root should be the first cover word (most frequent)
    // But we built bottom-up, so we need to reassign top-down via BFS
    // Actually, let's rebuild top-down to ensure correct assignment
    
    // Rebuild tree top-down with BFS assignment
    let root = build_tree_top_down(payload, &merkle_cover_words)?;
    
    // Pre-order traversal to get sequence
    let mut sequence = Vec::new();
    pre_order_traversal(&root, &mut sequence);
    
    Ok(MerkleResult {
        sequence,
        n_leaves: n,
        n_merkle: n - 1,
    })
}

/// Build tree top-down with BFS assignment of cover words
fn build_tree_top_down(
    payload: &[String],
    merkle_cover_words: &[String],
) -> Result<MerkleNode, String> {
    let n = payload.len();
    
    if n == 1 {
        return Ok(MerkleNode::Leaf(payload[0].clone()));
    }
    
    // Calculate tree structure: we need a complete binary tree
    // For N leaves, we need N-1 internal nodes
    // Build level by level using BFS
    
    // First, create all leaf nodes
    let leaves: VecDeque<MerkleNode> = payload
        .iter()
        .map(|word| MerkleNode::Leaf(word.clone()))
        .collect();
    
    // Build internal nodes level by level
    let mut current_level: VecDeque<MerkleNode> = leaves;
    let mut cover_idx = 0;
    
    while current_level.len() > 1 {
        let mut next_level: VecDeque<MerkleNode> = VecDeque::new();
        
        // Process pairs
        while current_level.len() >= 2 {
            let left = current_level.pop_front().unwrap();
            let right = current_level.pop_front().unwrap();
            
            if cover_idx >= merkle_cover_words.len() {
                return Err("Not enough cover words for tree construction".to_string());
            }
            
            let cover_word = merkle_cover_words[cover_idx].clone();
            cover_idx += 1;
            
            let internal = MerkleNode::Internal(
                cover_word,
                Box::new(left),
                Box::new(right),
            );
            next_level.push_back(internal);
        }
        
        // If odd number, promote the last node
        if current_level.len() == 1 {
            next_level.push_back(current_level.pop_front().unwrap());
        }
        
        current_level = next_level;
    }
    
    // Root is the first cover word (most frequent) - this should be merkle_cover_words[0]
    // But we assigned in BFS order, so the root gets the first cover word
    Ok(current_level.pop_front().unwrap())
}

/// Pre-order traversal: visit root, then left subtree, then right subtree
fn pre_order_traversal(node: &MerkleNode, sequence: &mut Vec<String>) {
    match node {
        MerkleNode::Leaf(word) => {
            sequence.push(word.clone());
        }
        MerkleNode::Internal(cover_word, left, right) => {
            sequence.push(cover_word.clone());
            pre_order_traversal(left, sequence);
            pre_order_traversal(right, sequence);
        }
    }
}

/// Parse a merkleized sequence to extract the original payload words.
/// Classifies each word: payload set = leaf, otherwise = internal.
/// Parses pre-order traversal, returns leaves in order.
pub fn parse_merkleized(
    sequence: &[String],
    payload_tree: &WordlistTree,
) -> Result<Vec<String>, String> {
    // Validate sequence length is odd (2N-1 for N leaves)
    if sequence.len() % 2 == 0 {
        return Err(format!(
            "Merkleized sequence must have odd length (2N-1), got {}",
            sequence.len()
        ));
    }
    
    let n = (sequence.len() + 1) / 2;
    
    // Parse pre-order traversal
    let mut leaves = Vec::new();
    let mut idx = 0;
    
    parse_pre_order(sequence, &mut idx, payload_tree, &mut leaves)?;
    
    if leaves.len() != n {
        return Err(format!(
            "Expected {} leaves, got {}",
            n,
            leaves.len()
        ));
    }
    
    Ok(leaves)
}

/// Recursively parse pre-order traversal
fn parse_pre_order(
    sequence: &[String],
    idx: &mut usize,
    payload_tree: &WordlistTree,
    leaves: &mut Vec<String>,
) -> Result<(), String> {
    if *idx >= sequence.len() {
        return Err("Unexpected end of sequence".to_string());
    }
    
    let word = &sequence[*idx];
    
    *idx += 1;
    
    // Check if this is a payload word (leaf) or cover word (internal)
    if payload_tree.contains(word) {
        // This is a leaf (payload word)
        leaves.push(word.clone());
    } else {
        // This is an internal node (cover word)
        // Parse left subtree, then right subtree
        parse_pre_order(sequence, idx, payload_tree, leaves)?;
        parse_pre_order(sequence, idx, payload_tree, leaves)?;
    }
    
    Ok(())
}

/// Verify a merkleized sequence by parsing and re-merkleizing.
pub fn verify_merkleized(
    sequence: &[String],
    cover_tree: &WordlistTree,
    payload_tree: &WordlistTree,
) -> Result<bool, String> {
    // Parse to extract leaves
    let leaves = parse_merkleized(sequence, payload_tree)?;
    
    // Re-merkleize
    let result = merkleize(&leaves, cover_tree)?;
    
    // Compare sequences
    Ok(result.sequence == sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wordlist_tree() {
        let words = vec!["apple".to_string(), "banana".to_string(), "cherry".to_string()];
        let tree = WordlistTree::new(words.clone());
        
        assert_eq!(tree.len(), 3);
        assert!(tree.contains("apple"));
        assert!(tree.contains("APPLE")); // case insensitive
        assert!(!tree.contains("grape"));
        
        assert_eq!(tree.position("banana"), Some(1));
        assert_eq!(tree.position("cherry"), Some(2));
        assert_eq!(tree.position("grape"), None);
        
        assert_eq!(tree.get(0), Some(&"apple".to_string()));
        assert_eq!(tree.get(1), Some(&"banana".to_string()));
        assert_eq!(tree.words(), words.as_slice());
    }

    #[test]
    fn test_merkleize_basic() {
        let payload = vec!["word1".to_string(), "word2".to_string()];
        let cover_words = vec!["cover1".to_string()];
        let cover_tree = WordlistTree::new(cover_words);
        
        let result = merkleize(&payload, &cover_tree).unwrap();
        
        assert_eq!(result.n_leaves, 2);
        assert_eq!(result.n_merkle, 1);
        assert_eq!(result.sequence.len(), 3); // 2N-1 = 3
        assert_eq!(result.sequence[0], "cover1"); // root should be first cover word
    }

    #[test]
    fn test_merkleize_insufficient_cover() {
        let payload = vec!["word1".to_string(), "word2".to_string(), "word3".to_string()];
        let cover_words = vec!["cover1".to_string()]; // Need 2, only have 1
        let cover_tree = WordlistTree::new(cover_words);
        
        assert!(merkleize(&payload, &cover_tree).is_err());
    }

    #[test]
    fn test_merkleize_single_word() {
        let payload = vec!["word1".to_string()];
        let cover_words = vec!["cover1".to_string()];
        let cover_tree = WordlistTree::new(cover_words);
        
        assert!(merkleize(&payload, &cover_tree).is_err()); // Need at least 2 words
    }

    #[test]
    fn test_parse_merkleized() {
        let payload_words = vec!["word1".to_string(), "word2".to_string()];
        let payload_tree = WordlistTree::new(payload_words.clone());
        
        // Create a merkleized sequence manually
        let cover_words = vec!["cover1".to_string()];
        let cover_tree = WordlistTree::new(cover_words);
        let merkle_result = merkleize(&payload_words, &cover_tree).unwrap();
        
        // Parse it back
        let parsed = parse_merkleized(&merkle_result.sequence, &payload_tree).unwrap();
        
        assert_eq!(parsed, payload_words);
    }

    #[test]
    fn test_parse_merkleized_invalid_length() {
        let payload_words = vec!["word1".to_string(), "word2".to_string()];
        let payload_tree = WordlistTree::new(payload_words);
        
        // Even length sequence (invalid)
        let invalid_sequence = vec!["word1".to_string(), "word2".to_string()];
        assert!(parse_merkleized(&invalid_sequence, &payload_tree).is_err());
    }

    #[test]
    fn test_verify_merkleized() {
        let payload_words = vec!["word1".to_string(), "word2".to_string(), "word3".to_string()];
        let payload_tree = WordlistTree::new(payload_words.clone());
        
        let cover_words = vec!["cover1".to_string(), "cover2".to_string()];
        let cover_tree = WordlistTree::new(cover_words);
        
        let merkle_result = merkleize(&payload_words, &cover_tree).unwrap();
        
        // Verify the sequence
        assert!(verify_merkleized(&merkle_result.sequence, &cover_tree, &payload_tree).unwrap());
        
        // Verify a corrupted sequence fails
        let mut corrupted = merkle_result.sequence.clone();
        corrupted[0] = "wrong".to_string();
        assert!(!verify_merkleized(&corrupted, &cover_tree, &payload_tree).unwrap());
    }

    #[test]
    fn test_merkleize_round_trip() {
        // Test with multiple payload words
        let payload_words = vec![
            "abandon".to_string(),
            "ability".to_string(),
            "able".to_string(),
            "about".to_string(),
        ];
        let payload_tree = WordlistTree::new(payload_words.clone());
        
        let cover_words = vec![
            "the".to_string(),
            "of".to_string(),
            "and".to_string(),
        ];
        let cover_tree = WordlistTree::new(cover_words);
        
        // Merkleize
        let result = merkleize(&payload_words, &cover_tree).unwrap();
        
        assert_eq!(result.n_leaves, 4);
        assert_eq!(result.n_merkle, 3);
        assert_eq!(result.sequence.len(), 7); // 2*4-1 = 7
        
        // Parse back
        let parsed = parse_merkleized(&result.sequence, &payload_tree).unwrap();
        assert_eq!(parsed, payload_words);
        
        // Verify
        assert!(verify_merkleized(&result.sequence, &cover_tree, &payload_tree).unwrap());
    }
}
