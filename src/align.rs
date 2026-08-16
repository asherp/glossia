//! Token alignment between received prose and the rendering it should have been.
//!
//! Decoding filters prose against the payload wordlist, which means transcription
//! damage does not stay put. A payload word mangled off the wordlist does not
//! arrive wrong — it does not arrive at all, and every later word slides up one
//! slot. A cover word mangled onto the wordlist arrives as a payload word that
//! was never sent, and every later word slides down. Either way the harvested
//! sequence is no longer positionally comparable to the sequence that was
//! encoded, so a positional code cannot be applied to it and a per-word verdict
//! cannot be given.
//!
//! Aligning the received text against its expected rendering recovers those
//! positions. The output is [`Alignment::payload_slots`]: one entry per payload
//! word the rendering expected, holding the word that actually arrived there or
//! `None` where nothing usable did. That is the shape an error-correcting code
//! wants — a codeword of known length with its holes marked — and
//! [`Alignment::erasures`] names the holes.
//!
//! This is a verification aid, not a bootstrap. It needs the expected rendering,
//! which means it needs a candidate decode to render from. The caller supplies
//! both; what this module contributes is the mapping between them.

use crate::codec::{normalize_token, Markup};

/// What became of one position in the alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// The received token is the word the rendering expected.
    Same,
    /// One word written in place of another. Reported as a single substitution
    /// rather than a deletion followed by an insertion: when skipping either
    /// side costs the same, "this should say X" is both shorter and truer to
    /// what a transcriber did.
    Sub,
    /// The received text carries a token the rendering does not — a word added,
    /// or a cover word mangled onto the payload wordlist.
    Insert,
    /// The rendering carries a word the received text does not — a word dropped,
    /// or a payload word mangled off the wordlist.
    Delete,
}

/// One aligned position, carrying both sides where both exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedToken {
    pub op: Op,
    /// The received token, normalized. Absent for [`Op::Delete`].
    pub received: Option<String>,
    /// Index into the received token sequence. Absent for [`Op::Delete`].
    pub received_index: Option<usize>,
    /// The word the rendering expected here. Absent for [`Op::Insert`].
    pub expected: Option<String>,
    /// Index into the rendered token sequence. Absent for [`Op::Insert`].
    pub expected_index: Option<usize>,
    /// Which payload slot the expected word occupies, if it is a payload word.
    /// This is the index into [`Alignment::payload_slots`], and the coordinate
    /// an error-correcting code works in.
    pub payload_index: Option<usize>,
    /// Whether the received token is itself a payload word. A substitution where
    /// this is false is a payload word knocked off the wordlist — invisible to
    /// the harvest, and the reason the slot is an erasure rather than an error.
    pub received_is_payload: bool,
}

/// The result of aligning received prose against its expected rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alignment {
    /// Every position, in reading order.
    pub tokens: Vec<AlignedToken>,
    /// How many tokens matched outright — the length of the longest common
    /// subsequence.
    pub matched: usize,
    /// One entry per payload word the rendering expected: the word that arrived
    /// in that slot, or `None` where nothing usable did. Length is the expected
    /// payload word count, whatever the received text did to it.
    pub payload_slots: Vec<Option<String>>,
    /// Slots of `payload_slots` that are `None`, in ascending order. Known-position
    /// unknown-value damage: an erasure costs an error-correcting code half what
    /// an unlocated error does.
    pub erasures: Vec<usize>,
    /// Received payload words with no counterpart in the rendering — cover words
    /// mangled onto the wordlist. These must be dropped from the harvest, not
    /// fed to a decoder, and their received indices are given here.
    pub spurious: Vec<usize>,
}

impl Alignment {
    /// Whether the received text reproduces the rendering word for word.
    pub fn is_clean(&self) -> bool {
        self.tokens.iter().all(|t| t.op == Op::Same)
    }

    /// How many payload slots arrived intact.
    pub fn payload_intact(&self) -> usize {
        self.payload_slots.len() - self.erasures.len()
    }
}

