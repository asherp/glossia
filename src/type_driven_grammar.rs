// Type-driven grammar generation system
// Uses lambda calculus types from grammar.yaml to generate POS sequences

use crate::lambda_parser::parse_lambda_expr;
use crate::lambda_terms::LambdaTerm;
use crate::semantic_types::{SemanticType, build_pos_type_mapping};
use crate::types::Pos;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub lambda_expr: String,  // e.g., "λNP: (e->t). λVP: ((e->t)->t). NP(VP)"
    pub weight: f64,
    #[serde(skip)]
    pub parsed_term: Option<LambdaTerm>,  // Cached parsed lambda term
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
            
            // Parse POS name
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
            
            let productions: Vec<TypeProduction> = if let Some(cfg_prods) = &rule_data.cfg_productions {
                cfg_prods.iter()
                    .map(|prod| {
                        Ok(TypeProduction {
                            lambda_expr: prod.lambda.clone().unwrap_or_default(),
                            weight: prod.weight,
                            parsed_term: None,
                        })
                    })
                    .collect::<Result<_, Box<dyn std::error::Error>>>()?
            } else {
                // Fallback: create production from lambda expression
                vec![TypeProduction {
                    lambda_expr: rule_data.lambda.clone(),
                    weight: 1.0,
                    parsed_term: None,
                }]
            };
            
            type_rules.insert(rule_name.clone(), TypeRule {
                name: rule_name.clone(),
                target_type,
                productions,
            });
        }
        
        Ok(LanguageConfig {
            name: doc.grammar.name,
            pos_to_type,
            type_rules,
            start_types: vec![SemanticType::Truth],
            constraints: doc.grammar.constraints,
        })
    }
    
    /// Generate POS sequence from semantic type
    pub fn generate_from_type(
        &self,
        target_type: &SemanticType,
        k: usize,
        rng: &mut impl Rng,
    ) -> Result<Vec<Pos>, String> {
        // Find applicable rules
        let applicable: Vec<_> = self.type_rules
            .values()
            .filter(|rule| rule.target_type.compatible_with(target_type))
            .collect();
        
        if applicable.is_empty() {
            return self.generate_from_function_type(target_type, k, rng);
        }
        
        // Select rule probabilistically
        let rule = select_weighted(&applicable, rng)?;
        
        // Select production probabilistically
        let production = select_weighted(&rule.productions, rng)?;
        
        // Parse lambda expression if not cached
        let lambda_term: LambdaTerm = if let Some(ref term) = production.parsed_term {
            term.clone()
        } else {
            parse_lambda_expr(&production.lambda_expr)
                .map_err(|e| format!("Failed to parse lambda expression '{}': {}", production.lambda_expr, e))?
        };
        
        // Generate POS sequence from lambda term
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
                // Generate argument and function, then combine
                let arg_k = k / 2;
                let arg_pos = self.generate_from_type(domain, arg_k, rng)?;
                let func_k = k - arg_k;
                let func_pos = self.generate_from_type(codomain, func_k, rng)?;
                
                // Combine: argument first, then function (SOV order for Latin)
                Ok([arg_pos, func_pos].concat())
            }
            _ => {
                // Try to find POS tag that matches this type
                self.generate_from_pos_constraint(target_type, k, rng)
            }
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
                // Look up variable type from context
                // For now, generate from type constraints
                Err(format!("Cannot generate from variable: {}", name))
            }
        }
    }
    
    fn generate_from_pos_constraint(
        &self,
        target_type: &SemanticType,
        k: usize,
        _rng: &mut impl Rng,
    ) -> Result<Vec<Pos>, String> {
        // Find POS tags that satisfy this type
        let valid_pos: Vec<Pos> = self.pos_to_type
            .iter()
            .filter(|(_, pos_type)| pos_type.compatible_with(target_type))
            .map(|(pos, _)| *pos)
            .collect();
        
        if valid_pos.is_empty() {
            return Err(format!("No POS tag matches type: {}", target_type));
        }
        
        // Return single POS (or repeat if k > 1, but typically k=1 for terminals)
        Ok(vec![valid_pos[0]; k.min(1)])
    }
}

fn select_weighted<'a, T>(items: &'a [T], rng: &mut impl Rng) -> Result<&'a T, String> {
    if items.is_empty() {
        return Err("No items to select from".to_string());
    }
    
    // For now, uniform selection (weights would be normalized)
    let idx = rng.gen_range(0..items.len());
    Ok(&items[idx])
}

fn infer_target_type_from_lambda(lambda_expr: &str) -> Result<SemanticType, String> {
    // Parse lambda expression and infer its type
    let term = parse_lambda_expr(lambda_expr)?;
    let context = HashMap::new();
    term.infer_type(&context)
}

fn parse_pos_name(name: &str) -> Result<Pos, String> {
    // Try exact match first (canonical form)
    if let Some(pos) = Pos::from_str(name) {
        return Ok(pos);
    }
    // Try case-insensitive match against canonical names
    for pos in Pos::ALL {
        if pos.as_str().eq_ignore_ascii_case(name) {
            return Ok(*pos);
        }
    }
    Err(format!("Unknown POS name: {}", name))
}

// YAML structure for grammar.yaml
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

/// Dialect-specific rule overrides
#[derive(Debug, Serialize, Deserialize)]
pub struct DialectDefinition {
    pub rules: HashMap<String, RuleDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrammarConstraints {
    /// Prime ordering constraint for math/primes language
    /// When enabled, cover words (non-primes) must satisfy: left_prime < cover_word < right_prime
    #[serde(default)]
    pub prime_ordering: Option<PrimeOrderingConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeOrderingConstraint {
    /// Enable the prime ordering constraint
    pub enabled: bool,
    /// Description of the constraint
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

#[cfg(test)]
mod tests {
    use super::*;
    
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
}
