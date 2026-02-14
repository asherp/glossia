use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::{BTreeMap, HashMap, HashSet};

fn main() {
    // Tell Cargo to rerun this build script if languages directory changes
    println!("cargo:rerun-if-changed=languages");

    // Generate language index at build time
    let languages_dir = Path::new("languages");
    if languages_dir.exists() {
        match generate_language_index(languages_dir) {
            Ok(index_path) => {
                println!("cargo:rerun-if-changed={}", index_path.display());
            }
            Err(e) => {
                eprintln!("Warning: Failed to generate language index: {}", e);
            }
        }

        // Validate cover/payload disjointness at compile time
        validate_cover_payload_disjoint(languages_dir);

        // Validate payload wordlists are power-of-two sized
        validate_payload_power_of_two(languages_dir);
    }
}

/// Validate that cover and payload wordlists are disjoint for each language.
/// Panics (failing the build) if any overlap is found.
fn validate_cover_payload_disjoint(languages_dir: &Path) {
    let entries = match fs::read_dir(languages_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let lang = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // Find all payload*.yaml and cover*.yaml files in this language directory
        let files: Vec<_> = fs::read_dir(&path)
            .into_iter()
            .flat_map(|rd| rd.filter_map(|e| e.ok()))
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("yaml"))
            .collect();

        let payload_files: Vec<PathBuf> = files.iter()
            .filter(|e| e.file_name().to_string_lossy().starts_with("payload"))
            .map(|e| e.path())
            .collect();

        // For each payload file, find its paired cover file and check disjointness
        for payload_path in &payload_files {
            let payload_name = payload_path.file_name().unwrap().to_string_lossy();

            // Determine the paired cover file:
            // payload.yaml -> cover.yaml
            // payload_bip39.yaml -> cover.yaml (bip39 is the default)
            // payload_X.yaml -> cover_X.yaml
            let cover_path = if payload_name == "payload.yaml" || payload_name == "payload_bip39.yaml" {
                path.join("cover.yaml")
            } else {
                let suffix = payload_name.strip_prefix("payload").unwrap();
                path.join(format!("cover{}", suffix))
            };

            if !cover_path.exists() {
                continue; // No cover file for this payload — skip
            }

            let payload_words = extract_yaml_keys(payload_path);
            let cover_words = extract_yaml_keys(&cover_path);

            let payload_set: HashSet<String> = payload_words.into_iter().map(|w| w.to_lowercase()).collect();
            let overlap: Vec<String> = cover_words.iter()
                .filter(|w| payload_set.contains(&w.to_lowercase()))
                .cloned()
                .collect();

            if !overlap.is_empty() {
                let sample: Vec<&str> = overlap.iter().take(10).map(|s| s.as_str()).collect();
                panic!(
                    "Build error: {lang} cover/payload overlap detected!\n\
                     Payload: {}\n\
                     Cover: {}\n\
                     Overlapping words ({} total): {}{}\n\
                     Cover and payload wordlists must be disjoint for decoding to work.",
                    payload_path.display(),
                    cover_path.display(),
                    overlap.len(),
                    sample.join(", "),
                    if overlap.len() > 10 { ", ..." } else { "" }
                );
            }
        }
    }
}

/// Validate that every payload wordlist has a power-of-two number of words.
/// Panics (failing the build) if any payload wordlist has a non-power-of-two size.
fn validate_payload_power_of_two(languages_dir: &Path) {
    validate_power_of_two_recursive(languages_dir, languages_dir);
}

