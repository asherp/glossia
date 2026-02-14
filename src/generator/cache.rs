use std::collections::HashMap;
use std::sync::OnceLock;
use crate::generator::types::GenerationMode;
use crate::grammar::{Grammar, SequenceWithProbability};

static PRINTED_SENTENCE_KINDS: OnceLock<()> = OnceLock::new();

/// Precomputed sequences organized by start symbol and k
pub struct SequenceCache {
    by_start_symbol: HashMap<String, Vec<Vec<SequenceWithProbability>>>,
}

impl SequenceCache {
    /// Load sequences for all start symbols we might use, up to k_max.
    /// If `dialect_override` is provided, it overrides the mode-derived dialect.
    pub fn load(mode: GenerationMode, language: &str, k_max: usize, verbose: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let dialect = match mode {
            GenerationMode::Subject => "subject",
            GenerationMode::Body => "body",
            GenerationMode::PayloadOnly => "payload_only",
        };
        Self::load_with_dialect(mode, language, dialect, k_max, verbose)
    }

    /// Load sequences using an explicit dialect name.
    pub fn load_with_dialect(mode: GenerationMode, language: &str, dialect: &str, k_max: usize, verbose: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let grammar_label = dialect;
        let grammar = Grammar::from_language_dialect(language, dialect)?;
        
        let mut by_start_symbol = HashMap::new();
        
        // Load sequences for all possible start symbols
        // Body grammar only uses "S" (simplified), subject grammar may have S_* variants
        let start_symbols = if mode == GenerationMode::Body {
            vec!["S"]
        } else {
            vec!["S", "S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"]
        };
        
        for start_symbol in start_symbols {
            let sequences_by_k = grammar.precompute_sequences_with_probability(start_symbol, k_max);
            if !sequences_by_k.is_empty() {
                by_start_symbol.insert(start_symbol.to_string(), sequences_by_k);
            }
        }
        
        print_sentence_kinds_once(grammar_label, k_max, &by_start_symbol, verbose);

        Ok(SequenceCache { by_start_symbol })
    }
    
    /// Get sequences for a given start symbol and k
    pub fn get(&self, start_symbol: &str, k: usize) -> Option<&[SequenceWithProbability]> {
        self.by_start_symbol
            .get(start_symbol)?
            .get(k)
            .map(|v: &Vec<SequenceWithProbability>| v.as_slice())
    }
}

pub fn print_sentence_kinds_once(
    grammar_label: &str,
    k_max: usize,
    by_start_symbol: &HashMap<String, Vec<Vec<SequenceWithProbability>>>,
    verbose: bool,
) {
    if by_start_symbol.is_empty() || !verbose {
        return;
    }

    PRINTED_SENTENCE_KINDS.get_or_init(|| {
        let mut total_sequences = 0usize;
        let mut per_start: Vec<(String, usize)> = by_start_symbol
            .iter()
            .map(|(start_symbol, sequences_by_k): (&String, &Vec<Vec<SequenceWithProbability>>)| {
                let count = sequences_by_k.iter().map(|seqs: &Vec<SequenceWithProbability>| seqs.len()).sum::<usize>();
                total_sequences += count;
                (start_symbol.clone(), count)
            })
            .collect();
        per_start.sort_by(|a, b| a.0.cmp(&b.0));
        let per_start_str = per_start
            .iter()
            .map(|(s, c)| format!("{}={}", s, c))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "Grammar {} defines {} sentence kinds (k <= {}): {}",
            grammar_label,
            total_sequences,
            k_max,
            per_start_str
        );
    });
}
