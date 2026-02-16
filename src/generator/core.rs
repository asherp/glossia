use rand::{seq::SliceRandom, Rng};
use std::collections::{HashMap, HashSet};
use crate::types::Pos;
use crate::grammar::{Grammar, SequenceWithProbability};
use super::types::{PayloadTok, Lexicon, GenerationMode, SentenceLengthMode};
use super::cache::SequenceCache;
use super::utils::{
    payload_fits, get_grammar, start_nonterminal_for_pos, capitalize,
    normalize_token_for_bip39, starts_with_vowel_sound, is_bare_verb_form,
    is_likely_transitive_verb,
};

/// Join words respecting the grammar's payload separator.
///
/// For human languages (payload_separator = " "), this is just `words.join(" ")`.
/// For CS languages (payload_separator = ""), consecutive payload words are
/// concatenated without spaces. Cover words still get spaces.
///
/// If `payload_line_width` is set, payload runs are additionally line-wrapped.
fn join_words_with_payload_grammar(
    words: &[String],
    payload_set: &HashSet<String>,
    grammar: &Grammar,
) -> String {
    let sep = grammar.payload_separator();
    let line_width = grammar.payload_line_width();

    // Fast path: default separator — just join with space
    if sep == " " {
        return words.join(" ");
    }

    // Payload-aware join: consecutive payload words use payload_separator,
    // all other transitions use " ".
    // Line-break cover words (Conj "\n" or "<br>") are output as-is
    // without preceding spaces.
    let ends_with_break = |s: &str| s.ends_with('\n') || s.ends_with("<br>");
    let mut result = String::new();
    let mut payload_run = String::new(); // accumulates consecutive payload chars

    for (_i, word) in words.iter().enumerate() {
        let word_clean = normalize_token_for_bip39(word);
        let is_payload = !word_clean.is_empty() && payload_set.contains(&word_clean);
        let is_newline = word.contains('\n') || word == "<br>";

        if is_payload {
            // Accumulate into payload run
            payload_run.push_str(sep);
            payload_run.push_str(word);
        } else {
            // Flush any accumulated payload run
            if !payload_run.is_empty() {
                // Remove leading separator (if any)
                let payload_text = if sep.is_empty() {
                    payload_run.clone()
                } else {
                    payload_run.trim_start_matches(sep).to_string()
                };
                // Apply line wrapping if configured
                if let Some(width) = line_width {
                    let wrapped = wrap_payload(&payload_text, width);
                    // Only add a newline before payload if result doesn't already end with a break
                    if !result.is_empty() && !ends_with_break(&result) {
                        result.push('\n');
                    }
                    result.push_str(&wrapped);
                } else {
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push_str(&payload_text);
                }
                payload_run.clear();
            }
            // Add the cover word
            if is_newline {
                // Line-break cover words (Conj "\n" or "<br>") are output
                // directly — no preceding space.
                result.push_str(word);
            } else {
                if !result.is_empty() && !ends_with_break(&result) {
                    result.push(' ');
                }
                result.push_str(word);
            }
        }
    }

    // Flush trailing payload run
    if !payload_run.is_empty() {
        let payload_text = if sep.is_empty() {
            payload_run
        } else {
            payload_run.trim_start_matches(sep).to_string()
        };
        if let Some(width) = line_width {
            let wrapped = wrap_payload(&payload_text, width);
            if !result.is_empty() && !ends_with_break(&result) {
                result.push('\n');
            }
            result.push_str(&wrapped);
        } else {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(&payload_text);
        }
    }

    result
}

/// Wrap a payload string at the given character width.
fn wrap_payload(payload: &str, width: usize) -> String {
    if width == 0 || payload.len() <= width {
        return payload.to_string();
    }
    let mut result = String::new();
    for (i, ch) in payload.chars().enumerate() {
        if i > 0 && i % width == 0 {
            result.push('\n');
        }
        result.push(ch);
    }
    result
}

/// Find the maximum subsequence embedding of payload words into slots.
/// Returns Some(placement_map) where placement_map[slot_index] = payload_index if that slot should contain a payload word.
/// Returns None if j payload words cannot be embedded.
pub fn max_subsequence_embedding(
    slots: &[Pos],
    payload: &[PayloadTok],
    payload_start: usize,
    j: usize,
) -> Option<HashMap<usize, usize>> {
    if j == 0 {
        return Some(HashMap::new());
    }
    
    if payload_start + j > payload.len() {
        return None;
    }
    
    // Filter out Dot and function word slots that can't hold payload words
    // Dot is punctuation, Prefix/Aux/Cop/To are function words that must be cover words
    let word_slots: Vec<(usize, Pos)> = slots
        .iter()
        .enumerate()
        .filter(|(_, pos)| {
            **pos != Pos::Dot 
            && **pos != Pos::Prefix 
            && **pos != Pos::Aux 
            && **pos != Pos::Cop 
            && **pos != Pos::To
        })
        .map(|(idx, pos)| (idx, *pos))
        .collect();
    
    if word_slots.len() < j {
        return None;
    }
    
    // Greedy matching: try to place each payload word in order
    let mut placement = HashMap::new();
    let mut payload_idx = payload_start;
    let mut slot_idx_in_word_slots = 0;
    
    while payload_idx < payload_start + j && slot_idx_in_word_slots < word_slots.len() {
        let (original_slot_idx, slot_pos) = word_slots[slot_idx_in_word_slots];
        let payload_word = &payload[payload_idx];
        
        // Check if this payload word can go in this slot
        if payload_fits(payload_word, slot_pos) {
            placement.insert(original_slot_idx, payload_idx);
            payload_idx += 1;
        }
        
        slot_idx_in_word_slots += 1;
    }
    
    // Did we place all j words?
    if payload_idx == payload_start + j {
        Some(placement)
    } else {
        None
    }
}

