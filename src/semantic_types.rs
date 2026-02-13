// Semantic type system for Montague Grammar with CCG combinator support
//
// Types form a free algebra over {e, t} with the arrow constructor (→).
// CCG combinators operate on these types:
//
//   Forward application (>):  A/B  B  →  A        where A/B means A→B
//   Backward application (<): B  A\B  →  A        (= B  B→A  →  A)
//   Composition (B):          A/B  B/C  →  A/C
//   Type raising (T):         a  →  (a→b)→b

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

    // --- CCG combinator type operations ---

    /// Forward composition (B combinator at the type level):
    ///   Given f : B → C and g : A → B, return f ∘ g : A → C
    ///
    /// This checks that f's domain matches g's codomain.
    pub fn compose_type(f_type: &SemanticType, g_type: &SemanticType) -> Option<SemanticType> {
        // f : B → C
        let (b_f, c) = match f_type.base_type() {
            SemanticType::Function { domain, codomain } => (domain.as_ref(), codomain.as_ref()),
            _ => return None,
        };
        // g : A → B
        let (a, b_g) = match g_type.base_type() {
            SemanticType::Function { domain, codomain } => (domain.as_ref(), codomain.as_ref()),
            _ => return None,
        };
        // Check B_f == B_g
        if b_f.base_type() != b_g.base_type() {
            return None;
        }
        // Return A → C
        Some(SemanticType::Function {
            domain: Box::new(a.clone()),
            codomain: Box::new(c.clone()),
        })
    }

    /// Type raising (T combinator at the type level):
    ///   Given a : A, and a target function type (A → B),
    ///   return the raised type (A → B) → B
    ///
    /// In CCG: NP : e  becomes  S/(S\NP) : (e→t)→t
    pub fn type_raise(base: &SemanticType, target_codomain: &SemanticType) -> SemanticType {
        // T raises a : A to (A → B) → B
        let func_type = SemanticType::Function {
            domain: Box::new(base.clone()),
            codomain: Box::new(target_codomain.clone()),
        };
        SemanticType::Function {
            domain: Box::new(func_type),
            codomain: Box::new(target_codomain.clone()),
        }
    }

    /// Backward application: given B and B→A, return A.
    /// This is the type-level operation for backward function application (<).
    ///   B  A\B → A    (in CCG slash notation)
    ///   Equivalently: given arg_type and func_type = arg_type → result, return result.
    pub fn backward_apply(
        arg_type: &SemanticType,
        func_type: &SemanticType,
    ) -> Option<SemanticType> {
        // func_type must be arg_type → result
        func_type.can_apply_to(arg_type)
    }

    /// Flip type (C combinator at the type level):
    ///   Given f : A → B → C, return C(f) : B → A → C
    pub fn flip_type(f_type: &SemanticType) -> Option<SemanticType> {
        // f : A → (B → C)
        let (a, bc) = match f_type.base_type() {
            SemanticType::Function { domain, codomain } => (domain.as_ref(), codomain.as_ref()),
            _ => return None,
        };
        let (b, c) = match bc.base_type() {
            SemanticType::Function { domain, codomain } => (domain.as_ref(), codomain.as_ref()),
            _ => return None,
        };
        // Return B → (A → C)
        Some(SemanticType::Function {
            domain: Box::new(b.clone()),
            codomain: Box::new(SemanticType::Function {
                domain: Box::new(a.clone()),
                codomain: Box::new(c.clone()),
            }),
        })
    }

    /// Distribution type (S combinator at the type level):
    ///   Given f : A → B → C and g : A → B, return S(f)(g) : A → C
    pub fn distribute_type(
        f_type: &SemanticType,
        g_type: &SemanticType,
    ) -> Option<SemanticType> {
        // f : A → (B → C)
        let (a_f, bc) = match f_type.base_type() {
            SemanticType::Function { domain, codomain } => (domain.as_ref(), codomain.as_ref()),
            _ => return None,
        };
        let (b_f, c) = match bc.base_type() {
            SemanticType::Function { domain, codomain } => (domain.as_ref(), codomain.as_ref()),
            _ => return None,
        };
        // g : A → B
        let (a_g, b_g) = match g_type.base_type() {
            SemanticType::Function { domain, codomain } => (domain.as_ref(), codomain.as_ref()),
            _ => return None,
        };
        // Check A_f == A_g and B_f == B_g
        if a_f.base_type() != a_g.base_type() || b_f.base_type() != b_g.base_type() {
            return None;
        }
        // Return A → C
        Some(SemanticType::Function {
            domain: Box::new(a_f.clone()),
            codomain: Box::new(c.clone()),
        })
    }

    /// Check if this type is a function type and return domain and codomain
    pub fn as_function(&self) -> Option<(&SemanticType, &SemanticType)> {
        match self.base_type() {
            SemanticType::Function { domain, codomain } => Some((domain, codomain)),
            _ => None,
        }
    }

    /// Convenience: build a function type A → B
    pub fn arrow(domain: SemanticType, codomain: SemanticType) -> SemanticType {
        SemanticType::Function {
            domain: Box::new(domain),
            codomain: Box::new(codomain),
        }
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
        // Pronouns: e (entity reference)
        Pos::Pron => SemanticType::Entity,
    }
}

