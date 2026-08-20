//! Metrical sentence planning: syllable counts, scansion, and meter-aware cover
//! selection.
//!
//! Loads a per-word prosody dataset (`languages/<lang>/prosody.yaml`) and lets a
//! dialect declare a verse form (`meter:` in `grammar.yaml`). When one is
//! declared, the generator chooses sentence shapes the meter can survive and
//! fills their cover slots with words that keep the lines scanning. When none is
//! declared — every dialect that ships today — nothing here runs and generation
//! is byte-for-byte what it was.
//!
//! **Payload words are never chosen, reordered, or dropped for the meter.** A
//! payload word that will not scan where it lands is placed anyway and the line
//! simply breaks; best-of-N then prefers the candidates that did scan. So a
//! metered rendering decodes by exactly the rule every other rendering does —
//! filter the text against the payload wordlist.
//!
//! Why cover slots are enough to carry a meter (`experiments/prosody/`): a
//! monosyllable is metrically flexible, so placing one always scans *and* flips
//! the parity of everything after it. Parity is therefore repairable at any slot
//! that offers both a one- and a two-syllable word, and every content-bearing POS
//! in the cover list offers both. Measured against the real grammar, syllable-
//! counted verse costs no density at all and stress meter about 10%.

use crate::types::Pos;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use std::collections::HashMap;

/// How strictly stress must line up with the beat.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum StressMode {
    /// Syllable counting only — lines have the right length, stress is free.
    #[default]
    Free,
    /// No primary stress may fall on a weak beat. The standard reading of
    /// "does it scan", and the one English verse actually observes.
    Lenient,
    /// Additionally, no unstressed syllable of a polysyllable may sit on a
    /// strong beat. Stricter than real verse practice; fails often.
    Strict,
}

impl StressMode {
    fn parse(s: &str) -> Option<StressMode> {
        match s {
            "free" | "none" | "syllabic" => Some(StressMode::Free),
            "lenient" => Some(StressMode::Lenient),
            "strict" => Some(StressMode::Strict),
            _ => None,
        }
    }
}

/// A verse form: line lengths in syllables (cycling), and how stress must sit.
///
/// `rise` picks which parity carries the beat — `true` for rising feet (iambic,
/// da-DUM), `false` for falling (trochaic, DUM-da).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MeterSpec {
    pub lines: Vec<usize>,
    pub mode: StressMode,
    pub rise: bool,
}

impl MeterSpec {
    /// Line length at cycle position `line`.
    pub fn line_len(&self, line: usize) -> usize {
        self.lines[line % self.lines.len()]
    }

    /// Parse the `meter:` block of a dialect in `grammar.yaml`:
    ///
    /// ```yaml
    /// meter:
    ///   lines: [5, 7, 5]     # syllables per line, cycling
    ///   stress: lenient      # free | lenient | strict   (default free)
    ///   rise: true           # true = iambic, false = trochaic (default true)
    /// ```
    pub fn from_yaml(value: &serde_yaml::Value) -> Option<MeterSpec> {
        let map = value.as_mapping()?;
        let lines: Vec<usize> = map
            .get(serde_yaml::Value::from("lines"))?
            .as_sequence()?
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as usize))
            .filter(|&n| n > 0)
            .collect();
        if lines.is_empty() {
            return None;
        }
        let mode = map
            .get(serde_yaml::Value::from("stress"))
            .and_then(|v| v.as_str())
            .and_then(StressMode::parse)
            .unwrap_or_default();
        let rise = map
            .get(serde_yaml::Value::from("rise"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        Some(MeterSpec { lines, mode, rise })
    }
}

/// Where the meter currently stands: which line of the cycle, and how many
/// syllables into it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct MeterState {
    pub line: usize,
    pub off: usize,
}

/// A stress pattern: one entry per syllable, holding CMUdict's 0/1/2, or
/// `FLEX` for a syllable whose stress is unknown.
pub type Stress = Vec<u8>;

const FLEX: u8 = b'?';