/// Plan a sentence: find the best POS sequence and payload embedding for given k.
/// Returns (slots, refinements, forced_placement_map, j) where j is the number of payload words embedded.
/// If require_prefix is true, only consider sequences that start with Pos::Prefix.
pub fn plan_sentence<R: Rng>(
    rng: &mut R,
    cache: &SequenceCache,
    start_symbol: &str,
    k: usize,
    payload: &[PayloadTok],
    payload_start: usize,
    require_prefix: bool,
) -> Option<(Vec<Pos>, Vec<Option<String>>, HashMap<usize, usize>, usize)> {
    let sequences = cache.get(start_symbol, k)?;

    if sequences.is_empty() {
        return None;
    }

    let remaining_payload = payload.len().saturating_sub(payload_start);
    if remaining_payload == 0 {
        return None;
    }

    // Compute the set of POS tags needed by the remaining payload words.
    // We only need to look at payload[payload_start..] since those are the words
    // we're trying to embed.
    let payload_pos_needed: HashSet<Pos> = payload[payload_start..]
        .iter()
        .flat_map(|tok| tok.allowed.iter().copied())
        .collect();

    // Filter sequences:
    // 1. If require_prefix, only keep sequences starting with Pos::Prefix
    // 2. Skip sequences whose word slots have no POS overlap with payload needs
    //    (these can never embed any payload word, so embedding checks are wasted)
    let filtered_with_indices: Vec<(usize, &SequenceWithProbability)> = sequences.iter()
        .enumerate()
        .filter(|(_, seq_prob)| {
            // Prefix filter
            if require_prefix && (seq_prob.sequence.is_empty() || seq_prob.sequence[0] != Pos::Prefix) {
                return false;
            }
            // POS compatibility filter: skip if no word slot POS matches any payload POS
            !seq_prob.word_slot_pos.is_disjoint(&payload_pos_needed)
        })
        .collect();

    if filtered_with_indices.is_empty() {
        return None;
    }

    // m = number of word slots that can hold payload words (excluding Dot and function words)
    // Try j from min(remaining_payload, m) down to 1
    // For each j, try sequences in probability order

    // First, figure out m by looking at the first sequence
    let m = filtered_with_indices[0].1.word_slot_pos.len().max(
        filtered_with_indices[0].1.sequence.iter().filter(|&&pos| {
            !matches!(pos, Pos::Dot | Pos::Prefix | Pos::Aux | Pos::Cop | Pos::To)
        }).count()
    );

    let max_j = remaining_payload.min(m);

    // Try j from max_j down to 1
    for j in (1..=max_j).rev() {
        // Collect all sequences that can embed j payload words.
        // Then choose among them probabilistically by grammar probability.
        let mut candidates: Vec<(usize, HashMap<usize, usize>)> = Vec::new();
        let mut total_prob: f64 = 0.0;

        for (original_idx, seq_prob) in filtered_with_indices.iter() {
            if let Some(placement) = max_subsequence_embedding(
                &seq_prob.sequence,
                payload,
                payload_start,
                j,
            ) {
                total_prob += seq_prob.probability;
                candidates.push((*original_idx, placement));
            }
        }

        if candidates.is_empty() {
            continue;
        }

        // Weighted random selection by probability.
        // (If probabilities are all zeros, fall back to uniform.)
        if total_prob > 0.0 {
            let mut r = rng.gen::<f64>() * total_prob;
            let mut last: Option<(usize, HashMap<usize, usize>)> = None;

            for (idx, placement) in candidates.iter() {
                last = Some((*idx, placement.clone()));
                let w = sequences[*idx].probability;
                if r <= w {
                    return Some((sequences[*idx].sequence.clone(), sequences[*idx].refinements.clone(), placement.clone(), j));
                }
                r -= w;
            }

            // Numerical edge-case: fall back to last feasible candidate.
            let (idx, placement) = last.expect("candidates non-empty");
            return Some((sequences[idx].sequence.clone(), sequences[idx].refinements.clone(), placement, j));
        } else {
            let (idx, placement) = candidates
                .choose(rng)
                .expect("candidates non-empty")
                .clone();
            return Some((sequences[idx].sequence.clone(), sequences[idx].refinements.clone(), placement, j));
        }
    }

    None
}

/// Generate a minimal fallback sentence structure that can always embed a payload word.
/// This ensures payload preservation even if it results in grammar errors.
/// Returns (slots, refinements, forced_placement_map) where the word is forced into the first compatible slot.
/// Uses grammar introspection instead of language-name string checks.
pub(crate) fn generate_fallback_sentence(
    payload: &[PayloadTok],
    payload_start: usize,
    mode: GenerationMode,
    grammar: &crate::grammar::Grammar,
) -> Option<(Vec<Pos>, Vec<Option<String>>, HashMap<usize, usize>)> {
    if payload_start >= payload.len() {
        return None;
    }

    let word = &payload[payload_start];

    // Derive features from grammar instead of language name
    let has_punctuation = grammar.grammar_uses_pos(Pos::Dot);
    let has_determiners = grammar.grammar_uses_pos(Pos::Det);

    let include_dot = mode != GenerationMode::Subject && has_punctuation;
    let use_det = has_determiners;
    let det_prefix = if use_det { vec![Pos::Det] } else { vec![] };
    let det_offset = if use_det { 1 } else { 0 };
    
    // Create minimal sentence structures that can accommodate any POS
    // We prioritize the first allowed POS tag, but will force-place if needed
    let (slots, slot_idx) = if word.allowed.contains(&Pos::N) {
        // Simple: "[word]." or "The [word]." (language-dependent)
        if include_dot {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            s.push(Pos::Dot);
            (s, det_offset)
        } else {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            (s, det_offset)
        }
    } else if word.allowed.contains(&Pos::V) {
        // "[word] note." or "The note [word]." (language-dependent)
        if include_dot {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            s.push(Pos::V);
            s.push(Pos::Dot);
            (s, 1 + det_offset)
        } else {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            s.push(Pos::V);
            (s, 1 + det_offset)
        }
    } else if word.allowed.contains(&Pos::Adj) {
        // "[word] note." or "The [word] note." (language-dependent)
        if include_dot {
            let mut s = det_prefix.clone();
            s.push(Pos::Adj);
            s.push(Pos::N);
            s.push(Pos::Dot);
            (s, det_offset)
        } else {
            let mut s = det_prefix.clone();
            s.push(Pos::Adj);
            s.push(Pos::N);
            (s, det_offset)
        }
    } else if word.allowed.contains(&Pos::Adv) {
        // "note works [word]." or "The note works [word]." (language-dependent)
        if include_dot {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            s.push(Pos::V);
            s.push(Pos::Adv);
            s.push(Pos::Dot);
            (s, 2 + det_offset)
        } else {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            s.push(Pos::V);
            s.push(Pos::Adv);
            (s, 2 + det_offset)
        }
    } else if word.allowed.contains(&Pos::Prep) {
        // "note [word] user." or "The note [word] the user." (language-dependent)
        if include_dot {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            s.push(Pos::Prep);
            if use_det {
                s.push(Pos::Det);
            }
            s.push(Pos::N);
            s.push(Pos::Dot);
            (s, 1 + det_offset)
        } else {
            let mut s = det_prefix.clone();
            s.push(Pos::N);
            s.push(Pos::Prep);
            if use_det {
                s.push(Pos::Det);
            }
            s.push(Pos::N);
            (s, 1 + det_offset)
        }
    } else if word.allowed.contains(&Pos::Det) {
        // "[word] note works." or "[word] note works" for subject
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::V, Pos::Dot], 0)
        } else {
            (vec![Pos::Det, Pos::N, Pos::V], 0)
        }
    } else {
        // Last resort: force into any slot (will cause grammar error but preserves payload)
        // Use noun slot as most common
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::Dot], 1)
        } else {
            (vec![Pos::Det, Pos::N], 1)
        }
    };
    
    let refinements = vec![None; slots.len()];
    let mut forced = HashMap::new();
    forced.insert(slot_idx, payload_start);

    Some((slots, refinements, forced))
}

/// Apply English indefinite article phonological rule: "a" before consonant, "an" before vowel.
/// This is the only remaining surface-form rule — a/an have identical denotations in Montague Grammar.
fn apply_indef_phonology(next_word: Option<&str>) -> String {
    if let Some(next) = next_word {
        let normalized = normalize_token_for_bip39(next);
        if starts_with_vowel_sound(&normalized) {
            "an".to_string()
        } else {
            "a".to_string()
        }
    } else {
        "a".to_string()
    }
}

