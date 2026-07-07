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

/// Context needed to apply German agreement to one sentence.
pub struct Agreement {
    genders: Arc<HashMap<String, char>>,
    capitalize_nouns: bool,
}

impl Agreement {
    /// Rewrite determiner forms and capitalize nouns in `words`, using the
    /// parallel `slots`/`refinements` (aligned to `words`, i.e. one entry per
    /// emitted token — Dot slots that only append punctuation are not included).
    pub fn apply(&self, slots: &[Pos], refinements: &[Option<String>], words: &mut [String]) {
        debug_assert_eq!(slots.len(), words.len());
        debug_assert_eq!(refinements.len(), words.len());

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

/// Build the agreement context for a language whose grammar requests a known
/// morphology, or `None` if the morphology is unrecognized.
pub fn for_morphology(morphology: &str, language: &str) -> Option<Agreement> {
    match morphology {
        "german" => Some(Agreement {
            genders: load_noun_genders(language),
            capitalize_nouns: true,
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
}