/// Does a word with this stress pattern, starting at syllable `off` of a line,
/// break the meter?
///
/// Monosyllables never do: English monosyllables take their stress from context,
/// which is standard scansion practice and the reason a one-syllable cover word
/// is always a legal parity repair.
pub fn scans(pat: &[u8], off: usize, spec: &MeterSpec) -> bool {
    if spec.mode == StressMode::Free || pat.len() == 1 {
        return true;
    }
    for (i, &d) in pat.iter().enumerate() {
        let strong = ((off + i) % 2 == 1) == spec.rise;
        match d {
            b'1' if !strong => return false,
            b'0' if strong && spec.mode == StressMode::Strict => return false,
            _ => {}
        }
    }
    true
}

#[derive(Deserialize)]
struct ProsodyFile {
    stress: HashMap<String, String>,
    #[serde(default)]
    rhyme: HashMap<String, String>,
}

/// Per-word syllable, stress and rhyme data for one language.
#[derive(Debug)]
pub struct ProsodyModel {
    /// word -> pronunciation variants, each a stress pattern. A word with more
    /// than one is genuinely flexible for the meter ("record" is both RE-cord
    /// and re-CORD), which is free slack for the fitter.
    stress: FxHashMap<String, Vec<Stress>>,
    /// word -> perfect-rhyme key. Unused by the meter itself; kept because a
    /// rhymed dialect needs it and it costs nothing to carry.
    rhyme: FxHashMap<String, String>,
}

impl ProsodyModel {
    pub fn from_yaml_str(text: &str) -> Result<ProsodyModel, serde_yaml::Error> {
        let file: ProsodyFile = serde_yaml::from_str(text)?;
        let stress = file
            .stress
            .into_iter()
            .map(|(w, s)| {
                let vars: Vec<Stress> = s
                    .split('|')
                    .filter(|v| !v.is_empty())
                    .map(|v| v.bytes().collect())
                    .collect();
                (w.to_lowercase(), vars)
            })
            .filter(|(_, v): &(String, Vec<Stress>)| !v.is_empty())
            .collect();
        let rhyme = file
            .rhyme
            .into_iter()
            .filter(|(_, k)| !k.is_empty())
            .map(|(w, k)| (w.to_lowercase(), k))
            .collect();
        Ok(ProsodyModel { stress, rhyme })
    }

    /// Stress patterns for a word, or `None` when the word is not in the data.
    pub fn variants(&self, word: &str) -> Option<&[Stress]> {
        self.stress.get(&normalize(word)).map(|v| v.as_slice())
    }

    /// Syllable count of a word's primary pronunciation.
    pub fn syllables(&self, word: &str) -> Option<usize> {
        self.variants(word).map(|v| v[0].len())
    }

    /// Perfect-rhyme key, for a dialect that rhymes.
    pub fn rhyme_key(&self, word: &str) -> Option<&str> {
        self.rhyme.get(&normalize(word)).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.stress.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stress.is_empty()
    }

    /// A word the data does not know contributes one syllable of unknown stress,
    /// so it never blocks a line — it just carries no evidence. Inflected cover
    /// forms are the usual case.
    fn variants_or_flex(&self, word: &str) -> Vec<Stress> {
        match self.variants(word) {
            Some(v) => v.to_vec(),
            None => vec![vec![FLEX]],
        }
    }
}

/// Trailing punctuation is part of the rendering, not the word.
fn normalize(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != ':')
        .to_lowercase()
}

/// The prosody data plus the index a filler actually queries: for a cover slot
/// of a given POS and refinement, which stress patterns are available.
///
/// Built once when the model is attached to a `Lexicon`. Each POS has only a
/// handful of distinct patterns, so the per-slot scan is over ~20 entries rather
/// than the whole wordlist.
#[derive(Debug)]
pub struct ProsodyLex {
    model: std::sync::Arc<ProsodyModel>,
    patterns: FxHashMap<(Pos, Option<String>), Vec<Stress>>,
}

impl ProsodyLex {
    pub fn new(
        model: std::sync::Arc<ProsodyModel>,
        cover_by_pos: &HashMap<Pos, Vec<String>>,
        refined_cover: &HashMap<(Pos, String), Vec<String>>,
    ) -> ProsodyLex {
        let mut patterns: FxHashMap<(Pos, Option<String>), Vec<Stress>> = FxHashMap::default();
        let mut add = |key: (Pos, Option<String>), words: &[String]| {
            let mut seen: FxHashSet<Stress> = FxHashSet::default();
            let mut out = Vec::new();
            for w in words {
                for v in model.variants_or_flex(w) {
                    if seen.insert(v.clone()) {
                        out.push(v);
                    }
                }
            }
            out.sort();
            patterns.insert(key, out);
        };
        for (pos, words) in cover_by_pos {
            add((*pos, None), words);
        }
        for ((pos, tag), words) in refined_cover {
            add((*pos, Some(tag.clone())), words);
        }
        ProsodyLex { model, patterns }
    }