/// Fill a slot stream with cover words + payload words (in-order).
/// Returns words vector.
/// `prev_words` are the last few words from the previous sentence (if any), to prevent repetition across sentences.
/// `expected_first_pos` is the POS that should appear first (if set), used to ensure payload word placement.
/// `forced_placements` maps slot_index -> payload_index for slots that must contain specific payload words.
/// `payload_only_mode`: if true, use payload words for all slots (even function words)
pub fn fill_slots<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    slots: &[Pos],
    refinements: &[Option<String>],
    payload: &[PayloadTok],
    payload_i: &mut usize,
    prev_words: &[&str],
    _expected_first_pos: Option<Pos>,
    forced_placements: Option<&HashMap<usize, usize>>,
    payload_only_mode: bool,
    prime_constraint_enabled: bool,
    dot_is_punctuation: bool,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    const REPETITION_WINDOW: usize = 3;
    // Track which payload words have been used (by index)
    let mut used_payload_indices: HashSet<usize> = HashSet::new();

    for (i, &slot) in slots.iter().enumerate() {
        if slot == Pos::Dot && dot_is_punctuation {
            if let Some(last) = out.last_mut() {
                last.push('.');
            } else {
                out.push(".".to_string());
            }
            continue;
        }

        // --- Payload placement (same for all POS, unchanged) ---
        let must_use_cover = !payload_only_mode && matches!(
            slot,
            Pos::Aux | Pos::Cop | Pos::To | Pos::Prefix | Pos::Modal | Pos::Conj
        );

        let payload_word_idx = if let Some(forced) = forced_placements {
            forced.get(&i).copied()
        } else if must_use_cover {
            None
        } else if slot == Pos::Det {
            // Allow embedding payload determiners
            if *payload_i < payload.len()
                && !used_payload_indices.contains(payload_i)
                && payload_fits(&payload[*payload_i], Pos::Det)
            {
                Some(*payload_i)
            } else {
                None
            }
        } else if *payload_i < payload.len()
            && !used_payload_indices.contains(payload_i)
            && (payload_only_mode || payload_fits(&payload[*payload_i], slot))
        {
            Some(*payload_i)
        } else {
            None
        };

        if let Some(idx) = payload_word_idx {
            // Refinement-aware validation: verify the payload word is valid for this slot's
            // refinement tag. This is an independent safety check — the pre-filtered wordlist
            // should already guarantee correctness, so a mismatch indicates a bug.
            let ref_tag = refinements.get(i).and_then(|r| r.as_deref());
            debug_assert!(
                lex.payload_valid_for_refinement(&payload[idx].word, ref_tag),
                "Payload word '{}' not valid for refinement {:?} at slot {}",
                payload[idx].word, ref_tag, i
            );
            out.push(payload[idx].word.clone());
            used_payload_indices.insert(idx);
            if forced_placements.is_none() {
                *payload_i += 1;
            }
            continue;
        }

        // Advance past used payload words
        if *payload_i < payload.len() && used_payload_indices.contains(payload_i) {
            while *payload_i < payload.len() && used_payload_indices.contains(payload_i) {
                *payload_i += 1;
            }
        }

        // --- Cover word selection (refinement-driven) ---
        let mut recent_words: Vec<&str> = prev_words.to_vec();
        let start_idx = out.len().saturating_sub(REPETITION_WINDOW);
        recent_words.extend(out[start_idx..].iter().map(|s| s.as_str()));

        let ref_tag = refinements.get(i).and_then(|r| r.as_deref());

        let cover_word = if prime_constraint_enabled {
            // Prime ordering constraint (math/primes language)
            let is_prime_word = |w: &str| -> Option<i64> {
                w.parse::<i64>().ok().filter(|&n| {
                    if n < 2 { return false; }
                    if n == 2 { return true; }
                    if n % 2 == 0 { return false; }
                    let sqrt_n = (n as f64).sqrt() as i64;
                    for ii in (3..=sqrt_n).step_by(2) {
                        if n % ii == 0 { return false; }
                    }
                    true
                })
            };

            let left_prime = out.last().and_then(|w| is_prime_word(w.as_str()));
            let right_prime = if *payload_i < payload.len()
                && !used_payload_indices.contains(payload_i)
                && payload_fits(&payload[*payload_i], slots.get(i + 1).copied().unwrap_or(slot))
            {
                is_prime_word(&payload[*payload_i].word)
            } else {
                None
            };

            if let (Some(_left), Some(_right)) = (left_prime, right_prime) {
                lex.pick_cover_with_prime_constraint(
                    rng,
                    slot,
                    &recent_words,
                    out.last().map(|s| s.as_str()),
                    right_prime.map(|_| payload[*payload_i].word.as_str()),
                ).unwrap_or_else(|| lex.pick_cover(rng, slot, &recent_words))
            } else {
                lex.pick_cover(rng, slot, &recent_words)
            }
        } else if slot == Pos::V {
            // Lightweight verb agreement (Modal -> bare V, V -> NP transitivity)
            let prev_slot = if i > 0 { Some(slots[i - 1]) } else { None };
            let next_slot = slots.get(i + 1).copied();
            let after_modal = matches!(prev_slot, Some(Pos::Modal));
            let want_transitive = matches!(next_slot, Some(Pos::Det) | Some(Pos::N));

            let constrained = if after_modal && want_transitive {
                lex.pick_cover_filtered(rng, slot, &recent_words, |w| {
                    is_bare_verb_form(w) && is_likely_transitive_verb(w)
                })
            } else if after_modal {
                lex.pick_cover_filtered(rng, slot, &recent_words, |w| is_bare_verb_form(w))
            } else if want_transitive {
                lex.pick_cover_filtered(rng, slot, &recent_words, |w| is_likely_transitive_verb(w))
            } else {
                None
            };

            constrained.unwrap_or_else(|| lex.pick_cover_refined(rng, slot, ref_tag, &recent_words))
        } else if slot == Pos::Det && ref_tag == Some("indef") {
            // Special phonological rule for indefinite article: a/an based on next word
            let next_word_str: Option<String> = if let Some(forced) = forced_placements {
                forced
                    .get(&(i + 1))
                    .and_then(|&pidx| payload.get(pidx))
                    .map(|t| t.word.clone())
            } else if *payload_i < payload.len()
                && !used_payload_indices.contains(payload_i)
                && slots.get(i + 1).map_or(false, |&ns| payload_fits(&payload[*payload_i], ns))
            {
                Some(payload[*payload_i].word.clone())
            } else {
                // Peek at what cover word would be chosen for next slot
                slots.get(i + 1).map(|&ns| lex.pick_cover(rng, ns, &recent_words))
            };
            apply_indef_phonology(next_word_str.as_deref())
        } else {
            // All other POS: use refinement-aware cover word selection
            lex.pick_cover_refined(rng, slot, ref_tag, &recent_words)
        };

        out.push(cover_word);
    }

    // Advance payload_i past used words
    while *payload_i < payload.len() && used_payload_indices.contains(payload_i) {
        *payload_i += 1;
    }

    out
}

