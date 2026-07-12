//! Semantic sentence-planning support.
//!
//! Loads a per-word semantic dataset (`languages/<lang>/semantics.yaml`) and
//! scores a candidate POS placement by how well payload words land in
//! semantically coherent verb-argument roles. The generator uses this only as a
//! *soft* bias when choosing among equally-dense candidate sentence skeletons in
//! `plan_sentence`: it never drops, reorders, or blocks a payload word, so
//! decoding is completely unaffected whether or not this data is present.
//!
//! Classes are top-level (`animate | agentive | thing | place | abstract`); a
//! verb frame states which classes its subject and object accept. Roles are
//! inferred from the flat POS sequence: the nearest payload-filled noun slot to
//! a verb's left is its subject, to its right its object (bounded by clause
//! punctuation and other verbs). This mirrors the offline prototype in
//! `experiments/semantic_planner/`.

use crate::generator::types::PayloadTok;
use crate::types::Pos;
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level semantic class of a noun/entity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SemClass {
    Animate,
    Agentive,
    Thing,
    Place,
    Abstract,
}

impl SemClass {
    fn parse(s: &str) -> Option<SemClass> {
        match s {
            "animate" => Some(SemClass::Animate),
            "agentive" => Some(SemClass::Agentive),
            "thing" => Some(SemClass::Thing),
            "place" => Some(SemClass::Place),
            "abstract" => Some(SemClass::Abstract),
            _ => None,
        }
    }
}

/// Selectional restriction on a verb argument: accept anything, or one of a set.
#[derive(Clone, Debug)]
pub enum Sel {
    Any,
    Classes(Vec<SemClass>),
}

impl Sel {
    fn accepts(&self, c: SemClass) -> bool {
        match self {
            Sel::Any => true,
            Sel::Classes(v) => v.contains(&c),
        }
    }
}

/// A verb's subject/object expectations.
#[derive(Clone, Debug)]
pub struct Frame {
    pub subj: Sel,
    pub obj: Sel,
}

/// Multiplicative penalty applied per incoherent verb-argument edge. Chosen so
/// coherent skeletons are strongly preferred but incoherent ones are never
/// impossible (payload placement stays exempt from hard constraints).
const EDGE_PENALTY: f64 = 0.15;
/// Score floor so a candidate weight never collapses to exactly zero.
const SCORE_FLOOR: f64 = 0.02;

#[derive(Clone, Debug, Default)]
pub struct SemanticModel {
    classes: HashMap<String, SemClass>,
    frames: HashMap<String, Frame>,
}

// --- YAML shape ---------------------------------------------------------- //

#[derive(Deserialize)]
struct RawFile {
    #[serde(default)]
    classes: HashMap<String, String>,
    #[serde(default)]
    frames: HashMap<String, RawFrame>,
}

