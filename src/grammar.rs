use rand::Rng;
use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use crate::types::{Pos, Sym};
use crate::type_driven_grammar::LanguageConfig;
use crate::semantic_types::{SemanticType, pos_to_semantic_type};
use crate::scale::ScaleDefinition;

#[derive(Clone, Debug)]
pub struct Production {
    pub symbols: Vec<Sym>,
    pub refinements: Vec<Option<String>>,  // parallel to symbols; e.g. Det[def] -> Some("def")
    pub weight: f64,
}

#[derive(Clone, Debug)]
pub struct GrammarRule {
    pub productions: Vec<Production>,
}

#[derive(Clone, Debug)]
pub struct Grammar {
    pub(crate) rules: HashMap<String, GrammarRule>,
    // Type-driven grammar configuration (new implementation)
    pub(crate) language_config: Option<LanguageConfig>,
    pub(crate) dialect: Option<String>,
    /// Separator between consecutive payload words in the output.
    /// Default: " " (space). CS languages use "" for character concatenation.
    /// This is the **payload grammar**: how the payload is formatted within the
    /// sentence structure. Decoding must use the same separator to tokenize.
    pub(crate) payload_separator: String,
    /// Whether the Dot POS appends a period to the previous word (true, default)
    /// or is treated as a normal cover-word slot (false).
    /// Set to false for languages like CS where Dot produces structural tokens
    /// (e.g., "-----" for ASCII armor) rather than sentence-ending punctuation.
    pub(crate) dot_is_punctuation: bool,
    /// If set, payload words are line-wrapped at this character width.
    /// Used for ASCII armor where the body is wrapped at 76 chars per line.
    pub(crate) payload_line_width: Option<usize>,
    /// Maximum sequence length for enumeration. Grammars with recursive rules
    /// (e.g., VP_PP_TAIL → PP VP_PP_TAIL) can set this to cap the combinatorial
    /// explosion during `precompute_sequences_with_probability`. If absent,
    /// the caller's default k_max applies.
    pub(crate) max_k: Option<usize>,
    /// Codec selection: "bitpack" for power-of-2 uniform distribution,
    /// "base_n" for big-integer base-N encoding.
    /// Declared in grammar.yaml; defaults to "bitpack" for backward compat.
    pub(crate) codec: String,
}

/// A POS sequence with its probability according to the grammar
#[derive(Clone, Debug)]
pub struct SequenceWithProbability {
    pub sequence: Vec<crate::types::Pos>,
    pub refinements: Vec<Option<String>>,  // parallel to sequence
    pub probability: f64,
    /// Precomputed set of POS tags in word slots (excluding Dot, Prefix, Aux, Cop, To).
    /// Used by plan_sentence to skip sequences that can't embed any remaining payload words.
    pub word_slot_pos: HashSet<Pos>,
}

impl SequenceWithProbability {
    /// Create a new SequenceWithProbability, automatically computing word_slot_pos.
    pub fn new(sequence: Vec<Pos>, refinements: Vec<Option<String>>, probability: f64) -> Self {
        let word_slot_pos = sequence.iter()
            .filter(|&&pos| !matches!(pos, Pos::Dot | Pos::Prefix | Pos::Aux | Pos::Cop | Pos::To))
            .copied()
            .collect();
        SequenceWithProbability { sequence, refinements, probability, word_slot_pos }
    }
}

/// Bundles a grammar with its dialect-specified wordlist references.
///
/// Each dialect in grammar.yaml may declare `payload_wordlist` and `cover_wordlist`
/// to bind a specific payload/cover pair. If omitted, both default to `"default"`.
///
/// The CLI `--wordlist` flag overrides `payload_wordlist` (and its implied cover).
///
/// # Examples
///
/// ```no_run
/// use glossia::grammar::DialectConfig;
///
/// // Load the Latin spells dialect — automatically resolves payload_hp.yaml + cover.yaml
/// let config = DialectConfig::from_language_dialect("latin", "spells").unwrap();
/// assert_eq!(config.payload_wordlist(), "hp");
/// assert_eq!(config.cover_wordlist(), "default");
/// ```
#[derive(Clone, Debug)]
pub struct DialectConfig {
    /// The grammar (CFG rules + type system) for this dialect.
    pub grammar: Grammar,
    /// Language name (e.g., "latin", "english", "cs").
    language: String,
    /// Dialect name (e.g., "body", "subject", "spells").
    dialect: String,
    /// Payload wordlist profile (e.g., "default", "hp", "bip39", "base58").
    /// Resolved via `wordlist_filenames()` to `payload_{name}.yaml` (or `payload.yaml` for "default").
    /// For scale-derived dialects, this is set to the dialect name (e.g., "pentatonic").
    payload_wl: String,
    /// Cover wordlist profile.
    /// Resolved via `wordlist_filenames()` to `cover_{name}.yaml` (or `cover.yaml` for "default").
    cover_wl: String,
    /// Optional override for the language used to resolve payload wordlist files.
    /// When set (e.g., `"cs"`), payload files are loaded from that language's directory
    /// instead of the current language. This enables sharing character-level wordlists
    /// (bech32, base58) across crypto sub-languages without duplication.
    payload_language: Option<String>,
    /// Optional scale definition (intervals + root) for scale-derived dialects.
    /// When present, the payload wordlist is derived at runtime from the base
    /// chromatic payload filtered by the scale's interval pattern.
    scale: Option<ScaleDefinition>,
}

impl DialectConfig {
    /// Load a dialect configuration from grammar.yaml.
    ///
    /// Reads the `dialects.<dialect>.payload_wordlist` and `dialects.<dialect>.cover_wordlist`
    /// fields. Falls back to `"default"` when fields are absent (backward compatible).
    ///
    /// If the dialect defines a `scale:` section (intervals + root), the payload wordlist
    /// is derived at runtime from the base chromatic payload filtered by the scale pattern.
    /// The derived words are injected into the in-memory cache so that subsequent
    /// `load_payload_words_for_wordlist()` calls find them transparently.
    pub fn from_language_dialect(language: &str, dialect: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let grammar = Grammar::from_language_dialect(language, dialect)?;
        let (mut payload_wl, payload_language, cover_wl) = Self::parse_wordlist_refs(language, dialect);
        let scale = Self::parse_scale_ref(language, dialect);

        // If this dialect has a scale definition, derive the payload wordlist
        // from the base chromatic payload and inject it into the cache.
        if let Some(ref scale_def) = scale {
            // Use dialect name as the wordlist identifier for scale-derived payloads.
            if payload_wl == "default" {
                payload_wl = dialect.to_string();
            }

            // Load the base chromatic payload and filter by scale.
            let chromatic = crate::generator::load_payload_words_for_wordlist(language, "default")
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let derived = crate::scale::filter_payload_by_scale(&chromatic, scale_def)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            crate::generator::inject_scale_payload(language, &payload_wl, derived)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        }

        Ok(DialectConfig {
            grammar,
            language: language.to_string(),
            dialect: dialect.to_string(),
            payload_wl,
            cover_wl,
            payload_language,
            scale,
        })
    }

    /// Language name.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Dialect name.
    pub fn dialect(&self) -> &str {
        &self.dialect
    }

    /// Payload wordlist profile name (e.g., "default", "hp", "bip39").
    pub fn payload_wordlist(&self) -> &str {
        &self.payload_wl
    }

    /// Scale definition, if this dialect derives its payload from interval patterns.
    pub fn scale(&self) -> Option<&ScaleDefinition> {
        self.scale.as_ref()
    }

    /// Cover wordlist profile name (e.g., "default").
    pub fn cover_wordlist(&self) -> &str {
        &self.cover_wl
    }

    /// The language used to resolve payload wordlist files.
    /// Returns `payload_language` if set, otherwise falls back to `self.language`.
    pub fn payload_language(&self) -> &str {
        self.payload_language.as_deref().unwrap_or(&self.language)
    }

    /// Get the refinement tag for N (payload note) slots, if any.
    ///
    /// Returns the auto-derived or explicit type_refinements tag for the N POS.
    /// This is used to populate `Lexicon::refined_payload` for runtime validation.
    pub fn n_refinement_tag(&self) -> Option<String> {
        // If there's a scale, derive the tag from it
        if let Some(ref scale_def) = self.scale {
            return Some(crate::scale::derive_refinement_tag(scale_def));
        }
        None
    }

    /// Override the payload wordlist (e.g., from CLI `--wordlist` flag).
    /// This also updates the cover wordlist to the corresponding named variant
    /// if one exists, otherwise leaves cover unchanged.
    pub fn with_payload_wordlist(mut self, wordlist: &str) -> Self {
        self.payload_wl = wordlist.to_string();
        // Check if a matching cover wordlist exists for this payload profile;
        // if so, switch to it. Otherwise keep the dialect's cover.
        let cover_key = format!("{}/cover_{}.yaml", self.language, wordlist);
        if crate::generator::data::get_embedded_yaml(&cover_key).is_some() {
            self.cover_wl = wordlist.to_string();
        }
        self
    }

    /// Override the cover wordlist explicitly.
    pub fn with_cover_wordlist(mut self, wordlist: &str) -> Self {
        self.cover_wl = wordlist.to_string();
        self
    }

    /// Resolve payload and cover filenames for this dialect.
    /// Returns `(payload_filename, cover_filename)` — e.g., `("payload_hp.yaml", "cover.yaml")`.
    /// Uses `payload_language` (if set) to resolve payload files from a different language.
    pub fn wordlist_filenames(&self) -> (String, String) {
        let lang = self.payload_language.as_deref().unwrap_or(&self.language);
        crate::generator::data::wordlist_filenames(lang, &self.payload_wl)
    }