/// Compute k candidates based on the length mode.
/// Returns a vector of k values to try in order.
pub(crate) fn compute_k_candidates<R: Rng>(
    rng: &mut R,
    cache: &SequenceCache,
    start_symbol: &str,
    k_min: usize,
    k_max: usize,
    length_mode: SentenceLengthMode,
    require_prefix: bool,
) -> Vec<usize> {
    match length_mode {
        SentenceLengthMode::Compact => {
            // Compact mode: try k from k_min to k_max, shortest first
            let k_start = if require_prefix { k_min + 1 } else { k_min };
            (k_start..=k_max).collect()
        }
        SentenceLengthMode::Natural => {
            // Natural mode: sample k from grammar's length distribution
            // Respect k_min as a floor, but sample naturally above it
            let natural_k_start = if require_prefix { k_min.max(2) } else { k_min.max(1) };
            
            // Compute weights for each k >= k_min
            let mut k_weights: Vec<(usize, f64)> = Vec::new();
            for k in natural_k_start..=k_max {
                if let Some(sequences) = cache.get(start_symbol, k) {
                    let weight: f64 = if require_prefix {
                        sequences.iter()
                            .filter(|seq_prob| !seq_prob.sequence.is_empty() && seq_prob.sequence[0] == Pos::Prefix)
                            .map(|seq_prob| seq_prob.probability)
                            .sum()
                    } else {
                        sequences.iter()
                            .map(|seq_prob| seq_prob.probability)
                            .sum()
                    };
                    if weight > 0.0 {
                        k_weights.push((k, weight));
                    }
                }
            }
            
            if k_weights.is_empty() {
                // Fallback to compact mode if no weights found
                let k_start = if require_prefix { k_min + 1 } else { k_min };
                return (k_start..=k_max).collect();
            }
            
            // Sample one k from the distribution
            let total_weight: f64 = k_weights.iter().map(|(_, w)| w).sum();
            if total_weight <= 0.0 {
                // Fallback to compact mode if total weight is zero
                let k_start = if require_prefix { k_min + 1 } else { k_min };
                return (k_start..=k_max).collect();
            }
            
            let mut r = rng.gen::<f64>() * total_weight;
            let mut sampled_k = None;
            for (k, weight) in &k_weights {
                if r <= *weight {
                    sampled_k = Some(*k);
                    break;
                }
                r -= weight;
            }
            let sampled_k = sampled_k.unwrap_or_else(|| k_weights[0].0);
            
            // Build candidate list:
            // 1. Sampled k first
            // 2. Remaining k's in descending weight order
            // 3. Compact fallback (k_min..=k_max) for robustness
            
            let mut candidates = vec![sampled_k];
            
            // Add remaining k's in descending weight order (excluding sampled_k)
            let mut remaining: Vec<(usize, f64)> = k_weights.iter()
                .filter(|(k, _)| *k != sampled_k)
                .cloned()
                .collect();
            remaining.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            candidates.extend(remaining.into_iter().map(|(k, _)| k));
            
            // Add compact fallback for robustness (only k's not already in candidates)
            // Still respect k_min for the fallback to ensure we don't miss any valid k values
            let candidates_set: std::collections::HashSet<usize> = candidates.iter().cloned().collect();
            let k_start = if require_prefix { k_min + 1 } else { k_min };
            for k in k_start..=k_max {
                if !candidates_set.contains(&k) {
                    candidates.push(k);
                }
            }
            
            candidates
        }
    }
}

/// Generate sentences until all payload tokens are embedded.
/// Returns (text, payload_set) where text is the generated text (without highlighting).
/// The payload_set can be used by the caller to apply highlighting if needed.
pub fn generate_text<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    payload: &[PayloadTok],
    verbose: bool,
    mode: GenerationMode,
    language: &str,
    k_min: usize,
    k_max: usize,
    length_mode: SentenceLengthMode,
    delimiter: &str,
) -> (String, HashSet<String>) {
    generate_text_with_original_payload(rng, lex, payload, None, verbose, mode, language, None, k_min, k_max, length_mode, delimiter)
}

