//! Script to get the top N most common words with 6 or fewer characters
//! from word frequency data, formatted for BIP39 encode usage.
//!
//! Output format: word|POS1,POS2 (matching cover_POS.txt format)
//!
//! Supports multiple data sources:
//! 1. COCA word frequency data from wordfrequency.info (recommended)
//! 2. Google Books Ngram 1-gram files
//! 3. CSV frequency files
//!
//! Optional nlprule lemmatization pass: collapses inflected forms (runs, running, ran)
//! into their base lemma (run), aggregating POS frequencies under the lemma.
//!
//! Data source: https://www.wordfrequency.info/samples.asp

use clap::Parser;
use flate2::read::GzDecoder;
use glossia::GrammarChecker;
use rayon::prelude::*;
use regex::Regex;
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WordData {
    freq: f64,
    pos: HashSet<String>,
    /// Per-POS frequency counts for computing weights.
    /// Key is the normalized POS tag (e.g. "N", "V", "Adj"), value is the
    /// total frequency observed for that POS.
    pos_freq: HashMap<String, f64>,
}

/// Lemmas that must stay in the cover wordlist (grammatical glue).
/// Excluded from payload so they remain available as high-frequency filler.
const COVER_ONLY_LEMMAS: &[&str] = &["a", "i"];

/// Normalize POS tags to simplified format used in cover_POS.txt
fn normalize_pos(pos_str: &str) -> HashSet<String> {
    let mut pos_tags = HashSet::new();
    let pos_lower = pos_str.trim().to_lowercase();
    
    // Noun
    if Regex::new(r"\bn\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("noun") {
        pos_tags.insert("N".to_string());
    }
    
    // Verb
    if Regex::new(r"\bv\.?\s*(t\.?|i\.?)?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("verb") {
        pos_tags.insert("V".to_string());
    }
    
    // Adjective
    if Regex::new(r"\ba\.?\b").unwrap().is_match(&pos_lower)
        || Regex::new(r"\badj\.?\b").unwrap().is_match(&pos_lower)
        || pos_lower.contains("adjective")
    {
        pos_tags.insert("Adj".to_string());
    }
    
    // Adverb
    if Regex::new(r"\badv\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("adverb") {
        pos_tags.insert("Adv".to_string());
    }
    
    // Preposition
    if Regex::new(r"\bprep\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("preposition") {
        pos_tags.insert("Prep".to_string());
    }
    
    // Conjunction
    if Regex::new(r"\bconj\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("conjunction") {
        pos_tags.insert("Conj".to_string());
    }
    
    // Pronoun
    if Regex::new(r"\bpron\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("pronoun") {
        pos_tags.insert("Pron".to_string());
    }
    
    // Determiner
    if pos_lower.contains("def. art.") || pos_lower.contains("definite article") || pos_lower.contains("det.") {
        pos_tags.insert("Det".to_string());
    }
    
    pos_tags
}

/// Parse a line from Google Books Ngram v3 format.
/// v3 format: word_POS\tyear,match_count,volume_count\tyear,match_count,volume_count\t...
/// Also supports older format: word\tyear\tmatch_count\tpage_count\tvolume_count
/// Returns (word, pos_tags, vec_of_(year, match_count))
fn parse_ngram_line(line: &str) -> Option<(String, HashSet<String>, Vec<(i32, i64)>)> {
    let parts: Vec<&str> = line.trim().split('\t').collect();
    if parts.len() < 2 {
        return None;
    }
    
    let raw_token = parts[0];
    let mut pos_tags = HashSet::new();
    
    // Extract POS tags from the ORIGINAL (pre-lowercase) token,
    // since Ngram POS suffixes are uppercase (e.g. word_NOUN, word_VERB)
    // Early-exit: check if the word part (before _POS suffix) is pure ASCII alphabetic.
    // This skips year-count parsing for entries starting with digits, symbols, or non-Latin chars,
    // which dominate the first few Ngram shard files.
    let word_part = if let Some(underscore_pos) = raw_token.rfind('_') {
        let pos_part = &raw_token[underscore_pos + 1..];
        if !pos_part.is_empty() && pos_part.chars().all(|c| c.is_ascii_uppercase()) {
            &raw_token[..underscore_pos]
        } else {
            raw_token
        }
    } else {
        raw_token
    };
    if word_part.is_empty() || !word_part.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    let word = if let Some(underscore_pos) = raw_token.rfind('_') {
        let pos_part = &raw_token[underscore_pos + 1..];
        // Only treat as POS if it looks like a POS tag (all uppercase ASCII letters)
        if !pos_part.is_empty() && pos_part.chars().all(|c| c.is_ascii_uppercase()) {
            pos_tags = normalize_pos(pos_part);
            raw_token[..underscore_pos].to_lowercase()
        } else {
            raw_token.to_lowercase()
        }
    } else {
        raw_token.to_lowercase()
    };
    
    let mut year_counts = Vec::new();
    
    // Try v3 format first: each field after the word is "year,count,volumes"
    for part in &parts[1..] {
        let subparts: Vec<&str> = part.split(',').collect();
        if subparts.len() >= 2 {
            if let (Ok(year), Ok(count)) = (subparts[0].parse::<i32>(), subparts[1].parse::<i64>()) {
                year_counts.push((year, count));
            }
        } else if subparts.len() == 1 {
            // Try older format: word\tyear\tcount\t...
            // In this case parts[1] is year, parts[2] is count
            if parts.len() >= 3 {
                if let (Ok(year), Ok(count)) = (parts[1].parse::<i32>(), parts[2].parse::<i64>()) {
                    year_counts.push((year, count));
                    break; // Older format has one year per line
                }
            }
        }
    }
    
    if year_counts.is_empty() {
        return None;
    }
    
    Some((word, pos_tags, year_counts))
}