/// Check if a language directory opts out of bit-packing validation.
///
/// Returns true when either:
/// - `grammar.bitpacking` is explicitly `false`, or
/// - `grammar.payload_separator` is `""` (character-level encoding, legacy check)
///
/// Languages that don't bit-pack (meta-language, character-level CS alphabets)
/// are free to have non-power-of-two payload sizes.
fn skips_bitpacking(dir: &Path) -> bool {
    let grammar_path = dir.join("grammar.yaml");
    let content = match fs::read_to_string(&grammar_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let data: HashMap<String, serde_yaml::Value> = match serde_yaml::from_str(&content) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if let Some(grammar) = data.get("grammar") {
        // Explicit opt-out: bitpacking: false
        if let Some(bp) = grammar.get("bitpacking") {
            if bp.as_bool() == Some(false) {
                return true;
            }
        }
        // Legacy: character-level encoding (payload_separator: "")
        if let Some(sep) = grammar.get("payload_separator") {
            if sep.as_str() == Some("") {
                return true;
            }
        }
    }
    false
}

fn validate_power_of_two_recursive(base_dir: &Path, current_dir: &Path) {
    // Skip languages that opt out of bit-packing (character-level alphabets,
    // meta-language, etc.) — the power-of-two constraint doesn't apply.
    if skips_bitpacking(current_dir) {
        return;
    }

    let entries = match fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            validate_power_of_two_recursive(base_dir, &path);
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("payload") || !name.ends_with(".yaml") {
            continue;
        }

        let words = extract_yaml_keys(&path);
        let n = words.len();
        if n == 0 {
            continue;
        }
        if n & (n - 1) != 0 {
            let lang = current_dir.strip_prefix(base_dir)
                .unwrap_or(current_dir)
                .to_string_lossy();
            let prev_pow2: usize = 1 << (usize::BITS - 1 - n.leading_zeros());
            let next_pow2: usize = prev_pow2 << 1;
            panic!(
                "Build error: {lang} payload wordlist size is not a power of two!\n\
                 File: {}\n\
                 Word count: {n}\n\
                 Nearest powers of two: {prev_pow2} (2^{}) or {next_pow2} (2^{})\n\
                 Payload wordlists must be powers of two for bit-packing to work.\n\
                 Either pad to {next_pow2} words or trim to {prev_pow2} words.",
                path.display(),
                prev_pow2.trailing_zeros(),
                next_pow2.trailing_zeros(),
            );
        }
    }

}

/// Extract top-level keys from a YAML file (word -> {POS: weight} format).
fn extract_yaml_keys(path: &PathBuf) -> Vec<String> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let data: HashMap<String, serde_yaml::Value> = match serde_yaml::from_str(&content) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    data.keys().cloned().collect()
}

#[derive(Debug, Clone)]
struct LanguageFiles {
    payload: Option<PathBuf>,
    cover: Option<PathBuf>,
    grammar: Option<PathBuf>,
    pos_mapping: Option<PathBuf>,
    dialect: Option<PathBuf>,
    // Other YAML files
    other: Vec<(String, PathBuf)>,
}

