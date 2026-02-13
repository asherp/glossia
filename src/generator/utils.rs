use regex::Regex;
use crate::types::Pos;
use crate::generator::types::{PayloadTok, GenerationMode};
use crate::grammar::{Grammar, DialectConfig};

/// Check if a payload token can fit in a given POS slot
pub fn payload_fits(tok: &PayloadTok, slot: Pos) -> bool {
    // Since BIP39 words now have only one POS tag, strict matching only
    tok.allowed.contains(&slot)
}

/// Get the grammar instance for the given mode and language
/// Note: Returns owned Grammar since we now support per-language grammars
pub fn get_grammar(mode: GenerationMode, language: &str) -> Grammar {
    let dialect = match mode {
        GenerationMode::Subject => "subject",
        GenerationMode::Body => "body",
        GenerationMode::PayloadOnly => "payload_only",
    };
    Grammar::from_language_dialect(language, dialect)
        .expect(&format!("Failed to load {} grammar for language {}", dialect, language))
}

/// Map GenerationMode to dialect name string.
pub fn mode_to_dialect(mode: GenerationMode) -> &'static str {
    match mode {
        GenerationMode::Subject => "subject",
        GenerationMode::Body => "body",
        GenerationMode::PayloadOnly => "payload_only",
    }
}

/// Get the full dialect configuration (grammar + wordlist refs) for the given mode and language.
///
/// This is the preferred entry point when you need both the grammar and the
/// dialect-specified wordlists. Use `with_payload_wordlist()` on the result
/// to apply a CLI `--wordlist` override.
pub fn get_dialect_config(mode: GenerationMode, language: &str) -> DialectConfig {
    let dialect = mode_to_dialect(mode);
    DialectConfig::from_language_dialect(language, dialect)
        .expect(&format!("Failed to load {} dialect config for language {}", dialect, language))
}

/// Get the start nonterminal for a given POS
pub fn start_nonterminal_for_pos(pos: Pos) -> &'static str {
    // Body grammar uses only "S" with weighted alternatives
    // Subject grammar may still have S_* variants, but we default to "S" for simplicity
    // The planner will find sequences that match naturally
    match pos {
        Pos::N | Pos::V | Pos::Adj | Pos::Adv | Pos::Prep | Pos::Det |
        Pos::Modal | Pos::Aux | Pos::Cop | Pos::To | Pos::Conj | Pos::Dot |
        Pos::Prefix | Pos::Pron => "S",
    }
}

/// Capitalize the first letter of a string
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Normalize tokens for payload word matching.
/// Supports both alphabetic words (BIP39) and numeric words (primes, etc.).
/// Strip ANSI escape codes (e.g., \x1b[1m, \x1b[0m), highlighting bars (|), and punctuation.
pub fn normalize_token_for_bip39(s: &str) -> String {
    let mut result = s.to_string();
    
    // Remove ANSI escape codes: ESC[ followed by digits and 'm'
    // Pattern: \x1b[ or ESC[ followed by optional digits and 'm'
    result = Regex::new(r"\x1b\[[0-9;]*m")
        .unwrap()
        .replace_all(&result, "")
        .to_string();
    
    // Remove highlighting bars
    result = result.replace('|', "");
    
    // Strip leading/trailing punctuation/quotes, but preserve alphanumeric content
    result = result.trim()
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_lowercase();
    
    result
}