/// Generate text with optional original payload set for validation (used in merkle mode)
pub fn generate_text_with_original_payload<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    payload: &[PayloadTok],
    original_payload_set: Option<&HashSet<String>>,
    verbose: bool,
    mode: GenerationMode,
    language: &str,
    grammar_dialect: Option<&str>,
    k_min: usize,
    k_max: usize,
    length_mode: SentenceLengthMode,
    delimiter: &str,
) -> (String, HashSet<String>) {
    // Build payload set for highlighting (returned to caller)
    // Note: In merkle mode, this includes Merkle words too, but we check against original_payload_set
    // for sentence validation.
    let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
    
    // Use original_payload_set for validation if provided, otherwise use payload_set
    let validation_payload_set: HashSet<String> = original_payload_set
        .map(|s| s.clone())
        .unwrap_or_else(|| payload_set.clone());

    // In payload-only mode, simply return the payload words
    // No grammar processing, no slot filling, no cover words
    if matches!(mode, GenerationMode::PayloadOnly) {
        let words: Vec<String> = payload.iter().map(|tok| tok.word.clone()).collect();
        let text = words.join(delimiter);
        return (text, payload_set);
    }

    let mut words: Vec<String> = Vec::new();
    let mut payload_i: usize = 0;

    // Check if prime ordering constraint is enabled in grammar
    // Use explicit dialect if provided, otherwise derive from mode
    let dialect_str = grammar_dialect.unwrap_or(match mode {
        GenerationMode::Subject => "subject",
        GenerationMode::Body => "body",
        GenerationMode::PayloadOnly => "payload_only",
    });
    let grammar = Grammar::from_language_dialect(language, dialect_str)
        .expect(&format!("Failed to load {} grammar for language {}", dialect_str, language));
    let prime_constraint_enabled = grammar.language_config.as_ref()
        .and_then(|config| config.constraints.as_ref())
        .and_then(|constraints| constraints.prime_ordering.as_ref())
        .map(|c| c.enabled)
        .unwrap_or(false);

    // Load precomputed sequences
    let cache = match SequenceCache::load_with_dialect(mode, language, dialect_str, k_max, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading sequence cache: {}", e);
            eprintln!("Falling back to random generation");
            // Fall back to old random generation - but we still need to update fill_slots calls
            // For now, just panic - we'll handle this better later
            panic!("Sequence cache required for new algorithm");
        }
    };

    // For subject mode, generate a single sentence with all payload words
    // For body mode, generate multiple sentences as before
    if mode == GenerationMode::Subject {
        // Generate sentences until all payload words are embedded
        // Keep generating sentences and concatenating them until all words are used
        let mut all_sentence_words: Vec<String> = Vec::new();
        let mut current_payload_i = 0;
        let mut prev_words_strings: Vec<String> = Vec::new(); // Store owned strings
        let mut sentence_count = 0;
        const MAX_SENTENCES: usize = 100; // Safety limit to prevent infinite loops
        
        while current_payload_i < payload.len() && sentence_count < MAX_SENTENCES {
            sentence_count += 1;
            
            // Get the next payload word's POS for start_symbol selection
            // Prefix presence is now dialect-driven (subject vs subject_re vs subject_fwd),
            // not random. The grammar's sentence rule deterministically includes or excludes Prefix.
            let (start_symbol, want_prefix) = if sentence_count == 1 && mode == GenerationMode::Subject {
                ("S", false)  // Prefix is controlled by the dialect grammar, not by a coin flip
            } else if current_payload_i < payload.len() {
                let next_word = &payload[current_payload_i];
                if next_word.allowed.is_empty() {
                    panic!(
                        "BUG: Payload word '{}' has no allowed POS tags!\n\
                         This indicates a POS tagging failure. Check:\n\
                         1. Is '{}' in {}/payload.yaml?\n\
                         2. Does it have POS tags assigned in the YAML file?\n\
                         3. Is the POS tag parsing working correctly?\n\
                         Note: For language '{}', check {}/payload.yaml, not bip39_POS.txt",
                        next_word.word,
                        next_word.word,
                        language,
                        language,
                        language
                    );
                }
                
                let pos = if next_word.allowed.contains(&Pos::N) {
                    Pos::N
                } else if next_word.allowed.contains(&Pos::V) {
                    Pos::V
                } else if next_word.allowed.contains(&Pos::Adj) {
                    Pos::Adj
                } else if next_word.allowed.contains(&Pos::Adv) {
                    Pos::Adv
                } else if next_word.allowed.contains(&Pos::Prep) {
                    Pos::Prep
                } else {
                    next_word.allowed.iter().next().copied().expect("Payload word should have at least one POS tag")
                };
                
                let nt = start_nonterminal_for_pos(pos);
                let symbol = if grammar.rules.contains_key(nt) || grammar.language_config.is_some() {
                    nt
                } else {
                    // Fallback: for subsequent sentences in subject mode, use POS-specific start
                    // to avoid Prefix. For first sentence or body mode, "S" is fine.
                    if sentence_count > 1 && mode == GenerationMode::Subject {
                        // Try to find any POS-specific start symbol that exists (subject grammar may have S_* variants)
                        // This ensures we don't get Prefix in subsequent sentences
                        let alternatives = ["S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"];
                        alternatives.iter()
                            .find(|&&alt| grammar.rules.contains_key(alt))
                            .copied()
                            .unwrap_or("S")  // Fallback to S (body grammar only uses S)
                    } else {
                        "S"
                    }
                };
                (symbol, false)  // Subsequent sentences never want prefix
            } else {
                // No more payload words - use "S" for body mode, but for subject mode
                // subsequent sentences, prefer non-Prefix start symbols
                let symbol = if sentence_count > 1 && mode == GenerationMode::Subject {
                    // Try POS-specific start symbols (subject grammar may have S_* variants)
                    let alternatives = ["S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"];
                    alternatives.iter()
                        .find(|&&alt| grammar.rules.contains_key(alt))
                        .copied()
                        .unwrap_or("S")  // Fallback to S (body grammar only uses S)
                } else {
                    "S"
                };
                (symbol, false)  // No payload words remaining, no prefix
            };
            
            // Compute k candidates based on length mode
            let k_candidates = compute_k_candidates(
                rng,
                &cache,
                start_symbol,
                k_min,
                k_max,
                length_mode,
                want_prefix,
            );
            let mut planned = None;
            for k in k_candidates {
                if let Some((slots, refinements, forced_placements, j)) = plan_sentence(
                    rng,
                    &cache,
                    start_symbol,
                    k,
                    payload,
                    current_payload_i,
                    want_prefix,
                ) {
                    planned = Some((slots, refinements, forced_placements, j));
                    break; // Found a plan, use it
                }
            }
            
            // If we wanted a prefix but didn't find one, fall back to non-prefix
            if planned.is_none() && want_prefix {
                let k_candidates_fallback = compute_k_candidates(
                    rng,
                    &cache,
                    start_symbol,
                    k_min,
                    k_max,
                    length_mode,
                    false,  // Don't require prefix in fallback
                );
                for k in k_candidates_fallback {
                    if let Some((slots, refinements, forced_placements, j)) = plan_sentence(
                        rng,
                        &cache,
                        start_symbol,
                        k,
                        payload,
                        current_payload_i,
                        false,  // Don't require prefix in fallback
                    ) {
                        planned = Some((slots, refinements, forced_placements, j));
                        break;
                    }
                }
            }
            
            let (slots, refinements, forced_placements, _j) = match planned {
                Some(p) => p,
                None => {
                    // Fallback: generate minimal sentence structure to always embed the word
                    // This preserves payload order even if it results in grammar errors
                    let word_name = if current_payload_i < payload.len() {
                        payload[current_payload_i].word.as_str()
                    } else {
                        "unknown"
                    };
                    if verbose {
                        eprintln!("Warning: Could not plan sentence for word '{}' (index {}). Using fallback structure (may have grammar errors).", 
                                 word_name, current_payload_i);
                    }
                    match generate_fallback_sentence(payload, current_payload_i, mode, &grammar) {
                        Some((fallback_slots, fallback_refs, fallback_placements)) => {
                            (fallback_slots, fallback_refs, fallback_placements, 1)
                        }
                        None => {
                            // This should never happen, but if it does, panic rather than skip
                            panic!("BUG: Cannot generate fallback sentence for word '{}' at index {}. This should never happen.",
                                   word_name, current_payload_i);
                        }
                    }
                }
            };

            let payload_i_before = current_payload_i;
            // Advance payload_i to account for forced placements (they're in order)
            let max_forced_idx = forced_placements.values().max().copied().unwrap_or(current_payload_i.saturating_sub(1));
            let mut temp_payload_i = (max_forced_idx + 1).max(current_payload_i);
            
            // Convert prev_words_strings to slice for fill_slots
            let prev_words_refs: Vec<&str> = prev_words_strings.iter().map(|s| s.as_str()).collect();
            let payload_only_mode = matches!(mode, GenerationMode::PayloadOnly);
            let mut sentence_words = fill_slots(
                rng,
                lex,
                &slots,
                &refinements,
                payload,
                &mut temp_payload_i,
                &prev_words_refs,
                None,
                Some(&forced_placements),
                payload_only_mode,
                prime_constraint_enabled,
                grammar.dot_is_punctuation(),
            );

            // Update current_payload_i to reflect what was actually used
            current_payload_i = temp_payload_i.max(max_forced_idx + 1);

            // Capitalize the first word of the first sentence only
            if all_sentence_words.is_empty() {
                if let Some(first) = sentence_words.first_mut() {
                    *first = capitalize(first);
                }
            }
            
            // Update prev_words_strings with last few words from this sentence for next iteration
            // Extract strings before appending to avoid lifetime issues
            let start_idx = sentence_words.len().saturating_sub(3);
            prev_words_strings = sentence_words[start_idx..].iter().cloned().collect();
            
            // Append this sentence to all sentences
            all_sentence_words.append(&mut sentence_words);
            
            // If no progress was made, break to avoid infinite loop
            if current_payload_i == payload_i_before {
                if verbose {
                    eprintln!("Warning: No progress embedding words. Stopping at {}/{} words embedded.", current_payload_i, payload.len());
                }
                break;
            }
        }
        
        // Verify all payload words were embedded
        if current_payload_i < payload.len() {
            if verbose {
                eprintln!("Warning: Not all payload words embedded in subject mode. Embedded {}/{} after {} sentences", current_payload_i, payload.len(), sentence_count);
            }
        }
        
        let mut sentence_words = all_sentence_words;
        
        // First word should already be capitalized (done in loop), but ensure it's capitalized
        if let Some(first) = sentence_words.first_mut() {
            *first = capitalize(first);
        }

        // Print sentence as it's generated if verbose
        if verbose {
            let sentence_text: String = sentence_words.iter()
                .map(|w| {
                    let word_clean = normalize_token_for_bip39(w);
                    if !word_clean.is_empty() && payload_set.contains(&word_clean) {
                        // In library version, don't apply highlighting - just return the word
                        w.clone()
                    } else {
                        w.clone()
                    }
                })
                .collect::<Vec<String>>()
                .join(" ");
            eprintln!("{}", sentence_text);
        }
        
        words = sentence_words;
    } else {
        // Body mode: Keep generating sentences until all payload tokens are embedded
        
        // In merkle mode (when original_payload_set is provided), use segmentation approach
        // Segment the sequence into chunks ending with 1-2 payload words
        if original_payload_set.is_some() {
            return generate_text_merkle_segmented(
                rng,
                lex,
                payload,
                original_payload_set.unwrap(),
                verbose,
                language,
                k_min,
                k_max,
                length_mode,
                prime_constraint_enabled,
                &cache,
            );
        }
        
        let mut sentence_count = 0;
        const MAX_SENTENCES: usize = 200; // Safety limit to prevent infinite loops
        while payload_i < payload.len() && sentence_count < MAX_SENTENCES {
            sentence_count += 1;
        // Make each sentence size adapt to remaining needs.
        let remaining_payload = payload.len().saturating_sub(payload_i);
        // Adapt sentence length based on remaining payload
        let _sentence_min = if remaining_payload > 10 {
            18
        } else if remaining_payload > 5 {
            14
        } else {
            5
        };

        // Get the next payload word's POS for start_symbol selection
        let next_word = if payload_i < payload.len() {
            Some(&payload[payload_i])
        } else {
            None
        };
        
        let start_symbol = if let Some(next_word) = next_word {
            // Panic if the payload word has no allowed POS tags - this indicates a POS tagging failure
            if next_word.allowed.is_empty() {
                panic!(
                    "BUG: Payload word '{}' has no allowed POS tags!\n\
                     This indicates a POS tagging failure. Check:\n\
                     1. Is '{}' in {}/payload.yaml?\n\
                     2. Does it have POS tags assigned in the YAML file?\n\
                     3. Is the POS tag parsing working correctly?\n\
                     Note: For language '{}', check {}/payload.yaml, not bip39_POS.txt",
                    next_word.word,
                    next_word.word,
                    language,
                    language,
                    language
                );
            }
            
            let pos = if next_word.allowed.contains(&Pos::N) {
                Pos::N
            } else if next_word.allowed.contains(&Pos::V) {
                Pos::V
            } else if next_word.allowed.contains(&Pos::Adj) {
                Pos::Adj
            } else if next_word.allowed.contains(&Pos::Adv) {
                Pos::Adv
            } else if next_word.allowed.contains(&Pos::Prep) {
                Pos::Prep
            } else {
                next_word.allowed.iter().next().copied().expect("Payload word should have at least one POS tag")
            };
            
            let nt = start_nonterminal_for_pos(pos);
            if grammar.rules.contains_key(nt) || grammar.language_config.is_some() {
                nt
            } else {
                "S"
            }
        } else {
            "S"
        };

        // Pass the last few words from previous sentence to prevent repetition across sentences
        const REPETITION_WINDOW: usize = 3;
        let prev_words: Vec<String> = words
            .iter()
            .rev()
            .take(REPETITION_WINDOW)
            .map(|s| {
                s.trim_end_matches('.').trim_end_matches(' ').to_lowercase()
            })
            .rev()
            .collect();
        let prev_words_refs: Vec<&str> = prev_words.iter().map(|s| s.as_str()).collect();
        let payload_i_before = payload_i;
        
        // Compute k candidates based on length mode (body mode never requires prefix)
        let k_candidates = compute_k_candidates(
            rng,
            &cache,
            start_symbol,
            k_min,
            k_max,
            length_mode,
            false,  // Body mode never requires prefix
        );
        let mut planned = None;
        for k in k_candidates {
            if let Some((slots, refinements, forced_placements, j)) = plan_sentence(
                rng,
                &cache,
                start_symbol,
                k,
                payload,
                payload_i,
                false,  // Body mode never requires prefix
            ) {
                planned = Some((slots, refinements, forced_placements, j));
                break; // Found a plan, use it
            }
        }
        
        // If planning failed with preferred start_symbol, try fallback with "S"
        if planned.is_none() && start_symbol != "S" {
            let k_candidates_fallback = compute_k_candidates(
                rng,
                &cache,
                "S",
                k_min,
                k_max,
                length_mode,
                false,  // Body mode never requires prefix
            );
            for k in k_candidates_fallback {
                if let Some((slots, refinements, forced_placements, j)) = plan_sentence(
                    rng,
                    &cache,
                    "S",
                    k,
                    payload,
                    payload_i,
                    false,  // Body mode never requires prefix
                ) {
                    planned = Some((slots, refinements, forced_placements, j));
                    break;
                }
            }
        }
        
        // If still no plan, try other POS tags from the word's allowed tags
        if planned.is_none() {
            if let Some(next_word) = next_word {
            for &alt_pos in &next_word.allowed {
                if alt_pos == Pos::N || alt_pos == Pos::V || alt_pos == Pos::Adj || alt_pos == Pos::Adv || alt_pos == Pos::Prep {
                    let alt_nt = start_nonterminal_for_pos(alt_pos);
                    let alt_start = if grammar.rules.contains_key(alt_nt) || grammar.language_config.is_some() {
                        alt_nt
                    } else {
                        "S"
                    };
                    
                    let k_candidates_alt = compute_k_candidates(
                        rng,
                        &cache,
                        alt_start,
                        k_min,
                        k_max,
                        length_mode,
                        false,  // Body mode never requires prefix
                    );
                    for k in k_candidates_alt {
                        if let Some((slots, refinements, forced_placements, j)) = plan_sentence(
                            rng,
                            &cache,
                            alt_start,
                            k,
                            payload,
                            payload_i,
                            false,  // Body mode never requires prefix
                        ) {
                            if verbose {
                                let grammar_str: Vec<String> = slots.iter().map(|pos| pos.to_string()).collect();
                                eprintln!("Selected grammar rule (alt): {} -> {} (k={}, j={} payload words)",
                                         alt_start, grammar_str.join(" "), k, j);
                            }
                            planned = Some((slots, refinements, forced_placements, j));
                            break;
                        }
                    }
                    if planned.is_some() {
                        break;
                    }
                }
            }
            }
        }
        
        let (slots, refinements, forced_placements, _j) = match planned {
            Some(p) => p,
            None => {
                // Fallback: generate minimal sentence structure to always embed the word
                // This preserves payload order even if it results in grammar errors
                let word_name = if payload_i < payload.len() {
                    payload[payload_i].word.as_str()
                } else {
                    "unknown"
                };
                if verbose {
                    eprintln!("Warning: Could not plan sentence for word '{}' (index {}). Using fallback structure (may have grammar errors).", 
                             word_name, payload_i);
                }
                match generate_fallback_sentence(payload, payload_i, mode, &grammar) {
                    Some((fallback_slots, fallback_refs, fallback_placements)) => {
                        (fallback_slots, fallback_refs, fallback_placements, 1)
                    }
                    None => {
                        // This should never happen, but if it does, panic rather than skip
                        panic!("BUG: Cannot generate fallback sentence for word '{}' at index {}. This should never happen.",
                               word_name, payload_i);
                    }
                }
            }
        };

        // Advance payload_i to account for forced placements
        let max_forced_idx = forced_placements.values().max().copied().unwrap_or(payload_i_before.saturating_sub(1));
        let mut temp_payload_i = (max_forced_idx + 1).max(payload_i_before);

        let payload_only_mode = matches!(mode, GenerationMode::PayloadOnly);
        let mut sentence_words = fill_slots(
            rng,
            lex,
            &slots,
            &refinements,
            payload,
            &mut temp_payload_i,
            &prev_words_refs,
            None,
            Some(&forced_placements),
            payload_only_mode,
            prime_constraint_enabled,
            grammar.dot_is_punctuation(),
        );

        // Update payload_i to reflect what was actually used
        payload_i = temp_payload_i.max(max_forced_idx + 1);
        
        // Check if payload word was placed - this should always be true with forced placements
        if payload_i <= payload_i_before && forced_placements.is_empty() {
            let slots_str: Vec<String> = slots.iter().map(|pos| pos.to_string()).collect();
            
            let next_payload_word = if payload_i_before < payload.len() {
                format!("{} (allowed POS: {:?})", payload[payload_i_before].word, payload[payload_i_before].allowed)
            } else {
                "none".to_string()
            };
            
            panic!(
                "BUG: Generated sentence with no payload words!\n\
                 Start symbol: {}\n\
                 Next payload word: {}\n\
                 Generated slots: {}\n\
                 Sentence: {}\n\
                 This should never happen - the planner should guarantee payload word placement.",
                start_symbol,
                next_payload_word,
                slots_str.join(" "),
                sentence_words.join(" ")
            );
        }
        
        payload_i = temp_payload_i.max(payload_i);

        // Count actual payload words in the generated sentence
        let actual_payload_count = sentence_words.iter()
            .filter(|word| {
                let word_clean = normalize_token_for_bip39(word);
                !word_clean.is_empty() && validation_payload_set.contains(&word_clean)
            })
            .count();

        // Print grammar structure in verbose mode
        if verbose {
            let grammar_str: Vec<String> = slots.iter().map(|pos| pos.to_string()).collect();
            eprintln!("Grammar rule: {} -> {} (embeds {} payload words)",
                     start_symbol, grammar_str.join(" "), actual_payload_count);
        }

        // Print actual word-to-POS mapping in verbose mode
        if verbose {
            let mut word_pos_mapping: Vec<String> = Vec::new();
            let mut word_idx = 0;
            let mut current_payload_idx = payload_i_before;
            for &slot in slots.iter() {
                if slot == Pos::Dot {
                    continue; // Skip Dot, punctuation is attached to previous word
                }
                if word_idx < sentence_words.len() {
                    let word_with_punct = &sentence_words[word_idx];
                    let word_clean = word_with_punct.trim_end_matches('.').to_lowercase();
                    let pos_str = slot.as_str();
                    // Mark payload words with * and show their allowed POS tags
                    if payload_set.contains(&word_clean) && current_payload_idx < payload.len() {
                        let payload_tok = &payload[current_payload_idx];
                        let allowed_pos: Vec<String> = payload_tok.allowed.iter().map(|p| p.to_string()).collect();
                        word_pos_mapping.push(format!("{}*:{}[{}]", word_clean, pos_str, allowed_pos.join(",")));
                        current_payload_idx += 1;
                    } else {
                        word_pos_mapping.push(format!("{}:{}", word_clean, pos_str));
                    }
                    word_idx += 1;
                }
            }
            eprintln!("Words:   {}", word_pos_mapping.join(" "));
        }

        // Only add the sentence if it contains at least one payload word
        // Check if any word placed by forced_placements is an actual payload word (not a Merkle word)
        // OR if any word in the sentence is an actual payload word
        let forced_contains_payload = forced_placements.values().any(|&idx| {
            idx < payload.len() && validation_payload_set.contains(&payload[idx].word.to_lowercase())
        });
        let sentence_contains_payload = sentence_words.iter().any(|word| {
            let word_clean = normalize_token_for_bip39(word);
            !word_clean.is_empty() && validation_payload_set.contains(&word_clean)
        });
        
        // Accept sentence if it has payload words OR if forced placements placed payload words
        // (forced placements should always place payload words, but check both for safety)
        if payload_i > payload_i_before && (sentence_contains_payload || forced_contains_payload) {
            // Capitalize the first word of the sentence.
            if let Some(first) = sentence_words.first_mut() {
                *first = capitalize(first);
            }

            // Print sentence as it's generated if verbose
            if verbose {
                let sentence_text: String = sentence_words.iter()
                    .map(|w| {
                        let word_clean = normalize_token_for_bip39(w);
                        if !word_clean.is_empty() && payload_set.contains(&word_clean) {
                            // In library version, don't apply highlighting - just return the word
                            w.clone()
                        } else {
                            w.clone()
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(" ");
                eprintln!("{}", sentence_text);
            }

            // Add spacing between sentences.
            if !words.is_empty() {
                // ensure previous ended with punctuation. (We put '.' on last token)
            }
            words.append(&mut sentence_words);
        } else {
            // Sentence contained no payload words - skip it
            // Reset payload_i since we didn't actually use this sentence
            payload_i = payload_i_before;
            if verbose {
                eprintln!("Skipping sentence with no payload words");
            }
        }
        }
    }

    // Post-fix: ensure output ends with a period (only for body mode, not subject mode)
    // Skip periods for primes language (only integers in vocabulary)
    // Skip periods for CS grammar where Dot is a structural token, not punctuation
    if mode == GenerationMode::Body && grammar.grammar_uses_pos(Pos::Dot) && grammar.dot_is_punctuation() {
        if let Some(last) = words.last_mut() {
            if !last.ends_with('.') {
                last.push('.');
            }
        }
    }

    // Return unhighlighted text (caller can apply highlighting if needed)
    let text = join_words_with_payload_grammar(&words, &payload_set, &grammar);
    (text, payload_set)
}

/// Generate text using merkle segmentation: segment sequence into chunks ending with 1-2 payload words
fn generate_text_merkle_segmented<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    payload: &[PayloadTok],
    original_payload_set: &HashSet<String>,
    verbose: bool,
    language: &str,
    k_min: usize,
    k_max: usize,
    length_mode: SentenceLengthMode,
    prime_constraint_enabled: bool,
    cache: &SequenceCache,
) -> (String, HashSet<String>) {
    use super::utils::normalize_token_for_bip39;

    let grammar = get_grammar(GenerationMode::Body, language);
    let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
    let mut words: Vec<String> = Vec::new();
    let mut segment_start = 0;
    let mut sentence_count = 0;
    const MAX_SENTENCES: usize = 200;
    
    while segment_start < payload.len() && sentence_count < MAX_SENTENCES {
        sentence_count += 1;
        
        // Find next segment: collect words until we find 1-2 payload words
        let mut segment_end = segment_start;
        let mut payload_count = 0;
        let mut payload_indices_in_segment = Vec::new();
        
        while segment_end < payload.len() && payload_count < 2 {
            let word_lower = payload[segment_end].word.to_lowercase();
            if original_payload_set.contains(&word_lower) {
                payload_indices_in_segment.push(segment_end);
                payload_count += 1;
            }
            segment_end += 1;
        }
        
        // If we didn't find any payload words, we're done
        if payload_indices_in_segment.is_empty() {
            if verbose {
                eprintln!("Warning: Segment starting at {} contains no payload words, ending generation", segment_start);
            }
            break;
        }
        
        // Get the first payload word's POS for start_symbol selection
        let first_payload_idx = payload_indices_in_segment[0];
        let first_payload_word = &payload[first_payload_idx];
        
        if first_payload_word.allowed.is_empty() {
            panic!(
                "BUG: Payload word '{}' has no allowed POS tags!",
                first_payload_word.word
            );
        }
        
        let pos = if first_payload_word.allowed.contains(&Pos::N) {
            Pos::N
        } else if first_payload_word.allowed.contains(&Pos::V) {
            Pos::V
        } else if first_payload_word.allowed.contains(&Pos::Adj) {
            Pos::Adj
        } else if first_payload_word.allowed.contains(&Pos::Adv) {
            Pos::Adv
        } else if first_payload_word.allowed.contains(&Pos::Prep) {
            Pos::Prep
        } else {
            first_payload_word.allowed.iter().next().copied().expect("Payload word should have at least one POS tag")
        };
        
        let nt = start_nonterminal_for_pos(pos);
        let start_symbol = if grammar.rules.contains_key(nt) || grammar.language_config.is_some() {
            nt
        } else {
            "S"
        };
        
        // Pass the last few words from previous sentence to prevent repetition
        const REPETITION_WINDOW: usize = 3;
        let prev_words: Vec<String> = words
            .iter()
            .rev()
            .take(REPETITION_WINDOW)
            .map(|s| {
                s.trim_end_matches('.').trim_end_matches(' ').to_lowercase()
            })
            .rev()
            .collect();
        let prev_words_refs: Vec<&str> = prev_words.iter().map(|s| s.as_str()).collect();
        
        // Plan sentence: need to embed payload words from this segment
        // Use the first payload word index as payload_i
        let payload_i_before = first_payload_idx;
        let mut payload_i = first_payload_idx;
        
        // Compute k candidates
        let k_candidates = compute_k_candidates(
            rng,
            cache,
            start_symbol,
            k_min,
            k_max,
            length_mode,
            false, // Body mode never requires prefix
        );
        
        let mut planned = None;
        for k in k_candidates {
            // Plan sentence that embeds the payload words from this segment
            // We need to ensure at least one payload word is placed
            if let Some((slots, refinements, forced_placements, j)) = plan_sentence(
                rng,
                cache,
                start_symbol,
                k,
                payload,
                payload_i,
                false, // Body mode never requires prefix
            ) {
                // Check if forced placements include our payload words
                let places_our_payload = forced_placements.values().any(|&idx| {
                    payload_indices_in_segment.contains(&idx)
                });
                
                if places_our_payload || j > 0 {
                    planned = Some((slots, refinements, forced_placements, j));
                    break;
                }
            }
        }
        
        // Fallback to "S" if preferred start_symbol failed
        if planned.is_none() && start_symbol != "S" {
            let k_candidates_fallback = compute_k_candidates(
                rng,
                cache,
                "S",
                k_min,
                k_max,
                length_mode,
                false,
            );
            for k in k_candidates_fallback {
                if let Some((slots, refinements, forced_placements, j)) = plan_sentence(
                    rng,
                    cache,
                    "S",
                    k,
                    payload,
                    payload_i,
                    false,
                ) {
                    let places_our_payload = forced_placements.values().any(|&idx| {
                        payload_indices_in_segment.contains(&idx)
                    });
                    if places_our_payload || j > 0 {
                        if verbose {
                            let grammar_str: Vec<String> = slots.iter().map(|pos| pos.to_string()).collect();
                            eprintln!("Segment {}: Selected grammar rule (fallback): S -> {} (k={}, j={} payload words)",
                                     sentence_count, grammar_str.join(" "), k, j);
                        }
                        planned = Some((slots, refinements, forced_placements, j));
                        break;
                    }
                }
            }
        }
        
        // Generate fallback if planning failed
        let (slots, refinements, forced_placements, _j) = match planned {
            Some((s, r, f, j)) => {
                (s, r, f, j)
            }
            None => {
                if verbose {
                    eprintln!("Warning: Could not plan sentence for segment. Using fallback structure.");
                }
                match generate_fallback_sentence(payload, payload_i, GenerationMode::Body, &grammar) {
                    Some((fallback_slots, fallback_refs, fallback_placements)) => {
                        if verbose {
                            let grammar_str: Vec<String> = fallback_slots.iter().map(|pos| pos.to_string()).collect();
                            eprintln!("Segment {}: Fallback grammar rule: {} -> {}",
                                     sentence_count, start_symbol, grammar_str.join(" "));
                        }
                        (fallback_slots, fallback_refs, fallback_placements, 1)
                    }
                    None => {
                        panic!("BUG: Cannot generate fallback sentence for segment starting at {}", segment_start);
                    }
                }
            }
        };
        
        // Fill slots
        let mut temp_payload_i = payload_i;
        let mut sentence_words = fill_slots(
            rng,
            lex,
            &slots,
            &refinements,
            payload,
            &mut temp_payload_i,
            &prev_words_refs,
            None,
            Some(&forced_placements),
            false,
            prime_constraint_enabled,
            grammar.dot_is_punctuation(),
        );

        // Update payload_i to reflect what was actually used
        let max_forced_idx = forced_placements.values().max().copied().unwrap_or(payload_i_before);
        payload_i = temp_payload_i.max(max_forced_idx + 1);
        
        // Count actual payload words in the generated sentence
        let actual_payload_count = sentence_words.iter()
            .filter(|word| {
                let word_clean = normalize_token_for_bip39(word);
                !word_clean.is_empty() && original_payload_set.contains(&word_clean)
            })
            .count();

        // Print grammar structure in verbose mode (after sentence generation)
        if verbose {
            let grammar_str: Vec<String> = slots.iter().map(|pos| pos.to_string()).collect();
            eprintln!("Segment {}: Grammar rule: {} -> {} (embeds {} payload words)",
                     sentence_count, start_symbol, grammar_str.join(" "), actual_payload_count);
        }

        // Verify sentence contains at least one payload word from our segment
        let sentence_contains_payload = actual_payload_count > 0;
        
        if sentence_contains_payload {
            // Capitalize first word
            if let Some(first) = sentence_words.first_mut() {
                *first = capitalize(first);
            }
            
            // Add sentence
            if !words.is_empty() {
                // Add space between sentences
            }
            words.append(&mut sentence_words);
            
            // Move to next segment: advance to after the last payload word we used
            segment_start = payload_i;
        } else {
            // Sentence didn't contain payload words - skip and try next segment
            if verbose {
                eprintln!("Skipping sentence with no payload words from segment");
            }
            // Advance segment_start to skip problematic words
            segment_start = segment_end.min(segment_start + 1);
        }
    }
    
    // Post-fix: ensure output ends with a period
    // Skip for CS grammar where Dot is structural, not punctuation
    if grammar.grammar_uses_pos(Pos::Dot) && grammar.dot_is_punctuation() {
        if let Some(last) = words.last_mut() {
            if !last.ends_with('.') {
                last.push('.');
            }
        }
    }

    let text = join_words_with_payload_grammar(&words, &payload_set, &grammar);
    (text, payload_set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_words_default_separator() {
        // Default separator " " — standard word joining
        let grammar = Grammar::from_language_dialect("english", "body")
            .expect("Failed to load English grammar");
        let words: Vec<String> = vec!["the", "cat", "sat."].iter().map(|s| s.to_string()).collect();
        let payload_set: HashSet<String> = ["cat"].iter().map(|s| s.to_string()).collect();
        let result = join_words_with_payload_grammar(&words, &payload_set, &grammar);
        assert_eq!(result, "the cat sat.");
    }

    #[test]
    fn test_join_words_concat_separator() {
        // CS grammar: payload_separator="" — payload words concatenated
        let grammar = Grammar::from_language_dialect("cs", "body")
            .expect("Failed to load CS grammar");
        // Simulate: HEADER cover words, then payload chars, then FOOTER cover words
        let words: Vec<String> = vec!["-----.", "BEGIN", "NIP-04", "a", "B", "3", "x", "-----.", "END", "NIP-04"]
            .iter().map(|s| s.to_string()).collect();
        // payload words: the base58 chars (lowercased for set matching)
        let payload_set: HashSet<String> = ["a", "b", "3", "x"].iter().map(|s| s.to_string()).collect();
        let result = join_words_with_payload_grammar(&words, &payload_set, &grammar);
        // Payload chars should be concatenated, cover words spaced
        assert!(result.contains("aB3x"), "Payload chars should be concatenated: got '{}'", result);
        assert!(result.contains("BEGIN"), "Cover words should be present");
    }

    #[test]
    fn test_wrap_payload() {
        assert_eq!(wrap_payload("abcdef", 3), "abc\ndef");
        assert_eq!(wrap_payload("ab", 3), "ab");
        assert_eq!(wrap_payload("abcdefghi", 3), "abc\ndef\nghi");
        assert_eq!(wrap_payload("", 3), "");
    }
}