fn process_ngram_file(
    file_path: &PathBuf,
    min_year: Option<i32>,
    max_year: Option<i32>,
) -> anyhow::Result<HashMap<String, WordData>> {
    let mut word_data: HashMap<String, WordData> = HashMap::new();
    
    if let (Some(min), Some(max)) = (min_year, max_year) {
        eprintln!("Processing {:?} (years {}-{})...", file_path, min, max);
    } else if let Some(min) = min_year {
        eprintln!("Processing {:?} (years {}+)...", file_path, min);
    } else if let Some(max) = max_year {
        eprintln!("Processing {:?} (years up to {})...", file_path, max);
    } else {
        eprintln!("Processing {:?} (all years)...", file_path);
    }
    
    let file = File::open(file_path)?;
    let reader: Box<dyn BufRead> = if file_path.extension().and_then(|s| s.to_str()) == Some("gz") {
        Box::new(BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    
    let mut line_count: u64 = 0;
    let mut year_entry_count: u64 = 0;
    let mut year_filtered_count: u64 = 0;
    for line_result in reader.lines() {
        let line = line_result?;
        if let Some((word, pos_tags, year_counts)) = parse_ngram_line(&line) {
            if word.chars().all(|c| c.is_ascii_alphabetic()) {
                // Sum frequencies across year entries, applying year filter
                let mut total_freq: i64 = 0;
                for (year, count) in &year_counts {
                    year_entry_count += 1;
                    if let Some(min) = min_year {
                        if *year < min {
                            year_filtered_count += 1;
                            continue;
                        }
                    }
                    if let Some(max) = max_year {
                        if *year > max {
                            year_filtered_count += 1;
                            continue;
                        }
                    }
                    total_freq += count;
                }
                
                if total_freq > 0 {
                    let entry = word_data.entry(word).or_insert_with(|| WordData {
                        freq: 0.0,
                        pos: HashSet::new(),
                        pos_freq: HashMap::new(),
                    });
                    entry.freq += total_freq as f64;
                    // Accumulate per-POS frequencies for weight computation
                    for pos_tag in &pos_tags {
                        *entry.pos_freq.entry(pos_tag.clone()).or_insert(0.0) += total_freq as f64;
                    }
                    entry.pos.extend(pos_tags);
                }
            }
        }
        
        line_count += 1;
        if line_count % 1_000_000 == 0 {
            eprintln!("  Processed {} lines ({} year entries, {} filtered by year)...",
                line_count, year_entry_count, year_filtered_count);
        }
    }
    
    eprintln!("Found {} unique words ({} year entries, {} filtered by year)",
        word_data.len(), year_entry_count, year_filtered_count);
    Ok(word_data)
}

fn download_wordfrequency_data(force_download: bool) -> anyhow::Result<PathBuf> {
    let txt_url = "https://www.wordfrequency.info/samples/lemmas_60k.txt";
    
    // Use a persistent cache file in the current directory or temp dir
    let cache_file = std::env::current_dir()
        .ok()
        .and_then(|dir| {
            let cache = dir.join("lemmas_60k.txt");
            if cache.exists() {
                Some(cache)
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            std::env::temp_dir().join("lemmas_60k.txt")
        });
    
    // Check if cached file exists and is recent (less than 7 days old)
    if !force_download && cache_file.exists() {
        if let Ok(metadata) = std::fs::metadata(&cache_file) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = modified.elapsed() {
                    if age.as_secs() < 7 * 24 * 60 * 60 {
                        eprintln!("Using cached file: {:?}", cache_file);
                        eprintln!("Cache age: {} days", age.as_secs() / (24 * 60 * 60));
                        return Ok(cache_file);
                    }
                }
            }
        }
    }
    
    // Download the file
    eprintln!("Downloading word frequency data from wordfrequency.info...");
    eprintln!("Source: https://www.wordfrequency.info/samples.asp");
    
    let response = reqwest::blocking::get(txt_url)?;
    let content = response.text()?;
    
    std::fs::write(&cache_file, content)?;
    
    eprintln!("Downloaded and cached to {:?}", cache_file);
    eprintln!("This file will be reused for future runs (cache expires after 7 days)");
    Ok(cache_file)
}

fn parse_wordfrequency_line(line: &str) -> Option<(String, f64, HashSet<String>)> {
    // Try different separators
    for sep in &['\t', '|', ','] {
        if line.contains(*sep) {
            let parts: Vec<&str> = line.trim().split(*sep).collect();
            if parts.len() >= 4 {
                if let Ok(_rank) = parts[0].parse::<i32>() {
                    let mut word = parts[1].trim().to_lowercase();
                    let mut pos_str = parts.get(2).map(|s| s.trim().to_string()).unwrap_or_default();
                    
                    // Remove POS tags if present in word (word_POS format)
                    if let Some(underscore_pos) = word.find('_') {
                        let pos_part = word[underscore_pos + 1..].to_string();
                        word = word[..underscore_pos].to_string();
                        if pos_str.is_empty() {
                            pos_str = pos_part;
                        }
                    }
                    
                    // Parse frequency
                    let mut freq = None;
                    for part in parts.iter().skip(3) {
                        if let Ok(f) = part.trim().parse::<f64>() {
                            freq = Some(f);
                            break;
                        }
                    }
                    
                    if let Some(f) = freq {
                        let pos_tags = normalize_pos(&pos_str);
                        return Some((word, f, pos_tags));
                    }
                }
            }
        }
    }
    None
}

fn get_top_words_from_wordfrequency(file_path: &PathBuf) -> anyhow::Result<HashMap<String, WordData>> {
    let mut word_data: HashMap<String, WordData> = HashMap::new();
    
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut header_skipped = false;
    
    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        
        // Skip header lines
        if !header_skipped {
            let line_lower = line.to_lowercase();
            if line_lower.contains("rank") || line_lower.contains("lemma") || line_num < 2 {
                header_skipped = true;
                continue;
            }
        }
        
        if let Some((word, freq, pos_tags)) = parse_wordfrequency_line(&line) {
            if word.chars().all(|c| c.is_ascii_alphabetic()) {
                let entry = word_data.entry(word).or_insert_with(|| WordData {
                    freq: 0.0,
                    pos: HashSet::new(),
                    pos_freq: HashMap::new(),
                });
                if freq > entry.freq {
                    entry.freq = freq;
                }
                entry.pos.extend(pos_tags);
            }
        }
    }
    
    Ok(word_data)
}

