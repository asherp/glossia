use std::collections::{HashMap, HashSet};
use rand::{seq::SliceRandom, Rng};
use crate::types::Pos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationMode {
    Subject,
    Body,
    PayloadOnly,  // Use only payload words, no cover words
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentenceLengthMode {
    Compact,
    Natural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Number {
    Singular,
    Plural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightMode {
    None,
    Bars,
    Color(u8),  // ANSI color code (30-37 for foreground colors)
    Madlib,
}

#[derive(Clone, Debug)]
pub struct PayloadTok {
    pub word: String,
    pub allowed: HashSet<Pos>,
}

impl PayloadTok {
    pub fn new(word: impl Into<String>, allowed: &[Pos]) -> Self {
        Self {
            word: word.into(),
            allowed: allowed.iter().copied().collect(),
        }
    }
}

/// Small, controlled, SFW lexicon by POS.
/// IMPORTANT: ensure cover lexicon does NOT contain any payload (BIP39) words,
/// so decoding (filtering BIP39 words) is trivial and unambiguous.
#[derive(Clone, Debug)]
pub struct Lexicon {
    by_pos: HashMap<Pos, Vec<String>>,
    /// Lowercased payload words (for filtering / repetition logic).
    payload_set: HashSet<String>,
    /// Lowercased full wordlist set (for collision checks when inflecting cover words).
    pub(crate) wordlist_set: HashSet<String>,
}

impl Lexicon {
    pub fn new(payload_set: HashSet<String>, wordlist_set: HashSet<String>) -> Self {
        Self {
            by_pos: HashMap::new(),
            payload_set,
            wordlist_set,
        }
    }

    pub fn with_words(mut self, pos: Pos, words: &[&str]) -> Self {
        self.by_pos
            .entry(pos)
            .or_insert_with(Vec::new)
            .extend(words.iter().map(|w| w.to_string()));
        self
    }

    pub fn pick_cover<R: Rng>(&self, rng: &mut R, pos: Pos, recent_words: &[&str]) -> String {
        let empty = Vec::new();
        let list = self.by_pos.get(&pos).unwrap_or(&empty);

        if list.is_empty() {
            // No cover words for this POS — return empty string (graceful degradation).
            // The cover.yaml should be updated to include words for all POS categories.
            return String::new();
        }

        // Filter out payload words and recent words (to avoid repetition within a window)
        let available: Vec<&String> = list
            .iter()
            .filter(|w| {
                !self.payload_set.contains(&w.to_lowercase()) &&
                !recent_words.iter().any(|&rw| rw == w.as_str())
            })
            .collect();

        if available.is_empty() {
            // If all words would be repeats, fall back to any non-payload word
            let fallback: Vec<&String> = list
                .iter()
                .filter(|w| !self.payload_set.contains(&w.to_lowercase()))
                .collect();
            if fallback.is_empty() {
                return String::new();
            }
            // Prioritize shorter words in fallback too
            let min_len = fallback.iter().map(|w| w.len()).min().unwrap_or(0);
            let shortest_fallback: Vec<&String> = fallback
                .iter()
                .filter(|w| w.len() == min_len)
                .copied()
                .collect();
            return shortest_fallback.choose(rng).unwrap().to_string();
        }

        // Find the shortest length among available words
        let min_len = available.iter().map(|w| w.len()).min().unwrap_or(0);
        
        // Filter to only words of the shortest length
        let shortest_words: Vec<&String> = available
            .iter()
            .filter(|w| w.len() == min_len)
            .copied()
            .collect();

        shortest_words.choose(rng).unwrap().to_string()
    }

    /// Like `pick_cover`, but allows an additional predicate to enforce lightweight grammar constraints
    /// (e.g., "bare verb after Modal", "transitive verb before NP").
    ///
    /// Returns `None` if no word satisfies the predicate (caller should fall back to `pick_cover`).
    pub fn pick_cover_filtered<R: Rng, F: Fn(&str) -> bool>(
        &self,
        rng: &mut R,
        pos: Pos,
        recent_words: &[&str],
        predicate: F,
    ) -> Option<String> {
        let list = self.by_pos.get(&pos)?;

        // Filter out payload words, recent words, and words failing the predicate.
        let available: Vec<&String> = list
            .iter()
            .filter(|w| {
                !self.payload_set.contains(&w.to_lowercase())
                    && !recent_words.iter().any(|&rw| rw == w.as_str())
                    && predicate(w.as_str())
            })
            .collect();

        if available.is_empty() {
            return None;
        }

        let min_len = available.iter().map(|w| w.len()).min().unwrap_or(0);
        let shortest: Vec<&String> = available
            .iter()
            .filter(|w| w.len() == min_len)
            .copied()
            .collect();

        Some(shortest.choose(rng).unwrap().to_string())
    }

    /// Pick cover word with prime ordering constraint for math/primes language.
    /// Cover word must be: left_prime < cover_word < right_prime
    /// Cover word must be a non-prime integer.
    pub fn pick_cover_with_prime_constraint<R: Rng>(
        &self,
        rng: &mut R,
        pos: Pos,
        recent_words: &[&str],
        left_word: Option<&str>,
        right_word: Option<&str>,
    ) -> Option<String> {
        // Helper to parse integer from word
        let parse_int = |w: &str| -> Option<i64> {
            w.parse::<i64>().ok()
        };

        // Helper to check if a number is prime (simple check)
        let is_prime = |n: i64| -> bool {
            if n < 2 {
                return false;
            }
            if n == 2 {
                return true;
            }
            if n % 2 == 0 {
                return false;
            }
            let sqrt_n = (n as f64).sqrt() as i64;
            for i in (3..=sqrt_n).step_by(2) {
                if n % i == 0 {
                    return false;
                }
            }
            true
        };

        // Parse left and right primes
        let left_prime = left_word.and_then(|w| parse_int(w));
        let right_prime = right_word.and_then(|w| parse_int(w));

        // If we don't have both bounds, fall back to regular pick_cover
        let (left_bound, right_bound) = match (left_prime, right_prime) {
            (Some(l), Some(r)) if l < r => (l, r),
            _ => {
                // No valid bounds, use regular pick_cover
                return Some(self.pick_cover(rng, pos, recent_words));
            }
        };

        let list = self.by_pos.get(&pos)?;

        // Filter: non-payload, non-recent, non-prime integer, within bounds
        let available: Vec<&String> = list
            .iter()
            .filter(|w| {
                // Exclude payload words
                if self.payload_set.contains(&w.to_lowercase()) {
                    return false;
                }
                // Exclude recent words
                if recent_words.iter().any(|&rw| rw == w.as_str()) {
                    return false;
                }
                // Must be a parseable integer
                if let Some(n) = parse_int(w) {
                    // Must be non-prime
                    if is_prime(n) {
                        return false;
                    }
                    // Must satisfy: left_bound < n < right_bound
                    n > left_bound && n < right_bound
                } else {
                    false
                }
            })
            .collect();

        if available.is_empty() {
            // Fall back to regular pick_cover if no valid cover word found
            return Some(self.pick_cover(rng, pos, recent_words));
        }

        // Prefer shorter words
        let min_len = available.iter().map(|w| w.len()).min().unwrap_or(0);
        let shortest: Vec<&String> = available
            .iter()
            .filter(|w| w.len() == min_len)
            .copied()
            .collect();

        Some(shortest.choose(rng).unwrap().to_string())
    }
}