    pub fn model(&self) -> &ProsodyModel {
        &self.model
    }

    /// Syllable counts a cover word could contribute at this slot and offset.
    fn cover_syllables(
        &self,
        pos: Pos,
        tag: Option<&str>,
        off: usize,
        spec: &MeterSpec,
    ) -> Vec<usize> {
        let key = (pos, tag.map(|s| s.to_string()));
        let pats = self
            .patterns
            .get(&key)
            .or_else(|| self.patterns.get(&(pos, None)));
        let mut out: Vec<usize> = Vec::new();
        if let Some(pats) = pats {
            for p in pats {
                if scans(p, off, spec) && !out.contains(&p.len()) {
                    out.push(p.len());
                }
            }
        }
        out
    }
}

/// One step of the walk: consuming `syl` syllables from `state`.
///
/// Returns `None` when the word overruns the line — a line is exact, so a word
/// that does not fit is not a candidate for this slot.
pub fn step(state: MeterState, syl: usize, spec: &MeterSpec) -> Option<MeterState> {
    let len = spec.line_len(state.line);
    match (state.off + syl).cmp(&len) {
        std::cmp::Ordering::Less => Some(MeterState { off: state.off + syl, ..state }),
        std::cmp::Ordering::Equal => Some(MeterState { line: state.line + 1, off: 0 }),
        std::cmp::Ordering::Greater => None,
    }
}

/// States, per slot index, from which the rest of this sentence shape can still
/// complete a metrical reading.
///
/// A backward pass, and the reason a filler can commit to a word without ever
/// backtracking: without it, a cover word that fits locally can leave the line
/// unfinishable. `forced` maps a slot index to the payload word pinned there.
///
/// The last entry (index `slots.len()`) accepts every state, because a sentence
/// may end mid-line — the next sentence continues it.
pub fn feasible_states(
    plex: &ProsodyLex,
    spec: &MeterSpec,
    slots: &[Pos],
    refinements: &[Option<String>],
    forced: &dyn Fn(usize) -> Option<String>,
    dot_is_punctuation: bool,
) -> Vec<FxHashSet<MeterState>> {
    // The cycle repeats, so line index only matters modulo the pattern length.
    let cycle = spec.lines.len();
    let all: Vec<MeterState> = (0..cycle)
        .flat_map(|line| (0..spec.line_len(line)).map(move |off| MeterState { line, off }))
        .collect();

    let n = slots.len();
    let mut feas: Vec<FxHashSet<MeterState>> = vec![FxHashSet::default(); n + 1];
    feas[n] = all.iter().copied().collect();

    for j in (0..n).rev() {
        let slot = slots[j];
        let tag = refinements.get(j).and_then(|r| r.as_deref());
        let word = forced(j);
        let next = feas[j + 1].clone();
        let mut good = FxHashSet::default();
        for &st in &all {
            let ok = if slot == Pos::Dot && dot_is_punctuation {
                next.contains(&st) // punctuation costs no syllables
            } else if let Some(w) = word.as_deref() {
                plex.model
                    .variants_or_flex(w)
                    .iter()
                    .any(|p| scans(p, st.off, spec)
                        && step(st, p.len(), spec).map(|s| next.contains(&wrap(s, cycle)))
                            .unwrap_or(false))
            } else {
                plex.cover_syllables(slot, tag, st.off, spec)
                    .into_iter()
                    .any(|syl| step(st, syl, spec).map(|s| next.contains(&wrap(s, cycle)))
                        .unwrap_or(false))
            };
            if ok {
                good.insert(st);
            }
        }
        feas[j] = good;
    }
    feas
}

/// Line indices live modulo the cycle length; the walk only ever needs the phase.
pub fn wrap(state: MeterState, cycle: usize) -> MeterState {
    MeterState { line: state.line % cycle, off: state.off }
}

