// Semantic type system for Montague Grammar
// Maps POS tags to lambda calculus types

use crate::types::Pos;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Semantic types in Montague Grammar style
/// e = entity, t = truth value
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticType {
    /// Entity type (e) - represents individuals/things
    Entity,
    /// Truth type (t) - represents propositions/sentences
    Truth,
    /// Function type (A -> B) - represents functions from A to B
    Function {
        domain: Box<SemanticType>,
        codomain: Box<SemanticType>,
    },
    /// Refined type - for language-specific features (e.g., Latin cases)
    Refined {
        refinement: String,
        base: Box<SemanticType>,
    },
}

impl SemanticType {
    /// Parse semantic type from string like "e -> t" or "(e -> t) -> t"
    pub fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        
        // Remove outer parentheses if entire string is parenthesized
        let s = if s.starts_with('(') && s.ends_with(')') {
            // Check if parentheses are balanced and outermost
            let mut depth = 0;
            let mut is_outermost = true;
            for (i, ch) in s.char_indices() {
                match ch {
                    '(' => {
                        depth += 1;
                        if i > 0 && depth == 1 {
                            is_outermost = false;
                            break;
                        }
                    }
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            if is_outermost && depth == 0 {
                &s[1..s.len()-1]
            } else {
                s
            }
        } else {
            s
        };
        
        // Find the rightmost arrow that's not inside parentheses
        let mut depth = 0;
        let mut arrow_pos = None;
        
        let bytes = s.as_bytes();
        for i in 0..bytes.len().saturating_sub(1) {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'-' if bytes[i + 1] == b'>' => {
                    if depth == 0 {
                        arrow_pos = Some(i);
                    }
                }
                _ => {}
            }
        }
        
        if let Some(pos) = arrow_pos {
            let domain_str = s[..pos].trim();
            let codomain_str = s[pos+2..].trim();
            
            let domain = Box::new(Self::from_str(domain_str)?);
            let codomain = Box::new(Self::from_str(codomain_str)?);
            
            return Ok(SemanticType::Function { domain, codomain });
        }
        
        // Atomic types
        match s.trim() {
            "e" => Ok(SemanticType::Entity),
            "t" => Ok(SemanticType::Truth),
            _ => Err(format!("Unknown atomic type: {}", s)),
        }
    }
    
