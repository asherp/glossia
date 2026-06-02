pub mod types;
pub mod core;
pub mod data;
pub mod cache;
pub mod utils;

// Re-export public API items
pub use types::{PayloadTok, Lexicon, GenerationMode, SentenceLengthMode};
pub use core::{generate_text, generate_text_with_original_payload, max_subsequence_embedding, plan_sentence, fill_slots};
pub use data::{load_payload_words, load_payload_words_for_wordlist, load_cover_words_by_pos, load_cover_words_by_pos_for_wordlist, build_pos_mapping, build_pos_mapping_for_wordlist, tag_word, select_random_words, wordlist_filenames, default_wordlist, load_payload_tree, load_cover_tree, load_cover_word_pos_tags, get_embedded_yaml, has_embedded_files, get_available_languages, get_available_wordlists, detect_dialect, detect_dialect_filtered, detect_dialect_with, detect_dialect_best, DialectMatch, DialectFilter, inject_scale_payload};
#[cfg(not(target_arch = "wasm32"))]
pub use data::get_wordlist_path;
pub use cache::{SequenceCache, print_sentence_kinds_once};
pub use utils::{normalize_token_for_bip39, wrap_payload_with_bars, wrap_payload_with_color, word_wrap, capitalize, payload_fits, get_grammar, get_dialect_config, mode_to_dialect, starts_with_vowel_sound};
