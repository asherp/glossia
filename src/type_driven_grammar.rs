// Type-driven grammar generation via CCG proof search
//
// Replaces CFG enumeration with combinator-driven derivation.
// A POS sequence of length k that type-checks as `t` is a proof
// that k terminals compose into a sentence.
//
// The proof search uses three CCG rules:
//   Forward application  (>):  f:A→B  a:A  →  B     (linearizes as f a)
//   Backward application (<):  a:A  f:A→B  →  B     (linearizes as a f)
//   Forward composition  (B):  f:B→C  g:A→B  →  A→C (linearizes as f g)

use crate::lambda_parser::parse_lambda_expr;
use crate::lambda_terms::LambdaTerm;
use crate::semantic_types::{build_pos_type_mapping, SemanticType};
use crate::types::Pos;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single derivation: a POS sequence with its probability
#[derive(Clone, Debug)]
pub struct Derivation {
    pub sequence: Vec<Pos>,
    pub refinements: Vec<Option<String>>,
    pub probability: f64,
    pub result_type: SemanticType,
}

/// Language configuration loaded from grammar.yaml
#[derive(Debug, Clone)]
pub struct LanguageConfig {
    pub name: String,
    pub pos_to_type: HashMap<Pos, SemanticType>,
    pub type_rules: HashMap<String, TypeRule>,
    pub start_types: Vec<SemanticType>,
    pub constraints: Option<GrammarConstraints>,
}

/// Type-driven grammar rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRule {
    pub name: String,
    pub target_type: SemanticType,
    pub productions: Vec<TypeProduction>,
}

/// Production in a type rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeProduction {
    pub lambda_expr: String,
    pub weight: f64,
    #[serde(skip)]
    pub parsed_term: Option<LambdaTerm>,
}

impl LanguageConfig {
    /// Load from grammar.yaml
    pub fn from_yaml(yaml_content: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let doc: GrammarYaml = serde_yaml::from_str(yaml_content)?;

        // Build POS to type mapping
        let mut pos_type_map = HashMap::new();
        for type_def in &doc.grammar.types {
            let pos_name = &type_def.name;
            let lambda_type_str = &type_def.lambda_type;

            let pos = parse_pos_name(pos_name)?;
            let semantic_type = SemanticType::from_str(lambda_type_str)?;
            pos_type_map.insert(pos, semantic_type);
        }

        // Use default mapping for any missing POS tags
        let mut pos_to_type = build_pos_type_mapping(&HashMap::new());
        pos_to_type.extend(pos_type_map);

        // Build type rules from lambda expressions
        let mut type_rules = HashMap::new();
        for (rule_name, rule_data) in &doc.grammar.rules {
            let target_type = infer_target_type_from_lambda(&rule_data.lambda)?;

            let productions: Vec<TypeProduction> =
                if let Some(cfg_prods) = &rule_data.cfg_productions {
                    cfg_prods
                        .iter()
                        .map(|prod| {
                            Ok(TypeProduction {
                                lambda_expr: prod.lambda.clone().unwrap_or_default(),
                                weight: prod.weight,
                                parsed_term: None,
                            })
                        })
                        .collect::<Result<_, Box<dyn std::error::Error>>>()?
                } else {
                    vec![TypeProduction {
                        lambda_expr: rule_data.lambda.clone(),
                        weight: 1.0,
                        parsed_term: None,
                    }]
                };

            type_rules.insert(
                rule_name.clone(),
                TypeRule {
                    name: rule_name.clone(),
                    target_type,
                    productions,
                },
            );
        }

        Ok(LanguageConfig {
            name: doc.grammar.name,
            pos_to_type,
            type_rules,
            start_types: vec![SemanticType::Truth],
            constraints: doc.grammar.constraints,
        })
    }

    // ========================================================================
    // CCG proof search: derive all POS sequences of length k for a target type
    // ========================================================================