    /// List all dialect names defined in a language's grammar.yaml.
    ///
    /// Returns the base dialect "body" plus all names under the `dialects:` key.
    pub fn available_dialects(language: &str) -> Vec<String> {
        let mut dialects = vec!["body".to_string()];

        // Try embedded first, then hardcoded fallbacks, then filesystem
        let yaml_content: Option<String> = crate::generator::data::get_embedded_yaml(
            &format!("{}/grammar.yaml", language),
        ).map(|s| s.to_string()).or_else(|| match language {
            "latin" => Some(include_str!("../languages/latin/grammar.yaml").to_string()),
            "english" => Some(include_str!("../languages/english/grammar.yaml").to_string()),
            _ => None,
        });

        #[cfg(not(target_arch = "wasm32"))]
        let yaml_content = yaml_content.or_else(|| {
            let path = format!("languages/{}/grammar.yaml", language);
            std::fs::read_to_string(&path).ok()
        });

        if let Some(content) = yaml_content {
            if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                if let Some(grammar) = doc.get("grammar") {
                    if let Some(dialect_map) = grammar.get("dialects").and_then(|d| d.as_mapping()) {
                        for (key, _) in dialect_map {
                            if let Some(name) = key.as_str() {
                                if name != "body" {
                                    dialects.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        dialects
    }

    /// Parse `payload_wordlist`, `payload_language`, and `cover_wordlist` from the grammar.yaml dialect section.
    /// Returns ("default", None, "default") when the dialect or its wordlist fields are absent.
    fn parse_wordlist_refs(language: &str, dialect: &str) -> (String, Option<String>, String) {
        // "body" dialect uses base rules (no dialect override section), so defaults apply
        if dialect == "body" {
            return ("default".to_string(), None, "default".to_string());
        }

        // Try embedded grammar.yaml first, then hardcoded fallbacks
        let yaml_content = crate::generator::data::get_embedded_yaml(
            &format!("{}/grammar.yaml", language),
        ).or_else(|| match language {
            "latin" => Some(include_str!("../languages/latin/grammar.yaml")),
            "english" => Some(include_str!("../languages/english/grammar.yaml")),
            _ => None,
        });

        // Helper: extract payload/cover/payload_language wordlist refs from a dialect YAML node,
        // falling back to parent dialect if the node has a `parent:` field.
        fn extract_wordlist_refs(dialects: &serde_yaml::Value, dialect: &str) -> Option<(String, Option<String>, String)> {
            let dialect_data = dialects.get(dialect)?;

            // Check for parent inheritance
            let parent_refs = dialect_data.get("parent")
                .and_then(|p| p.as_str())
                .and_then(|parent| extract_wordlist_refs(dialects, parent));

            let (default_payload, default_payload_lang, default_cover) = parent_refs
                .unwrap_or_else(|| ("default".to_string(), None, "default".to_string()));

            let payload = dialect_data.get("payload_wordlist")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or(default_payload);
            let payload_lang = dialect_data.get("payload_language")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or(default_payload_lang);
            let cover = dialect_data.get("cover_wordlist")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or(default_cover);
            Some((payload, payload_lang, cover))
        }

        if let Some(content) = yaml_content {
            if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                if let Some(grammar) = doc.get("grammar") {
                    if let Some(dialects) = grammar.get("dialects") {
                        if let Some(refs) = extract_wordlist_refs(dialects, dialect) {
                            return refs;
                        }
                    }
                }
            }
        }

        // Fallback: try reading from filesystem (native only)
        #[cfg(not(target_arch = "wasm32"))]
        {
            let grammar_yaml_path = format!("languages/{}/grammar.yaml", language);
            if let Ok(content) = std::fs::read_to_string(&grammar_yaml_path) {
                if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                    if let Some(grammar) = doc.get("grammar") {
                        if let Some(dialects) = grammar.get("dialects") {
                            if let Some(refs) = extract_wordlist_refs(dialects, dialect) {
                                return refs;
                            }
                        }
                    }
                }
            }
        }

        ("default".to_string(), None, "default".to_string())
    }

    /// Parse a `scale:` definition from a dialect in grammar.yaml.
    ///
    /// Walks the dialect inheritance chain (via `parent:`) to find a scale definition.
    /// Returns `None` if no scale is defined anywhere in the chain.
    fn parse_scale_ref(language: &str, dialect: &str) -> Option<ScaleDefinition> {
        if dialect == "body" {
            return None;
        }

        fn extract_scale(dialects: &serde_yaml::Value, dialect: &str) -> Option<ScaleDefinition> {
            let dialect_data = dialects.get(dialect)?;

            // Check this dialect's own scale definition first.
            if let Some(scale_value) = dialect_data.get("scale") {
                if let Ok(scale) = crate::scale::parse_scale_definition(scale_value) {
                    return Some(scale);
                }
            }

            // Fall back to parent's scale definition.
            dialect_data.get("parent")
                .and_then(|p| p.as_str())
                .and_then(|parent| extract_scale(dialects, parent))
        }

        // Try embedded grammar.yaml first
        let yaml_content = crate::generator::data::get_embedded_yaml(
            &format!("{}/grammar.yaml", language),
        ).or_else(|| match language {
            "latin" => Some(include_str!("../languages/latin/grammar.yaml")),
            "english" => Some(include_str!("../languages/english/grammar.yaml")),
            _ => None,
        });

        if let Some(content) = yaml_content {
            if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                if let Some(grammar) = doc.get("grammar") {
                    if let Some(dialects) = grammar.get("dialects") {
                        if let Some(scale) = extract_scale(dialects, dialect) {
                            return Some(scale);
                        }
                    }
                }
            }
        }

        // Fallback: try reading from filesystem (native only)
        #[cfg(not(target_arch = "wasm32"))]
        {
            let grammar_yaml_path = format!("languages/{}/grammar.yaml", language);
            if let Ok(content) = std::fs::read_to_string(&grammar_yaml_path) {
                if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                    if let Some(grammar) = doc.get("grammar") {
                        if let Some(dialects) = grammar.get("dialects") {
                            if let Some(scale) = extract_scale(dialects, dialect) {
                                return Some(scale);
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

impl Grammar {
    /// Separator between consecutive payload words in the output.
    pub fn payload_separator(&self) -> &str {
        &self.payload_separator
    }

    /// Whether Dot POS appends a period (true) or acts as a normal cover-word slot (false).
    pub fn dot_is_punctuation(&self) -> bool {
        self.dot_is_punctuation
    }

    /// Character width for payload line wrapping (None = no wrapping).
    pub fn payload_line_width(&self) -> Option<usize> {
        self.payload_line_width
    }

    /// Maximum sequence enumeration length declared by this grammar.
    /// Returns None if unconstrained (caller decides).
    pub fn max_k(&self) -> Option<usize> {
        self.max_k
    }

    /// Codec selection: "bitpack" or "base_n".
    pub fn codec(&self) -> &str {
        &self.codec
    }

    /// Load grammar from grammar.yaml (type-driven)
    pub fn default() -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_language_dialect("latin", "body")
    }
    
    /// Convert cfg_productions from grammar.yaml into CFG rules.
    /// If dialect != "body", overlay dialect-specific rules on top of base rules.
    fn build_rules_from_cfg_productions(yaml_content: &str, dialect: &str) -> Result<HashMap<String, GrammarRule>, Box<dyn std::error::Error>> {
        use serde_yaml::Value;
        let doc: Value = serde_yaml::from_str(yaml_content)?;

        // Map grammar.yaml rule names to CFG non-terminal names
        let rule_name_map: HashMap<&str, &str> = [
            ("verb_phrase", "VP"),
            ("noun_phrase", "NP"),
            ("prepositional_phrase", "PP"),
            ("sentence", "S"),
            ("sentence_content", "SContent"),
            ("vp_with_pp", "VP_PP_TAIL"),
            ("vp_base", "VP_BASE"),
        ].iter().cloned().collect();

        // Helper: parse a rules mapping (Value) into HashMap<String, GrammarRule>
        let parse_rules_map = |rules_map: &serde_yaml::Mapping| -> Result<HashMap<String, GrammarRule>, Box<dyn std::error::Error>> {
            let mut rules = HashMap::new();
            for (rule_name, rule_data) in rules_map {
                let rule_name_str = rule_name.as_str().ok_or("Invalid rule name")?;
                let cfg_name = rule_name_map.get(rule_name_str).copied().unwrap_or(rule_name_str);

                if let Some(cfg_prods) = rule_data.get("cfg_productions").and_then(|p| p.as_sequence()) {
                    let mut productions = Vec::new();

                    for prod in cfg_prods {
                        if let Some(prod_map) = prod.as_mapping() {
                            let production_str = prod_map.get("production")
                                .and_then(|p| p.as_str())
                                .ok_or("Missing production string")?;
                            let weight = prod_map.get("weight")
                                .and_then(|w| w.as_f64())
                                .unwrap_or(1.0);

                            let (symbols, refinements) = Self::parse_cfg_production_string(production_str)?;
                            productions.push(Production { symbols, refinements, weight });
                        }
                    }

                    if !productions.is_empty() {
                        // Normalize weights
                        let total_weight: f64 = productions.iter().map(|p| p.weight).sum();
                        if total_weight > 0.0 {
                            for prod in &mut productions {
                                prod.weight /= total_weight;
                            }
                        } else {
                            let equal_weight = 1.0 / productions.len() as f64;
                            for prod in &mut productions {
                                prod.weight = equal_weight;
                            }
                        }

                        rules.insert(cfg_name.to_string(), GrammarRule { productions });
                    }
                }
            }
            Ok(rules)
        };

        let mut rules = HashMap::new();

        // Parse base rules
        if let Some(grammar) = doc.get("grammar") {
            if let Some(rules_map) = grammar.get("rules").and_then(|r| r.as_mapping()) {
                rules = parse_rules_map(rules_map)?;
            }

            // If dialect != "body", overlay dialect-specific rules.
            // Supports `parent:` field for dialect inheritance (e.g., subject_re inherits from subject).
            if dialect != "body" {
                if let Some(dialects) = grammar.get("dialects") {
                    // Check if the dialect has a parent — if so, apply parent rules first.
                    if let Some(dialect_data) = dialects.get(dialect) {
                        if let Some(parent) = dialect_data.get("parent").and_then(|p| p.as_str()) {
                            if let Some(parent_data) = dialects.get(parent) {
                                if let Some(parent_rules_map) = parent_data.get("rules").and_then(|r| r.as_mapping()) {
                                    let parent_rules = parse_rules_map(parent_rules_map)?;
                                    for (name, rule) in parent_rules {
                                        rules.insert(name, rule);
                                    }
                                }
                            }
                        }
                    }
                    // Then apply the dialect's own rules on top.
                    if let Some(dialect_data) = dialects.get(dialect) {
                        if let Some(dialect_rules_map) = dialect_data.get("rules").and_then(|r| r.as_mapping()) {
                            let dialect_rules = parse_rules_map(dialect_rules_map)?;
                            // Override matching rules
                            for (name, rule) in dialect_rules {
                                rules.insert(name, rule);
                            }
                        }
                    }
                }
            }

            // --- Inject type_refinements into POS slots ---
            // After all rules are resolved, inject refinement tags from type_refinements
            // (explicit or auto-derived from scale definition).
            if dialect != "body" {
                if let Some(dialects) = grammar.get("dialects") {
                    let type_refs = Self::resolve_type_refinements(dialects, dialect);
                    if !type_refs.is_empty() {
                        Self::inject_type_refinements(&mut rules, &type_refs);
                    }
                }
            }
        }

        Ok(rules)
    }

    /// Resolve type_refinements for a dialect, walking the inheritance chain.
    ///
    /// Priority:
    /// 1. Explicit `type_refinements:` on the dialect itself
    /// 2. Auto-derived from `scale:` on the dialect (via `derive_refinement_tag`)
    /// 3. Inherited from parent dialect (same priority chain)
    /// 4. Empty (no refinements)
    fn resolve_type_refinements(
        dialects: &serde_yaml::Value,
        dialect: &str,
    ) -> HashMap<String, String> {
        let dialect_data = match dialects.get(dialect) {
            Some(d) => d,
            None => return HashMap::new(),
        };

        // 1. Explicit type_refinements on this dialect
        if let Some(tr) = dialect_data.get("type_refinements").and_then(|v| v.as_mapping()) {
            let mut refs = HashMap::new();
            for (pos_key, ref_val) in tr {
                if let (Some(pos_str), Some(ref_str)) = (pos_key.as_str(), ref_val.as_str()) {
                    refs.insert(pos_str.to_string(), ref_str.to_string());
                }
            }
            if !refs.is_empty() {
                return refs;
            }
        }

        // 2. Auto-derive from scale definition
        if let Some(scale_value) = dialect_data.get("scale") {
            if let Ok(scale_def) = crate::scale::parse_scale_definition(scale_value) {
                let tag = crate::scale::derive_refinement_tag(&scale_def);
                let mut refs = HashMap::new();
                refs.insert("N".to_string(), tag);
                return refs;
            }
        }

        // 3. Inherit from parent
        if let Some(parent) = dialect_data.get("parent").and_then(|p| p.as_str()) {
            return Self::resolve_type_refinements(dialects, parent);
        }

        HashMap::new()
    }

    /// Inject type refinement tags into all productions that contain matching POS slots.
    ///
    /// For each (POS, refinement_tag) in type_refinements, iterates all grammar rules
    /// and injects the refinement tag into any terminal slot matching that POS,
    /// unless the slot already has an explicit refinement.
    fn inject_type_refinements(
        rules: &mut HashMap<String, GrammarRule>,
        type_refs: &HashMap<String, String>,
    ) {
        for rule in rules.values_mut() {
            for prod in &mut rule.productions {
                for (i, sym) in prod.symbols.iter().enumerate() {
                    if let Sym::T(pos) = sym {
                        let pos_name = pos.as_str();
                        if let Some(ref_tag) = type_refs.get(pos_name) {
                            // Only inject if no explicit refinement already set
                            if prod.refinements.get(i).map_or(true, |r| r.is_none()) {
                                if i < prod.refinements.len() {
                                    prod.refinements[i] = Some(ref_tag.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Parse a CFG production string like "Cop Adj" or "Det[def] N" into symbol + refinement sequences.
    /// Refinement syntax: POS[tag] (e.g. Det[def], Cop[sg]).
    /// Non-terminals and unrefined terminals get refinement None.
    fn parse_cfg_production_string(production: &str) -> Result<(Vec<Sym>, Vec<Option<String>>), Box<dyn std::error::Error>> {
        let mut symbols = Vec::new();
        let mut refinements = Vec::new();

        for token in production.split_whitespace() {
            // Split off optional [refinement] suffix
            let (pos_str, refinement) = if let Some(bracket_start) = token.find('[') {
                let pos_part = &token[..bracket_start];
                let ref_part = token[bracket_start+1..].trim_end_matches(']');
                (pos_part, Some(ref_part.to_string()))
            } else {
                (token, None)
            };

            // Check if it's a POS terminal
            let sym = if let Some(pos) = Pos::from_str(pos_str) {
                Sym::T(pos)
            } else {
                // Assume it's a non-terminal
                Sym::NT(pos_str.to_string())
            };

            // Non-terminals don't carry refinements
            let ref_out = if matches!(sym, Sym::T(_)) { refinement } else { None };
            symbols.push(sym);
            refinements.push(ref_out);
        }

        Ok((symbols, refinements))
    }
    
    /// Extract payload_separator, dot_is_punctuation, payload_line_width, max_k, and codec from grammar YAML.
    /// Dialect-level overrides take precedence over grammar-level defaults.
    fn parse_payload_format(yaml_content: &str, dialect: &str) -> (String, bool, Option<usize>, Option<usize>, String) {
        let doc: Result<serde_yaml::Value, _> = serde_yaml::from_str(yaml_content);
        if let Ok(doc) = doc {
            if let Some(grammar) = doc.get("grammar") {
                // Grammar-level defaults
                let mut payload_sep = grammar.get("payload_separator")
                    .and_then(|v| v.as_str())
                    .unwrap_or(" ")
                    .to_string();
                let dot_is_punct = grammar.get("dot_is_punctuation")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let mut payload_line_width = grammar.get("payload_line_width")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let max_k = grammar.get("max_k")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let mut codec = grammar.get("codec")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| {
                        eprintln!("Warning: grammar.yaml missing 'codec' field, defaulting to 'bitpack'");
                        "bitpack"
                    })
                    .to_string();
                // Dialect-level overrides (if dialect != "body")
                if dialect != "body" {
                    if let Some(dialect_data) = grammar.get("dialects")
                        .and_then(|d| d.get(dialect))
                    {
                        // payload_separator override
                        if let Some(sep) = dialect_data.get("payload_separator")
                            .and_then(|v| v.as_str())
                        {
                            payload_sep = sep.to_string();
                        }
                        // payload_line_width: null in YAML → override to None (no wrapping)
                        if dialect_data.get("payload_line_width").is_some() {
                            payload_line_width = dialect_data.get("payload_line_width")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as usize);
                        }
                        // codec override
                        if let Some(c) = dialect_data.get("codec")
                            .and_then(|v| v.as_str())
                        {
                            codec = c.to_string();
                        }
                    }
                }

                return (payload_sep, dot_is_punct, payload_line_width, max_k, codec);
            }
        }
        (" ".to_string(), true, None, None, "bitpack".to_string())
    }

    /// Load grammar from language and dialect (YAML-only)
    pub fn from_language_dialect(language: &str, dialect: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Try embedded grammar.yaml first (via build.rs-generated index, then hardcoded fallback)
        let embedded_yaml = crate::generator::data::get_embedded_yaml(&format!("{}/grammar.yaml", language))
            .or_else(|| match language {
                "latin" => Some(include_str!("../languages/latin/grammar.yaml")),
                "english" => Some(include_str!("../languages/english/grammar.yaml")),
                _ => None,
            });

        if let Some(yaml_content) = embedded_yaml {
            match LanguageConfig::from_yaml(yaml_content) {
                Ok(language_config) => {
                    let rules = Self::build_rules_from_cfg_productions(yaml_content, dialect)?;
                    let (payload_separator, dot_is_punctuation, payload_line_width, max_k, codec) =
                        Self::parse_payload_format(yaml_content, dialect);
                    return Ok(Grammar {
                        rules,
                        language_config: Some(language_config),
                        dialect: Some(dialect.to_string()),
                        payload_separator,
                        dot_is_punctuation,
                        payload_line_width,
                        max_k,
                        codec,
                        });
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load embedded grammar.yaml: {}", e);
                }
            }
        }

        // Fall back to filesystem lookup (native only)
        #[cfg(not(target_arch = "wasm32"))]
        {
            let grammar_yaml_path = format!("languages/{}/grammar.yaml", language);
            let grammar_yaml_path = if std::path::Path::new(&grammar_yaml_path).exists() {
                Some(grammar_yaml_path)
            } else {
                find_grammar_file_recursive(language, "grammar.yaml")
            };

            if let Some(path) = grammar_yaml_path {
                let grammar_yaml = std::fs::read_to_string(&path)?;
                let language_config = LanguageConfig::from_yaml(&grammar_yaml)?;
                let rules = Self::build_rules_from_cfg_productions(&grammar_yaml, dialect)?;
                let (payload_separator, dot_is_punctuation, payload_line_width, max_k, codec) =
                    Self::parse_payload_format(&grammar_yaml, dialect);
                return Ok(Grammar {
                    rules,
                    language_config: Some(language_config),
                    dialect: Some(dialect.to_string()),
                    payload_separator,
                    dot_is_punctuation,
                    payload_line_width,
                    max_k,
                    codec,
                });
            }
        }

        Err(format!("No grammar.yaml found for language '{}'", language).into())
    }

    /// Load grammar from a YAML file on disk (CFG rules only, no type-driven generation).
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    pub fn from_file(grammar_path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let grammar_yaml = std::fs::read_to_string(grammar_path)?;
        let rules = Self::build_rules_from_cfg_productions(&grammar_yaml, "body")?;
        let (payload_separator, dot_is_punctuation, payload_line_width, max_k, codec) =
            Self::parse_payload_format(&grammar_yaml, "body");
        Ok(Grammar {
            rules,
            language_config: None,
            dialect: Some("body".to_string()),
            payload_separator,
            dot_is_punctuation,
            payload_line_width,
            max_k,
            codec,
        })
    }

    /// Convenience helper used by tests.
    /// Loads grammar from a YAML file and precomputes sequences.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    pub fn precompute_sequences_with_probability_cached_from_file(
        grammar_path: impl AsRef<Path>,
        start_symbol: &str,
        max_k: usize,
    ) -> Result<Vec<Vec<SequenceWithProbability>>, Box<dyn std::error::Error>> {
        let grammar = Self::from_file(grammar_path)?;
        Ok(grammar.precompute_sequences_with_probability(start_symbol, max_k))
    }
    
    /// Load subject grammar from grammar.yaml (type-driven) or fallback to CFG
    pub fn subject() -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_language_dialect("latin", "subject")
    }
    
    /// Get a production for a non-terminal, selecting randomly based on weights.
    /// Fully declarative: no special-casing of any symbol; the CFG defines behavior.
    #[allow(dead_code)]
    pub fn expand<R: Rng>(&self, rng: &mut R, non_terminal: &str) -> Option<Vec<Sym>> {
        let rule = self.rules.get(non_terminal)?;

        // Select production based on weights
        let mut rand_val = rng.gen::<f64>();
        for prod in &rule.productions {
            rand_val -= prod.weight;
            if rand_val <= 0.0 {
                return Some(prod.symbols.clone());
            }
        }

        // Fallback to first production
        rule.productions.first().map(|p| p.symbols.clone())
    }
    
    /// Enumerate all valid POS sequences of exactly length k with their probabilities.
    /// Sequences are sorted by probability (highest first).
    #[allow(dead_code)]
    pub fn enumerate_sequences_with_probability(
        &self,
        start_symbol: &str,
        k: usize,
    ) -> Vec<SequenceWithProbability> {
        // Memoization: (nonterminal, remaining_length) -> (sequence, refinements, probability)
        let mut memo: HashMap<(String, usize), Vec<(Vec<crate::types::Pos>, Vec<Option<String>>, f64)>> = HashMap::new();

        self.enumerate_sequences_with_probability_internal(start_symbol, k, &mut memo)
    }

    /// Precompute POS sequences (and their probabilities) for lengths 0..=max_k.
    ///
    /// Uses CFG rules for deterministic enumeration. Falls back to type-driven
    /// generation only when no CFG rules are available.
    pub fn precompute_sequences_with_probability(
        &self,
        start_symbol: &str,
        max_k: usize,
    ) -> Vec<Vec<SequenceWithProbability>> {
        // Prefer CFG generation when rules are available (deterministic, complete)
        if !self.rules.is_empty() {
            // Use CFG generation below
        } else if let Some(ref config) = self.language_config {
            // Type-driven generation only when no CFG rules exist
            return self.precompute_sequences_type_driven(config, start_symbol, max_k);
        }

        // CFG generation
        let mut memo: HashMap<(String, usize), Vec<(Vec<crate::types::Pos>, Vec<Option<String>>, f64)>> = HashMap::new();
        let mut by_k: Vec<Vec<SequenceWithProbability>> = vec![Vec::new(); max_k + 1];
        for k in 0..=max_k {
            by_k[k] = self.enumerate_sequences_with_probability_internal(start_symbol, k, &mut memo);
        }
        by_k
    }
    
    /// Type-driven sequence generation via CCG proof search.
    ///
    /// Replaces the old random-sampling approach with exhaustive
    /// combinator-driven derivation. Each POS sequence is a proof
    /// that the terminals compose into the target type.
    fn precompute_sequences_type_driven(
        &self,
        config: &LanguageConfig,
        start_symbol: &str,
        max_k: usize,
    ) -> Vec<Vec<SequenceWithProbability>> {
        // Map start symbol to semantic type
        let target_type = match start_symbol {
            "S" | "SContent" => SemanticType::Truth,
            _ => {
                if let Some(pos_str) = start_symbol.strip_prefix("S_") {
                    match pos_str.to_uppercase().as_str() {
                        "N" => pos_to_semantic_type(&Pos::N),
                        "V" => pos_to_semantic_type(&Pos::V),
                        "ADJ" => pos_to_semantic_type(&Pos::Adj),
                        "ADV" => pos_to_semantic_type(&Pos::Adv),
                        "PREP" => pos_to_semantic_type(&Pos::Prep),
                        "DET" => pos_to_semantic_type(&Pos::Det),
                        _ => SemanticType::Truth,
                    }
                } else {
                    SemanticType::Truth
                }
            }
        };

        // CCG proof search: derive all type-valid POS sequences
        let derivations_by_k = config.derive_all(&target_type, max_k);

        // Convert Derivation → SequenceWithProbability
        let mut by_k: Vec<Vec<SequenceWithProbability>> = vec![Vec::new(); max_k + 1];
        for k in 0..=max_k {
            by_k[k] = derivations_by_k[k]
                .iter()
                .map(|d| {
                    SequenceWithProbability::new(
                        d.sequence.clone(),
                        d.refinements.clone(),
                        d.probability,
                    )
                })
                .collect();
        }

        by_k
    }
    

    fn enumerate_sequences_with_probability_internal(
        &self,
        start_symbol: &str,
        k: usize,
        memo: &mut HashMap<(String, usize), Vec<(Vec<crate::types::Pos>, Vec<Option<String>>, f64)>>,
    ) -> Vec<SequenceWithProbability> {
        fn enumerate_recursive(
            grammar: &Grammar,
            sym: &Sym,
            sym_refinement: Option<&str>,
            remaining: usize,
            memo: &mut HashMap<(String, usize), Vec<(Vec<crate::types::Pos>, Vec<Option<String>>, f64)>>,
        ) -> Vec<(Vec<crate::types::Pos>, Vec<Option<String>>, f64)> {
            match sym {
                Sym::T(pos) => {
                    if remaining == 1 {
                        vec![(vec![*pos], vec![sym_refinement.map(|s| s.to_string())], 1.0)]
                    } else {
                        Vec::new()
                    }
                }
                Sym::Opt(inner) => {
                    let mut results = Vec::new();

                    // Include the optional symbol (probability 0.5)
                    let include_results = enumerate_recursive(grammar, inner, sym_refinement, remaining, memo)
                        .into_iter()
                        .map(|(seq, refs, prob)| (seq, refs, prob * 0.5))
                        .collect::<Vec<_>>();
                    results.extend(include_results);

                    // Exclude the optional symbol (probability 0.5, produces empty)
                    if remaining == 0 {
                        results.push((Vec::new(), Vec::new(), 0.5));
                    }

                    results
                }
                Sym::NT(nt) => {
                    let key = (nt.clone(), remaining);
                    if let Some(cached) = memo.get(&key) {
                        return cached.clone();
                    }

                    let rule = match grammar.rules.get(nt) {
                        Some(r) => r,
                        None => return Vec::new(),
                    };

                    let mut all_results = Vec::new();

                    // Try each production
                    for prod in &rule.productions {
                        let prod_weight = prod.weight;

                        let mut production_results: Vec<(Vec<crate::types::Pos>, Vec<Option<String>>, f64)> =
                            vec![(Vec::new(), Vec::new(), 1.0)];

                        for (sym_idx, symbol) in prod.symbols.iter().enumerate() {
                            let mut new_results = Vec::new();
                            let prod_ref = prod.refinements.get(sym_idx).and_then(|r| r.as_deref());

                            for (partial_seq, partial_refs, partial_prob) in production_results {
                                let used = partial_seq.len();
                                let available = remaining.saturating_sub(used);

                                for symbol_slots in 0..=available {
                                    let symbol_results = enumerate_recursive(
                                        grammar,
                                        symbol,
                                        prod_ref,
                                        symbol_slots,
                                        memo,
                                    );

                                    for (symbol_seq, symbol_refs, symbol_prob) in &symbol_results {
                                        if symbol_seq.len() != symbol_slots {
                                            continue;
                                        }

                                        let mut combined_seq = partial_seq.clone();
                                        combined_seq.extend(symbol_seq.iter().cloned());
                                        let mut combined_refs = partial_refs.clone();
                                        combined_refs.extend(symbol_refs.iter().cloned());

                                        if combined_seq.len() <= remaining {
                                            let combined_prob = partial_prob * symbol_prob;
                                            new_results.push((combined_seq, combined_refs, combined_prob));
                                        }
                                    }
                                }
                            }

                            production_results = new_results;
                        }

                        // Multiply by production weight and filter to exact length
                        for (seq, refs, symbol_prob) in production_results {
                            if seq.len() == remaining {
                                let final_prob = prod_weight * symbol_prob;
                                all_results.push((seq, refs, final_prob));
                            }
                        }
                    }

                    // Deduplicate: sum probabilities for identical (sequence, refinements) pairs
                    let mut prob_map: HashMap<(Vec<crate::types::Pos>, Vec<Option<String>>), f64> = HashMap::new();
                    for (seq, refs, prob) in all_results {
                        *prob_map.entry((seq, refs)).or_insert(0.0) += prob;
                    }

                    let mut final_results: Vec<(Vec<crate::types::Pos>, Vec<Option<String>>, f64)> =
                        prob_map.into_iter().map(|((seq, refs), prob)| (seq, refs, prob)).collect();
                    // Sort for deterministic memoization
                    final_results.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
                    memo.insert(key, final_results.clone());

                    final_results
                }
            }
        }

        let start_sym = Sym::NT(start_symbol.to_string());
        let results = enumerate_recursive(self, &start_sym, None, k, memo);

        // Deduplicate final results and convert to SequenceWithProbability
        let mut prob_map: HashMap<(Vec<crate::types::Pos>, Vec<Option<String>>), f64> = HashMap::new();
        for (seq, refs, prob) in results {
            *prob_map.entry((seq, refs)).or_insert(0.0) += prob;
        }

        let mut sequences: Vec<SequenceWithProbability> = prob_map
            .into_iter()
            .map(|((sequence, refinements), probability)| SequenceWithProbability::new(sequence, refinements, probability))
            .collect();

        // Sort by probability (highest first), breaking ties by sequence for determinism
        sequences.sort_by(|a, b| {
            b.probability.partial_cmp(&a.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.sequence.cmp(&b.sequence))
                .then_with(|| a.refinements.cmp(&b.refinements))
        });

        sequences
    }
    
    /// Check whether any production in this grammar uses a given POS terminal.
    /// Used to derive language features from the grammar instead of language-name string checks.
    pub fn grammar_uses_pos(&self, pos: Pos) -> bool {
        self.rules.values().any(|rule|
            rule.productions.iter().any(|prod|
                prod.symbols.iter().any(|sym| matches!(sym, Sym::T(p) if *p == pos))
            )
        )
    }

    /// Minimum number of terminal symbols the grammar can produce from "S".
    ///
    /// Used to compute a dynamic `k_max` for the generator: grammars with
    /// large fixed overhead (e.g., CS grammar: HEADER + BODY + FOOTER = 13
    /// terminals minimum) need a larger k_max than the default 12.
    ///
    /// Returns `None` if no valid derivation exists within 200 terminals.
    pub fn min_sentence_length(&self) -> Option<usize> {
        let mut memo = HashMap::new();
        for k in 1..=200 {
            let sequences = self.enumerate_sequences_with_probability_internal("S", k, &mut memo);
            if !sequences.is_empty() {
                return Some(k);
            }
        }
        None
    }

    /// Format the grammar rules in a concise text representation
    pub fn format_concise(&self) -> String {
        let mut output = String::new();
        let mut rules: Vec<_> = self.rules.iter().collect();
        rules.sort_by_key(|(name, _)| *name);
        
        if rules.is_empty() {
            return "No grammar rules found.\n".to_string();
        }
        
        for (non_terminal, rule) in rules {
            output.push_str(&format!("{} = ", non_terminal));
            
            let productions_str: Vec<String> = rule.productions.iter().map(|prod| {
                let symbols_str: Vec<String> = prod.symbols.iter().enumerate().map(|(idx, sym)| {
                    let base = match sym {
                        Sym::T(pos) => pos.as_str().to_string(),
                        Sym::NT(nt) => nt.clone(),
                        Sym::Opt(inner) => match &**inner {
                            Sym::T(pos) => format!("{:?}?", pos),
                            Sym::NT(nt) => format!("{}?", nt),
                            Sym::Opt(_) => "Opt?".to_string(),
                        },
                    };
                    // Append refinement if present
                    if let Some(Some(ref tag)) = prod.refinements.get(idx) {
                        format!("{}[{}]", base, tag)
                    } else {
                        base
                    }
                }).collect();
                let prod_str = symbols_str.join(" ");
                if (prod.weight - 1.0).abs() > 0.001 {
                    format!("({:.2}: {})", prod.weight, prod_str)
                } else {
                    prod_str
                }
            }).collect();
            
            output.push_str(&productions_str.join(" | "));
            output.push('\n');
        }
        
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Pos;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn format_pos_sequence(seq: &[Pos]) -> String {
        seq.iter().map(|pos| pos.as_str()).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn test_enumerate_sequences_with_probability() {
        let grammar = Grammar::default().expect("Failed to load body grammar");

        // Precompute once (shares DP memo across k values) and then read by length.
        let precomputed = grammar.precompute_sequences_with_probability("S", 20);
        
        let mut total_prob_all_k: f64 = 0.0;
        let mut k_values: Vec<usize> = Vec::new();
        let mut prob_by_k: Vec<(usize, f64)> = Vec::new();
        
        // Test with different values of k
        for k in 3..=20 {
            println!("\n=== Testing k = {} ===", k);
            let sequences = &precomputed[k];
            
            if sequences.is_empty() {
                println!("No valid sequences found for k = {}", k);
                continue;
            }
            
            println!("Found {} valid sequence(s):", sequences.len());
            println!("{:-<80}", "");
            
            let total_prob: f64 = sequences.iter().map(|s| s.probability).sum();
            println!("Total probability: {:.6} (probability that grammar generates exactly {} terminals)", total_prob, k);
            println!("{:-<80}", "");
            
            total_prob_all_k += total_prob;
            k_values.push(k);
            prob_by_k.push((k, total_prob));
            
            for (i, seq_prob) in sequences.iter().take(20).enumerate() {
                let seq_str = format_pos_sequence(&seq_prob.sequence);
                println!("{}. [{:>6.4}%] {}", 
                    i + 1, 
                    seq_prob.probability * 100.0,
                    seq_str
                );
            }
            if sequences.len() > 20 {
                println!("... and {} more sequences", sequences.len() - 20);
            }
        }
        
        // Print summary across all k values
        println!("\n{:=<80}", "");
        println!("SUMMARY: Probability distribution across all sequence lengths");
        println!("{:=<80}", "");
        for (k, prob) in &prob_by_k {
            println!("k = {:2}: {:.6} ({:.2}%)", k, prob, prob * 100.0);
        }
        println!("{:-<80}", "");
        println!("Sum across all k values: {:.6}", total_prob_all_k);
        println!("Expected: close to 1.0 (all possible sequences from grammar)");
        println!("{:=<80}", "");
    }

    #[test]
    fn test_enumerate_simple_cases() {
        let grammar = Grammar::default().expect("Failed to load body grammar");
        
        // VP should NOT have k=1 sequences once bare `V` is removed from the grammar.
        println!("Testing VP with k=1:");
        let vp_1 = grammar.enumerate_sequences_with_probability("VP", 1);
        println!("  Found {} sequences", vp_1.len());
        assert!(
            vp_1.is_empty(),
            "Expected no VP sequences of length 1 after removing bare V, got: {:?}",
            vp_1
        );

        // VP with k=2 should still work (e.g., Cop Adj, V NP with NP=N, Modal V).
        println!("Testing VP with k=2:");
        let vp_2 = grammar.enumerate_sequences_with_probability("VP", 2);
        println!("  Found {} sequences", vp_2.len());
        assert!(!vp_2.is_empty(), "Expected some VP sequences of length 2");
        
        // Test NP with k=1 (should return N)
        println!("Testing NP with k=1:");
        let np_1 = grammar.enumerate_sequences_with_probability("NP", 1);
        println!("  Found {} sequences", np_1.len());
        for seq in &np_1 {
            println!("    {:?} (prob: {})", seq.sequence, seq.probability);
        }
        
        // Test S_N with k=3: N(1) + VP(1) + Dot(1)
        println!("Testing S_N with k=3:");
        let s_n_3 = grammar.enumerate_sequences_with_probability("S_N", 3);
        println!("  Found {} sequences", s_n_3.len());
        for seq in &s_n_3 {
            println!("    {:?} (prob: {})", seq.sequence, seq.probability);
        }
    }

    #[test]
    fn test_enumerate_start_symbols() {
        let grammar = Grammar::default().expect("Failed to load body grammar");
        
        let start_symbols = vec!["S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"];
        
        for start in start_symbols {
            println!("\n=== Testing start symbol: {} ===", start);
            let sequences = grammar.enumerate_sequences_with_probability(start, 5);
            
            if sequences.is_empty() {
                println!("No valid sequences found for {} with k=5", start);
                continue;
            }
            
            println!("Found {} valid sequence(s):", sequences.len());
            for (i, seq_prob) in sequences.iter().take(10).enumerate() {
                let seq_str = format_pos_sequence(&seq_prob.sequence);
                println!("  {}. [{:>6.4}%] {}", 
                    i + 1, 
                    seq_prob.probability * 100.0,
                    seq_str
                );
            }
            if sequences.len() > 10 {
                println!("  ... and {} more", sequences.len() - 10);
            }
        }
    }

    // ========== Refinement system tests ==========

    #[test]
    fn test_parse_refinement_syntax_det_def() {
        // Verify "Det[def] N" parses Det with refinement "def", N with None
        let (symbols, refinements) = Grammar::parse_cfg_production_string("Det[def] N").unwrap();
        assert_eq!(symbols.len(), 2);
        assert!(matches!(&symbols[0], Sym::T(Pos::Det)));
        assert!(matches!(&symbols[1], Sym::T(Pos::N)));
        assert_eq!(refinements[0], Some("def".to_string()));
        assert_eq!(refinements[1], None);
    }

    #[test]
    fn test_parse_refinement_syntax_multiple() {
        // Verify "Det[indef] Adj Cop[sg]" parses with refinements on Det and Cop only
        let (symbols, refinements) = Grammar::parse_cfg_production_string("Det[indef] Adj Cop[sg]").unwrap();
        assert_eq!(symbols.len(), 3);
        assert!(matches!(&symbols[0], Sym::T(Pos::Det)));
        assert!(matches!(&symbols[1], Sym::T(Pos::Adj)));
        assert!(matches!(&symbols[2], Sym::T(Pos::Cop)));
        assert_eq!(refinements[0], Some("indef".to_string()));
        assert_eq!(refinements[1], None);
        assert_eq!(refinements[2], Some("sg".to_string()));
    }

    #[test]
    fn test_parse_no_refinement_productions() {
        // Verify "N V NP" produces all-None refinements
        let (symbols, refinements) = Grammar::parse_cfg_production_string("N V NP").unwrap();
        assert_eq!(symbols.len(), 3);
        assert!(matches!(&symbols[0], Sym::T(Pos::N)));
        assert!(matches!(&symbols[1], Sym::T(Pos::V)));
        assert!(matches!(&symbols[2], Sym::NT(ref s) if s == "NP"));
        assert!(refinements.iter().all(|r| r.is_none()),
            "All refinements should be None for unrefined production");
    }

    #[test]
    fn test_parse_nonterminal_ignores_refinement() {
        // Non-terminals should strip any accidental refinement syntax
        let (symbols, refinements) = Grammar::parse_cfg_production_string("NP[foo] V").unwrap();
        // NP is a nonterminal -- refinements are forced to None for non-terminals
        assert!(matches!(&symbols[0], Sym::NT(ref s) if s == "NP"));
        assert_eq!(refinements[0], None, "Non-terminals must not carry refinement");
        assert!(matches!(&symbols[1], Sym::T(Pos::V)));
        assert_eq!(refinements[1], None);
    }

    #[test]
    fn test_refinement_propagation_through_enumeration() {
        // Load English body grammar (has Det[def], Det[indef], Cop[sg], Cop[pl] refinements)
        let grammar = Grammar::from_language_dialect("english", "body").expect("Failed to load English body grammar");

        // NP k=2 should produce sequences like [Det, N] with refinements [Some("def"), None], [Some("indef"), None], etc.
        let np_2 = grammar.enumerate_sequences_with_probability("NP", 2);
        assert!(!np_2.is_empty(), "Expected NP sequences of length 2");

        // Check that at least one sequence has a non-None refinement (from Det[def]/Det[indef] productions)
        let has_refinement = np_2.iter().any(|seq| {
            seq.refinements.iter().any(|r| r.is_some())
        });
        assert!(has_refinement,
            "Expected at least one NP(k=2) sequence with a refinement annotation (Det[def] or Det[indef])");

        // Check that Det refinements are preserved correctly
        for seq in &np_2 {
            if seq.sequence.len() == 2 && seq.sequence[0] == Pos::Det {
                // Det at position 0 should carry a refinement from the grammar
                let ref_tag = &seq.refinements[0];
                if ref_tag.is_some() {
                    let tag = ref_tag.as_ref().unwrap();
                    assert!(
                        tag == "def" || tag == "indef" || tag == "quant",
                        "Expected Det refinement to be def, indef, or quant, got: {}",
                        tag
                    );
                }
            }
        }
    }

    #[test]
    fn test_refinement_cop_in_vp() {
        // VP sequences should carry Cop refinements (sg/pl) from the grammar
        let grammar = Grammar::from_language_dialect("english", "body").expect("Failed to load grammar");
        let vp_2 = grammar.enumerate_sequences_with_probability("VP", 2);

        // Look for [Cop, Adj] sequences -- these come from "Cop[sg] Adj" and "Cop[pl] Adj"
        let cop_adj_seqs: Vec<&SequenceWithProbability> = vp_2.iter()
            .filter(|s| s.sequence == vec![Pos::Cop, Pos::Adj])
            .collect();

        assert!(!cop_adj_seqs.is_empty(), "Expected [Cop, Adj] sequences in VP(k=2)");

        // Collect all Cop refinements
        let cop_refs: Vec<Option<String>> = cop_adj_seqs.iter()
            .map(|s| s.refinements[0].clone())
            .collect();

        assert!(cop_refs.contains(&Some("sg".to_string())),
            "Expected Cop[sg] refinement in VP Cop Adj sequences, got: {:?}", cop_refs);
        assert!(cop_refs.contains(&Some("pl".to_string())),
            "Expected Cop[pl] refinement in VP Cop Adj sequences, got: {:?}", cop_refs);
    }

    #[test]
    fn test_grammar_uses_pos_true() {
        // English body grammar uses Dot, Det, Cop, N, V, Adj, Adv, Prep, Modal
        let grammar = Grammar::from_language_dialect("english", "body").expect("Failed to load grammar");
        assert!(grammar.grammar_uses_pos(Pos::Dot), "English body grammar should use Dot");
        assert!(grammar.grammar_uses_pos(Pos::Det), "English body grammar should use Det");
        assert!(grammar.grammar_uses_pos(Pos::Cop), "English body grammar should use Cop");
        assert!(grammar.grammar_uses_pos(Pos::N), "English body grammar should use N");
        assert!(grammar.grammar_uses_pos(Pos::V), "English body grammar should use V");
        assert!(grammar.grammar_uses_pos(Pos::Adj), "English body grammar should use Adj");
        assert!(grammar.grammar_uses_pos(Pos::Adv), "English body grammar should use Adv");
        assert!(grammar.grammar_uses_pos(Pos::Modal), "English body grammar should use Modal");
        assert!(grammar.grammar_uses_pos(Pos::Prep), "English body grammar should use Prep");
    }

    #[test]
    fn test_grammar_uses_pos_false() {
        // English body grammar does NOT use Conj (no Conj in body.cfg productions)
        let grammar = Grammar::from_language_dialect("english", "body").expect("Failed to load grammar");
        assert!(!grammar.grammar_uses_pos(Pos::Conj), "English body grammar should not use Conj");
        // English body grammar does NOT use Prefix (only subject dialect uses Prefix)
        assert!(!grammar.grammar_uses_pos(Pos::Prefix), "English body grammar should not use Prefix");
    }

    #[test]
    fn test_grammar_uses_pos_latin_no_det() {
        // Latin body grammar should NOT use Det (Latin has no articles)
        let grammar = Grammar::from_language_dialect("latin", "body").expect("Failed to load Latin grammar");
        assert!(!grammar.grammar_uses_pos(Pos::Det), "Latin body grammar should not use Det");
        // But it should use N, V, etc.
        assert!(grammar.grammar_uses_pos(Pos::N), "Latin body grammar should use N");
        assert!(grammar.grammar_uses_pos(Pos::V), "Latin body grammar should use V");
        assert!(grammar.grammar_uses_pos(Pos::Dot), "Latin body grammar should use Dot");
    }

    #[test]
    fn test_grammar_uses_pos_subject_no_prefix() {
        // Base subject dialect should NOT use Prefix (prefix is dialect-driven, not random)
        let grammar = Grammar::from_language_dialect("english", "subject").expect("Failed to load grammar");
        assert!(!grammar.grammar_uses_pos(Pos::Prefix), "Base subject grammar should not use Prefix");
    }

    #[test]
    fn test_grammar_uses_pos_subject_re_has_prefix() {
        // subject_re dialect should use Prefix[re]
        let grammar = Grammar::from_language_dialect("english", "subject_re").expect("Failed to load subject_re grammar");
        assert!(grammar.grammar_uses_pos(Pos::Prefix), "subject_re grammar should use Prefix");
    }

    #[test]
    fn test_grammar_uses_pos_subject_fwd_has_prefix() {
        // subject_fwd dialect should use Prefix[fwd]
        let grammar = Grammar::from_language_dialect("english", "subject_fwd").expect("Failed to load subject_fwd grammar");
        assert!(grammar.grammar_uses_pos(Pos::Prefix), "subject_fwd grammar should use Prefix");
    }

    #[test]
    fn test_english_prose_dialect_loads() {
        let grammar = Grammar::from_language_dialect("english", "prose").expect("Failed to load English prose grammar");
        // Prose dialect should use Conj (conjoined clauses), Pron, and standard POS tags
        assert!(grammar.grammar_uses_pos(Pos::Dot), "Prose grammar should use Dot");
        assert!(grammar.grammar_uses_pos(Pos::V), "Prose grammar should use V");
        assert!(grammar.grammar_uses_pos(Pos::Det), "Prose grammar should use Det");
        assert!(grammar.grammar_uses_pos(Pos::Conj), "Prose grammar should use Conj");
        assert!(grammar.grammar_uses_pos(Pos::Pron), "Prose grammar should use Pron");
        // Prose should produce sequences
        let seqs = grammar.precompute_sequences_with_probability("S", 10);
        let total: usize = seqs.iter().map(|v| v.len()).sum();
        assert!(total > 0, "Prose grammar should produce at least some sequences");
    }

    #[test]
    fn test_latin_spells_dialect_loads() {
        let grammar = Grammar::from_language_dialect("latin", "spells").expect("Failed to load Latin spells grammar");
        // Spells use V, N, Adj, Dot — short formulaic patterns
        assert!(grammar.grammar_uses_pos(Pos::Dot), "Spells grammar should use Dot");
        assert!(grammar.grammar_uses_pos(Pos::V), "Spells grammar should use V");
        assert!(grammar.grammar_uses_pos(Pos::N), "Spells grammar should use N");
        assert!(grammar.grammar_uses_pos(Pos::Adj), "Spells grammar should use Adj");
        // Spells should NOT use Det (Latin has no articles)
        assert!(!grammar.grammar_uses_pos(Pos::Det), "Spells grammar should not use Det");
        // Spells produce short sequences — check k=2 (V Dot), k=3 (V N Dot)
        let seqs_k2 = grammar.enumerate_sequences_with_probability("S", 2);
        assert!(!seqs_k2.is_empty(), "Spells should produce k=2 sequences (V Dot, N Dot)");
        let seqs_k3 = grammar.enumerate_sequences_with_probability("S", 3);
        assert!(!seqs_k3.is_empty(), "Spells should produce k=3 sequences (V N Dot, Adj N Dot, etc.)");
    }

    // ========== Meta-grammar tests ==========

    #[test]
    fn test_meta_grammar_loads() {
        let grammar = Grammar::from_language_dialect("meta", "body")
            .expect("Failed to load meta body grammar");
        assert_eq!(grammar.max_k(), Some(12), "Meta grammar should declare max_k=12");
        assert!(grammar.grammar_uses_pos(Pos::Dot), "Meta grammar should use Dot");
        assert!(grammar.grammar_uses_pos(Pos::N), "Meta grammar should use N");
        assert!(grammar.grammar_uses_pos(Pos::V), "Meta grammar should use V");
        assert!(grammar.grammar_uses_pos(Pos::Adj), "Meta grammar should use Adj");
        assert!(grammar.grammar_uses_pos(Pos::Det), "Meta grammar should use Det");
        assert!(grammar.grammar_uses_pos(Pos::Prep), "Meta grammar should use Prep");
        assert!(grammar.grammar_uses_pos(Pos::Cop), "Meta grammar should use Cop");
        assert!(grammar.grammar_uses_pos(Pos::Conj), "Meta grammar should use Conj");
        assert!(grammar.grammar_uses_pos(Pos::Pron), "Meta grammar should use Pron");
        assert!(grammar.grammar_uses_pos(Pos::Modal), "Meta grammar should use Modal");
    }

    #[test]
    fn test_meta_grammar_enumerates_sequences() {
        let grammar = Grammar::from_language_dialect("meta", "body")
            .expect("Failed to load meta body grammar");

        // Use grammar's own max_k for enumeration
        let max_k = grammar.max_k().unwrap_or(12);
        let precomputed = grammar.precompute_sequences_with_probability("S", max_k);

        let mut total_sequences: usize = 0;
        let mut total_prob: f64 = 0.0;

        for k in 3..=max_k {
            let seqs = &precomputed[k];
            total_sequences += seqs.len();
            total_prob += seqs.iter().map(|s| s.probability).sum::<f64>();
            if !seqs.is_empty() {
                println!("Meta k={}: {} sequences (prob={:.6})", k, seqs.len(),
                    seqs.iter().map(|s| s.probability).sum::<f64>());
            }
        }

        println!("Meta grammar total: {} sequences, prob sum={:.6}", total_sequences, total_prob);
        assert!(total_sequences > 0, "Meta grammar should produce at least some sequences");
        // With max_k=12, probability sum should be substantial (close to 1.0)
        assert!(total_prob > 0.5, "Meta grammar should cover >50% probability mass within k=12");
    }

    #[test]
    fn test_meta_grammar_minimum_sentence() {
        // Meta grammar minimum sentence: NP VP Dot where NP=N and VP=... something short
        let grammar = Grammar::from_language_dialect("meta", "body")
            .expect("Failed to load meta body grammar");

        // k=4 should produce valid sentences (e.g., N V N Dot, N Cop Adj Dot)
        let seqs_k4 = grammar.enumerate_sequences_with_probability("S", 4);
        assert!(!seqs_k4.is_empty(),
            "Meta grammar should produce sentences at k=4");

        for seq in seqs_k4.iter().take(5) {
            let seq_str = format_pos_sequence(&seq.sequence);
            println!("Meta k=4: {} (prob={:.4})", seq_str, seq.probability);
            // Every sequence should end with Dot
            assert_eq!(*seq.sequence.last().unwrap(), Pos::Dot,
                "Meta sentence should end with Dot: {}", seq_str);
        }
    }

    #[test]
    fn test_cs_grammar_payload_format() {
        // CS grammar should have payload_separator="" and dot_is_punctuation=false
        let grammar = Grammar::from_language_dialect("cs", "body").expect("Failed to load CS grammar");
        assert_eq!(grammar.payload_separator(), "", "CS grammar should concatenate payload words");
        assert!(!grammar.dot_is_punctuation(), "CS grammar Dot should produce cover words, not periods");
        assert_eq!(grammar.payload_line_width(), Some(76), "CS grammar should wrap payload at 76 chars");
        // CS should use N (payload characters) and structural POS tags
        assert!(grammar.grammar_uses_pos(Pos::N), "CS grammar should use N");
        assert!(grammar.grammar_uses_pos(Pos::Dot), "CS grammar should use Dot");
        assert!(grammar.grammar_uses_pos(Pos::Aux), "CS grammar should use Aux");
    }

    #[test]
    fn test_payload_separator_dialect_override() {
        // CS grammar-level default is "" (concatenated).
        // A dialect with payload_separator: " " should override to space-separated.
        let base = Grammar::from_language_dialect("cs", "body")
            .expect("Failed to load CS body grammar");
        assert_eq!(base.payload_separator(), "",
            "CS body should use grammar-level default (concatenated)");

        // seal_nostr inherits "" from CS grammar (no override)
        let seal = Grammar::from_language_dialect("cs", "seal_nostr")
            .expect("Failed to load CS seal_nostr grammar");
        assert_eq!(seal.payload_separator(), "",
            "seal_nostr should inherit concatenated separator from CS grammar");
    }

    #[test]
    fn test_english_grammar_default_payload_format() {
        // English grammar should have default payload format (space separator, dot as period)
        let grammar = Grammar::from_language_dialect("english", "body").expect("Failed to load grammar");
        assert_eq!(grammar.payload_separator(), " ", "English grammar should space-separate payload words");
        assert!(grammar.dot_is_punctuation(), "English grammar Dot should append periods");
        assert_eq!(grammar.payload_line_width(), None, "English grammar should not wrap payload lines");
    }

    #[test]
    fn test_sequence_cache_roundtrip_tempdir() {
        // Write a temporary grammar.yaml file, build the cache, then ensure a subsequent call reads it.
        let grammar_str = include_str!("../languages/english/grammar.yaml");
        let tmp = std::env::temp_dir();
        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let grammar_path = tmp.join(format!("glossia_grammar_{uniq}.yaml"));
        std::fs::write(&grammar_path, grammar_str).unwrap();

        let by_k_1 = Grammar::precompute_sequences_with_probability_cached_from_file(&grammar_path, "S", 20)
            .expect("cache compute 1");
        let by_k_2 = Grammar::precompute_sequences_with_probability_cached_from_file(&grammar_path, "S", 20)
            .expect("cache compute 2");

        // Basic sanity: same sizes and non-empty for a known length.
        assert_eq!(by_k_1.len(), by_k_2.len());
        // Find any non-empty k to verify roundtrip
        let has_sequences = (3..=20).any(|k| !by_k_1[k].is_empty());
        assert!(has_sequences, "Should have at least some non-empty sequence lengths");
        // Verify consistency between two loads
        for k in 3..=20 {
            assert_eq!(by_k_1[k].len(), by_k_2[k].len(), "Mismatch at k={}", k);
        }
    }

    // ── DialectConfig tests ────────────────────────────────────────

    #[test]
    fn test_dialect_config_latin_body_defaults() {
        let config = DialectConfig::from_language_dialect("latin", "body")
            .expect("Failed to load Latin body dialect config");
        assert_eq!(config.language(), "latin");
        assert_eq!(config.dialect(), "body");
        assert_eq!(config.payload_wordlist(), "default");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_latin_spells_hp_wordlist() {
        let config = DialectConfig::from_language_dialect("latin", "spells")
            .expect("Failed to load Latin spells dialect config");
        assert_eq!(config.language(), "latin");
        assert_eq!(config.dialect(), "spells");
        assert_eq!(config.payload_wordlist(), "hp",
            "Latin spells dialect should use payload_hp.yaml");
        assert_eq!(config.cover_wordlist(), "default",
            "Latin spells dialect should use default cover.yaml");
    }

    #[test]
    fn test_dialect_config_english_subject_defaults() {
        let config = DialectConfig::from_language_dialect("english", "subject")
            .expect("Failed to load English subject dialect config");
        assert_eq!(config.payload_wordlist(), "default");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_english_prose_defaults() {
        let config = DialectConfig::from_language_dialect("english", "prose")
            .expect("Failed to load English prose dialect config");
        assert_eq!(config.payload_wordlist(), "default");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_cs_nip04_base64() {
        let config = DialectConfig::from_language_dialect("cs", "nip04")
            .expect("Failed to load CS nip04 dialect config");
        assert_eq!(config.payload_wordlist(), "base64",
            "CS nip04 should use payload_base64.yaml (base64 is the NIP-04 wire format)");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_with_payload_override() {
        let config = DialectConfig::from_language_dialect("latin", "body")
            .expect("Failed to load Latin body dialect config");
        assert_eq!(config.payload_wordlist(), "default");

        // Override with a named wordlist
        let overridden = config.with_payload_wordlist("hp");
        assert_eq!(overridden.payload_wordlist(), "hp",
            "with_payload_wordlist should update payload_wl");
        // Cover stays default since there's no cover_hp.yaml for latin
        assert_eq!(overridden.cover_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_with_cover_override() {
        let config = DialectConfig::from_language_dialect("latin", "body")
            .expect("Failed to load Latin body dialect config");
        let overridden = config.with_cover_wordlist("custom");
        assert_eq!(overridden.cover_wordlist(), "custom");
        // Payload stays unchanged
        assert_eq!(overridden.payload_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_wordlist_filenames() {
        let config = DialectConfig::from_language_dialect("latin", "spells")
            .expect("Failed to load Latin spells dialect config");
        let (payload_file, cover_file) = config.wordlist_filenames();
        assert_eq!(payload_file, "payload_hp.yaml",
            "Spells dialect should resolve to payload_hp.yaml");
        assert_eq!(cover_file, "cover.yaml",
            "Spells dialect cover should resolve to cover.yaml (fallback)");
    }

    #[test]
    fn test_dialect_config_grammar_is_valid() {
        // The grammar inside DialectConfig should be a valid Grammar
        let config = DialectConfig::from_language_dialect("latin", "spells")
            .expect("Failed to load Latin spells dialect config");
        // Spells grammar should produce short sequences
        let seqs = config.grammar.enumerate_sequences_with_probability("S", 2);
        assert!(!seqs.is_empty(), "Spells grammar should produce k=2 sequences");
    }

    #[test]
    fn test_dialect_config_available_dialects_latin() {
        let dialects = DialectConfig::available_dialects("latin");
        assert!(dialects.contains(&"body".to_string()), "Should include body");
        assert!(dialects.contains(&"subject".to_string()), "Should include subject");
        assert!(dialects.contains(&"spells".to_string()), "Should include spells");
        assert!(dialects.contains(&"payload_only".to_string()), "Should include payload_only");
    }

    #[test]
    fn test_dialect_config_available_dialects_english() {
        let dialects = DialectConfig::available_dialects("english");
        assert!(dialects.contains(&"body".to_string()), "Should include body");
        assert!(dialects.contains(&"subject".to_string()), "Should include subject");
        assert!(dialects.contains(&"prose".to_string()), "Should include prose");
        assert!(dialects.contains(&"payload_only".to_string()), "Should include payload_only");
    }

    #[test]
    fn test_dialect_config_unknown_dialect_defaults() {
        // An unknown dialect should still parse (defaulting wordlists to "default")
        // Grammar::from_language_dialect for unknown dialect uses base rules
        let config = DialectConfig::from_language_dialect("latin", "nonexistent")
            .expect("Unknown dialect should still load (uses base grammar)");
        assert_eq!(config.payload_wordlist(), "default");
        assert_eq!(config.cover_wordlist(), "default");
    }

    // ── Cryptographic signature block tests ──────────────────────────

    #[test]
    fn test_cs_sig_grammar_loads() {
        // Plain signature: ----- BEGIN SIGNATURE -----\n<payload>\n----- END SIGNATURE -----
        let grammar = Grammar::from_language_dialect("cs", "sig")
            .expect("Failed to load CS sig grammar");
        assert!(grammar.grammar_uses_pos(Pos::Dot), "Sig grammar should use Dot (-----)");
        assert!(grammar.grammar_uses_pos(Pos::Aux), "Sig grammar should use Aux (BEGIN/END)");
        assert!(grammar.grammar_uses_pos(Pos::Modal), "Sig grammar should use Modal (SIGNATURE)");
        assert!(grammar.grammar_uses_pos(Pos::N), "Sig grammar should use N (payload)");
        // Plain sig should NOT use Prefix (no protocol tag) or Cop (no ENCRYPTED)
        assert!(!grammar.grammar_uses_pos(Pos::Prefix), "Plain sig should not use Prefix");
        assert!(!grammar.grammar_uses_pos(Pos::Cop), "Sig grammar should not use Cop (ENCRYPTED)");
    }

    #[test]
    fn test_cs_sig_pgp_grammar_loads() {
        // PGP signature: ----- BEGIN PGP SIGNATURE -----\n<payload>\n----- END PGP SIGNATURE -----
        let grammar = Grammar::from_language_dialect("cs", "sig_pgp")
            .expect("Failed to load CS sig_pgp grammar");
        assert!(grammar.grammar_uses_pos(Pos::Dot), "Sig PGP should use Dot");
        assert!(grammar.grammar_uses_pos(Pos::Aux), "Sig PGP should use Aux");
        assert!(grammar.grammar_uses_pos(Pos::Modal), "Sig PGP should use Modal (SIGNATURE)");
        assert!(grammar.grammar_uses_pos(Pos::Prefix), "Sig PGP should use Prefix (PGP)");
        assert!(grammar.grammar_uses_pos(Pos::N), "Sig PGP should use N (payload)");
        // PGP sig should NOT use Cop (no ENCRYPTED) or To (no MESSAGE)
        assert!(!grammar.grammar_uses_pos(Pos::Cop), "Sig PGP should not use Cop");
        assert!(!grammar.grammar_uses_pos(Pos::To), "Sig PGP should not use To");
    }

    #[test]
    fn test_cs_sig_produces_sequences() {
        let grammar = Grammar::from_language_dialect("cs", "sig")
            .expect("Failed to load CS sig grammar");
        // Plain sig: HEADER(5) + BODY(1+) + FOOTER(5) = minimum 11 tokens
        // HEADER = Dot Aux[begin] Modal[sig] Dot Conj (5)
        // FOOTER = Conj Dot Aux[end] Modal[sig] Dot (5)
        // BODY = N (1)
        let seqs_11 = grammar.enumerate_sequences_with_probability("S", 11);
        assert!(!seqs_11.is_empty(),
            "Plain sig should produce k=11 sequences (5 header + 1 body + 5 footer)");
    }

    #[test]
    fn test_cs_sig_pgp_produces_sequences() {
        let grammar = Grammar::from_language_dialect("cs", "sig_pgp")
            .expect("Failed to load CS sig_pgp grammar");
        // PGP sig: HEADER(6) + BODY(1+) + FOOTER(6) = minimum 13 tokens
        // HEADER = Dot Aux[begin] Prefix[pgp] Modal[sig] Dot Conj (6)
        // FOOTER = Conj Dot Aux[end] Prefix[pgp] Modal[sig] Dot (6)
        // BODY = N (1)
        let seqs_13 = grammar.enumerate_sequences_with_probability("S", 13);
        assert!(!seqs_13.is_empty(),
            "PGP sig should produce k=13 sequences (6 header + 1 body + 6 footer)");
    }

    #[test]
    fn test_cs_sig_shorter_than_message() {
        // Signature headers are 1 token shorter than message headers
        // because SIGNATURE (1 word) replaces ENCRYPTED MESSAGE (2 words).
        let sig_grammar = Grammar::from_language_dialect("cs", "sig_pgp")
            .expect("Failed to load sig_pgp grammar");
        let msg_grammar = Grammar::from_language_dialect("cs", "pgp")
            .expect("Failed to load pgp grammar");
        let sig_min = sig_grammar.min_sentence_length()
            .expect("sig_pgp should have a minimum length");
        let msg_min = msg_grammar.min_sentence_length()
            .expect("pgp should have a minimum length");
        assert_eq!(msg_min - sig_min, 2,
            "PGP sig should be 2 tokens shorter than PGP message (1 less in header + 1 less in footer)");
    }

    #[test]
    fn test_dialect_config_cs_sig_base58() {
        let config = DialectConfig::from_language_dialect("cs", "sig")
            .expect("Failed to load CS sig dialect config");
        assert_eq!(config.payload_wordlist(), "base58",
            "CS sig should use payload_base58.yaml");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_cs_sig_pgp_base58() {
        let config = DialectConfig::from_language_dialect("cs", "sig_pgp")
            .expect("Failed to load CS sig_pgp dialect config");
        assert_eq!(config.payload_wordlist(), "base58",
            "CS sig_pgp should use payload_base58.yaml");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_cs_sig_nostr_grammar_loads() {
        // Nostr signature: ----- BEGIN NOSTR SIGNATURE -----\n<payload>\n----- END NOSTR SIGNATURE -----
        let grammar = Grammar::from_language_dialect("cs", "sig_nostr")
            .expect("Failed to load CS sig_nostr grammar");
        assert!(grammar.grammar_uses_pos(Pos::Prefix), "Sig Nostr should use Prefix (NOSTR)");
        assert!(grammar.grammar_uses_pos(Pos::Modal), "Sig Nostr should use Modal (SIGNATURE)");
        assert!(!grammar.grammar_uses_pos(Pos::Cop), "Sig Nostr should not use Cop (ENCRYPTED)");
        assert!(!grammar.grammar_uses_pos(Pos::To), "Sig Nostr should not use To (MESSAGE)");
    }

    #[test]
    fn test_cs_sig_nostr_produces_sequences() {
        let grammar = Grammar::from_language_dialect("cs", "sig_nostr")
            .expect("Failed to load CS sig_nostr grammar");
        // Same structure as sig_pgp: HEADER(6) + BODY(1+) + FOOTER(6) = minimum 13
        let seqs_13 = grammar.enumerate_sequences_with_probability("S", 13);
        assert!(!seqs_13.is_empty(),
            "Nostr sig should produce k=13 sequences (6 header + 1 body + 6 footer)");
    }

    #[test]
    fn test_cs_sig_nostr_same_framing_as_sig_pgp() {
        // Nostr and PGP sigs have identical framing overhead (both use Prefix + Modal[sig])
        let nostr = Grammar::from_language_dialect("cs", "sig_nostr")
            .expect("Failed to load sig_nostr");
        let pgp = Grammar::from_language_dialect("cs", "sig_pgp")
            .expect("Failed to load sig_pgp");
        assert_eq!(
            nostr.min_sentence_length(),
            pgp.min_sentence_length(),
            "Nostr and PGP sig should have same framing overhead"
        );
    }

    #[test]
    fn test_dialect_config_cs_sig_nostr_base16() {
        let config = DialectConfig::from_language_dialect("cs", "sig_nostr")
            .expect("Failed to load CS sig_nostr dialect config");
        assert_eq!(config.payload_wordlist(), "base16",
            "CS sig_nostr should use payload_base16.yaml (hex for Schnorr sigs)");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_cs_sig_latin_grammar_loads() {
        let grammar = Grammar::from_language_dialect("cs", "sig_latin")
            .expect("Failed to load CS sig_latin grammar");
        assert!(grammar.grammar_uses_pos(Pos::Prefix), "Sig Latin should use Prefix (NOSTR)");
        assert!(grammar.grammar_uses_pos(Pos::Modal), "Sig Latin should use Modal (SIGNATURE)");
        assert!(!grammar.grammar_uses_pos(Pos::Cop), "Sig Latin should not use Cop (ENCRYPTED)");
        assert!(!grammar.grammar_uses_pos(Pos::To), "Sig Latin should not use To (MESSAGE)");
    }

    #[test]
    fn test_cs_sig_latin_produces_sequences() {
        let grammar = Grammar::from_language_dialect("cs", "sig_latin")
            .expect("Failed to load CS sig_latin grammar");
        // Same structure as sig_nostr: HEADER(6) + BODY(1+) + FOOTER(6) = minimum 13
        let seqs_13 = grammar.enumerate_sequences_with_probability("S", 13);
        assert!(!seqs_13.is_empty(),
            "Sig Latin should produce k=13 sequences (6 header + 1 body + 6 footer)");
    }

    #[test]
    fn test_cs_sig_latin_same_framing_as_sig_nostr() {
        let latin = Grammar::from_language_dialect("cs", "sig_latin")
            .expect("Failed to load sig_latin");
        let nostr = Grammar::from_language_dialect("cs", "sig_nostr")
            .expect("Failed to load sig_nostr");
        assert_eq!(
            latin.min_sentence_length(),
            nostr.min_sentence_length(),
            "sig_latin and sig_nostr should have same framing overhead"
        );
    }

    #[test]
    fn test_dialect_config_cs_sig_latin() {
        let config = DialectConfig::from_language_dialect("cs", "sig_latin")
            .expect("Failed to load CS sig_latin dialect config");
        assert_eq!(config.payload_wordlist(), "default",
            "CS sig_latin should use default payload wordlist (Latin words)");
        assert_eq!(config.payload_language(), "latin",
            "CS sig_latin should resolve payload from Latin language");
    }

    #[test]
    fn test_cs_seal_nostr_grammar_loads() {
        let grammar = Grammar::from_language_dialect("cs", "seal_nostr")
            .expect("Failed to load CS seal_nostr grammar");
        assert!(grammar.grammar_uses_pos(Pos::Prefix), "Seal Nostr should use Prefix (NOSTR)");
        assert!(grammar.grammar_uses_pos(Pos::Modal), "Seal Nostr should use Modal (SEAL)");
        assert!(!grammar.grammar_uses_pos(Pos::Cop), "Seal Nostr should not use Cop (ENCRYPTED)");
        assert!(!grammar.grammar_uses_pos(Pos::To), "Seal Nostr should not use To (MESSAGE)");
    }

    #[test]
    fn test_cs_seal_nostr_produces_sequences() {
        let grammar = Grammar::from_language_dialect("cs", "seal_nostr")
            .expect("Failed to load CS seal_nostr grammar");
        // HEADER(6) + SEAL_PREFIX(1) + BODY(1+) + FOOTER(6) = minimum 14
        let seqs_14 = grammar.enumerate_sequences_with_probability("S", 14);
        assert!(!seqs_14.is_empty(),
            "Nostr seal should produce k=14 sequences (6 header + 1 prefix + 1 body + 6 footer)");
    }

    #[test]
    fn test_dialect_config_cs_seal_nostr_bech32() {
        let config = DialectConfig::from_language_dialect("cs", "seal_nostr")
            .expect("Failed to load CS seal_nostr dialect config");
        assert_eq!(config.payload_wordlist(), "bech32",
            "CS seal_nostr should use payload_bech32.yaml");
        assert_eq!(config.cover_wordlist(), "default");
    }

    #[test]
    fn test_dialect_config_available_dialects_cs_includes_sig() {
        let dialects = DialectConfig::available_dialects("cs");
        assert!(dialects.contains(&"sig".to_string()),
            "CS should include sig dialect");
        assert!(dialects.contains(&"sig_pgp".to_string()),
            "CS should include sig_pgp dialect");
        assert!(dialects.contains(&"sig_nostr".to_string()),
            "CS should include sig_nostr dialect");
        assert!(dialects.contains(&"seal_nostr".to_string()),
            "CS should include seal_nostr dialect");
    }

    #[test]
    fn test_music_pentatonic_scale_definition() {
        let config = DialectConfig::from_language_dialect("music", "pentatonic")
            .expect("Failed to load music pentatonic dialect config");

        // Scale should be parsed from grammar.yaml
        let scale = config.scale().expect("Pentatonic dialect should have a scale definition");
        assert_eq!(scale.intervals, vec![2, 2, 3, 2, 3], "Major pentatonic intervals");
        assert_eq!(scale.root, "C");

        // Payload wordlist should be "pentatonic" (dialect name, not "default")
        assert_eq!(config.payload_wordlist(), "pentatonic");

        // Should be able to load the derived payload words
        let words = crate::generator::load_payload_words_for_wordlist("music", "pentatonic")
            .expect("Should load scale-derived payload");

        // C major pentatonic has 5 pitch classes (C,D,E,G,A) across 9 octaves (0-8)
        // = 45 notes. But chromatic payload starts at octave -1 (C-1..B-1) + octaves 0-9.
        // Pitch class C,D,E,G,A appear in octaves -1,0,1,2,3,4,5,6,7,8,9
        // C-1..G9 covers 11 octaves: octaves -1 through 9.
        // But octave 9 is partial (C9..G9 = 8 notes). C,D,E,G = 4 pentatonic notes in oct 9.
        // A is pitch class 9 = A9 doesn't exist (MIDI 127 = G9).
        // Full octaves -1..8: 10 octaves × 5 = 50, + partial oct 9: C,D,E,G = 4 → 54 total
        assert!(words.len() > 40, "Should have at least 40 pentatonic notes, got {}", words.len());
        assert!(words.len() < 60, "Should have fewer than 60 notes, got {}", words.len());

        // Verify only pentatonic pitch classes are present
        for word in &words {
            let pc_name = crate::scale::pitch_class_of_note(word)
                .expect(&format!("Should extract pitch class from '{}'", word));
            let pc = crate::scale::pitch_class_from_name(pc_name)
                .expect(&format!("Should parse pitch class '{}'", pc_name));
            assert!(
                [0, 2, 4, 7, 9].contains(&pc),
                "Note '{}' has pitch class {} ({}) which is not in C major pentatonic",
                word, pc, pc_name
            );
        }
    }

    #[test]
    fn test_music_blues_scale_definition() {
        let config = DialectConfig::from_language_dialect("music", "blues")
            .expect("Failed to load music blues dialect config");

        let scale = config.scale().expect("Blues dialect should have a scale definition");
        assert_eq!(scale.intervals, vec![3, 2, 1, 1, 3, 2], "Blues scale intervals");
        assert_eq!(scale.root, "A");

        let words = crate::generator::load_payload_words_for_wordlist("music", "blues")
            .expect("Should load blues scale payload");

        // Blues scale from A: {A(9), C(0), D(2), Eb(3), E(4), G(7)} = 6 pitch classes
        // More notes than pentatonic (6 vs 5 per octave)
        assert!(words.len() > 50, "Blues should have more notes than pentatonic, got {}", words.len());

        // Verify only blues pitch classes
        let valid_pcs = [0, 2, 3, 4, 7, 9]; // A,C,D,Eb,E,G
        for word in &words {
            let pc = crate::scale::pitch_class_of_note(word)
                .and_then(crate::scale::pitch_class_from_name)
                .expect(&format!("Should get pitch class for '{}'", word));
            assert!(
                valid_pcs.contains(&pc),
                "Note '{}' (pc={}) not in A blues scale",
                word, pc
            );
        }
    }

    #[test]
    fn test_music_scored_dialect_no_scale() {
        let config = DialectConfig::from_language_dialect("music", "scored")
            .expect("Failed to load music scored dialect config");

        // Scored dialect has no scale — uses full chromatic payload
        assert!(config.scale().is_none(), "Scored dialect should not have a scale");
        assert_eq!(config.payload_wordlist(), "default");
    }

    #[test]
    fn test_music_pentatonic_scored_inherits_scale() {
        let config = DialectConfig::from_language_dialect("music", "pentatonic-scored")
            .expect("Failed to load music pentatonic-scored dialect config");

        // Should have scale (defined directly on pentatonic-scored, not inherited from scored)
        let scale = config.scale().expect("pentatonic-scored should have a scale");
        assert_eq!(scale.intervals, vec![2, 2, 3, 2, 3]);

        // Payload should be derived, not "default"
        assert_eq!(config.payload_wordlist(), "pentatonic-scored");
    }

    #[test]
    fn test_music_pentatonic_n_slots_carry_refinement() {
        // Verify that N slots in pentatonic sequences carry the auto-derived refinement tag.
        let config = DialectConfig::from_language_dialect("music", "pentatonic")
            .expect("Failed to load music pentatonic dialect config");

        let sequences = config.grammar.precompute_sequences_with_probability("S", 4);
        let mut found_n_with_refinement = false;

        for k_seqs in &sequences {
            for seq in k_seqs {
                for (i, &pos) in seq.sequence.iter().enumerate() {
                    if pos == Pos::N {
                        let ref_tag = seq.refinements.get(i).and_then(|r| r.as_deref());
                        assert_eq!(
                            ref_tag,
                            Some("pentatonic/C"),
                            "N slot at position {} should have refinement 'pentatonic/C', got {:?}",
                            i, ref_tag
                        );
                        found_n_with_refinement = true;
                    }
                }
            }
        }

        assert!(found_n_with_refinement, "Should have found at least one N slot in pentatonic sequences");
    }

    #[test]
    fn test_music_blues_n_slots_carry_refinement() {
        // Blues uses explicit type_refinements: N: "blues/A"
        let config = DialectConfig::from_language_dialect("music", "blues")
            .expect("Failed to load music blues dialect config");

        let sequences = config.grammar.precompute_sequences_with_probability("S", 4);
        let mut found_n = false;

        for k_seqs in &sequences {
            for seq in k_seqs {
                for (i, &pos) in seq.sequence.iter().enumerate() {
                    if pos == Pos::N {
                        let ref_tag = seq.refinements.get(i).and_then(|r| r.as_deref());
                        assert_eq!(
                            ref_tag,
                            Some("blues/A"),
                            "N slot at position {} should have refinement 'blues/A', got {:?}",
                            i, ref_tag
                        );
                        found_n = true;
                    }
                }
            }
        }

        assert!(found_n, "Should have found at least one N slot in blues sequences");
    }

    #[test]
    fn test_music_raw_chromatic_no_n_refinement() {
        // Raw chromatic dialect has no scale, so N slots should have no refinement.
        let config = DialectConfig::from_language_dialect("music", "raw")
            .expect("Failed to load music raw dialect config");

        let sequences = config.grammar.precompute_sequences_with_probability("S", 3);

        for k_seqs in &sequences {
            for seq in k_seqs {
                for (i, &pos) in seq.sequence.iter().enumerate() {
                    if pos == Pos::N {
                        let ref_tag = seq.refinements.get(i).and_then(|r| r.as_deref());
                        assert_eq!(
                            ref_tag, None,
                            "Raw chromatic N slot should have no refinement, got {:?}",
                            ref_tag
                        );
                    }
                }
            }
        }
    }
}

/// Recursively search for grammar files matching the language name
#[cfg(not(target_arch = "wasm32"))]
fn find_grammar_file_recursive(language: &str, filename: &str) -> Option<String> {
    // Try current directory first
    let languages_dir = "languages";
    if !std::path::Path::new(languages_dir).exists() {
        return None;
    }
    
    // Try exact path first
    let exact_path = format!("{}/{}/{}", languages_dir, language, filename);
    if std::path::Path::new(&exact_path).exists() {
        return Some(exact_path);
    }
    
    // Recursively search
    let languages_path = std::path::Path::new(languages_dir);
    if let Ok(entries) = std::fs::read_dir(languages_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_grammar_file_recursive_helper(&path, language, filename) {
                    return Some(found);
                }
            }
        }
    }
    
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn find_grammar_file_recursive_helper(dir: &std::path::Path, language: &str, filename: &str) -> Option<String> {
    // Check if this directory matches the language name (last component)
    if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
        // If language is just the directory name (e.g., "primes" matches "primes/")
        if dir_name == language {
            let file_path = dir.join(filename);
            if file_path.exists() {
                return Some(file_path.to_string_lossy().to_string());
            }
        }
        
        // If language contains slashes (e.g., "math/primes"), check if path ends with it
        if language.contains('/') {
            let lang_path = std::path::Path::new(language);
            if dir.ends_with(lang_path) {
                let file_path = dir.join(filename);
                if file_path.exists() {
                    return Some(file_path.to_string_lossy().to_string());
                }
            }
        }
    }
    
    // Recursively search subdirectories
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_grammar_file_recursive_helper(&path, language, filename) {
                    return Some(found);
                }
            }
        }
    }
    
    None
}