/// Split text into normalized tokens, dropping any that normalize away to
/// nothing. Comparison runs on the normalized form so that a changed capital or
/// a moved comma is not reported as damage; the wordlist does not carry either.
fn tokenize(text: &str, markup: Option<&Markup>) -> Vec<String> {
    text.split_whitespace()
        .map(|w| match markup {
            Some(m) => m.normalize_token(w),
            None => normalize_token(w),
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Longest-common-subsequence table over two token sequences, computed from the
/// end so that `l[i][j]` is the LCS of `a[i..]` and `b[j..]`. The forward walk in
/// [`align_tokens`] reads it in that direction.
fn lcs_table(a: &[String], b: &[String]) -> Vec<Vec<u32>> {
    let (n, m) = (a.len(), b.len());
    let mut l = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            l[i][j] = if a[i] == b[j] {
                l[i + 1][j + 1] + 1
            } else {
                l[i + 1][j].max(l[i][j + 1])
            };
        }
    }
    l
}

/// Align two token sequences, returning the edit script as a flat op list.
///
/// Exposed separately from [`align`] so the walk can be tested without a
/// wordlist, and so a caller that has already tokenized need not do it twice.
pub fn align_tokens(received: &[String], rendered: &[String]) -> (Vec<(Op, usize, usize)>, usize) {
    let (n, m) = (received.len(), rendered.len());
    let l = lcs_table(received, rendered);
    let matched = l[0][0] as usize;

    let mut ops = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if received[i] == rendered[j] {
            ops.push((Op::Same, i, j));
            i += 1;
            j += 1;
        } else if l[i + 1][j] == l[i][j + 1] {
            // Dropping either side costs the same, so read it as one word
            // written in place of another rather than as two separate edits.
            ops.push((Op::Sub, i, j));
            i += 1;
            j += 1;
        } else if l[i + 1][j] > l[i][j + 1] {
            ops.push((Op::Insert, i, j));
            i += 1;
        } else {
            ops.push((Op::Delete, i, j));
            j += 1;
        }
    }
    while i < n {
        ops.push((Op::Insert, i, m));
        i += 1;
    }
    while j < m {
        ops.push((Op::Delete, n, j));
        j += 1;
    }
    (ops, matched)
}

