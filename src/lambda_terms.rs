// Lambda term representation using moniker for proper variable binding
// Used for representing semantic composition in Montague Grammar

use crate::semantic_types::SemanticType;
use crate::types::Pos;
use std::collections::HashMap;
use std::fmt;

/// Lambda term representation with proper variable binding
/// Note: Not serializable due to moniker types - use string representation instead
#[derive(Clone, Debug)]
pub enum LambdaTerm {
    /// Variable reference (bound by abstraction)
    Variable(String),  // Store name as string for simplicity
    /// Constant (POS tag as semantic constant)
    Constant(Pos),
    /// Function application: f(a)
    Application {
        function: Box<LambdaTerm>,
        argument: Box<LambdaTerm>,
    },
    /// Lambda abstraction: λx: A. M
    Abstraction {
        var_name: String,
        var_type: SemanticType,
        body: Box<LambdaTerm>,
    },
}

impl LambdaTerm {
    /// Create a constant term from a POS tag
    pub fn constant(pos: Pos) -> Self {
        LambdaTerm::Constant(pos)
    }
    
    /// Create a variable term
    pub fn variable(name: String) -> Self {
        LambdaTerm::Variable(name)
    }
    
    /// Create an application term: f(a)
    pub fn apply(function: LambdaTerm, argument: LambdaTerm) -> Self {
        LambdaTerm::Application {
            function: Box::new(function),
            argument: Box::new(argument),
        }
    }
    
    /// Create an abstraction term: λx: A. M
    pub fn abstract_var(name: String, var_type: SemanticType, body: LambdaTerm) -> Self {
        LambdaTerm::Abstraction {
            var_name: name,
            var_type,
            body: Box::new(body),
        }
    }
    
    /// Infer the semantic type of this term
    pub fn infer_type(
        &self,
        context: &HashMap<String, SemanticType>,
    ) -> Result<SemanticType, String> {
        match self {
            LambdaTerm::Variable(name) => {
                context.get(name)
                    .ok_or_else(|| format!("Unknown variable: {}", name))
                    .map(|t| t.clone())
            }
            LambdaTerm::Constant(pos) => {
                Ok(crate::semantic_types::pos_to_semantic_type(pos))
            }
            LambdaTerm::Application { function, argument } => {
                let func_type = function.infer_type(context)?;
                let arg_type = argument.infer_type(context)?;
                func_type
                    .can_apply_to(&arg_type)
                    .ok_or_else(|| {
                        format!(
                            "Type mismatch: cannot apply {} to {}",
                            func_type, arg_type
                        )
                    })
            }
            LambdaTerm::Abstraction {
                var_name,
                var_type,
                body,
            } => {
                // Create new context with bound variable
                let mut new_context = context.clone();
                new_context.insert(var_name.clone(), var_type.clone());
                let body_type = body.infer_type(&new_context)?;
                Ok(SemanticType::Function {
                    domain: Box::new(var_type.clone()),
                    codomain: Box::new(body_type),
                })
            }
        }
    }
    
    /// Generate POS sequence from lambda term
    /// This extracts the POS tags in the order they should appear
    pub fn to_pos_sequence(&self) -> Vec<Pos> {
        match self {
            LambdaTerm::Constant(pos) => vec![*pos],
            LambdaTerm::Application { function, argument } => {
                // For application, we typically want: argument first, then function
                // But this depends on word order (SOV vs SVO)
                let mut result = argument.to_pos_sequence();
                result.extend(function.to_pos_sequence());
                result
            }
            LambdaTerm::Abstraction { body, .. } => {
                // Abstraction doesn't generate POS directly
                body.to_pos_sequence()
            }
            LambdaTerm::Variable(_) => {
                // Variables don't generate POS directly
                vec![]
            }
        }
    }
    
    /// Beta reduce the lambda term (apply abstractions)
    pub fn beta_reduce(&self) -> LambdaTerm {
        match self {
            LambdaTerm::Application {
                function,
                argument,
            } => {
                match function.as_ref() {
                    LambdaTerm::Abstraction { body, .. } => {
                        // Beta reduction: (λx.M)(N) -> M[N/x]
                        // For now, just return the body (substitution would require proper handling)
                        body.beta_reduce()
                    }
                    _ => {
                        // Reduce function and argument, then apply
                        LambdaTerm::Application {
                            function: Box::new(function.beta_reduce()),
                            argument: Box::new(argument.beta_reduce()),
                        }
                    }
                }
            }
            LambdaTerm::Abstraction { var_name, var_type, body } => {
                LambdaTerm::Abstraction {
                    var_name: var_name.clone(),
                    var_type: var_type.clone(),
                    body: Box::new(body.beta_reduce()),
                }
            }
            _ => self.clone(),
        }
    }
}

impl fmt::Display for LambdaTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LambdaTerm::Variable(name) => {
                write!(f, "{}", name)
            }
            LambdaTerm::Constant(pos) => {
                write!(f, "{:?}", pos)
            }
            LambdaTerm::Application { function, argument } => {
                write!(f, "({} {})", function, argument)
            }
            LambdaTerm::Abstraction {
                var_name,
                var_type,
                body,
            } => {
                write!(f, "λ{}: {}. {}", var_name, var_type, body)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_constant_term() {
        let term = LambdaTerm::constant(Pos::N);
        let pos_seq = term.to_pos_sequence();
        assert_eq!(pos_seq, vec![Pos::N]);
    }
    
    #[test]
    fn test_application_term() {
        let func = LambdaTerm::constant(Pos::V);
        let arg = LambdaTerm::constant(Pos::N);
        let app = LambdaTerm::apply(func, arg);
        let pos_seq = app.to_pos_sequence();
        assert_eq!(pos_seq.len(), 2);
    }
}