    /// Check if this type can be applied to another type
    /// Returns Some(result_type) if application is valid, None otherwise
    pub fn can_apply_to(&self, arg_type: &SemanticType) -> Option<SemanticType> {
        match self {
            SemanticType::Function { domain, codomain } => {
                if **domain == *arg_type {
                    Some(*codomain.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    
    /// Get the base type (unwrap refinements)
    pub fn base_type(&self) -> &SemanticType {
        match self {
            SemanticType::Refined { base, .. } => base.base_type(),
            _ => self,
        }
    }
    
    /// Check if types are compatible (ignoring refinements)
    pub fn compatible_with(&self, other: &SemanticType) -> bool {
        self.base_type() == other.base_type()
    }
}

impl fmt::Display for SemanticType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticType::Entity => write!(f, "e"),
            SemanticType::Truth => write!(f, "t"),
            SemanticType::Function { domain, codomain } => {
                let domain_str = if matches!(**domain, SemanticType::Function { .. }) {
                    format!("({})", domain)
                } else {
                    format!("{}", domain)
                };
                write!(f, "{} -> {}", domain_str, codomain)
            }
            SemanticType::Refined { refinement, base } => {
                write!(f, "{}[{}]", base, refinement)
            }
        }
    }
}

/// Map POS tags to semantic types
pub fn pos_to_semantic_type(pos: &Pos) -> SemanticType {
    match pos {
        // Nouns: e -> t (predicates over entities)
        Pos::N => SemanticType::Function {
            domain: Box::new(SemanticType::Entity),
            codomain: Box::new(SemanticType::Truth),
        },
        // Verbs: e -> (e -> t) (take entity, return predicate)
        Pos::V => SemanticType::Function {
            domain: Box::new(SemanticType::Entity),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Adjectives: e -> e (modify entities)
        Pos::Adj => SemanticType::Function {
            domain: Box::new(SemanticType::Entity),
            codomain: Box::new(SemanticType::Entity),
        },
        // Adverbs: (e -> t) -> (e -> t) (modify predicates)
        Pos::Adv => SemanticType::Function {
            domain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Determiners: (e -> t) -> (e -> t) (quantifiers)
        Pos::Det => SemanticType::Function {
            domain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Prepositions: e -> (e -> t) (prepositional modifiers)
        Pos::Prep => SemanticType::Function {
            domain: Box::new(SemanticType::Entity),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Conjunctions: t -> (t -> t) (connect propositions)
        Pos::Conj => SemanticType::Function {
            domain: Box::new(SemanticType::Truth),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Truth),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Copula: (e -> t) -> (e -> t) (links subject to predicate)
        Pos::Cop => SemanticType::Function {
            domain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Modal verbs: (e -> t) -> (e -> t) (modify predicates)
        Pos::Modal => SemanticType::Function {
            domain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Auxiliary verbs: (e -> t) -> (e -> t)
        Pos::Aux => SemanticType::Function {
            domain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // To (infinitive marker): (e -> t) -> (e -> t)
        Pos::To => SemanticType::Function {
            domain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            }),
        },
        // Dot/Period: t (completes sentence)
        Pos::Dot => SemanticType::Truth,
        // Prefix: t -> t (modifies sentence)
        Pos::Prefix => SemanticType::Function {
            domain: Box::new(SemanticType::Truth),
            codomain: Box::new(SemanticType::Truth),
        },
    }
}

/// Build POS to semantic type mapping from grammar.yaml
pub fn build_pos_type_mapping(
    type_definitions: &HashMap<String, String>,
) -> HashMap<Pos, SemanticType> {
    let mut mapping = HashMap::new();
    
    // Default mapping
    for pos in &[
        Pos::N, Pos::V, Pos::Adj, Pos::Adv, Pos::Det, Pos::Prep,
        Pos::Conj, Pos::Cop, Pos::Modal, Pos::Aux, Pos::To,
        Pos::Dot, Pos::Prefix,
    ] {
        mapping.insert(*pos, pos_to_semantic_type(pos));
    }
    
    // Override with custom types from grammar.yaml
    for (pos_name, lambda_type_str) in type_definitions {
        if let Ok(pos) = parse_pos_name(pos_name) {
            if let Ok(semantic_type) = SemanticType::from_str(lambda_type_str) {
                mapping.insert(pos, semantic_type);
            }
        }
    }
    
    mapping
}

fn parse_pos_name(name: &str) -> Result<Pos, String> {
    match name.to_lowercase().as_str() {
        "n" | "noun" => Ok(Pos::N),
        "v" | "verb" => Ok(Pos::V),
        "adj" | "adjective" => Ok(Pos::Adj),
        "adv" | "adverb" => Ok(Pos::Adv),
        "det" | "determiner" => Ok(Pos::Det),
        "prep" | "preposition" => Ok(Pos::Prep),
        "conj" | "conjunction" => Ok(Pos::Conj),
        "cop" | "copula" => Ok(Pos::Cop),
        "modal" => Ok(Pos::Modal),
        "aux" | "auxiliary" => Ok(Pos::Aux),
        "to" => Ok(Pos::To),
        "dot" | "period" => Ok(Pos::Dot),
        "prefix" => Ok(Pos::Prefix),
        _ => Err(format!("Unknown POS tag: {}", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_types() {
        assert_eq!(SemanticType::from_str("e"), Ok(SemanticType::Entity));
        assert_eq!(SemanticType::from_str("t"), Ok(SemanticType::Truth));
    }
    
    #[test]
    fn test_parse_function_types() {
        let e_to_t = SemanticType::from_str("e -> t").unwrap();
        assert!(matches!(e_to_t, SemanticType::Function { .. }));
        
        let e_to_e_to_t = SemanticType::from_str("e -> (e -> t)").unwrap();
        assert!(matches!(e_to_e_to_t, SemanticType::Function { .. }));
    }
    
    #[test]
    fn test_type_application() {
        let func_type = SemanticType::from_str("e -> t").unwrap();
        let arg_type = SemanticType::Entity;
        let result = func_type.can_apply_to(&arg_type);
        assert_eq!(result, Some(SemanticType::Truth));
    }
    
    #[test]
    fn test_pos_to_type() {
        let n_type = pos_to_semantic_type(&Pos::N);
        assert_eq!(n_type, SemanticType::from_str("e -> t").unwrap());
        
        let v_type = pos_to_semantic_type(&Pos::V);
        assert_eq!(v_type, SemanticType::from_str("e -> (e -> t)").unwrap());
    }
}
