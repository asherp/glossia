//! Scale derivation from interval patterns.
//!
//! A musical scale is defined by an interval pattern (sequence of semitone steps)
//! and a root note. Given the chromatic payload (128 MIDI notes), this module
//! derives the subset of notes belonging to any scale.
//!
//! # Examples
//!
//! Major pentatonic from C: intervals `[2,2,3,2,3]`, root `C`
//! → pitch classes {0,2,4,7,9} → C,D,E,G,A across all octaves
//!
//! Blues from A: intervals `[3,2,1,1,3,2]`, root `A`
//! → pitch classes {9,0,2,3,4,7} → A,C,D,Eb,E,G across all octaves

use std::collections::HashSet;

/// Scale definition parsed from grammar.yaml.
#[derive(Debug, Clone)]
pub struct ScaleDefinition {
    /// Semitone intervals between consecutive scale degrees.
    /// Must sum to 12 (one octave).
    pub intervals: Vec<u8>,
    /// Root note name (e.g., "C", "D", "Eb", "Gb").
    pub root: String,
}

/// The 12 chromatic pitch class names, using flats to match payload.yaml notation.
/// Index = pitch class number (C=0, Db=1, ..., B=11).
#[cfg(test)]
const PITCH_CLASS_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// Parse a pitch class name to its semitone number (0-11).
/// Accepts both flat and sharp notation for robustness.
pub fn pitch_class_from_name(name: &str) -> Option<u8> {
    match name {
        "C"  | "c"          => Some(0),
        "Db" | "db" | "C#" | "c#" => Some(1),
        "D"  | "d"          => Some(2),
        "Eb" | "eb" | "D#" | "d#" => Some(3),
        "E"  | "e"          => Some(4),
        "F"  | "f"          => Some(5),
        "Gb" | "gb" | "F#" | "f#" => Some(6),
        "G"  | "g"          => Some(7),
        "Ab" | "ab" | "G#" | "g#" => Some(8),
        "A"  | "a"          => Some(9),
        "Bb" | "bb" | "A#" | "a#" => Some(10),
        "B"  | "b"          => Some(11),
        _ => None,
    }
}

/// Extract the pitch class name from a MIDI note token (e.g., "Db4" → "Db", "C-1" → "C").
/// Case-insensitive: handles both "C4" and "c4" (payload words are lowercased during loading).
pub fn pitch_class_of_note(note: &str) -> Option<&str> {
    // Note names: letter + optional 'b' (flat) + octave number (possibly negative)
    // Examples: "C4", "Db-1", "Gb7", "A0", "Bb3", "c4", "db-1"
    if note.len() < 2 {
        return None;
    }

    // First char must be A-G (case-insensitive)
    let first = note.as_bytes()[0];
    if !(b'A'..=b'G').contains(&first) && !(b'a'..=b'g').contains(&first) {
        return None;
    }

    // Check if second char is 'b' (flat modifier)
    if note.as_bytes().get(1) == Some(&b'b') {
        // Flat note: pitch class is first two chars (e.g., "Db", "db")
        Some(&note[..2])
    } else {
        // Natural note: pitch class is first char (e.g., "C", "c")
        Some(&note[..1])
    }
}

/// Compute the set of valid pitch classes for a scale defined by interval pattern + root.
///
/// Starting from the root pitch class, accumulates intervals modulo 12 to produce
/// the set of pitch classes in the scale.
///
/// # Example
/// ```
/// use glossia::scale::scale_pitch_classes;
/// let pentatonic = scale_pitch_classes(&[2,2,3,2,3], 0); // C major pentatonic
/// assert_eq!(pentatonic, vec![0, 2, 4, 7, 9]); // C, D, E, G, A
/// ```
pub fn scale_pitch_classes(intervals: &[u8], root: u8) -> Vec<u8> {
    let mut classes = Vec::with_capacity(intervals.len());
    let mut current = root % 12;
    classes.push(current);
    for &interval in &intervals[..intervals.len().saturating_sub(1)] {
        current = (current + interval) % 12;
        classes.push(current);
    }
    classes
}