/// Build POS to semantic type mapping from grammar.yaml
pub fn build_pos_type_mapping(
    type_definitions: &HashMap<String, String>,
) -> HashMap<Pos, SemanticType> {
    let mut mapping = HashMap::new();

    // Default mapping from all known POS variants
    for pos in Pos::ALL {
        mapping.insert(*pos, pos_to_semantic_type(pos));
    }

    // Override with custom types from grammar.yaml
    for (pos_name, lambda_type_str) in type_definitions {
        if let Some(pos) = Pos::from_str(pos_name) {
            if let Ok(semantic_type) = SemanticType::from_str(lambda_type_str) {
                mapping.insert(pos, semantic_type);
            }
        }
    }

    mapping
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

    // --- CCG combinator type tests ---

    #[test]
    fn test_compose_type_prefix_n() {
        // B Prefix N: compose Prefix:t→t with N:e→t
        // f = Prefix : t→t  (B=t, C=t)
        // g = N : e→t       (A=e, B=t)
        // f.domain = t = g.codomain = t ✓
        // Result: A→C = e→t
        let prefix_type = pos_to_semantic_type(&Pos::Prefix); // t→t
        let n_type = pos_to_semantic_type(&Pos::N);            // e→t
        let composed = SemanticType::compose_type(&prefix_type, &n_type);
        assert!(composed.is_some());
        assert_eq!(composed.unwrap(), SemanticType::from_str("e -> t").unwrap());
    }

    #[test]
    fn test_compose_type_adv_cop() {
        // B Adv Cop: compose Adv:(e→t)→(e→t) with Cop:(e→t)→(e→t)
        // f = Adv : (e→t)→(e→t)  (B=(e→t), C=(e→t))
        // g = Cop : (e→t)→(e→t)  (A=(e→t), B=(e→t))
        // f.domain = (e→t) = g.codomain = (e→t) ✓
        // Result: (e→t)→(e→t)
        let adv_type = pos_to_semantic_type(&Pos::Adv);
        let cop_type = pos_to_semantic_type(&Pos::Cop);
        let composed = SemanticType::compose_type(&adv_type, &cop_type);
        assert!(composed.is_some());
        assert_eq!(
            composed.unwrap(),
            SemanticType::from_str("(e -> t) -> (e -> t)").unwrap()
        );
    }

    #[test]
    fn test_det_n_is_application_not_composition() {
        // Det:(e→t)→(e→t) applied to N:(e→t) gives (e→t) — this is application, not composition
        let det_type = pos_to_semantic_type(&Pos::Det);
        let n_type = pos_to_semantic_type(&Pos::N);
        // Application works:
        let applied = det_type.can_apply_to(&n_type);
        assert!(applied.is_some());
        assert_eq!(applied.unwrap(), SemanticType::from_str("e -> t").unwrap());
        // Composition doesn't (domain of Det is (e→t), codomain of N is t — mismatch):
        let composed = SemanticType::compose_type(&det_type, &n_type);
        assert!(composed.is_none());
    }

    #[test]
    fn test_compose_type_mismatch() {
        // Composing two incompatible types should fail
        let e_to_t = SemanticType::from_str("e -> t").unwrap();
        let t_to_t = SemanticType::from_str("t -> t").unwrap();
        // e_to_t : e→t (B=e, C=t), t_to_t : t→t (A=t, B=t)
        // B=e ≠ B=t → should fail
        let composed = SemanticType::compose_type(&e_to_t, &t_to_t);
        assert!(composed.is_none());
    }

    #[test]
    fn test_type_raise_np() {
        // Type-raise NP:e to (e→t)→t
        let raised = SemanticType::type_raise(&SemanticType::Entity, &SemanticType::Truth);
        // Should be (e→t)→t
        assert_eq!(raised, SemanticType::from_str("(e -> t) -> t").unwrap());
    }

    #[test]
    fn test_flip_type_verb() {
        // C(V): flip V : e→(e→t) to get e→(e→t)
        // Wait: V : e → (e → t) means A=e, B=e, C=t
        // C(V) : B → A → C = e → e → t = e → (e → t)
        // So flipping a verb doesn't change its type (both args are type e)!
        let v_type = pos_to_semantic_type(&Pos::V); // e → (e → t)
        let flipped = SemanticType::flip_type(&v_type);
        assert!(flipped.is_some());
        // e→(e→t) flipped is still e→(e→t) since both domain levels are e
        assert_eq!(flipped.unwrap(), v_type);
    }

    #[test]
    fn test_flip_type_conj() {
        // C(Conj): Conj : t→(t→t), A=t, B=t, C=t
        // C(Conj) : t→(t→t) — same since both args are t
        let conj_type = pos_to_semantic_type(&Pos::Conj);
        let flipped = SemanticType::flip_type(&conj_type);
        assert!(flipped.is_some());
        assert_eq!(flipped.unwrap(), conj_type);
    }

    #[test]
    fn test_backward_apply() {
        // Backward: NP:e→t applied to Det:(e→t)→(e→t) gives (e→t)
        let np_type = SemanticType::from_str("e -> t").unwrap();
        let det_type = pos_to_semantic_type(&Pos::Det); // (e→t)→(e→t)
        let result = SemanticType::backward_apply(&np_type, &det_type);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), SemanticType::from_str("e -> t").unwrap());
    }

    #[test]
    fn test_distribute_type() {
        // S f g : f:(A→B→C), g:(A→B) → (A→C)
        // Let f = V : e→(e→t) (A=e, B=e, C=t)
        // Let g = Adj : e→e    (A=e, B=e)
        // S(V)(Adj) : e→t
        let v_type = pos_to_semantic_type(&Pos::V);     // e → (e → t)
        let adj_type = pos_to_semantic_type(&Pos::Adj); // e → e
        let result = SemanticType::distribute_type(&v_type, &adj_type);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), SemanticType::from_str("e -> t").unwrap());
    }

    #[test]
    fn test_as_function() {
        let e_to_t = SemanticType::from_str("e -> t").unwrap();
        let (domain, codomain) = e_to_t.as_function().unwrap();
        assert_eq!(domain, &SemanticType::Entity);
        assert_eq!(codomain, &SemanticType::Truth);
    }

    #[test]
    fn test_arrow_convenience() {
        let built = SemanticType::arrow(SemanticType::Entity, SemanticType::Truth);
        let parsed = SemanticType::from_str("e -> t").unwrap();
        assert_eq!(built, parsed);
    }
}
