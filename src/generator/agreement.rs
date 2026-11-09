//! Language-specific morphological agreement, applied as a post-pass over a
//! generated sentence's (slot, word) sequence.
//!
//! Phase 1 covers **German**: determiner gender/case agreement plus noun
//! capitalization. The pass only ever rewrites *cover* determiners (whose forms
//! — der/die/das/den/dem, ein/eine/einen/einem/einer — never collide with any
//! payload word) and capitalizes noun tokens (decode lowercases, so this is
//! round-trip safe). Payload words are never altered in identity.
//!
//! Case is inferred from local structure: an NP is nominative by default,
//! accusative as a direct object (right after a verb), and takes a
//! preposition's governed case inside a PP. Attributive adjective declension
//! (strong/weak/mixed) is intentionally out of scope here — the German grammar
//! keeps adjectives in predicative position, where they are uninflected.
//!
//! **Korean** attaches case and topic particles to the noun before them. The
//! grammar places each particle as a cover `Prep` word tagged with its function
//! (`subj`, `top`, `obj`, `com`, `dir`, ...). This pass picks the allomorph the
//! noun's last syllable calls for (이/가, 은/는, 을/를, 과/와, 으로/로) and
//! writes the particle onto the noun. Attachment merges two tokens into one, so
//! [`Agreement::apply`] reports which tokens it absorbed and the caller drops
//! them; decoding detaches the particle again (`codec::payload_form`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use crate::types::Pos;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Case {
    Nom,
    Acc,
    Dat,
}

/// German definite/indefinite article for (det_type, gender, case), singular.
/// `gender` ∈ {'m','f','n'}; `det_type` ∈ {"def","indef"}.
fn german_article(det_type: &str, gender: char, case: Case) -> Option<&'static str> {
    use Case::*;
    let a = match (det_type, gender, case) {
        ("def", 'm', Nom) => "der",
        ("def", 'm', Acc) => "den",
        ("def", 'm', Dat) => "dem",
        ("def", 'f', Nom) | ("def", 'f', Acc) => "die",
        ("def", 'f', Dat) => "der",
        ("def", 'n', Nom) | ("def", 'n', Acc) => "das",
        ("def", 'n', Dat) => "dem",
        ("indef", 'm', Nom) => "ein",
        ("indef", 'm', Acc) => "einen",
        ("indef", 'm', Dat) => "einem",
        ("indef", 'f', Nom) | ("indef", 'f', Acc) => "eine",
        ("indef", 'f', Dat) => "einer",
        ("indef", 'n', Nom) | ("indef", 'n', Acc) => "ein",
        ("indef", 'n', Dat) => "einem",
        _ => return None,
    };
    Some(a)
}

/// Governed case for a German preposition. Two-way (Wechsel) prepositions
/// default to dative (the static-location reading), which reads naturally for
/// the simple locative PPs this grammar produces.
fn german_prep_case(prep: &str) -> Case {
    match prep {
        "für" | "um" | "durch" | "gegen" | "ohne" | "bis" | "entlang" => Case::Acc,
        "mit" | "aus" | "bei" | "nach" | "von" | "zu" | "seit" => Case::Dat,
        // two-way: in, an, auf, über, unter, vor, hinter, neben, zwischen
        _ => Case::Dat,
    }
}

fn def_articles() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| ["der", "die", "das", "den", "dem", "des"].into_iter().collect())
}

fn indef_articles() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| {
        ["ein", "eine", "einen", "einem", "einer", "eines"]
            .into_iter()
            .collect()
    })
}