/// Everything the generator needs to keep a line scanning while it fills slots.
///
/// Carried by value through one sentence; `state` advances as words are emitted,
/// so it continues across sentence boundaries — verse lines do not respect
/// sentence ends any more than prose paragraphs do.
pub struct MeterCtx<'a> {
    pub plex: &'a ProsodyLex,
    pub spec: &'a MeterSpec,
    pub state: &'a mut MeterState,
}

/// How many syllables this word would contribute here, if it can be read
/// metrically at `st` and still leave the rest of the sentence completable.
///
/// `None` means the word does not belong in this slot under the meter — the
/// caller then asks the wordlist for another, and only if none exists does the
/// meter yield.
pub fn fitting_syllables(
    plex: &ProsodyLex,
    spec: &MeterSpec,
    word: &str,
    st: MeterState,
    next: &FxHashSet<MeterState>,
) -> Option<usize> {
    let cycle = spec.lines.len();
    plex.model
        .variants_or_flex(word)
        .into_iter()
        .filter(|p| scans(p, st.off, spec))
        .find(|p| {
            step(st, p.len(), spec)
                .map(|s| next.contains(&wrap(s, cycle)))
                .unwrap_or(false)
        })
        .map(|p| p.len())
}

/// Advance the meter by a word that has already been committed to.
///
/// A word that overruns its line starts the next one instead of being rejected:
/// by the time this runs the word is in the text, and a payload word is never
/// removed for the meter. The line simply breaks, and best-of-N prefers the
/// candidates where that did not happen.
pub fn advance(st: MeterState, syl: usize, spec: &MeterSpec) -> MeterState {
    match step(st, syl, spec) {
        Some(next) => next,
        None => MeterState { line: st.line + 1, off: syl.min(spec.line_len(st.line + 1)) },
    }
}

/// Split a finished word sequence into verse lines.
///
/// Deterministic and independent of how the text was built: it re-reads the
/// syllable counts and cuts where the pattern says. A trailing partial line is
/// kept as-is — a text ends where the payload ends, not where a stanza does.
pub fn layout(words: &[String], model: &ProsodyModel, spec: &MeterSpec) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut state = MeterState::default();
    for w in words {
        let len = spec.line_len(state.line);
        // A word with more than one pronunciation was placed under whichever
        // reading fit — "our" is one syllable or two, and the filler took the one
        // the line needed. Read it back the same way, preferring the variant that
        // completes the line exactly, then any that fits, and only then the
        // primary. Reading the primary unconditionally would break lines a
        // syllable away from where the filler built them.
        let vars = model.variants_or_flex(w);
        let syl = vars
            .iter()
            .map(|p| p.len())
            .find(|&n| state.off + n == len)
            .or_else(|| vars.iter().map(|p| p.len()).find(|&n| state.off + n < len))
            .unwrap_or_else(|| vars[0].len());
        cur.push(w.as_str());
        if state.off + syl >= len {
            lines.push(cur.join(" "));
            cur.clear();
            state = MeterState { line: state.line + 1, off: 0 };
        } else {
            state.off += syl;
        }
    }
    if !cur.is_empty() {
        lines.push(cur.join(" "));
    }
    lines
}