fn get_top_words_from_csv(csv_file: &PathBuf) -> anyhow::Result<HashMap<String, WordData>> {
    let mut word_data: HashMap<String, WordData> = HashMap::new();
    
    let file = File::open(csv_file)?;
    let mut reader = csv::Reader::from_reader(file);
    let mut header_skipped = false;
    
    for result in reader.records() {
        let record: csv::StringRecord = result?;
        
        if !header_skipped {
            header_skipped = true;
            continue;
        }
        
        if record.len() >= 2 {
            let word: String = record.get(0).unwrap().trim().to_lowercase();
            if let Ok(freq) = record.get(1).unwrap().trim().parse::<f64>() {
                if word.chars().all(|c| c.is_ascii_alphabetic()) {
                    let entry = word_data.entry(word).or_insert_with(|| WordData {
                        freq: 0.0,
                        pos: HashSet::new(),
                        pos_freq: HashMap::new(),
                    });
                    if freq > entry.freq {
                        entry.freq = freq;
                    }
                }
            }
        }
    }
    
    Ok(word_data)
}

#[derive(Parser)]
#[command(
    name = "get_top_words",
    about = "Get top N most common words with 6 or fewer characters from word frequency data",
    long_about = None
)]
struct Args {
    /// Number of top words to return
    #[arg(short = 'n', long = "top-n", default_value_t = 1000)]
    top_n: usize,

    /// Output file path (default: stdout)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Path(s) to Google Books Ngram 1-gram file(s) (.gz or plain text)
    #[arg(long = "ngram", num_args = 1..)]
    ngram: Option<Vec<PathBuf>>,

    /// Path to CSV frequency file (format: word,frequency)
    #[arg(long = "csv")]
    csv: Option<PathBuf>,

    /// Path to wordfrequency.info format file (lemmas_60k.txt format)
    #[arg(long = "wordfreq")]
    wordfreq: Option<PathBuf>,

    /// Download free COCA word frequency data from wordfrequency.info
    #[arg(long = "download-coca")]
    download_coca: bool,

    /// Force re-download even if cached file exists
    #[arg(long = "force-download")]
    force_download: bool,

    /// Minimum word length (no default; omit for no minimum)
    #[arg(long = "min-length")]
    min_length: Option<usize>,

    /// Maximum word length (no default; omit for no maximum)
    #[arg(long = "max-length")]
    max_length: Option<usize>,

    /// Output words only (no POS tags), useful for piping to tag_words
    #[arg(long = "words-only")]
    words_only: bool,

    /// Minimum year to include (for Ngram data). Only aggregate entries from this year onward.
    #[arg(long = "min-year")]
    min_year: Option<i32>,

    /// Maximum year to include (for Ngram data). Only aggregate entries up to this year.
    #[arg(long = "max-year")]
    max_year: Option<i32>,

    /// Output in YAML payload format (word:\n  POS: weight) with POS weights derived from
    /// per-POS frequency ratios. Useful for generating payload.yaml files.
    #[arg(long = "yaml")]
    yaml: bool,

    /// Number of decimal places for POS weights in YAML output (default: 2)
    #[arg(long = "weight-precision", default_value_t = 2)]
    weight_precision: usize,

    /// Minimum POS weight to include in YAML output (default: 0.01).
    /// Weights below this threshold are dropped and remaining weights renormalized.
    #[arg(long = "min-weight", default_value_t = 0.01)]
    min_weight: f64,

    /// Path to a YAML file whose words should be excluded from the output.
    /// Useful for ensuring cover.yaml is disjoint from payload.yaml.
    /// The file should be in the same format as payload.yaml (word:\n  POS: weight).
    #[arg(long = "exclude-yaml", num_args = 1..)]
    exclude_yaml: Option<Vec<PathBuf>>,

    /// Enable nlprule lemmatization pass. Collapses inflected forms (runs, running, ran)
    /// into their base lemma (run), aggregating POS frequencies under the lemma.
    /// Requires nlprule data files (en_tokenizer.bin, en_rules.bin).
    #[arg(long = "lemmatize")]
    lemmatize: bool,

    /// Drop single-letter words whose only POS tag is N (noun).
    /// Keeps functional single-letter words like "a" (Det) and "I" (Pron)
    /// but removes bare alphabet letters ("b", "c", "x") that appear in Ngrams
    /// as nouns (grades, vitamins, variables, etc.).
    #[arg(long = "drop-single-letter-nouns")]
    drop_single_letter_nouns: bool,

    /// Save parsed surface forms to a bincode cache file (skips lemmatization/output).
    /// Use this to cache the expensive Ngram parse for reuse.
    #[arg(long = "save-cache")]
    save_cache: Option<PathBuf>,

    /// Load surface forms from a bincode cache file instead of parsing Ngram files.
    /// Skips the Ngram parse entirely.
    #[arg(long = "load-cache")]
    load_cache: Option<PathBuf>,

    /// Pre-filter factor: before lemmatization, keep only the top N * factor surface forms
    /// (where N = max(top_n, cover_n)). Reduces lemmatization work. Default: 8.
    #[arg(long = "pre-filter-factor", default_value_t = 8)]
    pre_filter_factor: usize,

    /// Write a cover wordlist (words NOT in the payload) to this file.
    /// The cover is generated from the same source data, excluding payload words.
    #[arg(long = "cover-output")]
    cover_output: Option<PathBuf>,

    /// Number of words in the cover wordlist. Defaults to --top-n if not specified.
    #[arg(long = "cover-n")]
    cover_n: Option<usize>,

    /// Minimum number of distinct Ngram surface forms a lemma must have
    /// to qualify for the payload wordlist. Lemmas with fewer forms are
    /// excluded from payload (they may still appear in cover as surface forms).
    #[arg(long = "min-surface-forms", default_value_t = 2)]
    min_surface_forms: usize,

    /// Path to a file with one WordNet lemma per line. When set, use Ngram data
    /// (--load-cache or --ngram) to sort these lemmas by frequency and assign
    /// POS weights from Ngram. Output YAML to -o (default: wordnet_lemmas.yaml).
    #[arg(long = "wordnet-lemmas")]
    wordnet_lemmas: Option<PathBuf>,
}