/// Align received prose against the rendering it should have been.
///
/// `is_payload` decides which words are payload — the same predicate the decode
/// harvest uses, and it must be the same one, since a slot exists in
/// [`Alignment::payload_slots`] exactly when the rendering put a payload word
/// there. `markup` strips declared decoration from token edges first, for
/// formats that set prose inside sigils.
pub fn align<F>(
    received: &str,
    rendered: &str,
    markup: Option<&Markup>,
    is_payload: F,
) -> Alignment
where
    F: Fn(&str) -> bool,
{
    let recv = tokenize(received, markup);
    let rend = tokenize(rendered, markup);

    // Payload slot numbers are assigned by walking the RENDERING, because the
    // rendering is the sequence whose length and order are known to be right.
    // The received text's own payload count is exactly what damage corrupts.
    let mut slot_of_rendered = vec![None; rend.len()];
    let mut payload_len = 0usize;
    for (k, w) in rend.iter().enumerate() {
        if is_payload(w) {
            slot_of_rendered[k] = Some(payload_len);
            payload_len += 1;
        }
    }

    let (ops, matched) = align_tokens(&recv, &rend);

    let mut payload_slots: Vec<Option<String>> = vec![None; payload_len];
    let mut spurious = Vec::new();
    let mut tokens = Vec::with_capacity(ops.len());

    for (op, i, j) in ops {
        let received_word = if op == Op::Delete { None } else { recv.get(i).cloned() };
        let expected_word = if op == Op::Insert { None } else { rend.get(j).cloned() };
        let payload_index = if op == Op::Insert { None } else { slot_of_rendered.get(j).copied().flatten() };
        let received_is_payload = received_word.as_deref().is_some_and(&is_payload);

        match op {
            // The expected word arrived. Fill its slot.
            Op::Same => {
                if let (Some(slot), Some(w)) = (payload_index, received_word.as_ref()) {
                    payload_slots[slot] = Some(w.clone());
                }
            }
            // Something else arrived in place of the expected word. Which of
            // the two wordlists each side belongs to decides what that means,
            // and all four combinations occur:
            //
            //   payload → payload   the slot holds the wrong symbol (an error)
            //   payload → cover     the harvest lost it (the slot is a hole)
            //   cover   → payload   a symbol nobody sent (spurious)
            //   cover   → cover     cover damage; no payload consequence
            //
            // The third is the one a pure-insertion reading misses: a cover
            // word mangled onto the wordlist most often lands ON another word
            // rather than beside it, so it arrives here and not as an Insert.
            Op::Sub => match (payload_index, received_is_payload) {
                (Some(slot), true) => {
                    payload_slots[slot] = received_word.clone();
                }
                (Some(_), false) => {}
                (None, true) => spurious.push(i),
                (None, false) => {}
            },
            // Nothing arrived in the slot; it stays None.
            Op::Delete => {}
            // A token with no counterpart. Only a payload one matters: it will
            // appear in the harvest without having been sent.
            Op::Insert => {
                if received_is_payload {
                    spurious.push(i);
                }
            }
        }

        tokens.push(AlignedToken {
            op,
            received: received_word,
            received_index: if op == Op::Delete { None } else { Some(i) },
            expected: expected_word,
            expected_index: if op == Op::Insert { None } else { Some(j) },
            payload_index,
            received_is_payload,
        });
    }

    let erasures = payload_slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_none())
        .map(|(k, _)| k)
        .collect();

    Alignment { tokens, matched, payload_slots, erasures, spurious }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn payload_set(words: &[&str]) -> HashSet<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    /// A clean transcription fills every slot and reports no damage.
    #[test]
    fn clean_text_aligns_wholly() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align(
            "the absorb of banana saw cactus",
            "the absorb of banana saw cactus",
            None,
            |w| set.contains(w),
        );
        assert!(a.is_clean());
        assert_eq!(a.payload_slots.len(), 3);
        assert!(a.erasures.is_empty());
        assert!(a.spurious.is_empty());
        assert_eq!(a.payload_intact(), 3);
    }

    /// Case and punctuation are not damage: the wordlist carries neither, and
    /// reporting them would bury real damage in noise.
    #[test]
    fn casing_and_punctuation_are_not_damage() {
        let set = payload_set(&["absorb", "banana"]);
        let a = align("Absorb, then banana.", "absorb then banana", None, |w| {
            set.contains(w)
        });
        assert!(a.is_clean());
        assert!(a.erasures.is_empty());
    }

    /// A payload word replaced by another payload word is a located error, not
    /// an erasure: the harvest still delivers a symbol, it is simply the wrong
    /// one. The slot is filled, and the op says it is a substitution.
    #[test]
    fn payload_word_swapped_for_another_fills_the_slot() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align(
            "the absorb of cactus saw cactus",
            "the absorb of banana saw cactus",
            None,
            |w| set.contains(w),
        );
        assert!(a.erasures.is_empty(), "a payload-for-payload swap is not an erasure");
        assert_eq!(a.payload_slots[1].as_deref(), Some("cactus"));
        let sub = a.tokens.iter().find(|t| t.op == Op::Sub).expect("a substitution");
        assert_eq!(sub.payload_index, Some(1));
        assert_eq!(sub.expected.as_deref(), Some("banana"));
        assert!(sub.received_is_payload);
    }

    /// A payload word mangled OFF the wordlist vanishes from the harvest. The
    /// slot must come back empty rather than absorbing the junk word.
    #[test]
    fn payload_word_knocked_off_the_list_is_an_erasure() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align(
            "the absorb of bananna saw cactus",
            "the absorb of banana saw cactus",
            None,
            |w| set.contains(w),
        );
        assert_eq!(a.erasures, vec![1]);
        assert_eq!(a.payload_slots[1], None);
        assert_eq!(a.payload_slots[0].as_deref(), Some("absorb"));
        assert_eq!(a.payload_slots[2].as_deref(), Some("cactus"));
    }

    /// A dropped payload word shifts every later word up one slot. Alignment has
    /// to undo that shift, or slots 2..n all read as damaged.
    #[test]
    fn dropped_payload_word_does_not_shift_later_slots() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align(
            "the absorb of saw cactus",
            "the absorb of banana saw cactus",
            None,
            |w| set.contains(w),
        );
        assert_eq!(a.erasures, vec![1], "only the dropped slot is damaged");
        assert_eq!(
            a.payload_slots[2].as_deref(),
            Some("cactus"),
            "the word after the hole keeps its own slot"
        );
    }

    /// A cover word mangled ONTO the wordlist arrives as a payload word nobody
    /// sent. It must be reported as spurious and must not consume a slot.
    #[test]
    fn cover_word_mangled_onto_the_list_is_spurious() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align(
            "the absorb of banana cactus saw cactus",
            "the absorb of banana saw cactus",
            None,
            |w| set.contains(w),
        );
        assert_eq!(a.spurious.len(), 1, "the extra payload word is spurious");
        assert!(a.erasures.is_empty(), "no slot was actually damaged");
        assert_eq!(a.payload_slots.len(), 3);
        assert_eq!(a.payload_slots[2].as_deref(), Some("cactus"));
    }

    /// A cover word mangled onto the wordlist usually lands ON a word rather
    /// than beside it, so it arrives as a substitution. It is still a symbol
    /// nobody sent, and reporting it only in the pure-insertion case would let
    /// the commonest form of it through into the harvest unremarked.
    #[test]
    fn cover_replaced_in_place_by_a_payload_word_is_spurious() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align(
            "the absorb of banana cactus cactus",
            "the absorb of banana saw cactus",
            None,
            |w| set.contains(w),
        );
        assert_eq!(a.spurious.len(), 1, "the substituted-in payload word is spurious");
        assert!(a.erasures.is_empty(), "no slot lost its word");
        assert_eq!(a.payload_slots.len(), 3);
        assert_eq!(a.payload_slots[2].as_deref(), Some("cactus"));
    }

    /// The mirror case: a payload word overwritten by a cover word leaves the
    /// slot empty rather than holding the cover word.
    #[test]
    fn payload_replaced_in_place_by_a_cover_word_leaves_a_hole() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align(
            "the absorb of saw saw cactus",
            "the absorb of banana saw cactus",
            None,
            |w| set.contains(w),
        );
        assert_eq!(a.erasures, vec![1]);
        assert_eq!(a.payload_slots[1], None, "a cover word is not a symbol");
        assert!(a.spurious.is_empty());
    }

    /// The combination is the case a positional code cannot survive unaided: one
    /// word lost and one gained leaves the harvest the right LENGTH but shifted
    /// through the middle, so a naive decode is wrong everywhere between them
    /// while looking perfectly well-formed.
    #[test]
    fn deletion_and_insertion_together_stay_localized() {
        let set = payload_set(&["absorb", "banana", "cactus", "dolphin"]);
        let a = align(
            "absorb cover cactus dolphin extra dolphin",
            "absorb banana cactus dolphin cover dolphin",
            None,
            |w| set.contains(w),
        );
        // banana was knocked off the list; a spurious dolphin arrived later.
        assert_eq!(a.erasures, vec![1]);
        assert_eq!(a.payload_slots[0].as_deref(), Some("absorb"));
        assert_eq!(a.payload_slots[2].as_deref(), Some("cactus"));
        assert_eq!(a.payload_slots[3].as_deref(), Some("dolphin"));
    }

    /// Slot count follows the rendering, never the received text — that is the
    /// whole point of aligning against a re-render.
    #[test]
    fn slot_count_follows_the_rendering() {
        let set = payload_set(&["absorb", "banana", "cactus"]);
        let a = align("", "absorb banana cactus", None, |w| set.contains(w));
        assert_eq!(a.payload_slots.len(), 3);
        assert_eq!(a.erasures, vec![0, 1, 2], "nothing arrived, so every slot is a hole");
        assert_eq!(a.matched, 0);
    }

    /// Received text with no rendering to compare against yields no slots and no
    /// erasures — there is nothing to be right or wrong about.
    #[test]
    fn empty_rendering_yields_no_slots() {
        let set = payload_set(&["absorb"]);
        let a = align("absorb banana", "", None, |w| set.contains(w));
        assert!(a.payload_slots.is_empty());
        assert!(a.erasures.is_empty());
        assert_eq!(a.spurious.len(), 1);
    }

    /// Declared markup is stripped before comparison, so a format that sets its
    /// prose inside sigils does not read every decorated token as damage.
    #[test]
    fn markup_is_stripped_before_comparing() {
        let set = payload_set(&["absorb", "banana"]);
        let markup = Markup::new(['β', '§'], &["absorb", "banana"]).expect("markup disjoint from the wordlist");
        let a = align("βabsorb §banana", "absorb banana", Some(&markup), |w| {
            set.contains(w)
        });
        assert!(a.is_clean());
        assert_eq!(a.payload_slots[0].as_deref(), Some("absorb"));
    }
}