/// Filter a chromatic payload word list to only notes belonging to a given scale.
///
/// Takes the full chromatic payload (128 MIDI note names) and returns the subset
/// whose pitch classes match the scale defined by `intervals` + `root`.
/// Preserves the original ordering.
pub fn filter_payload_by_scale(
    chromatic_words: &[String],
    scale: &ScaleDefinition,
) -> Result<Vec<String>, String> {
    let root_pc = pitch_class_from_name(&scale.root)
        .ok_or_else(|| format!("Unknown root note: '{}'. Expected C, D, Eb, Gb, etc.", scale.root))?;

    let interval_sum: u16 = scale.intervals.iter().map(|&i| i as u16).sum();
    if interval_sum != 12 {
        return Err(format!(
            "Scale intervals {:?} sum to {} (expected 12 semitones = one octave)",
            scale.intervals, interval_sum
        ));
    }

    let valid_classes: HashSet<u8> = scale_pitch_classes(&scale.intervals, root_pc)
        .into_iter()
        .collect();

    let filtered: Vec<String> = chromatic_words
        .iter()
        .filter(|word| {
            pitch_class_of_note(word)
                .and_then(pitch_class_from_name)
                .map(|pc| valid_classes.contains(&pc))
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    if filtered.is_empty() {
        return Err(format!(
            "No notes matched scale {:?} from root {}",
            scale.intervals, scale.root
        ));
    }

    Ok(filtered)
}

/// Parse a scale definition from grammar.yaml dialect data.
///
/// Expects a YAML mapping with:
///   scale:
///     intervals: [2, 2, 3, 2, 3]
///     root: C
pub fn parse_scale_definition(scale_value: &serde_yaml::Value) -> Result<ScaleDefinition, String> {
    let intervals = scale_value.get("intervals")
        .and_then(|v| v.as_sequence())
        .ok_or("scale: missing 'intervals' (expected array of semitone steps)")?
        .iter()
        .map(|v| v.as_u64()
            .ok_or_else(|| "scale intervals must be positive integers".to_string())
            .and_then(|n| u8::try_from(n).map_err(|_| "interval too large".to_string())))
        .collect::<Result<Vec<u8>, String>>()?;

    let root = scale_value.get("root")
        .and_then(|v| v.as_str())
        .ok_or("scale: missing 'root' (expected note name like C, D, Eb)")?
        .to_string();

    Ok(ScaleDefinition { intervals, root })
}

/// Derive a canonical refinement tag from a scale definition.
///
/// Known interval patterns map to human-readable scale names:
///   - `[2,2,3,2,3]` → `"pentatonic"`
///   - `[3,2,2,3,2]` → `"minor-pentatonic"`
///   - `[3,2,1,1,3,2]` → `"blues"`
///   - `[2,2,1,2,2,2,1]` → `"diatonic"`
///   - `[2,1,2,2,1,2,2]` → `"minor"`
///   - `[1,1,1,1,1,1,1,1,1,1,1,1]` → `"chromatic"`
///
/// Unrecognized patterns get a canonical fallback: `"scale/{root}/{intervals-joined-by-dash}"`.
///
/// The returned tag includes the root note: `"{scale_name}/{root}"` (e.g., `"pentatonic/C"`).
/// Exception: chromatic has no root distinction, so it returns just `"chromatic"`.
pub fn derive_refinement_tag(scale: &ScaleDefinition) -> String {
    let scale_name = match scale.intervals.as_slice() {
        [2, 2, 3, 2, 3] => "pentatonic",
        [3, 2, 2, 3, 2] => "minor-pentatonic",
        [3, 2, 1, 1, 3, 2] => "blues",
        [2, 2, 1, 2, 2, 2, 1] => "diatonic",
        [2, 1, 2, 2, 1, 2, 2] => "minor",
        [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1] => return "chromatic".to_string(),
        _ => {
            // Fallback: canonical encoding of the interval pattern
            let intervals_str: Vec<String> = scale.intervals.iter().map(|i| i.to_string()).collect();
            return format!("scale/{}/{}", scale.root, intervals_str.join("-"));
        }
    };
    format!("{}/{}", scale_name, scale.root)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pitch_class_from_name() {
        assert_eq!(pitch_class_from_name("C"), Some(0));
        assert_eq!(pitch_class_from_name("Db"), Some(1));
        assert_eq!(pitch_class_from_name("D"), Some(2));
        assert_eq!(pitch_class_from_name("Eb"), Some(3));
        assert_eq!(pitch_class_from_name("E"), Some(4));
        assert_eq!(pitch_class_from_name("F"), Some(5));
        assert_eq!(pitch_class_from_name("Gb"), Some(6));
        assert_eq!(pitch_class_from_name("G"), Some(7));
        assert_eq!(pitch_class_from_name("Ab"), Some(8));
        assert_eq!(pitch_class_from_name("A"), Some(9));
        assert_eq!(pitch_class_from_name("Bb"), Some(10));
        assert_eq!(pitch_class_from_name("B"), Some(11));
        // Sharp aliases
        assert_eq!(pitch_class_from_name("C#"), Some(1));
        assert_eq!(pitch_class_from_name("F#"), Some(6));
        assert_eq!(pitch_class_from_name("G#"), Some(8));
        // Invalid
        assert_eq!(pitch_class_from_name("H"), None);
        assert_eq!(pitch_class_from_name(""), None);
    }

    #[test]
    fn test_pitch_class_of_note() {
        assert_eq!(pitch_class_of_note("C4"), Some("C"));
        assert_eq!(pitch_class_of_note("Db-1"), Some("Db"));
        assert_eq!(pitch_class_of_note("Gb7"), Some("Gb"));
        assert_eq!(pitch_class_of_note("A0"), Some("A"));
        assert_eq!(pitch_class_of_note("Bb3"), Some("Bb"));
        assert_eq!(pitch_class_of_note("B8"), Some("B"));
        assert_eq!(pitch_class_of_note("E5"), Some("E"));
        // Lowercase (payload words are lowercased during loading)
        assert_eq!(pitch_class_of_note("c4"), Some("c"));
        assert_eq!(pitch_class_of_note("db-1"), Some("db"));
        assert_eq!(pitch_class_of_note("gb7"), Some("gb"));
        assert_eq!(pitch_class_of_note("a0"), Some("a"));
        // Edge cases
        assert_eq!(pitch_class_of_note("x"), None); // too short and invalid
        assert_eq!(pitch_class_of_note(""), None);
    }

    #[test]
    fn test_c_major_pentatonic_pitch_classes() {
        let classes = scale_pitch_classes(&[2, 2, 3, 2, 3], 0); // root C
        assert_eq!(classes, vec![0, 2, 4, 7, 9]); // C, D, E, G, A
    }

    #[test]
    fn test_d_major_pentatonic_pitch_classes() {
        let classes = scale_pitch_classes(&[2, 2, 3, 2, 3], 2); // root D
        assert_eq!(classes, vec![2, 4, 6, 9, 11]); // D, E, Gb, A, B
    }

    #[test]
    fn test_a_minor_pentatonic_pitch_classes() {
        let classes = scale_pitch_classes(&[3, 2, 2, 3, 2], 9); // root A
        assert_eq!(classes, vec![9, 0, 2, 4, 7]); // A, C, D, E, G
    }

    #[test]
    fn test_blues_scale_pitch_classes() {
        let classes = scale_pitch_classes(&[3, 2, 1, 1, 3, 2], 9); // root A
        assert_eq!(classes, vec![9, 0, 2, 3, 4, 7]); // A, C, D, Eb, E, G
    }

    #[test]
    fn test_chromatic_scale_has_all_12() {
        let classes = scale_pitch_classes(&[1,1,1,1,1,1,1,1,1,1,1,1], 0);
        assert_eq!(classes, (0..12).collect::<Vec<u8>>());
    }

    #[test]
    fn test_filter_payload_c_major_pentatonic() {
        // Simulate a small chromatic payload (one octave)
        let chromatic: Vec<String> = vec![
            "C4", "Db4", "D4", "Eb4", "E4", "F4",
            "Gb4", "G4", "Ab4", "A4", "Bb4", "B4",
        ].into_iter().map(String::from).collect();

        let scale = ScaleDefinition {
            intervals: vec![2, 2, 3, 2, 3],
            root: "C".to_string(),
        };

        let filtered = filter_payload_by_scale(&chromatic, &scale).unwrap();
        assert_eq!(filtered, vec!["C4", "D4", "E4", "G4", "A4"]);
    }

    #[test]
    fn test_filter_payload_d_major_pentatonic() {
        let chromatic: Vec<String> = vec![
            "C4", "Db4", "D4", "Eb4", "E4", "F4",
            "Gb4", "G4", "Ab4", "A4", "Bb4", "B4",
        ].into_iter().map(String::from).collect();

        let scale = ScaleDefinition {
            intervals: vec![2, 2, 3, 2, 3],
            root: "D".to_string(),
        };

        let filtered = filter_payload_by_scale(&chromatic, &scale).unwrap();
        assert_eq!(filtered, vec!["D4", "E4", "Gb4", "A4", "B4"]);
    }

    #[test]
    fn test_bad_interval_sum() {
        let chromatic = vec!["C4".to_string()];
        let scale = ScaleDefinition {
            intervals: vec![2, 2, 3, 2], // sums to 9, not 12
            root: "C".to_string(),
        };
        assert!(filter_payload_by_scale(&chromatic, &scale).is_err());
    }

    #[test]
    fn test_bad_root() {
        let chromatic = vec!["C4".to_string()];
        let scale = ScaleDefinition {
            intervals: vec![2, 2, 3, 2, 3],
            root: "X".to_string(),
        };
        assert!(filter_payload_by_scale(&chromatic, &scale).is_err());
    }

    #[test]
    fn test_parse_scale_definition() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(r#"
            intervals: [2, 2, 3, 2, 3]
            root: C
        "#).unwrap();

        let scale = parse_scale_definition(&yaml).unwrap();
        assert_eq!(scale.intervals, vec![2, 2, 3, 2, 3]);
        assert_eq!(scale.root, "C");
    }

    #[test]
    fn test_parse_scale_definition_sharp_root() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(r#"
            intervals: [2, 2, 3, 2, 3]
            root: "F#"
        "#).unwrap();

        let scale = parse_scale_definition(&yaml).unwrap();
        assert_eq!(scale.root, "F#");

        // Should resolve to Gb (pitch class 6)
        let pc = pitch_class_from_name(&scale.root).unwrap();
        assert_eq!(pc, 6);
    }

    #[test]
    fn test_pitch_class_name_roundtrip() {
        for (i, name) in PITCH_CLASS_NAMES.iter().enumerate() {
            assert_eq!(pitch_class_from_name(name), Some(i as u8),
                "Pitch class name '{}' should map to {}", name, i);
        }
    }

    #[test]
    fn test_derive_refinement_tag_known_scales() {
        let penta = ScaleDefinition { intervals: vec![2,2,3,2,3], root: "C".into() };
        assert_eq!(derive_refinement_tag(&penta), "pentatonic/C");

        let penta_d = ScaleDefinition { intervals: vec![2,2,3,2,3], root: "D".into() };
        assert_eq!(derive_refinement_tag(&penta_d), "pentatonic/D");

        let minor_penta = ScaleDefinition { intervals: vec![3,2,2,3,2], root: "A".into() };
        assert_eq!(derive_refinement_tag(&minor_penta), "minor-pentatonic/A");

        let blues = ScaleDefinition { intervals: vec![3,2,1,1,3,2], root: "A".into() };
        assert_eq!(derive_refinement_tag(&blues), "blues/A");

        let diatonic = ScaleDefinition { intervals: vec![2,2,1,2,2,2,1], root: "C".into() };
        assert_eq!(derive_refinement_tag(&diatonic), "diatonic/C");

        let minor = ScaleDefinition { intervals: vec![2,1,2,2,1,2,2], root: "A".into() };
        assert_eq!(derive_refinement_tag(&minor), "minor/A");
    }

    #[test]
    fn test_derive_refinement_tag_chromatic() {
        let chromatic = ScaleDefinition {
            intervals: vec![1,1,1,1,1,1,1,1,1,1,1,1],
            root: "C".into(),
        };
        assert_eq!(derive_refinement_tag(&chromatic), "chromatic");
    }

    #[test]
    fn test_derive_refinement_tag_unknown_pattern() {
        let exotic = ScaleDefinition { intervals: vec![4,3,5], root: "E".into() };
        assert_eq!(derive_refinement_tag(&exotic), "scale/E/4-3-5");
    }
}