/// Compute normalized POS weights from per-POS frequency counts.
/// Returns a sorted vec of (POS, weight) pairs where weights sum to 1.0.
/// Weights below `min_weight` are dropped and remaining weights renormalized.
/// If `pos_freq` is empty but `pos_set` is not, falls back to equal weights.
fn compute_pos_weights(
    pos_freq: &HashMap<String, f64>,
    pos_set: &HashSet<String>,
    min_weight: f64,
    precision: usize,
) -> Vec<(String, f64)> {
    let mut weights: Vec<(String, f64)> = if !pos_freq.is_empty() {
        // Compute weights from frequency ratios
        let total: f64 = pos_freq.values().sum();
        if total == 0.0 {
            return Vec::new();
        }
        pos_freq.iter()
            .map(|(pos, freq)| (pos.clone(), freq / total))
            .collect()
    } else if !pos_set.is_empty() {
        // Fall back to equal weights
        let w = 1.0 / pos_set.len() as f64;
        pos_set.iter().map(|pos| (pos.clone(), w)).collect()
    } else {
        return Vec::new();
    };
    
    // Filter out weights below threshold
    weights.retain(|(_, w)| *w >= min_weight);
    
    // Renormalize
    let total: f64 = weights.iter().map(|(_, w)| *w).sum();
    if total > 0.0 {
        for (_, w) in &mut weights {
            *w /= total;
        }
    }
    
    // Round to desired precision
    let factor = 10f64.powi(precision as i32);
    for (_, w) in &mut weights {
        *w = (*w * factor).round() / factor;
    }
    
    // Fix rounding errors: adjust largest weight so they sum to 1.0
    let sum: f64 = weights.iter().map(|(_, w)| *w).sum();
    let diff = 1.0 - sum;
    if diff.abs() > 0.0001 && !weights.is_empty() {
        // Find the largest weight and adjust it
        if let Some(max_idx) = weights.iter().enumerate().max_by(|a, b| a.1.1.partial_cmp(&b.1.1).unwrap()).map(|(i, _)| i) {
            weights[max_idx].1 = ((weights[max_idx].1 + diff) * factor).round() / factor;
        }
    }
    
    // Sort alphabetically by POS tag
    weights.sort_by(|a, b| a.0.cmp(&b.0));
    weights
}

/// Load a word list from a file (one word per line, lowercased, alphabetic only).
fn load_word_list(path: &PathBuf) -> anyhow::Result<Vec<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut words = Vec::new();
    for line_result in reader.lines() {
        let line = line_result?;
        let w = line.trim().to_lowercase();
        if w.is_empty() || !w.chars().all(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        words.push(w);
    }
    Ok(words)
}

/// Load word keys from a YAML file (payload.yaml format: "word:\n  POS: weight").
/// Returns the set of lowercased words found as top-level keys.
fn load_yaml_word_keys(path: &PathBuf) -> anyhow::Result<HashSet<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut words = HashSet::new();
    
    for line_result in reader.lines() {
        let line = line_result?;
        let trimmed = line.trim();
        // A top-level YAML key looks like "word:" at column 0 (no leading spaces)
        // Skip comments and blank lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            if let Some(word) = trimmed.strip_suffix(':') {
                let word = word.trim().to_lowercase();
                // Strip surrounding quotes (single or double) for YAML-reserved words
                // like 'true', 'false', "null", etc.
                let word = word.strip_prefix('\'').unwrap_or(&word);
                let word = word.strip_suffix('\'').unwrap_or(word);
                let word = word.strip_prefix('"').unwrap_or(word);
                let word = word.strip_suffix('"').unwrap_or(word);
                if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphabetic()) {
                    words.insert(word.to_string());
                }
            }
        }
    }
    
    eprintln!("Loaded {} words to exclude from {:?}", words.len(), path);
    Ok(words)
}

/// Convert nlprule POS tags (Penn Treebank) to our simplified format.
/// Same mapping used in tag_words.rs and validate_pos_weights.rs.
fn normalize_nlprule_pos(nlprule_tag: &str) -> Option<&'static str> {
    match nlprule_tag {
        // Nouns
        "NN" | "NNS" | "NNP" | "NNPS" => Some("N"),
        // Verbs
        "VB" | "VBD" | "VBG" | "VBN" | "VBP" | "VBZ" => Some("V"),
        // Adjectives
        "JJ" | "JJR" | "JJS" => Some("Adj"),
        // Adverbs
        "RB" | "RBR" | "RBS" => Some("Adv"),
        // Prepositions
        "IN" => Some("Prep"),
        // Determiners
        "DT" => Some("Det"),
        // Conjunctions
        "CC" => Some("Conj"),
        // Pronouns
        "PRP" | "PRP$" | "WP" | "WP$" => Some("Pron"),
        _ => None,
    }
}

/// For a given word, build a mapping from our simplified POS tag to the nlprule lemma.
/// E.g. for "running": {"V" -> "run", "N" -> "running"}
fn get_pos_lemma_map(checker: &GrammarChecker, word: &str) -> HashMap<String, String> {
    let mut pos_to_lemma: HashMap<String, String> = HashMap::new();

    // Put the word in a simple sentence context so nlprule can tokenize it.
    // The dictionary lookup returns all possible (POS, lemma) pairs regardless of context.
    let sentence = format!("The {} is good.", word);
    for sent in checker.tokenize(&sentence) {
        for token in sent.tokens() {
            let token_text = token.word().as_str().to_lowercase();
            if token_text == word.to_lowercase() {
                for tag in token.word().tags() {
                    let pos = tag.pos().as_str();
                    let lemma = tag.lemma().as_str().to_lowercase();
                    if let Some(normalized_pos) = normalize_nlprule_pos(pos) {
                        // Only accept lemmas that are pure alphabetic
                        if !lemma.is_empty() && lemma.chars().all(|c| c.is_ascii_alphabetic()) {
                            // Keep the first lemma we find per POS (they're from the dictionary)
                            pos_to_lemma.entry(normalized_pos.to_string())
                                .or_insert(lemma);
                        }
                    }
                }
            }
        }
    }

    pos_to_lemma
}

