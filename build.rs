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
    
    // Generate language index map
    code.push_str("/// Get the list of available languages\n");
    code.push_str("pub fn get_available_languages() -> &'static [&'static str] {\n");
    code.push_str("    &[\n");
    for lang in languages.keys() {
        code.push_str(&format!("        \"{}\",\n", lang));
    }
    code.push_str("    ]\n");
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
    current_dir: &Path,
    languages: &mut HashMap<String, LanguageFiles>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: Vec<_> = fs::read_dir(current_dir)?
        .filter_map(|e| e.ok())
        .collect();
    // Sort directory entries for deterministic scanning order
    entries.sort_by_key(|e| e.path());
    
    for entry in entries {
        let path = entry.path();
        
        if path.is_dir() {
            // Recursively scan subdirectories
            scan_languages_dir(base_dir, &path, languages)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            // Found a YAML file
            let rel_path = path.strip_prefix(base_dir)?.to_path_buf();
            let components: Vec<&str> = rel_path
                .iter()
                .map(|c| c.to_str().unwrap())
                .collect();
            
            if components.len() >= 2 {
                let lang = components[0..components.len() - 1].join("/");
                let filename = components.last().unwrap();
                
                let files = languages.entry(lang.clone()).or_insert_with(|| LanguageFiles {
                    payload: None,
                    cover: None,
                    grammar: None,
                    pos_mapping: None,
                    dialect: None,
                    other: Vec::new(),
                });
                
                if filename.starts_with("payload") && filename.ends_with(".yaml") {
                    // Mark that this language has payload (for has_embedded_files)
                    if files.payload.is_none() {
                        files.payload = Some(path.clone());
                    } else {
                        // Additional payload variants go in other
                        files.other.push((filename.to_string(), path.clone()));
                    }
                } else if filename.starts_with("cover") && filename.ends_with(".yaml") {
                    if files.cover.is_none() {
                        files.cover = Some(path.clone());
                    } else {
                        files.other.push((filename.to_string(), path.clone()));
                    }
                } else {
                    match *filename {
                        "grammar.yaml" => files.grammar = Some(path.clone()),
                        "pos_mapping.yaml" => files.pos_mapping = Some(path.clone()),
                        "dialect.yaml" => files.dialect = Some(path.clone()),
                        _ => {
                            files.other.push((filename.to_string(), path.clone()));
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}