/// Check if a word starts with a vowel sound (needed for a/an selection).
pub fn starts_with_vowel_sound(word: &str) -> bool {
    // Normalize first so highlighting, bars, punctuation, and case don't break a/an selection.
    let normalized = normalize_token_for_bip39(word);
    if normalized.is_empty() {
        return false;
    }
    let first_char = normalized.chars().next().unwrap();
    matches!(first_char, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// Heuristic: "bare" verb form appropriate after a modal (e.g., "can go", not "can going/goed/goes").
pub fn is_bare_verb_form(word: &str) -> bool {
    let w = normalize_token_for_bip39(word).to_lowercase();
    if w.is_empty() {
        return false;
    }
    // Very cheap morphology filter: exclude common inflections.
    if w.ends_with("ing") || w.ends_with("ed") || w.ends_with("s") {
        // Allow a couple of irregular bare forms that end with 's' in writing? (none for modals)
        return false;
    }
    true
}

/// Tiny transitivity heuristic for common cover verbs.
/// We only use this to bias *cover word selection* in `V NP` contexts.
pub fn is_likely_transitive_verb(word: &str) -> bool {
    let w = normalize_token_for_bip39(word).to_lowercase();
    // Small whitelist: keep it short and safe; we can expand later.
    matches!(
        w.as_str(),
        "add"
            | "bring"
            | "build"
            | "call"
            | "change"
            | "check"
            | "close"
            | "create"
            | "deliver"
            | "find"
            | "fix"
            | "get"
            | "give"
            | "help"
            | "hold"
            | "keep"
            | "leave"
            | "make"
            | "need"
            | "open"
            | "put"
            | "read"
            | "save"
            | "see"
            | "send"
            | "set"
            | "show"
            | "take"
            | "tell"
            | "use"
            | "verify"
            | "write"
    )
}

/// Pluralize a cover noun (conservative, ASCII-only rules)
pub fn pluralize_cover_noun(s: &str) -> String {
    // Conservative, ASCII-only rules are fine here because cover lexicon is lowercase ASCII.
    if s.is_empty() {
        return String::new();
    }
    // If it's already plural-ish, leave it (avoid "classs").
    if s.ends_with('s') {
        return s.to_string();
    }
    if s.ends_with("ch") || s.ends_with("sh") || s.ends_with('x') || s.ends_with('z') {
        return format!("{s}es");
    }
    if s.ends_with('y') && s.len() >= 2 {
        let prev = s.as_bytes()[s.len() - 2] as char;
        if !matches!(prev, 'a' | 'e' | 'i' | 'o' | 'u') {
            return format!("{}ies", &s[..s.len() - 1]);
        }
    }
    format!("{s}s")
}

/// Surround the word token with | | while keeping trailing punctuation outside the bars.
/// Example: "abandon." -> "|abandon|."
/// Example: "Abandon"  -> "|Abandon|"
pub fn wrap_payload_with_bars(word_with_punct: &str) -> String {
    let mut core = word_with_punct.to_string();
    let mut suffix = String::new();
    while let Some(last) = core.chars().last() {
        if last.is_ascii_alphabetic() {
            break;
        }
        core.pop();
        suffix.insert(0, last);
    }
    format!("|{core}|{suffix}")
}

/// Highlight the word using ANSI escape codes while keeping trailing punctuation outside.
/// Example: "abandon." -> "\x1b[32mabandon\x1b[0m."
/// Example: "Abandon"  -> "\x1b[32mAbandon\x1b[0m"
/// ANSI color codes: 30 = black, 31 = red, 32 = green, 33 = yellow, 34 = blue, 35 = magenta, 36 = cyan, 37 = white
pub fn wrap_payload_with_color(word_with_punct: &str, color_code: u8) -> String {
    let mut core = word_with_punct.to_string();
    let mut suffix = String::new();
    while let Some(last) = core.chars().last() {
        if last.is_ascii_alphabetic() {
            break;
        }
        core.pop();
        suffix.insert(0, last);
    }
    format!("\x1b[{}m{core}\x1b[0m{suffix}", color_code)
}

/// Word wrap text to specified width, preserving sentence boundaries
pub fn word_wrap(text: &str, width: usize) -> String {
    let mut result = Vec::new();
    let mut current_line = String::new();
    
    // Split by sentences first to preserve sentence boundaries
    let sentences: Vec<&str> = text.split_inclusive('.').collect();
    
    for sentence in sentences {
        let trimmed_sentence = sentence.trim();
        if trimmed_sentence.is_empty() {
            continue;
        }
        
        // If adding this sentence would exceed width, start a new line
        if !current_line.is_empty() {
            let test_line = format!("{} {}", current_line, trimmed_sentence);
            if test_line.len() > width {
                result.push(current_line.clone());
                current_line = trimmed_sentence.to_string();
            } else {
                current_line = test_line;
            }
        } else {
            current_line = trimmed_sentence.to_string();
        }
        
        // If current line exceeds width, wrap it word by word
        if current_line.len() > width {
            let words: Vec<String> = current_line.split_whitespace().map(|s| s.to_string()).collect();
            current_line.clear();
            
            for word in words {
                if current_line.is_empty() {
                    current_line = word;
                } else {
                    let test_line = format!("{} {}", current_line, word);
                    if test_line.len() > width {
                        result.push(current_line.clone());
                        current_line = word;
                    } else {
                        current_line = test_line;
                    }
                }
            }
        }
    }
    
    // Add any remaining line
    if !current_line.is_empty() {
        result.push(current_line);
    }
    
    result.join("\n")
}