/// Lemmatize word_data by collapsing inflected forms into their base lemma.
///
/// For each word, nlprule provides a POS->lemma mapping. We split the word's
/// per-POS frequencies and route each to the appropriate lemma. For example:
///   "running" {V: 1000, N: 200} → nlprule says V->"run", N->"running"
///   So "run" gets V:1000 added, "running" keeps N:200.
///
/// The nlprule lookups are parallelized with rayon; the merge step is sequential.
/// Returns (lemma_data, surface_form_counts) where surface_form_counts maps each
/// lemma to the number of distinct surface forms that collapsed into it.
fn lemmatize_word_data(
    checker: &GrammarChecker,
    word_data: HashMap<String, WordData>,
) -> (HashMap<String, WordData>, HashMap<String, usize>) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let total = word_data.len();
    let progress = AtomicUsize::new(0);

    // Phase 1: parallel nlprule lookups — produce (word, data, pos_lemma_map) triples
    let word_vec: Vec<(String, WordData)> = word_data.into_iter().collect();
    let lookups: Vec<(String, WordData, HashMap<String, String>)> = word_vec
        .into_par_iter()
        .map(|(word, data)| {
            let count = progress.fetch_add(1, Ordering::Relaxed) + 1;
            if count % 5000 == 0 {
                eprintln!("  Lemmatizing {}/{}...", count, total);
            }
            let pos_lemma_map = get_pos_lemma_map(checker, &word);
            (word, data, pos_lemma_map)
        })
        .collect();

    // Phase 2: sequential merge into lemma_data and track surface forms per lemma
    let mut lemma_data: HashMap<String, WordData> = HashMap::new();
    let mut surface_forms: HashMap<String, HashSet<String>> = HashMap::new();
    let mut collapsed = 0u64;

    for (word, data, pos_lemma_map) in lookups {
        if data.pos_freq.is_empty() {
            // No per-POS breakdown: pick the best lemma we can find, or keep the word
            let lemma = pos_lemma_map.values().next()
                .cloned()
                .unwrap_or_else(|| word.clone());
            if lemma != word {
                collapsed += 1;
            }
            surface_forms.entry(lemma.clone()).or_default().insert(word.clone());
            let entry = lemma_data.entry(lemma).or_insert_with(|| WordData {
                freq: 0.0,
                pos: HashSet::new(),
                pos_freq: HashMap::new(),
            });
            entry.freq += data.freq;
            entry.pos.extend(data.pos);
        } else {
            // Route each POS's frequency to the appropriate lemma
            for (pos, freq) in &data.pos_freq {
                let lemma = pos_lemma_map.get(pos)
                    .cloned()
                    .unwrap_or_else(|| word.clone());
                if lemma != word {
                    collapsed += 1;
                }
                surface_forms.entry(lemma.clone()).or_default().insert(word.clone());
                let entry = lemma_data.entry(lemma).or_insert_with(|| WordData {
                    freq: 0.0,
                    pos: HashSet::new(),
                    pos_freq: HashMap::new(),
                });
                entry.freq += freq;
                entry.pos.insert(pos.clone());
                *entry.pos_freq.entry(pos.clone()).or_insert(0.0) += freq;
            }
        }
    }

    let surface_form_counts: HashMap<String, usize> = surface_forms
        .into_iter()
        .map(|(lemma, set)| (lemma, set.len()))
        .collect();

    eprintln!("Lemmatization complete: {} surface forms → {} lemmas ({} forms collapsed)",
        total, lemma_data.len(), collapsed);
    (lemma_data, surface_form_counts)
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    let mut word_data: HashMap<String, WordData>;
    word_data = HashMap::new();
    
    // --load-cache: deserialize from bincode, skipping all parsing
    if let Some(cache_path) = &args.load_cache {
        eprintln!("Loading surface forms from cache: {:?}", cache_path);
        let file = File::open(cache_path)?;
        let reader = BufReader::new(file);
        word_data = bincode::deserialize_from(reader)?;
        eprintln!("Loaded {} surface forms from cache", word_data.len());
    } else if let Some(ngram_files) = &args.ngram {
        for f in ngram_files.iter().filter(|f| !f.as_path().exists()) {
            eprintln!("Error: File not found: {:?}", f);
        }
        let min_year = args.min_year;
        let max_year = args.max_year;
        let results: Vec<anyhow::Result<HashMap<String, WordData>>> = ngram_files
            .par_iter()
            .filter(|f| f.as_path().exists())
            .map(|f| process_ngram_file(f, min_year, max_year))
            .collect();
        for result in results {
            let file_data = result?;
            for (word, data) in file_data {
                let entry = word_data.entry(word).or_insert_with(|| WordData {
                    freq: 0.0,
                    pos: HashSet::new(),
                    pos_freq: HashMap::new(),
                });
                entry.freq += data.freq;
                entry.pos.extend(data.pos);
                for (pos_tag, freq) in data.pos_freq {
                    *entry.pos_freq.entry(pos_tag).or_insert(0.0) += freq;
                }
            }
        }
        if word_data.is_empty() {
            eprintln!("No words found in Ngram files.");
            std::process::exit(1);
        }
    } else if args.download_coca {
        // Download COCA word frequency data (cached locally)
        let cache_file = download_wordfrequency_data(args.force_download)?;
        word_data = get_top_words_from_wordfrequency(&cache_file)?;
        // Keep the cache file for future use
    } else if let Some(wordfreq_file) = args.wordfreq {
        // Use wordfrequency.info format file
        eprintln!("Reading wordfrequency.info file: {:?}", wordfreq_file);
        word_data = get_top_words_from_wordfrequency(&wordfreq_file)?;
    } else if let Some(csv_file) = args.csv {
        // Use CSV frequency file
        eprintln!("Reading CSV file: {:?}", csv_file);
        word_data = get_top_words_from_csv(&csv_file)?;
    } else if args.wordnet_lemmas.is_some() {
        eprintln!("Error: When using --wordnet-lemmas, specify --load-cache or --ngram for Ngram data.");
        eprintln!("Example: get_top_words --load-cache data/ngram-eng-cache.bin --wordnet-lemmas languages/english/wordnet_lemmas.txt --yaml -o languages/english/wordnet_lemmas.yaml");
        std::process::exit(1);
    } else {
        eprintln!("Error: Must specify one of: --download-coca, --wordfreq, --csv, or --ngram");
        eprintln!("\nExamples:");
        eprintln!("  # Download and use COCA word frequency data (recommended)");
        eprintln!("  cargo run --bin get_top_words -- -n 1000 --download-coca -o output.txt");
        eprintln!("\n  # Use a wordfrequency.info format file");
        eprintln!("  cargo run --bin get_top_words -- -n 1000 --wordfreq lemmas_60k.txt -o output.txt");
        eprintln!("\n  # Use a CSV frequency file");
        eprintln!("  cargo run --bin get_top_words -- -n 1000 --csv word-freq.csv -o output.txt");
        eprintln!("\n  # Use Google Ngram data filtered to 20th century+");
        eprintln!("  cargo run --bin get_top_words -- -n 100000 --ngram googlebooks-eng-all-1gram-*.gz --min-year 1900 -o output.txt");
        eprintln!("\n  # Use Ngram data with lemmatization (collapse inflected forms)");
        eprintln!("  cargo run --bin get_top_words -- -n 50000 --ngram googlebooks-eng-all-1gram-*.gz --min-year 1900 --lemmatize --yaml -o lemmas.yaml");
        std::process::exit(1);
    }
    
    if word_data.is_empty() {
        eprintln!("No words found. Check your input files.");
        std::process::exit(1);
    }
    
    // --wordnet-lemmas: filter to lemma list, sort by Ngram frequency, write YAML and exit
    if let Some(lemma_path) = &args.wordnet_lemmas {
        eprintln!("Loading WordNet lemma list from {:?}...", lemma_path);
        let lemma_list = load_word_list(lemma_path)?;
        let lemma_set: HashSet<String> = lemma_list.into_iter().collect();
        eprintln!("{} lemmas in list; looking up in Ngram data...", lemma_set.len());
        let filtered: HashMap<String, WordData> = word_data
            .into_iter()
            .filter(|(k, _)| lemma_set.contains(k))
            .collect();
        let mut sorted: Vec<(String, WordData)> = filtered.into_iter().collect();
        sorted.sort_by(|a, b| b.1.freq.partial_cmp(&a.1.freq).unwrap());
        let output_path = args.output.clone().unwrap_or_else(|| PathBuf::from("wordnet_lemmas.yaml"));
        eprintln!("Writing {} lemmas (by Ngram frequency) to {:?}", sorted.len(), output_path);
        let mut out = File::create(&output_path)?;
        let yaml_reserved: HashSet<&str> = ["true", "false", "yes", "no", "on", "off", "null"]
            .iter().cloned().collect();
        writeln!(out, "# WordNet lemmas with Ngram frequency order and POS weights")?;
        writeln!(out, "# Generated by get_top_words --wordnet-lemmas with --load-cache / --ngram")?;
        writeln!(out, "# Total: {}", sorted.len())?;
        if let (Some(first), Some(last)) = (sorted.first(), sorted.last()) {
            writeln!(out, "# Frequency range: {:.0} - {:.0}", last.1.freq, first.1.freq)?;
        }
        writeln!(out, "#")?;
        for (word, data) in &sorted {
            let weights = compute_pos_weights(&data.pos_freq, &data.pos, args.min_weight, args.weight_precision);
            let yaml_key = if yaml_reserved.contains(word.as_str()) {
                format!("'{}'", word)
            } else {
                word.clone()
            };
            if weights.is_empty() {
                writeln!(out, "{}:", yaml_key)?;
                writeln!(out, "  N: 1.0")?;
            } else {
                writeln!(out, "{}:", yaml_key)?;
                for (pos, weight) in &weights {
                    writeln!(out, "  {}: {}", pos, weight)?;
                }
            }
        }
        eprintln!("Done.");
        return Ok(());
    }
    
    // --save-cache: serialize surface forms to bincode and exit early
    if let Some(cache_path) = &args.save_cache {
        eprintln!("Saving {} surface forms to cache: {:?}", word_data.len(), cache_path);
        let file = File::create(cache_path)?;
        let writer = std::io::BufWriter::new(file);
        bincode::serialize_into(writer, &word_data)?;
        eprintln!("Cache saved.");
        return Ok(());
    }
    
    // Pre-filter: before the expensive lemmatization, keep only the top N * factor
    // surface forms (where N = max(top_n, cover_n)).
    if args.lemmatize {
        let cover_n = args.cover_n.unwrap_or(args.top_n);
        let pre_filter_size = args.top_n.max(cover_n) * args.pre_filter_factor;
        if word_data.len() > pre_filter_size {
            eprintln!("Pre-filtering: keeping top {} of {} surface forms...",
                pre_filter_size, word_data.len());
            let mut sorted: Vec<_> = word_data.into_iter().collect();
            sorted.sort_by(|a, b| b.1.freq.partial_cmp(&a.1.freq).unwrap());
            sorted.truncate(pre_filter_size);
            word_data = sorted.into_iter().collect();
        }
    }
    
    // Clone surface forms BEFORE lemmatization for cover generation
    let surface_forms_for_cover: Option<HashMap<String, WordData>> = if args.cover_output.is_some() {
        Some(word_data.clone())
    } else {
        None
    };
    
    // Optional lemmatization pass: collapse inflected forms into lemmas
    let mut surface_form_counts: Option<HashMap<String, usize>> = None;
    if args.lemmatize {
        eprintln!("\nLoading nlprule tokenizer for lemmatization...");
        let checker = match GrammarChecker::from_language(glossia::Language::English) {
            Ok(checker) => checker,
            Err(e) => {
                eprintln!("Error: Could not load nlprule data files for lemmatization.");
                eprintln!("Please ensure en_tokenizer.bin and en_rules.bin are available.");
                eprintln!("They should be in: current directory, data/, /app/data/, or /opt/nlprule-data/");
                eprintln!("\nError details: {}", e);
                return Err(e);
            }
        };
        eprintln!("Lemmatizing {} words...", word_data.len());
        let (lemma_data, counts) = lemmatize_word_data(&checker, word_data);
        word_data = lemma_data;
        surface_form_counts = Some(counts);
    }
    
    // Load excluded words from YAML file(s)
    let mut excluded_words: HashSet<String> = HashSet::new();
    if let Some(exclude_files) = &args.exclude_yaml {
        for exclude_file in exclude_files {
            match load_yaml_word_keys(exclude_file) {
                Ok(words) => excluded_words.extend(words),
                Err(e) => eprintln!("Warning: Could not load exclude file {:?}: {}", exclude_file, e),
            }
        }
        if !excluded_words.is_empty() {
            eprintln!("Total excluded words: {}", excluded_words.len());
        }
    }
    
    // Filter for words by length, no punctuation, and not in exclusion set.
    // When lemmatized with surface_form_counts: prefer lemmas with >= min_surface_forms,
    // then fill to top_n with single-form lemmas so we hit the target size (e.g. 2^17).
    let (mut multi_form, mut single_form): (Vec<(String, WordData)>, Vec<(String, WordData)>) = word_data
        .into_iter()
        .filter(|(w, data)| {
            let len = w.len();
            let ok_length = args.min_length.map_or(true, |min| len >= min)
                && args.max_length.map_or(true, |max| len <= max);
            let ok_alpha = w.chars().all(|c| c.is_ascii_alphabetic());
            let ok_excluded = !excluded_words.contains(w);
            let ok_single_letter = !(args.drop_single_letter_nouns
                && len == 1
                && data.pos.len() == 1
                && data.pos.contains("N"));
            let ok_not_cover_only = args.cover_output.is_none()
                || !COVER_ONLY_LEMMAS.contains(&w.as_str());
            ok_length && ok_alpha && ok_excluded && ok_single_letter && ok_not_cover_only
        })
        .partition(|(w, _)| {
            match &surface_form_counts {
                Some(counts) => counts.get(w).map_or(false, |&c| c >= args.min_surface_forms),
                None => true,
            }
        });

    if let Some(ref counts) = surface_form_counts {
        eprintln!("Payload: {} lemmas with >={} surface forms, {} single-form (filling to -n {})",
            multi_form.len(), args.min_surface_forms, single_form.len(), args.top_n);
    }
    let mut sorted_words: Vec<(String, WordData)> = multi_form;
    sorted_words.append(&mut single_form);
    sorted_words.sort_by(|a, b| b.1.freq.partial_cmp(&a.1.freq).unwrap());
    sorted_words.truncate(args.top_n);
    
    // Output results
    let output_path = args.output.clone();
    let mut output: Box<dyn Write> = if let Some(ref path) = output_path {
        Box::new(File::create(path)?)
    } else {
        Box::new(io::stdout())
    };
    
    // Collect POS distribution stats for YAML header
    let mut pos_distribution: HashMap<String, usize> = HashMap::new();
    
    if args.yaml {
        // YAML payload format with POS weights
        // Write header comment
        let freq_min = sorted_words.last().map(|(_, d)| d.freq).unwrap_or(0.0);
        let freq_max = sorted_words.first().map(|(_, d)| d.freq).unwrap_or(0.0);
        let freq_avg: f64 = if sorted_words.is_empty() {
            0.0
        } else {
            sorted_words.iter().map(|(_, d)| d.freq).sum::<f64>() / sorted_words.len() as f64
        };
        
        // Count POS distribution
        for (_, data) in &sorted_words {
            for pos in data.pos.iter() {
                *pos_distribution.entry(pos.clone()).or_insert(0) += 1;
            }
        }
        
        writeln!(output, "# English Payload Wordlist for Glossia")?;
        if args.lemmatize {
            writeln!(output, "# Lemmas with multiple inflected forms; carry encrypted payload data.")?;
            writeln!(output, "# Generated by get_top_words (Google Ngram v3 + nlprule lemmatization)")?;
        } else {
            writeln!(output, "# Generated by get_top_words (Google Ngram v3)")?;
        }
        writeln!(output, "#")?;
        if let Some(min) = args.min_year {
            writeln!(output, "# Year filter: {} - {}", min, args.max_year.unwrap_or(2019))?;
        }
        match (args.min_length, args.max_length) {
            (Some(min), Some(max)) => writeln!(output, "# Word length: {} - {}", min, max)?,
            (Some(min), None) => writeln!(output, "# Word length: {}+", min)?,
            (None, Some(max)) => writeln!(output, "# Word length: up to {}", max)?,
            (None, None) => writeln!(output, "# Word length: unrestricted")?,
        }
        writeln!(output, "#")?;
        writeln!(output, "# Statistics:")?;
        writeln!(output, "#   Total words: {:?}", sorted_words.len())?;
        writeln!(output, "#   Frequency range: {:.0} - {:.0}", freq_min, freq_max)?;
        writeln!(output, "#   Average frequency: {:.1}", freq_avg)?;
        writeln!(output, "#")?;
        writeln!(output, "# POS Tag Distribution:")?;
        let mut pos_dist_sorted: Vec<_> = pos_distribution.iter().collect();
        pos_dist_sorted.sort_by_key(|(k, _)| (*k).clone());
        for (pos, count) in &pos_dist_sorted {
            let pct = **count as f64 / sorted_words.len() as f64 * 100.0;
            writeln!(output, "#   {}: {} ({:.1}%)", pos, count, pct)?;
        }
        writeln!(output, "#")?;
        writeln!(output, "# Wordlist (sorted by frequency, most frequent first):")?;
        writeln!(output, "#")?;
        
        // YAML reserved words that need quoting
        let yaml_reserved: HashSet<&str> = ["true", "false", "yes", "no", "on", "off", "null"]
            .iter().cloned().collect();
        
        for (word, data) in &sorted_words {
            // Compute POS weights from per-POS frequencies
            let weights = compute_pos_weights(&data.pos_freq, &data.pos, args.min_weight, args.weight_precision);
            
            // Quote YAML-reserved words to prevent them being parsed as booleans/null
            let yaml_key = if yaml_reserved.contains(word.as_str()) {
                format!("'{}'", word)
            } else {
                word.clone()
            };
            
            if weights.is_empty() {
                // No POS info — output with empty map (word will still be in the list)
                writeln!(output, "{}:", yaml_key)?;
                writeln!(output, "  N: 1.0")?;  // Default to noun
            } else {
                writeln!(output, "{}:", yaml_key)?;
                for (pos, weight) in &weights {
                    writeln!(output, "  {}: {}", pos, weight)?;
                }
            }
        }
    } else {
        // cover_POS.txt format
        for (word, data) in &sorted_words {
            if args.words_only {
                writeln!(output, "{}", word)?;
            } else {
                let mut pos_tags: Vec<String> = data.pos.iter().cloned().collect();
                pos_tags.sort();
                
                if !pos_tags.is_empty() {
                    let pos_str = pos_tags.join(",");
                    writeln!(output, "{}|{}", word, pos_str)?;
                } else {
                    writeln!(output, "{}", word)?;
                }
            }
        }
    }
    
    if let Some(ref path) = output_path {
        let words_with_pos = sorted_words.iter().filter(|(_, d)| !d.pos.is_empty()).count();
        eprintln!("\nTop {} words saved to {:?}", sorted_words.len(), path);
        eprintln!("Words with POS tags: {}/{}", words_with_pos, sorted_words.len());
        if let Some((_, last_data)) = sorted_words.last() {
            if let Some((_, first_data)) = sorted_words.first() {
                eprintln!(
                    "Frequency range: {:.0} to {:.0}",
                    last_data.freq,
                    first_data.freq
                );
            }
        }
        if !pos_distribution.is_empty() {
            let mut pos_dist_sorted: Vec<_> = pos_distribution.iter().collect();
            pos_dist_sorted.sort_by_key(|(k, _)| (*k).clone());
            eprintln!("POS distribution:");
            for (pos, count) in pos_dist_sorted {
                eprintln!("  {}: {}", pos, count);
            }
        }
    } else {
        eprintln!("\nTotal words: {}", sorted_words.len());
    }
    
    // --- Cover generation ---
    // Generate cover wordlist from the pre-lemmatization surface forms,
    // excluding exact payload lemma strings (disjoint wordlists for decoder).
    if let (Some(cover_path), Some(surface_forms)) = (&args.cover_output, surface_forms_for_cover) {
        let cover_n = args.cover_n.unwrap_or(args.top_n);
        
        // Collect the set of payload lemma strings to exclude from cover
        let payload_lemma_set: HashSet<String> = sorted_words.iter()
            .map(|(w, _)| w.clone())
            .collect();
        eprintln!("\nGenerating cover wordlist (excluding {} payload lemma strings)...", payload_lemma_set.len());
        
        // Filter surface forms: exclude exact payload strings, apply length/exclusion/single-letter-noun filters
        let cover_filtered: HashMap<String, WordData> = surface_forms
            .into_iter()
            .filter(|(w, data)| {
                let len = w.len();
                !payload_lemma_set.contains(w)
                    && args.min_length.map_or(true, |min| len >= min)
                    && args.max_length.map_or(true, |max| len <= max)
                    && w.chars().all(|c| c.is_ascii_alphabetic())
                    && !excluded_words.contains(w)
                    && !(args.drop_single_letter_nouns
                         && len == 1
                         && data.pos.len() == 1
                         && data.pos.contains("N"))
            })
            .collect();
        
        // Sort by frequency and take top cover_n
        let mut cover_sorted: Vec<(String, WordData)> = cover_filtered.into_iter().collect();
        cover_sorted.sort_by(|a, b| b.1.freq.partial_cmp(&a.1.freq).unwrap());
        cover_sorted.truncate(cover_n);
        
        // Write cover file in YAML format
        let mut cover_out: Box<dyn Write> = Box::new(File::create(cover_path)?);
        
        let yaml_reserved: HashSet<&str> = ["true", "false", "yes", "no", "on", "off", "null"]
            .iter().cloned().collect();
        
        // Cover header
        writeln!(cover_out, "# English Cover Wordlist for Glossia")?;
        writeln!(cover_out, "# Surface forms for grammar filler and merkle proofs.")?;
        writeln!(cover_out, "# Generated by get_top_words (surface forms, disjoint from payload lemmas)")?;
        writeln!(cover_out, "#")?;
        writeln!(cover_out, "# Total words: {}", cover_sorted.len())?;
        if let (Some(last), Some(first)) = (cover_sorted.last(), cover_sorted.first()) {
            writeln!(cover_out, "# Frequency range: {:.0} - {:.0}", last.1.freq, first.1.freq)?;
        }
        writeln!(cover_out, "#")?;
        
        for (word, data) in &cover_sorted {
            let weights = compute_pos_weights(&data.pos_freq, &data.pos, args.min_weight, args.weight_precision);
            
            let yaml_key = if yaml_reserved.contains(word.as_str()) {
                format!("'{}'", word)
            } else {
                word.clone()
            };
            
            if weights.is_empty() {
                writeln!(cover_out, "{}:", yaml_key)?;
                writeln!(cover_out, "  N: 1.0")?;
            } else {
                writeln!(cover_out, "{}:", yaml_key)?;
                for (pos, weight) in &weights {
                    writeln!(cover_out, "  {}: {}", pos, weight)?;
                }
            }
        }
        
        // Verify zero overlap
        let overlap: Vec<_> = cover_sorted.iter()
            .filter(|(w, _)| payload_lemma_set.contains(w))
            .map(|(w, _)| w.clone())
            .collect();
        if !overlap.is_empty() {
            eprintln!("WARNING: {} words overlap between payload and cover!", overlap.len());
        }
        
        eprintln!("Cover: {} words saved to {:?} (0 overlap with payload)", cover_sorted.len(), cover_path);
    }
    
    Ok(())
}