#[derive(Deserialize)]
struct RawFrame {
    subj: RawSel,
    obj: RawSel,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawSel {
    /// The scalar `any` (any other bare string is treated as `any` too).
    /// The captured string is only used by serde to match the scalar form.
    Any(#[allow(dead_code)] String),
    /// A list like `[animate, agentive]`.
    List(Vec<String>),
}

impl RawSel {
    fn into_sel(self) -> Sel {
        match self {
            RawSel::Any(_) => Sel::Any,
            RawSel::List(v) => {
                let classes: Vec<SemClass> = v.iter().filter_map(|s| SemClass::parse(s)).collect();
                // An empty/unparseable list means "no constraint we can enforce".
                if classes.is_empty() {
                    Sel::Any
                } else {
                    Sel::Classes(classes)
                }
            }
        }
    }
}

impl SemanticModel {
    /// Parse a `semantics.yaml` document. Unknown class strings are dropped
    /// (the word simply carries no class), so a malformed entry degrades to
    /// "no semantic opinion" rather than an error.
    pub fn from_yaml(content: &str) -> Result<SemanticModel, String> {
        let raw: RawFile =
            serde_yaml::from_str(content).map_err(|e| format!("semantics.yaml parse error: {e}"))?;
        let classes = raw
            .classes
            .into_iter()
            .filter_map(|(w, c)| SemClass::parse(&c).map(|cls| (w.to_lowercase(), cls)))
            .collect();
        let frames = raw
            .frames
            .into_iter()
            .map(|(w, rf)| {
                (
                    w.to_lowercase(),
                    Frame {
                        subj: rf.subj.into_sel(),
                        obj: rf.obj.into_sel(),
                    },
                )
            })
            .collect();
        Ok(SemanticModel { classes, frames })
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.frames.is_empty()
    }

    /// `(number of classified words, number of verb frames)` — for diagnostics
    /// and tests that a real dataset loaded.
    pub fn stats(&self) -> (usize, usize) {
        (self.classes.len(), self.frames.len())
    }

    fn class_of(&self, word: &str) -> Option<SemClass> {
        self.classes.get(&word.to_lowercase()).copied()
    }

    /// Class of the nearest payload-filled noun slot on one side of `from`,
    /// stopping at clause punctuation or another verb (a clause boundary).
    fn nearest_payload_noun(
        &self,
        slots: &[Pos],
        placement: &HashMap<usize, usize>,
        payload: &[PayloadTok],
        from: usize,
        forward: bool,
    ) -> Option<SemClass> {
        let idxs: Vec<usize> = if forward {
            (from + 1..slots.len()).collect()
        } else {
            (0..from).rev().collect()
        };
        for i in idxs {
            match slots[i] {
                Pos::Dot | Pos::V => break, // clause boundary
                Pos::N => {
                    if let Some(&pidx) = placement.get(&i) {
                        if let Some(c) = self.class_of(&payload[pidx].word) {
                            return Some(c);
                        }
                    }
                    // an unclassified or cover-filled noun: keep looking is wrong
                    // (nearest noun is the argument); stop at the first noun slot.
                    break;
                }
                _ => {} // Det / Adj / Prep etc. — skip over
            }
        }
        None
    }

    /// Coherence multiplier in (0, 1] for a candidate placement. 1.0 means every
    /// payload verb whose subject/object is also a payload word is satisfied (or
    /// unknown). Each violated edge multiplies the score by `EDGE_PENALTY`.
    pub fn placement_score(
        &self,
        slots: &[Pos],
        placement: &HashMap<usize, usize>,
        payload: &[PayloadTok],
    ) -> f64 {
        let mut score = 1.0f64;
        for (i, pos) in slots.iter().enumerate() {
            if *pos != Pos::V {
                continue;
            }
            let vidx = match placement.get(&i) {
                Some(&x) => x,
                None => continue, // cover verb — frame unknown at plan time
            };
            let frame = match self.frames.get(&payload[vidx].word.to_lowercase()) {
                Some(f) => f,
                None => continue,
            };
            if let Some(c) = self.nearest_payload_noun(slots, placement, payload, i, false) {
                if !frame.subj.accepts(c) {
                    score *= EDGE_PENALTY;
                }
            }
            if let Some(c) = self.nearest_payload_noun(slots, placement, payload, i, true) {
                if !frame.obj.accepts(c) {
                    score *= EDGE_PENALTY;
                }
            }
        }
        score.max(SCORE_FLOOR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> SemanticModel {
        let yaml = r#"
classes:
  clock: thing
  captain: animate
  engine: agentive
  mountain: place
  idea: abstract
frames:
  discover: { subj: [animate], obj: any }
  process:  { subj: [animate, agentive], obj: any }
  exist:    { subj: any, obj: any }
"#;
        SemanticModel::from_yaml(yaml).unwrap()
    }

    fn tok(word: &str, pos: Pos) -> PayloadTok {
        PayloadTok::new(word, &[pos])
    }

    #[test]
    fn parses_classes_and_frames() {
        let m = model();
        assert_eq!(m.class_of("clock"), Some(SemClass::Thing));
        assert_eq!(m.class_of("CAPTAIN"), Some(SemClass::Animate)); // case-insensitive
        assert!(m.frames.contains_key("discover"));
        assert!(m.class_of("nonesuch").is_none());
    }

    #[test]
    fn sel_any_accepts_all() {
        let m = model();
        // exist has subj: any -> no penalty regardless of subject class
        let slots = vec![Pos::N, Pos::V, Pos::Dot];
        let payload = vec![tok("clock", Pos::N), tok("exist", Pos::V)];
        let mut placement = HashMap::new();
        placement.insert(0, 0);
        placement.insert(1, 1);
        assert_eq!(m.placement_score(&slots, &placement, &payload), 1.0);
    }

    #[test]
    fn incoherent_subject_penalized() {
        let m = model();
        // "clock discover ..." — discover wants animate subject, clock is thing.
        let slots = vec![Pos::N, Pos::V, Pos::N, Pos::Dot];
        let payload = vec![tok("clock", Pos::N), tok("discover", Pos::V), tok("idea", Pos::N)];
        let mut p = HashMap::new();
        p.insert(0, 0);
        p.insert(1, 1);
        p.insert(2, 2);
        let s = m.placement_score(&slots, &p, &payload);
        assert!(s < 1.0, "incoherent subject should be penalized, got {s}");
        assert!((s - EDGE_PENALTY).abs() < 1e-9, "one violated edge, got {s}");
    }

    #[test]
    fn coherent_subject_unpenalized() {
        let m = model();
        // "captain discover idea" — animate subject, obj any -> fully coherent.
        let slots = vec![Pos::N, Pos::V, Pos::N, Pos::Dot];
        let payload = vec![tok("captain", Pos::N), tok("discover", Pos::V), tok("idea", Pos::N)];
        let mut p = HashMap::new();
        p.insert(0, 0);
        p.insert(1, 1);
        p.insert(2, 2);
        assert_eq!(m.placement_score(&slots, &p, &payload), 1.0);
    }

    #[test]
    fn agentive_subject_allowed_for_process() {
        let m = model();
        // "engine process idea" — process accepts agentive; must NOT be penalized.
        let slots = vec![Pos::N, Pos::V, Pos::N, Pos::Dot];
        let payload = vec![tok("engine", Pos::N), tok("process", Pos::V), tok("idea", Pos::N)];
        let mut p = HashMap::new();
        p.insert(0, 0);
        p.insert(1, 1);
        p.insert(2, 2);
        assert_eq!(m.placement_score(&slots, &p, &payload), 1.0);
    }

    #[test]
    fn cover_filled_verb_is_ignored() {
        let m = model();
        // verb slot not in placement (cover verb) -> no scoring, score 1.0
        let slots = vec![Pos::N, Pos::V, Pos::Dot];
        let payload = vec![tok("clock", Pos::N)];
        let mut p = HashMap::new();
        p.insert(0, 0); // only the noun is a payload word
        assert_eq!(m.placement_score(&slots, &p, &payload), 1.0);
    }

    #[test]
    fn score_never_zero() {
        let m = model();
        // many violations still floored above zero
        let slots = vec![Pos::N, Pos::V, Pos::Dot, Pos::N, Pos::V, Pos::Dot];
        let payload = vec![
            tok("clock", Pos::N),
            tok("discover", Pos::V),
            tok("clock", Pos::N),
            tok("discover", Pos::V),
        ];
        // Not a fully realistic placement, but exercises the floor.
        let mut p = HashMap::new();
        p.insert(0, 0);
        p.insert(1, 1);
        p.insert(3, 2);
        p.insert(4, 3);
        let s = m.placement_score(&slots, &p, &payload);
        assert!(s >= SCORE_FLOOR);
    }
}