/// Strip surrounding punctuation and lowercase — for gender lookup and article
/// comparison against tokens that may carry a trailing '.'.
fn norm(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Capitalize the first alphabetic character, leaving any leading markup intact.
fn capitalize_first(word: &str) -> String {
    let mut done = false;
    word.chars()
        .map(|c| {
            if !done && c.is_alphabetic() {
                done = true;
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Which language's agreement rules an [`Agreement`] applies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Morph {
    German,
    Korean,
}

/// Context needed to apply a language's agreement to one sentence.
pub struct Agreement {
    morph: Morph,
    genders: Arc<HashMap<String, char>>,
    capitalize_nouns: bool,
}

/// True when a Hangul syllable ends in a final consonant (batchim).
/// `None` for a character that is not a precomposed Hangul syllable.
fn has_batchim(c: char) -> Option<bool> {
    let cp = c as u32;
    if !(0xAC00..=0xD7A3).contains(&cp) {
        return None;
    }
    Some((cp - 0xAC00) % 28 != 0)
}

/// True when a Hangul syllable's final consonant is ㄹ, which takes 로 rather
/// than 으로.
fn batchim_is_rieul(c: char) -> bool {
    let cp = c as u32;
    (0xAC00..=0xD7A3).contains(&cp) && (cp - 0xAC00) % 28 == 8
}

/// The particle form for a function, given the syllable it attaches to.
/// `None` for a word that is not a known Korean particle.
fn korean_particle(current: &str, prev_last: char) -> Option<&'static str> {
    let batchim = has_batchim(prev_last).unwrap_or(false);
    let pick = |with: &'static str, without: &'static str| if batchim { with } else { without };
    Some(match current {
        "이" | "가" => pick("이", "가"),
        "은" | "는" => pick("은", "는"),
        "을" | "를" => pick("을", "를"),
        "과" | "와" => pick("과", "와"),
        "으로" | "로" => {
            if batchim && !batchim_is_rieul(prev_last) { "으로" } else { "로" }
        }
        "에" => "에",
        "에서" => "에서",
        "의" => "의",
        _ => return None,
    })
}

impl Agreement {
    /// Apply agreement to `words`, using the parallel `slots`/`refinements`
    /// (aligned to `words`, i.e. one entry per emitted token — Dot slots that
    /// only append punctuation are not included).
    ///
    /// Returns the indices of tokens that were merged into the token before
    /// them (Korean particles), in ascending order. The caller removes those
    /// tokens; their text is already part of the preceding word. German never
    /// merges and returns an empty list.
    pub fn apply(&self, slots: &[Pos], refinements: &[Option<String>], words: &mut [String]) -> Vec<usize> {
        debug_assert_eq!(slots.len(), words.len());
        debug_assert_eq!(refinements.len(), words.len());
        match self.morph {
            Morph::German => {
                self.apply_german(slots, refinements, words);
                Vec::new()
            }
            Morph::Korean => apply_korean(slots, words),
        }
    }

    fn apply_german(&self, slots: &[Pos], refinements: &[Option<String>], words: &mut [String]) {

        for i in 0..words.len() {
            if slots[i] != Pos::Det {
                continue;
            }
            let det_type = match refinements.get(i).and_then(|r| r.as_deref()) {
                Some(t @ ("def" | "indef")) => t,
                _ => continue, // leave quantifiers / untagged determiners alone
            };

            // Guard: only touch a slot that currently holds a cover article, so
            // an embedded payload determiner is never corrupted.
            let current = norm(&words[i]);
            let is_cover_article = match det_type {
                "def" => def_articles().contains(current.as_str()),
                _ => indef_articles().contains(current.as_str()),
            };
            if !is_cover_article {
                continue;
            }

            // Head noun = next N slot, skipping any adjectives in between.
            let mut j = i + 1;
            while j < slots.len() && slots[j] == Pos::Adj {
                j += 1;
            }
            if j >= slots.len() || slots[j] != Pos::N {
                continue;
            }
            let gender = match self.genders.get(&norm(&words[j])) {
                Some(&g) => g,
                None => continue, // unknown gender: leave the article as-is
            };

            // Case from local structure.
            let case = if i == 0 {
                Case::Nom
            } else {
                match slots[i - 1] {
                    Pos::Prep => german_prep_case(&norm(&words[i - 1])),
                    Pos::V => Case::Acc,
                    _ => Case::Nom,
                }
            };

            if let Some(article) = german_article(det_type, gender, case) {
                words[i] = article.to_string();
            }
        }

        if self.capitalize_nouns {
            for i in 0..words.len() {
                if slots[i] == Pos::N {
                    words[i] = capitalize_first(&words[i]);
                }
            }
        }
    }
}

/// Attach Korean particles to the noun before them.
///
/// Only a `Prep` slot holding a known particle that directly follows a noun or
/// pronoun is touched, so a payload word is never rewritten: payload words are
/// the nouns particles attach *to*, and their text only gains a suffix the
/// decoder removes.
fn apply_korean(slots: &[Pos], words: &mut [String]) -> Vec<usize> {
    let mut merged = Vec::new();
    for i in 1..words.len() {
        if slots[i] != Pos::Prep || !matches!(slots[i - 1], Pos::N | Pos::Pron) {
            continue;
        }
        if merged.last() == Some(&(i - 1)) {
            continue; // the previous token is itself absorbed; never chain
        }
        let Some(last) = words[i - 1].chars().last() else { continue };
        let Some(form) = korean_particle(words[i].as_str(), last) else { continue };
        words[i - 1].push_str(form);
        words[i].clear();
        merged.push(i);
    }
    merged
}

/// Build the agreement context for a language whose grammar requests a known
/// morphology, or `None` if the morphology is unrecognized.
pub fn for_morphology(morphology: &str, language: &str) -> Option<Agreement> {
    match morphology {
        "german" => Some(Agreement {
            morph: Morph::German,
            genders: load_noun_genders(language),
            capitalize_nouns: true,
        }),
        "korean" => Some(Agreement {
            morph: Morph::Korean,
            genders: Arc::new(HashMap::new()),
            capitalize_nouns: false,
        }),
        _ => None,
    }
}

/// Load a `word -> gender` map for a language by scanning its payload.yaml and
/// cover.yaml for `gender:` annotations. Cached per language.
fn load_noun_genders(language: &str) -> Arc<HashMap<String, char>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<HashMap<String, char>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(g) = cache.lock().unwrap().get(language) {
        return g.clone();
    }

    let mut map: HashMap<String, char> = HashMap::new();
    for file in ["payload.yaml", "cover.yaml"] {
        if let Some(content) = read_language_yaml(language, file) {
            merge_genders(&content, &mut map);
        }
    }

    let arc = Arc::new(map);
    cache.lock().unwrap().insert(language.to_string(), arc.clone());
    arc
}

/// Read a language YAML file from the embedded index (release) or the
/// filesystem (debug / dev), mirroring the payload loader's resolution.
fn read_language_yaml(language: &str, filename: &str) -> Option<String> {
    if let Some(embedded) =
        crate::generator::data::get_embedded_yaml(&format!("{}/{}", language, filename))
    {
        return Some(embedded.to_string());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(path) = crate::generator::data::find_language_file(language, filename) {
            return std::fs::read_to_string(path).ok();
        }
    }
    None
}

fn merge_genders(yaml_content: &str, map: &mut HashMap<String, char>) {
    let doc: serde_yaml::Value = match serde_yaml::from_str(yaml_content) {
        Ok(d) => d,
        Err(_) => return,
    };
    let Some(mapping) = doc.as_mapping() else {
        return;
    };
    for (key, value) in mapping {
        let (Some(word), Some(entry)) = (key.as_str(), value.as_mapping()) else {
            continue;
        };
        if let Some(g) = entry
            .get(serde_yaml::Value::from("gender"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.chars().next())
        {
            if matches!(g, 'm' | 'f' | 'n') {
                map.entry(word.to_lowercase()).or_insert(g);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ag(pairs: &[(&str, char)]) -> Agreement {
        Agreement {
            morph: Morph::German,
            genders: Arc::new(pairs.iter().map(|(w, g)| (w.to_string(), *g)).collect()),
            capitalize_nouns: true,
        }
    }

    #[test]
    fn article_paradigm_core_cells() {
        assert_eq!(german_article("def", 'm', Case::Nom), Some("der"));
        assert_eq!(german_article("def", 'm', Case::Acc), Some("den"));
        assert_eq!(german_article("def", 'f', Case::Dat), Some("der"));
        assert_eq!(german_article("def", 'n', Case::Nom), Some("das"));
        assert_eq!(german_article("indef", 'f', Case::Nom), Some("eine"));
        assert_eq!(german_article("indef", 'm', Case::Acc), Some("einen"));
    }

    #[test]
    fn nominative_subject_gets_gendered_article() {
        // "das galerie" (wrong) with galerie=f, subject → "die"
        let a = ag(&[("galerie", 'f')]);
        let slots = vec![Pos::Det, Pos::N];
        let refs = vec![Some("def".to_string()), None];
        let mut words = vec!["das".to_string(), "galerie".to_string()];
        a.apply(&slots, &refs, &mut words);
        assert_eq!(words[0], "die");
        assert_eq!(words[1], "Galerie"); // capitalized
    }

    #[test]
    fn accusative_object_masculine_becomes_den() {
        let a = ag(&[("becher", 'm')]);
        // "... sieht der becher" → object of V → accusative → "den"
        let slots = vec![Pos::V, Pos::Det, Pos::N];
        let refs = vec![None, Some("def".to_string()), None];
        let mut words = vec!["sieht".to_string(), "der".to_string(), "becher".to_string()];
        a.apply(&slots, &refs, &mut words);
        assert_eq!(words[1], "den");
    }

    #[test]
    fn dative_preposition_masculine_becomes_dem() {
        let a = ag(&[("becher", 'm')]);
        // "mit der becher" → mit governs dative → "dem"
        let slots = vec![Pos::Prep, Pos::Det, Pos::N];
        let refs = vec![None, Some("def".to_string()), None];
        let mut words = vec!["mit".to_string(), "der".to_string(), "becher".to_string()];
        a.apply(&slots, &refs, &mut words);
        assert_eq!(words[1], "dem");
    }

    #[test]
    fn payload_determiner_not_corrupted() {
        // If the Det slot holds a non-article (an embedded payload determiner),
        // leave it untouched.
        let a = ag(&[("becher", 'm')]);
        let slots = vec![Pos::Det, Pos::N];
        let refs = vec![Some("def".to_string()), None];
        let mut words = vec!["etliche".to_string(), "becher".to_string()];
        a.apply(&slots, &refs, &mut words);
        assert_eq!(words[0], "etliche");
    }

    #[test]
    fn unknown_gender_left_alone() {
        let a = ag(&[]);
        let slots = vec![Pos::Det, Pos::N];
        let refs = vec![Some("def".to_string()), None];
        let mut words = vec!["der".to_string(), "xyzzy".to_string()];
        a.apply(&slots, &refs, &mut words);
        assert_eq!(words[0], "der");
    }

    // Exercises the real gender-loading pipeline (parsing the German
    // payload.yaml / cover.yaml gender annotations from disk or the embedded
    // index) end-to-end through apply().
    #[test]
    fn german_genders_load_and_drive_agreement() {
        let a = for_morphology("german", "german")
            .expect("german morphology should be available");
        assert_eq!(a.genders.get("becher"), Some(&'m'));
        assert_eq!(a.genders.get("milch"), Some(&'f'));
        assert_eq!(a.genders.get("fenster"), Some(&'n'));

        // "das milch" (wrong def article) as a subject → feminine nominative "die".
        let slots = vec![Pos::Det, Pos::N];
        let refs = vec![Some("def".to_string()), None];
        let mut words = vec!["das".to_string(), "milch".to_string()];
        a.apply(&slots, &refs, &mut words);
        assert_eq!(words[0], "die");
        assert_eq!(words[1], "Milch");

        // "sieht der fenster" object → neuter accusative "das".
        let slots = vec![Pos::V, Pos::Det, Pos::N];
        let refs = vec![None, Some("def".to_string()), None];
        let mut words = vec!["sieht".to_string(), "der".to_string(), "fenster".to_string()];
        a.apply(&slots, &refs, &mut words);
        assert_eq!(words[1], "das");
        assert_eq!(words[2], "Fenster");
    }

    fn korean() -> Agreement {
        for_morphology("korean", "korean").expect("korean morphology")
    }

    fn run(slots: &[Pos], words: &[&str]) -> (Vec<String>, Vec<usize>) {
        let refs = vec![None; slots.len()];
        let mut w: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        let merged = korean().apply(slots, &refs, &mut w);
        (w, merged)
    }

    #[test]
    fn korean_particle_follows_batchim() {
        // 가이드 ends in 드 (no batchim) -> 는; 가격 ends in ㄱ (batchim) -> 을.
        let (w, m) = run(&[Pos::N, Pos::Prep, Pos::N, Pos::Prep, Pos::V],
                         &["가이드", "은", "가격", "를", "보았다"]);
        assert_eq!(m, vec![1, 3]);
        assert_eq!(w[0], "가이드는");
        assert_eq!(w[2], "가격을");
    }

    #[test]
    fn korean_directional_particle_after_rieul() {
        // 가을 ends in ㄹ -> 로, not 으로; 가방 ends in ㅇ -> 으로; 가이드 -> 로.
        for (noun, want) in [("가을", "가을로"), ("가방", "가방으로"), ("가이드", "가이드로")] {
            let (w, _) = run(&[Pos::N, Pos::Prep], &[noun, "으로"]);
            assert_eq!(w[0], want);
        }
    }

    #[test]
    fn korean_particle_not_attached_after_non_noun() {
        // A particle after an adverb is left as its own token.
        let (w, m) = run(&[Pos::Adv, Pos::Prep], &["갑자기", "가"]);
        assert!(m.is_empty());
        assert_eq!(w, vec!["갑자기".to_string(), "가".to_string()]);
    }
}
