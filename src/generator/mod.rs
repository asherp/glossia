pub mod types;
pub mod core;
pub mod data;
pub mod cache;
pub mod utils;

// Re-export public API items
pub use types::{PayloadTok, Lexicon, GenerationMode, SentenceLengthMode};
pub use core::{generate_text, generate_text_with_original_payload, max_subsequence_embedding, plan_sentence, fill_slots};
pub use data::{load_payload_words, load_payload_words_for_wordlist, load_cover_words_by_pos, load_cover_words_by_pos_for_wordlist, build_pos_mapping, build_pos_mapping_for_wordlist, tag_word, select_random_words, wordlist_filenames, default_wordlist};
pub use cache::SequenceCache;