    /// Derive all POS sequences of exactly length `k` that produce `target_type`.
    ///
    /// Uses memoized bottom-up proof search with three CCG rules:
    /// - Forward application  (>): f a  where f:A→B, a:A → B
    /// - Backward application (<): a f  where a:A, f:A→B → B
    /// - Forward composition  (B): f g  where f:B→C, g:A→B → A→C
    ///
    /// Returns derivations sorted by probability (highest first).
    pub fn derive_sequences(
        &self,
        target_type: &SemanticType,
        k: usize,
        memo: &mut HashMap<(SemanticType, usize), Vec<Derivation>>,
    ) -> Vec<Derivation> {
        let key = (target_type.clone(), k);
        if let Some(cached) = memo.get(&key) {
            return cached.clone();
        }

        let mut results: Vec<Derivation> = Vec::new();

        if k == 0 {
            memo.insert(key, Vec::new());
            return Vec::new();
        }

        // --- Base case: k=1, a single POS constant ---
        if k == 1 {
            for (pos, pos_type) in &self.pos_to_type {
                if pos_type.compatible_with(target_type) {
                    results.push(Derivation {
                        sequence: vec![*pos],
                        refinements: vec![None],
                        probability: 1.0,
                        result_type: pos_type.clone(),
                    });
                }
            }
            // Sort by POS for determinism
            results.sort_by(|a, b| a.sequence.cmp(&b.sequence));
            memo.insert(key, results.clone());
            return results;
        }

        // --- Recursive case: k>1, try all binary splits ---
        // For each split k = k_left + k_right where k_left >= 1 and k_right >= 1:
        for k_left in 1..k {
            let k_right = k - k_left;

            // Collect all derivations for left and right at these lengths
            // We need to try all possible intermediate types.
            // For efficiency, enumerate over types that actually appear.
            let left_types = self.collect_derivable_types(k_left, memo);
            let right_types = self.collect_derivable_types(k_right, memo);

            // --- Forward application (>): left:A→B  right:A → B ---
            for left_type in &left_types {
                if let Some((domain, codomain)) = left_type.as_function() {
                    if codomain.compatible_with(target_type) {
                        // left is the function, right must have type = domain
                        let left_derivs = self.derive_sequences(left_type, k_left, memo);
                        let right_derivs = self.derive_sequences(domain, k_right, memo);

                        for ld in &left_derivs {
                            for rd in &right_derivs {
                                let mut seq = ld.sequence.clone();
                                seq.extend(&rd.sequence);
                                let mut refs = ld.refinements.clone();
                                refs.extend(rd.refinements.iter().cloned());
                                results.push(Derivation {
                                    sequence: seq,
                                    refinements: refs,
                                    probability: ld.probability * rd.probability,
                                    result_type: codomain.clone(),
                                });
                            }
                        }
                    }
                }
            }

            // --- Backward application (<): left:A  right:A→B → B ---
            for right_type in &right_types {
                if let Some((domain, codomain)) = right_type.as_function() {
                    if codomain.compatible_with(target_type) {
                        // right is the function, left must have type = domain
                        let left_derivs = self.derive_sequences(domain, k_left, memo);
                        let right_derivs = self.derive_sequences(right_type, k_right, memo);

                        for ld in &left_derivs {
                            for rd in &right_derivs {
                                let mut seq = ld.sequence.clone();
                                seq.extend(&rd.sequence);
                                let mut refs = ld.refinements.clone();
                                refs.extend(rd.refinements.iter().cloned());
                                results.push(Derivation {
                                    sequence: seq,
                                    refinements: refs,
                                    probability: ld.probability * rd.probability,
                                    result_type: codomain.clone(),
                                });
                            }
                        }
                    }
                }
            }

            // --- Forward composition (B): left:B→C  right:A→B → A→C ---
            // Only if target_type is a function type A→C
            if let Some((target_a, target_c)) = target_type.as_function() {
                for left_type in &left_types {
                    if let Some((b_left, c_left)) = left_type.as_function() {
                        if c_left.compatible_with(target_c) {
                            // Need right : A→B where A=target_a, B=b_left
                            let right_needed = SemanticType::arrow(
                                target_a.clone(),
                                b_left.clone(),
                            );
                            let left_derivs = self.derive_sequences(left_type, k_left, memo);
                            let right_derivs =
                                self.derive_sequences(&right_needed, k_right, memo);

                            for ld in &left_derivs {
                                for rd in &right_derivs {
                                    let mut seq = ld.sequence.clone();
                                    seq.extend(&rd.sequence);
                                    let mut refs = ld.refinements.clone();
                                    refs.extend(rd.refinements.iter().cloned());
                                    // Composition weight: slightly lower than application
                                    // to prefer simpler derivations
                                    results.push(Derivation {
                                        sequence: seq,
                                        refinements: refs,
                                        probability: ld.probability * rd.probability * 0.5,
                                        result_type: target_type.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Deduplicate: sum probabilities for identical (sequence, refinements) pairs
        let mut prob_map: HashMap<(Vec<Pos>, Vec<Option<String>>), (f64, SemanticType)> =
            HashMap::new();
        for d in results {
            let entry = prob_map
                .entry((d.sequence.clone(), d.refinements.clone()))
                .or_insert((0.0, d.result_type.clone()));
            entry.0 += d.probability;
        }

        let mut deduped: Vec<Derivation> = prob_map
            .into_iter()
            .map(|((sequence, refinements), (probability, result_type))| Derivation {
                sequence,
                refinements,
                probability,
                result_type,
            })
            .collect();

        // Sort by probability (highest first), then by sequence for determinism
        deduped.sort_by(|a, b| {
            b.probability
                .partial_cmp(&a.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.sequence.cmp(&b.sequence))
        });

        memo.insert(key, deduped.clone());
        deduped
    }

    /// Collect all semantic types that are derivable at length k.
    /// Used to enumerate possible intermediate types for binary splits.
    fn collect_derivable_types(
        &self,
        k: usize,
        memo: &mut HashMap<(SemanticType, usize), Vec<Derivation>>,
    ) -> Vec<SemanticType> {
        if k == 1 {
            // At k=1, the derivable types are exactly the POS constant types
            let mut types: Vec<SemanticType> =
                self.pos_to_type.values().cloned().collect();
            types.sort_by(|a, b| format!("{}", a).cmp(&format!("{}", b)));
            types.dedup();
            return types;
        }

        // For k>1, we need to check which types have non-empty derivations.
        // To avoid infinite recursion, check the memo first.
        // If not memoized, we conservatively return all types that *could* be derived
        // (all function codomains and POS types).
        let mut candidate_types: Vec<SemanticType> = Vec::new();

        // All POS types
        for pos_type in self.pos_to_type.values() {
            candidate_types.push(pos_type.clone());
        }

        // All codomains of function types (reachable via application)
        for pos_type in self.pos_to_type.values() {
            if let Some((_, codomain)) = pos_type.as_function() {
                candidate_types.push(codomain.clone());
                // And nested codomains
                if let Some((_, inner_cod)) = codomain.as_function() {
                    candidate_types.push(inner_cod.clone());
                }
            }
        }

        // Add the core types that are always potentially derivable
        candidate_types.push(SemanticType::Entity);
        candidate_types.push(SemanticType::Truth);

        // Dedup
        candidate_types.sort_by(|a, b| format!("{}", a).cmp(&format!("{}", b)));
        candidate_types.dedup();

        // Filter to types that actually have derivations (check memo)
        candidate_types
            .into_iter()
            .filter(|t| {
                let key = (t.clone(), k);
                if let Some(cached) = memo.get(&key) {
                    !cached.is_empty()
                } else {
                    true // Not yet computed — keep as candidate
                }
            })
            .collect()
    }

    /// Derive all sequences for lengths 0..=max_k, returning them indexed by k.
    ///
    /// This is the main entry point that replaces CFG enumeration.
    /// The output format matches `Grammar::precompute_sequences_with_probability`.
    pub fn derive_all(
        &self,
        target_type: &SemanticType,
        max_k: usize,
    ) -> Vec<Vec<Derivation>> {
        let mut memo: HashMap<(SemanticType, usize), Vec<Derivation>> = HashMap::new();
        let mut by_k: Vec<Vec<Derivation>> = vec![Vec::new(); max_k + 1];

        // Build bottom-up: derive for k=1 first, then k=2, etc.
        // This ensures the memo is populated for smaller k when computing larger k.
        for k in 1..=max_k {
            by_k[k] = self.derive_sequences(target_type, k, &mut memo);
        }

        by_k
    }

    // --- Legacy methods (kept for compatibility during transition) ---

    /// Generate POS sequence from semantic type (legacy random method)
    pub fn generate_from_type(
        &self,
        target_type: &SemanticType,
        k: usize,
        rng: &mut impl Rng,
    ) -> Result<Vec<Pos>, String> {
        let applicable: Vec<_> = self
            .type_rules
            .values()
            .filter(|rule| rule.target_type.compatible_with(target_type))
            .collect();

        if applicable.is_empty() {
            return self.generate_from_function_type(target_type, k, rng);
        }

        let rule = select_weighted(&applicable, rng)?;
        let production = select_weighted(&rule.productions, rng)?;

        let lambda_term: LambdaTerm = if let Some(ref term) = production.parsed_term {
            term.clone()
        } else {
            parse_lambda_expr(&production.lambda_expr)
                .map_err(|e| {
                    format!(
                        "Failed to parse lambda expression '{}': {}",
                        production.lambda_expr, e
                    )
                })?
        };

        self.generate_from_lambda_term(&lambda_term, k, rng)
    }

    fn generate_from_function_type(
        &self,
        target_type: &SemanticType,
        k: usize,
        rng: &mut impl Rng,
    ) -> Result<Vec<Pos>, String> {
        match target_type {
            SemanticType::Function { domain, codomain } => {
                let arg_k = k / 2;
                let arg_pos = self.generate_from_type(domain, arg_k, rng)?;
                let func_k = k - arg_k;
                let func_pos = self.generate_from_type(codomain, func_k, rng)?;
                Ok([arg_pos, func_pos].concat())
            }
            _ => self.generate_from_pos_constraint(target_type, k, rng),
        }
    }

    fn generate_from_lambda_term(
        &self,
        term: &LambdaTerm,
        k: usize,
        rng: &mut impl Rng,
    ) -> Result<Vec<Pos>, String> {
        match term {
            LambdaTerm::Constant(pos) => Ok(vec![*pos]),
            LambdaTerm::Application { function, argument } => {
                let arg_k = k / 2;
                let arg_pos = self.generate_from_lambda_term(argument, arg_k, rng)?;
                let func_k = k - arg_k;
                let func_pos = self.generate_from_lambda_term(function, func_k, rng)?;
                Ok([arg_pos, func_pos].concat())
            }
            LambdaTerm::Abstraction { body, .. } => {
                self.generate_from_lambda_term(body, k, rng)
            }
            LambdaTerm::Variable(name) => {
                Err(format!("Cannot generate from variable: {}", name))
            }
            LambdaTerm::Combinator(c) => {
                Err(format!("Cannot generate from unapplied combinator: {}", c))
            }
        }
    }

    fn generate_from_pos_constraint(
        &self,
        target_type: &SemanticType,
        k: usize,
        _rng: &mut impl Rng,
    ) -> Result<Vec<Pos>, String> {
        let valid_pos: Vec<Pos> = self
            .pos_to_type
            .iter()
            .filter(|(_, pos_type)| pos_type.compatible_with(target_type))
            .map(|(pos, _)| *pos)
            .collect();

        if valid_pos.is_empty() {
            return Err(format!("No POS tag matches type: {}", target_type));
        }

        Ok(vec![valid_pos[0]; k.min(1)])
    }
}

fn select_weighted<'a, T>(items: &'a [T], rng: &mut impl Rng) -> Result<&'a T, String> {
    if items.is_empty() {
        return Err("No items to select from".to_string());
    }
    let idx = rng.gen_range(0..items.len());
    Ok(&items[idx])
}

fn infer_target_type_from_lambda(lambda_expr: &str) -> Result<SemanticType, String> {
    let term = parse_lambda_expr(lambda_expr)?;
    let context = HashMap::new();
    term.infer_type(&context)
}

fn parse_pos_name(name: &str) -> Result<Pos, String> {
    if let Some(pos) = Pos::from_str(name) {
        return Ok(pos);
    }
    for pos in Pos::ALL {
        if pos.as_str().eq_ignore_ascii_case(name) {
            return Ok(*pos);
        }
    }
    Err(format!("Unknown POS name: {}", name))
}

// ============================================================================
// YAML structures
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct GrammarYaml {
    grammar: GrammarDoc,
}

#[derive(Debug, Serialize, Deserialize)]
struct GrammarDoc {
    name: String,
    types: Vec<TypeDefinition>,
    rules: HashMap<String, RuleDefinition>,
    #[serde(default)]
    constraints: Option<GrammarConstraints>,
    #[serde(default)]
    dialects: Option<HashMap<String, DialectDefinition>>,
}

/// Dialect-specific rule overrides.
/// All fields are optional — a dialect may only specify `parent:`, `scale:`,
/// or `payload_wordlist:` without overriding any rules.
#[derive(Debug, Serialize, Deserialize)]
pub struct DialectDefinition {
    #[serde(default)]
    pub rules: HashMap<String, RuleDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrammarConstraints {
    #[serde(default)]
    pub prime_ordering: Option<PrimeOrderingConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeOrderingConstraint {
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TypeDefinition {
    name: String,
    lambda_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub lambda: String,
    #[serde(default)]
    pub cfg_productions: Option<Vec<CfgProduction>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CfgProduction {
    pub production: String,
    pub weight: f64,
    pub lambda: Option<String>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_types::pos_to_semantic_type;

    #[test]
    fn test_load_config() {
        let yaml = r#"
grammar:
  name: "Test Grammar"
  types:
    - name: "N"
      lambda_type: "e -> t"
  rules:
    sentence:
      lambda: "λNP: (e->t). λVP: ((e->t)->t). NP(VP)"
"#;

        let config = LanguageConfig::from_yaml(yaml);
        assert!(config.is_ok());
    }

    #[test]
    fn test_derive_k1_entity() {
        // At k=1, deriving type `e` should find Pron (type e)
        let config = make_test_config();
        let mut memo = HashMap::new();
        let derivs = config.derive_sequences(&SemanticType::Entity, 1, &mut memo);
        assert!(
            !derivs.is_empty(),
            "Should find at least one k=1 derivation for type e"
        );
        // Pron has type e
        assert!(
            derivs.iter().any(|d| d.sequence == vec![Pos::Pron]),
            "Pron should derive type e"
        );
    }

    #[test]
    fn test_derive_k1_predicate() {
        // At k=1, deriving type `e→t` should find N
        let config = make_test_config();
        let mut memo = HashMap::new();
        let e_to_t = SemanticType::arrow(SemanticType::Entity, SemanticType::Truth);
        let derivs = config.derive_sequences(&e_to_t, 1, &mut memo);
        assert!(
            derivs.iter().any(|d| d.sequence == vec![Pos::N]),
            "N should derive type e→t"
        );
    }

    #[test]
    fn test_derive_k1_truth() {
        // At k=1, deriving type `t` should find Dot
        let config = make_test_config();
        let mut memo = HashMap::new();
        let derivs = config.derive_sequences(&SemanticType::Truth, 1, &mut memo);
        assert!(
            derivs.iter().any(|d| d.sequence == vec![Pos::Dot]),
            "Dot should derive type t"
        );
    }

    #[test]
    fn test_derive_k2_forward_application() {
        // At k=2, Det N should derive via forward application:
        //   Det:(e→t)→(e→t)  N:(e→t)  →  (e→t)
        let config = make_test_config();
        let mut memo = HashMap::new();
        let e_to_t = SemanticType::arrow(SemanticType::Entity, SemanticType::Truth);
        let derivs = config.derive_sequences(&e_to_t, 2, &mut memo);
        let has_det_n = derivs
            .iter()
            .any(|d| d.sequence == vec![Pos::Det, Pos::N]);
        assert!(has_det_n, "Det N should derive type e→t at k=2. Got: {:?}",
            derivs.iter().map(|d| &d.sequence).collect::<Vec<_>>());
    }

    #[test]
    fn test_derive_k2_backward_application() {
        // At k=2, N Det should also derive via backward application:
        //   N:(e→t)  Det:(e→t)→(e→t)  →  (e→t)  (N is arg, Det is func)
        let config = make_test_config();
        let mut memo = HashMap::new();
        let e_to_t = SemanticType::arrow(SemanticType::Entity, SemanticType::Truth);
        let derivs = config.derive_sequences(&e_to_t, 2, &mut memo);
        let has_n_det = derivs
            .iter()
            .any(|d| d.sequence == vec![Pos::N, Pos::Det]);
        assert!(
            has_n_det,
            "N Det should derive type e→t at k=2 via backward application. Got: {:?}",
            derivs.iter().map(|d| &d.sequence).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_derive_k2_cop_adj() {
        // Cop:(e→t)→(e→t) applied to Adj:e→e
        // Wait — Adj:e→e has type e→e, not e→t.
        // Cop expects (e→t), so Cop(Adj) doesn't type-check.
        // But Cop Adj is a common VP in the CFG. Let's check what types work.
        //
        // Actually Cop:(e→t)→(e→t) needs arg of type (e→t).
        // Adj:(e→e) is NOT (e→t). So this doesn't type-check in Montague.
        // The CFG allows it because it doesn't check types.
        //
        // In the CCG approach, to get "Cop Adj" we'd need to adjust types.
        // For now, verify the proof search doesn't produce it.
        let config = make_test_config();
        let mut memo = HashMap::new();
        let e_to_t = SemanticType::arrow(SemanticType::Entity, SemanticType::Truth);
        let derivs = config.derive_sequences(&e_to_t, 2, &mut memo);
        let has_cop_adj = derivs
            .iter()
            .any(|d| d.sequence == vec![Pos::Cop, Pos::Adj]);
        // This should NOT type-check (Cop needs e→t, Adj provides e→e)
        assert!(
            !has_cop_adj,
            "Cop Adj should NOT type-check (Cop expects e→t, Adj is e→e)"
        );
    }

    #[test]
    fn test_derive_truth_k2() {
        // At k=2, deriving t: we need f:A→t applied to a:A
        // Prefix:t→t applied to Dot:t → t
        // N:e→t applied to Pron:e → t
        let config = make_test_config();
        let mut memo = HashMap::new();
        let derivs = config.derive_sequences(&SemanticType::Truth, 2, &mut memo);
        assert!(
            !derivs.is_empty(),
            "Should have derivations for type t at k=2"
        );
        // N(Pron) should give t (N:e→t applied to Pron:e → t)
        // But linearization is N Pron (function first) for forward, or Pron N for backward
        let has_n_pron = derivs.iter().any(|d| {
            (d.sequence == vec![Pos::N, Pos::Pron])
                || (d.sequence == vec![Pos::Pron, Pos::N])
        });
        assert!(
            has_n_pron,
            "N applied to Pron (or Pron backward-applied to N) should give t at k=2. Got: {:?}",
            derivs.iter().map(|d| &d.sequence).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_derive_all_lengths() {
        let config = make_test_config();
        let by_k = config.derive_all(&SemanticType::Truth, 5);

        // k=0: no derivations
        assert!(by_k[0].is_empty());
        // k=1: should have Dot:t
        assert!(by_k[1].iter().any(|d| d.sequence == vec![Pos::Dot]));
        // k=2+: should have some derivations
        let total_k2_plus: usize = (2..=5).map(|k| by_k[k].len()).sum();
        assert!(
            total_k2_plus > 0,
            "Should have derivations for k>=2"
        );
    }

    /// Build a minimal config for testing with a few key POS types
    fn make_test_config() -> LanguageConfig {
        let mut pos_to_type = HashMap::new();
        // Only include types that are useful for testing
        for pos in &[
            Pos::N, Pos::V, Pos::Adj, Pos::Det, Pos::Adv, Pos::Cop, Pos::Modal,
            Pos::Prep, Pos::Pron, Pos::Dot, Pos::Prefix,
        ] {
            pos_to_type.insert(*pos, pos_to_semantic_type(pos));
        }

        LanguageConfig {
            name: "Test".to_string(),
            pos_to_type,
            type_rules: HashMap::new(),
            start_types: vec![SemanticType::Truth],
            constraints: None,
        }
    }
}