/// Does this finished text scan under `spec`?
///
/// Used by tests and by best-of-N tie-breaking. Mirrors `layout`'s cutting rule,
/// but insists every line be exact except the last.
pub fn scans_text(words: &[String], model: &ProsodyModel, spec: &MeterSpec) -> bool {
    let mut state = MeterState::default();
    for w in words {
        // Must read a word exactly as the filler read it, including the
        // one-flexible-syllable fallback for words the data does not know
        // (inflected cover forms). Skipping them instead would shift every
        // later offset and report breaks that are not there.
        let vars = model.variants_or_flex(w);
        let Some(p) = vars.iter().find(|p| scans(p, state.off, spec)) else {
            return false;
        };
        match step(state, p.len(), spec) {
            Some(s) => state = s,
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(lines: &[usize], mode: StressMode) -> MeterSpec {
        MeterSpec { lines: lines.to_vec(), mode, rise: true }
    }

    #[test]
    fn monosyllables_always_scan() {
        // The whole parity-repair argument rests on this.
        for off in 0..4 {
            assert!(scans(b"1", off, &spec(&[10], StressMode::Strict)));
            assert!(scans(b"0", off, &spec(&[10], StressMode::Strict)));
        }
    }

    #[test]
    fn a_trochee_is_an_iamb_when_it_starts_on_the_beat() {
        let s = spec(&[10], StressMode::Lenient);
        assert!(!scans(b"10", 0, &s), "DON-key at offset 0 puts stress on a weak beat");
        assert!(scans(b"10", 1, &s), "the DON-key scans");
    }

    #[test]
    fn strict_rejects_what_lenient_allows() {
        assert!(scans(b"100", 1, &spec(&[10], StressMode::Lenient)));
        assert!(!scans(b"100", 1, &spec(&[10], StressMode::Strict)));
    }

    #[test]
    fn free_mode_counts_syllables_only() {
        let s = spec(&[5, 7, 5], StressMode::Free);
        assert!(scans(b"10", 0, &s));
        assert!(scans(b"01", 0, &s));
    }

    #[test]
    fn step_refuses_to_overrun_a_line() {
        let s = spec(&[5], StressMode::Free);
        let st = MeterState { line: 0, off: 3 };
        assert_eq!(step(st, 2, &s), Some(MeterState { line: 1, off: 0 }));
        assert_eq!(step(st, 1, &s), Some(MeterState { line: 0, off: 4 }));
        assert_eq!(step(st, 3, &s), None);
    }

    #[test]
    fn meter_spec_parses_and_defaults() {
        let v: serde_yaml::Value =
            serde_yaml::from_str("lines: [5, 7, 5]\nstress: lenient\nrise: false").unwrap();
        let m = MeterSpec::from_yaml(&v).unwrap();
        assert_eq!(m.lines, vec![5, 7, 5]);
        assert_eq!(m.mode, StressMode::Lenient);
        assert!(!m.rise);

        let v: serde_yaml::Value = serde_yaml::from_str("lines: [10]").unwrap();
        let m = MeterSpec::from_yaml(&v).unwrap();
        assert_eq!(m.mode, StressMode::Free, "stress is free unless asked for");
        assert!(m.rise);

        let v: serde_yaml::Value = serde_yaml::from_str("stress: lenient").unwrap();
        assert!(MeterSpec::from_yaml(&v).is_none(), "no lines, no meter");
    }

    #[test]
    fn model_reads_variants_and_syllables() {
        let m = ProsodyModel::from_yaml_str(
            "stress: {\"donkey\": \"10\", \"record\": \"010|100\", \"no\": \"1\"}\n\
             rhyme: {\"donkey\": \"AONGKIY\"}",
        )
        .unwrap();
        assert_eq!(m.syllables("donkey"), Some(2));
        assert_eq!(m.variants("record").unwrap().len(), 2, "both readings kept");
        assert_eq!(m.syllables("no"), Some(1), "YAML boolean-looking words survive");
        assert_eq!(m.syllables("Donkey."), Some(2), "case and punctuation normalized");
        assert_eq!(m.rhyme_key("donkey"), Some("AONGKIY"));
        assert_eq!(m.syllables("nonesuch"), None);
    }

    #[test]
    fn layout_reads_a_word_the_way_the_filler_placed_it() {
        // "our" is one syllable or two. The filler takes whichever the line
        // needs; reading back the primary unconditionally would cut the line a
        // syllable early.
        let m = ProsodyModel::from_yaml_str(
            "stress: {\"our\": \"1|10\", \"a\": \"1\", \"bb\": \"10\"}\nrhyme: {}",
        )
        .unwrap();
        let words: Vec<String> = ["a", "our", "a", "bb"].iter().map(|s| s.to_string()).collect();
        // Line of 3: "a"(1) + "our" must be read as 2 to land exactly.
        assert_eq!(
            layout(&words, &m, &spec(&[3], StressMode::Free)),
            vec!["a our", "a bb"],
        );
    }

    #[test]
    fn layout_cuts_where_the_pattern_says() {
        let m = ProsodyModel::from_yaml_str(
            "stress: {\"a\": \"1\", \"bb\": \"10\", \"ccc\": \"100\"}\nrhyme: {}",
        )
        .unwrap();
        let words: Vec<String> = ["a", "bb", "ccc", "a", "bb", "a"]
            .iter().map(|s| s.to_string()).collect();
        let lines = layout(&words, &m, &spec(&[3], StressMode::Free));
        assert_eq!(lines, vec!["a bb", "ccc", "a bb", "a"]);
    }
}