fn generate_language_index(languages_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut languages: HashMap<String, LanguageFiles> = HashMap::new();
    
    // Recursively scan languages directory
    scan_languages_dir(languages_dir, languages_dir, &mut languages)?;
    
    // Sort languages for deterministic output (prevents unnecessary rebuilds)
    let languages: BTreeMap<String, LanguageFiles> = languages.into_iter().collect();
    
    // Generate Rust code
    let out_dir = env::var("OUT_DIR")?;
    let index_path = Path::new(&out_dir).join("language_index.rs");
    
    let mut code = String::from("// Auto-generated language index - do not edit manually\n");
    code.push_str("// Generated at build time by scanning languages/ directory\n\n");
    
    // Generate embedded file function
    code.push_str("/// Get embedded YAML file content by path relative to languages/ directory.\n");
    code.push_str("/// Returns Some(content) if file is embedded in release builds, None otherwise.\n");
    code.push_str("pub fn get_embedded_yaml(path: &str) -> Option<&'static str> {\n");
    code.push_str("    if cfg!(not(debug_assertions)) {\n");
    code.push_str("        // Release build: all YAML files are embedded\n");
    code.push_str("        match path {\n");
    
    // Generate match arms for each language file (BTreeMap ensures sorted order)
    for (_lang, files) in &languages {
        if let Some(ref payload) = files.payload {
            let rel_path = payload.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
        
        if let Some(ref cover) = files.cover {
            let rel_path = cover.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
        
        if let Some(ref grammar) = files.grammar {
            let rel_path = grammar.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
        
        if let Some(ref pos_mapping) = files.pos_mapping {
            let rel_path = pos_mapping.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
        
        if let Some(ref dialect) = files.dialect {
            let rel_path = dialect.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
        
        for (_name, path) in &files.other {
            let rel_path = path.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
    }
    
    code.push_str("            _ => None,\n");
    code.push_str("        }\n");
    code.push_str("    } else {\n");
    code.push_str("        // Debug build: only English files embedded\n");
    code.push_str("        match path {\n");

    // Debug build: embed all English files (payload, cover, grammar, other)
    if let Some(files) = languages.get("english") {
        if let Some(ref payload) = files.payload {
            let rel_path = payload.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
        if let Some(ref cover) = files.cover {
            let rel_path = cover.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
        for (_name, path) in &files.other {
            let rel_path = path.strip_prefix(languages_dir).unwrap();
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "            \"{}\" => Some(include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/languages/{}\"))),\n",
                rel_str, rel_str
            ));
        }
    }
    
    code.push_str("            _ => None,\n");
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");
    
    // Generate has_embedded_files function
    code.push_str("/// Check if a language has embedded files (packaged with the binary)\n");
    code.push_str("/// In release builds, all languages with YAML files are embedded.\n");
    code.push_str("/// In debug builds, only English is embedded (for faster iteration).\n");
    code.push_str("pub fn has_embedded_files(language: &str) -> bool {\n");
    code.push_str("    if cfg!(not(debug_assertions)) {\n");
    code.push_str("        // Release build: embed all languages that have payload.yaml\n");
    code.push_str("        matches!(language, ");
    
    // Already sorted since languages is a BTreeMap
    let lang_list: Vec<String> = languages.iter()
        .filter(|(_, files)| files.payload.is_some())
        .map(|(lang, _)| format!("\"{}\"", lang))
        .collect();
    code.push_str(&lang_list.join(" | "));
    code.push_str(")\n");
    code.push_str("    } else {\n");
    code.push_str("        // Debug build: only English embedded (faster rebuilds during development)\n");
    code.push_str("        matches!(language, \"english\")\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");
    
    // Generate language index map (only languages with a payload file)
    code.push_str("/// Get the list of available languages\n");
    code.push_str("pub fn get_available_languages() -> &'static [&'static str] {\n");
    code.push_str("    &[\n");
    for (lang, files) in &languages {
        if files.payload.is_some() {
            code.push_str(&format!("        \"{}\",\n", lang));
        }
    }
    code.push_str("    ]\n");
    code.push_str("}\n\n");

    // Generate per-language wordlist profiles from actual filenames
    code.push_str("/// Get available wordlist profiles for a language.\n");
    code.push_str("/// Derived at compile time from payload_*.yaml filenames.\n");
    code.push_str("/// \"default\" means payload.yaml exists; named profiles come from payload_{name}.yaml.\n");
    code.push_str("pub fn get_wordlist_profiles(language: &str) -> &'static [&'static str] {\n");
    code.push_str("    match language {\n");

    for (lang, files) in &languages {
        // Collect profile names from payload filenames
        let mut profiles: Vec<String> = Vec::new();

        // Gather all payload filenames (first payload + others)
        let mut payload_filenames: Vec<String> = Vec::new();
        if let Some(ref p) = files.payload {
            if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                payload_filenames.push(fname.to_string());
            }
        }
        for (name, _) in &files.other {
            if name.starts_with("payload") && name.ends_with(".yaml") {
                payload_filenames.push(name.clone());
            }
        }

        for fname in &payload_filenames {
            if fname == "payload.yaml" {
                profiles.push("default".to_string());
            } else if let Some(rest) = fname.strip_prefix("payload_") {
                if let Some(name) = rest.strip_suffix(".yaml") {
                    profiles.push(name.to_string());
                }
            }
        }

        // Sort: "default" first, then alphabetical
        profiles.sort_by(|a, b| {
            if a == "default" { std::cmp::Ordering::Less }
            else if b == "default" { std::cmp::Ordering::Greater }
            else { a.cmp(b) }
        });

        if !profiles.is_empty() {
            code.push_str(&format!("        \"{}\" => &[", lang));
            for (i, p) in profiles.iter().enumerate() {
                if i > 0 { code.push_str(", "); }
                code.push_str(&format!("\"{}\"", p));
            }
            code.push_str("],\n");
        }
    }

    code.push_str("        _ => &[],\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // Generate precomputed payload word index for fast dialect detection.
    // For each (language, wordlist profile), we extract payload words at build time,
    // sort them, and write a newline-delimited text file to OUT_DIR.
    // At runtime, binary search on these sorted texts gives O(log n) membership testing
    // instead of scanning raw YAML strings.
    let mut payload_index_entries: Vec<(String, String, String, usize)> = Vec::new(); // (lang, profile, filename, word_count)

    for (lang, files) in &languages {
        // Collect all payload files with their profile names
        let mut payload_paths: Vec<(String, PathBuf)> = Vec::new();

        if let Some(ref p) = files.payload {
            let fname = p.file_name().unwrap().to_str().unwrap();
            let profile = if fname == "payload.yaml" {
                "default".to_string()
            } else {
                fname.strip_prefix("payload_").unwrap()
                    .strip_suffix(".yaml").unwrap()
                    .to_string()
            };
            payload_paths.push((profile, p.clone()));
        }
        for (name, path) in &files.other {
            if name.starts_with("payload") && name.ends_with(".yaml") {
                let profile = if name == "payload.yaml" {
                    "default".to_string()
                } else {
                    name.strip_prefix("payload_").unwrap()
                        .strip_suffix(".yaml").unwrap()
                        .to_string()
                };
                payload_paths.push((profile, path.clone()));
            }
        }

        for (profile, path) in &payload_paths {
            let words = extract_yaml_keys(&path);
            let mut sorted_words: Vec<String> = words.into_iter()
                .map(|w| w.to_lowercase())
                .collect();
            sorted_words.sort();
            sorted_words.dedup();

            let word_count = sorted_words.len();
            let sanitized_lang = lang.replace('/', "_");
            let filename = format!("payload_words_{}_{}.txt", sanitized_lang, profile);
            let word_file_path = Path::new(&out_dir).join(&filename);

            let content = sorted_words.join("\n");
            // Only write if changed (avoids triggering unnecessary rebuilds)
            let should_write_words = match fs::read_to_string(&word_file_path) {
                Ok(existing) => existing != content,
                Err(_) => true,
            };
            if should_write_words {
                fs::write(&word_file_path, &content).unwrap();
            }

            payload_index_entries.push((lang.clone(), profile.clone(), filename, word_count));
        }
    }

    // Emit get_payload_word_index: returns sorted newline-delimited word list
    code.push_str("/// Get a precomputed sorted word list for a payload wordlist.\n");
    code.push_str("/// Returns a sorted, newline-delimited string of all payload words (lowercase).\n");
    code.push_str("/// Use `binary_search_sorted_words()` on the result for O(log n) membership testing.\n");
    code.push_str("pub fn get_payload_word_index(language: &str, wordlist: &str) -> Option<&'static str> {\n");
    code.push_str("    match (language, wordlist) {\n");

    for (lang, profile, filename, _count) in &payload_index_entries {
        code.push_str(&format!(
            "        (\"{}\", \"{}\") => Some(include_str!(concat!(env!(\"OUT_DIR\"), \"/{}\")))  ,\n",
            lang, profile, filename
        ));
    }

    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // Emit get_payload_word_count: precomputed word counts (exact, no estimation)
    code.push_str("/// Get the exact precomputed word count for a payload wordlist.\n");
    code.push_str("pub fn get_payload_word_count(language: &str, wordlist: &str) -> usize {\n");
    code.push_str("    match (language, wordlist) {\n");

    for (lang, profile, _filename, count) in &payload_index_entries {
        code.push_str(&format!(
            "        (\"{}\", \"{}\") => {},\n",
            lang, profile, count
        ));
    }

    code.push_str("        _ => 0,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // Generate language file paths map
    code.push_str("/// Get file paths for a language\n");
    code.push_str("#[derive(Debug, Clone)]\n");
    code.push_str("pub struct LanguageFilePaths {\n");
    code.push_str("    pub payload: Option<&'static str>,\n");
    code.push_str("    pub cover: Option<&'static str>,\n");
    code.push_str("    pub grammar: Option<&'static str>,\n");
    code.push_str("    pub pos_mapping: Option<&'static str>,\n");
    code.push_str("    pub dialect: Option<&'static str>,\n");
    code.push_str("}\n\n");
    
    code.push_str("/// Get file paths for a language (relative to languages/)\n");
    code.push_str("pub fn get_language_file_paths(language: &str) -> Option<LanguageFilePaths> {\n");
    code.push_str("    match language {\n");
    
    for (lang, files) in &languages {
        code.push_str(&format!("        \"{}\" => Some(LanguageFilePaths {{\n", lang));
        code.push_str(&format!(
            "            payload: {},\n",
            if files.payload.is_some() {
                format!(
                    "Some(\"{}\")",
                    files.payload.as_ref().unwrap()
                        .strip_prefix(languages_dir).unwrap()
                        .to_string_lossy().replace('\\', "/")
                )
            } else {
                "None".to_string()
            }
        ));
        code.push_str(&format!(
            "            cover: {},\n",
            if files.cover.is_some() {
                format!(
                    "Some(\"{}\")",
                    files.cover.as_ref().unwrap()
                        .strip_prefix(languages_dir).unwrap()
                        .to_string_lossy().replace('\\', "/")
                )
            } else {
                "None".to_string()
            }
        ));
        code.push_str(&format!(
            "            grammar: {},\n",
            if files.grammar.is_some() {
                format!(
                    "Some(\"{}\")",
                    files.grammar.as_ref().unwrap()
                        .strip_prefix(languages_dir).unwrap()
                        .to_string_lossy().replace('\\', "/")
                )
            } else {
                "None".to_string()
            }
        ));
        code.push_str(&format!(
            "            pos_mapping: {},\n",
            if files.pos_mapping.is_some() {
                format!(
                    "Some(\"{}\")",
                    files.pos_mapping.as_ref().unwrap()
                        .strip_prefix(languages_dir).unwrap()
                        .to_string_lossy().replace('\\', "/")
                )
            } else {
                "None".to_string()
            }
        ));
        code.push_str(&format!(
            "            dialect: {},\n",
            if files.dialect.is_some() {
                format!(
                    "Some(\"{}\")",
                    files.dialect.as_ref().unwrap()
                        .strip_prefix(languages_dir).unwrap()
                        .to_string_lossy().replace('\\', "/")
                )
            } else {
                "None".to_string()
            }
        ));
        code.push_str("        }),\n");
    }
    
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n");
    
    // Only write if content changed (avoids triggering rebuild when nothing changed)
    let should_write = match fs::read_to_string(&index_path) {
        Ok(existing) => existing != code,
        Err(_) => true,
    };
    if should_write {
        fs::write(&index_path, code)?;
    }
    
    Ok(index_path)
}

fn scan_languages_dir(
    base_dir: &Path,
    _current_dir: &Path,
    languages: &mut HashMap<String, LanguageFiles>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Use `git ls-files` to only pick up version-controlled files.
    // This prevents untracked scratch directories from being embedded.
    let tracked_files = std::process::Command::new("git")
        .args(["ls-files", "--full-name", "languages/"])
        .output();

    let file_list: Vec<PathBuf> = match tracked_files {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.lines()
                .filter(|line| line.ends_with(".yaml"))
                .map(|line| PathBuf::from(line))
                .collect()
        }
        _ => {
            // Fallback: scan filesystem if git is not available (e.g. published crate)
            return scan_languages_dir_fs(base_dir, base_dir, languages);
        }
    };

    for rel_from_repo in file_list {
        // rel_from_repo is like "languages/latin/payload.yaml"
        let path = rel_from_repo.clone();
        let rel_path = match rel_from_repo.strip_prefix("languages") {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        let components: Vec<&str> = rel_path
            .iter()
            .map(|c| c.to_str().unwrap())
            .collect();

        if components.len() >= 2 {
            let lang = components[0..components.len() - 1].join("/");
            let filename = components.last().unwrap();
            // Use absolute path for include_str!
            let abs_path = base_dir.join(&rel_path);

            let files = languages.entry(lang.clone()).or_insert_with(|| LanguageFiles {
                payload: None,
                cover: None,
                grammar: None,
                pos_mapping: None,
                dialect: None,
                other: Vec::new(),
            });

            classify_language_file(files, filename, &abs_path);
        }
    }

    Ok(())
}

/// Classify a YAML file into the appropriate LanguageFiles field.
fn classify_language_file(files: &mut LanguageFiles, filename: &str, path: &Path) {
    if filename.starts_with("payload") && filename.ends_with(".yaml") {
        if files.payload.is_none() {
            files.payload = Some(path.to_path_buf());
        } else {
            files.other.push((filename.to_string(), path.to_path_buf()));
        }
    } else if filename.starts_with("cover") && filename.ends_with(".yaml") {
        if files.cover.is_none() {
            files.cover = Some(path.to_path_buf());
        } else {
            files.other.push((filename.to_string(), path.to_path_buf()));
        }
    } else {
        match filename {
            "grammar.yaml" => files.grammar = Some(path.to_path_buf()),
            "pos_mapping.yaml" => files.pos_mapping = Some(path.to_path_buf()),
            "dialect.yaml" => files.dialect = Some(path.to_path_buf()),
            _ => {
                files.other.push((filename.to_string(), path.to_path_buf()));
            }
        }
    }
}

/// Filesystem fallback when git is not available (e.g. building from a published crate).
fn scan_languages_dir_fs(
    base_dir: &Path,
    current_dir: &Path,
    languages: &mut HashMap<String, LanguageFiles>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: Vec<_> = fs::read_dir(current_dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            scan_languages_dir_fs(base_dir, &path, languages)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            let rel_path = path.strip_prefix(base_dir)?.to_path_buf();
            let components: Vec<&str> = rel_path
                .iter()
                .map(|c| c.to_str().unwrap())
                .collect();

            if components.len() >= 2 {
                let lang = components[0..components.len() - 1].join("/");
                let filename = components.last().unwrap();

                let files = languages.entry(lang).or_insert_with(|| LanguageFiles {
                    payload: None,
                    cover: None,
                    grammar: None,
                    pos_mapping: None,
                    dialect: None,
                    other: Vec::new(),
                });

                classify_language_file(files, filename, &path);
            }
        }
    }

    Ok(())
}
