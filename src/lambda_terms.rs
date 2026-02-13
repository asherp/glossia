// Lambda term representation for Montague Grammar with CCG combinators
//
// Implements the lambda calculus core plus the five standard combinators
// from Combinatory Categorial Grammar (CCG):
//
//   B f g x   = f(g(x))        -- forward composition
//   C f x y   = f(y)(x)        -- permutation (word order)
//   S f g x   = f(x)(g(x))     -- distribution (argument sharing)
//   T x f     = f(x)           -- type raising
//   I x       = x              -- identity
//
// Sentence construction is proof search: a POS sequence of length k that
// type-checks as `t` is a proof that k terminals compose into a sentence.

use crate::semantic_types::SemanticType;
use crate::types::Pos;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// CCG combinators
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Combinator {
    /// B f g x = f(g(x)) — forward composition: (b→c) → (a→b) → a → c
    B,
    /// C f x y = f(y)(x) — permutation: (a→b→c) → b → a → c
    C,
    /// S f g x = f(x)(g(x)) — distribution: (a→b→c) → (a→b) → a → c
    S,
    /// T x f = f(x) — type raising: a → (a→b) → b
    T,
    /// I x = x — identity: a → a
    I,
}

impl fmt::Display for Combinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Combinator::B => write!(f, "B"),
            Combinator::C => write!(f, "C"),
            Combinator::S => write!(f, "S"),
            Combinator::T => write!(f, "T"),
            Combinator::I => write!(f, "I"),
        }
    }
}

/// Lambda term representation with CCG combinators
#[derive(Clone, Debug)]
pub enum LambdaTerm {
    /// Variable reference (bound by abstraction)
    Variable(String),
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
    /// CCG combinator (B, C, S, T, I)
    Combinator(Combinator),
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

    /// Create a combinator term
    pub fn combinator(c: Combinator) -> Self {
        LambdaTerm::Combinator(c)
    }

    // --- Combinator constructors (convenience) ---

    /// B f g — forward composition: f ∘ g
    pub fn compose(f: LambdaTerm, g: LambdaTerm) -> Self {
        LambdaTerm::apply(LambdaTerm::apply(LambdaTerm::combinator(Combinator::B), f), g)
    }

    /// C f — flip argument order
    pub fn flip(f: LambdaTerm) -> Self {
        LambdaTerm::apply(LambdaTerm::combinator(Combinator::C), f)
    }

    /// T x — type raise x
    pub fn type_raise(x: LambdaTerm) -> Self {
        LambdaTerm::apply(LambdaTerm::combinator(Combinator::T), x)
    }

    // --- Free variables ---

    /// Collect all free variables in this term
    pub fn free_vars(&self) -> HashSet<String> {
        match self {
            LambdaTerm::Variable(name) => {
                let mut s = HashSet::new();
                s.insert(name.clone());
                s
            }
            LambdaTerm::Constant(_) | LambdaTerm::Combinator(_) => HashSet::new(),
            LambdaTerm::Application { function, argument } => {
                let mut fv = function.free_vars();
                fv.extend(argument.free_vars());
                fv
            }
            LambdaTerm::Abstraction {
                var_name, body, ..
            } => {
                let mut fv = body.free_vars();
                fv.remove(var_name);
                fv
            }
        }
    }

    /// Generate a fresh variable name not in `avoid`
    fn fresh_var(base: &str, avoid: &HashSet<String>) -> String {
        if !avoid.contains(base) {
            return base.to_string();
        }
        let mut i = 0;
        loop {
            let candidate = format!("{}{}", base, i);
            if !avoid.contains(&candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    // --- Substitution ---

    /// Capture-avoiding substitution: self[replacement/var]
    ///
    /// Replaces all free occurrences of `var` in `self` with `replacement`,
    /// renaming bound variables as needed to avoid capture.
    pub fn substitute(&self, var: &str, replacement: &LambdaTerm) -> LambdaTerm {
        match self {
            LambdaTerm::Variable(name) => {
                if name == var {
                    replacement.clone()
                } else {
                    self.clone()
                }
            }
            LambdaTerm::Constant(_) | LambdaTerm::Combinator(_) => self.clone(),
            LambdaTerm::Application { function, argument } => LambdaTerm::Application {
                function: Box::new(function.substitute(var, replacement)),
                argument: Box::new(argument.substitute(var, replacement)),
            },
            LambdaTerm::Abstraction {
                var_name,
                var_type,
                body,
            } => {
                if var_name == var {
                    // var is shadowed by this binding — no substitution in body
                    self.clone()
                } else if replacement.free_vars().contains(var_name) {
                    // Would capture: rename the bound variable first
                    let mut avoid = body.free_vars();
                    avoid.extend(replacement.free_vars());
                    avoid.insert(var.to_string());
                    let fresh = Self::fresh_var(var_name, &avoid);
                    let renamed_body =
                        body.substitute(var_name, &LambdaTerm::Variable(fresh.clone()));
                    LambdaTerm::Abstraction {
                        var_name: fresh,
                        var_type: var_type.clone(),
                        body: Box::new(renamed_body.substitute(var, replacement)),
                    }
                } else {
                    // Safe to substitute directly
                    LambdaTerm::Abstraction {
                        var_name: var_name.clone(),
                        var_type: var_type.clone(),
                        body: Box::new(body.substitute(var, replacement)),
                    }
                }
            }
        }
    }

    // --- Reduction ---

    /// Beta-reduce: (λx.M)(N) → M[N/x]
    /// Also reduces combinator applications when fully saturated.
    /// Performs one pass of reduction (call repeatedly for full normalization).
    pub fn beta_reduce(&self) -> LambdaTerm {
        match self {
            LambdaTerm::Application { function, argument } => {
                // First try combinator reduction
                if let Some(reduced) = self.reduce_combinator() {
                    return reduced;
                }

                let func_reduced = function.beta_reduce();
                let arg_reduced = argument.beta_reduce();

                match &func_reduced {
                    LambdaTerm::Abstraction {
                        var_name, body, ..
                    } => {
                        // Beta reduction: (λx.M)(N) → M[N/x]
                        body.substitute(var_name, &arg_reduced).beta_reduce()
                    }
                    _ => LambdaTerm::Application {
                        function: Box::new(func_reduced),
                        argument: Box::new(arg_reduced),
                    },
                }
            }
            LambdaTerm::Abstraction {
                var_name,
                var_type,
                body,
            } => LambdaTerm::Abstraction {
                var_name: var_name.clone(),
                var_type: var_type.clone(),
                body: Box::new(body.beta_reduce()),
            },
            _ => self.clone(),
        }
    }

    /// Normalize: reduce until no more reductions apply (with fuel limit).
    pub fn normalize(&self, max_steps: usize) -> LambdaTerm {
        let mut current = self.clone();
        for _ in 0..max_steps {
            let next = current.beta_reduce();
            let next_str = format!("{}", next);
            let curr_str = format!("{}", current);
            if next_str == curr_str {
                return next;
            }
            current = next;
        }
        current
    }

    /// Try to reduce a combinator application.
    /// Returns Some(reduced) if the outermost application is a fully-saturated combinator.
    fn reduce_combinator(&self) -> Option<LambdaTerm> {
        // Peel off nested applications to find the head and its arguments
        let (head, args) = self.uncurry();

        match head {
            LambdaTerm::Combinator(c) => {
                match c {
                    Combinator::I if args.len() >= 1 => {
                        // I x = x
                        let x = &args[0];
                        let mut result = (*x).clone();
                        // Re-apply remaining arguments
                        for arg in &args[1..] {
                            result = LambdaTerm::apply(result, (*arg).clone());
                        }
                        Some(result)
                    }
                    Combinator::T if args.len() >= 2 => {
                        // T x f = f(x)
                        let x = &args[0];
                        let f = &args[1];
                        let mut result = LambdaTerm::apply((*f).clone(), (*x).clone());
                        for arg in &args[2..] {
                            result = LambdaTerm::apply(result, (*arg).clone());
                        }
                        Some(result)
                    }
                    Combinator::B if args.len() >= 3 => {
                        // B f g x = f(g(x))
                        let f = &args[0];
                        let g = &args[1];
                        let x = &args[2];
                        let gx = LambdaTerm::apply((*g).clone(), (*x).clone());
                        let mut result = LambdaTerm::apply((*f).clone(), gx);
                        for arg in &args[3..] {
                            result = LambdaTerm::apply(result, (*arg).clone());
                        }
                        Some(result)
                    }
                    Combinator::C if args.len() >= 3 => {
                        // C f x y = f(y)(x)
                        let f = &args[0];
                        let x = &args[1];
                        let y = &args[2];
                        let fy = LambdaTerm::apply((*f).clone(), (*y).clone());
                        let mut result = LambdaTerm::apply(fy, (*x).clone());
                        for arg in &args[3..] {
                            result = LambdaTerm::apply(result, (*arg).clone());
                        }
                        Some(result)
                    }
                    Combinator::S if args.len() >= 3 => {
                        // S f g x = f(x)(g(x))
                        let f = &args[0];
                        let g = &args[1];
                        let x = &args[2];
                        let fx = LambdaTerm::apply((*f).clone(), (*x).clone());
                        let gx = LambdaTerm::apply((*g).clone(), (*x).clone());
                        let mut result = LambdaTerm::apply(fx, gx);
                        for arg in &args[3..] {
                            result = LambdaTerm::apply(result, (*arg).clone());
                        }
                        Some(result)
                    }
                    _ => None, // Not enough arguments yet (partial application)
                }
            }
            _ => None,
        }
    }

    /// Decompose a curried application into (head, [arg1, arg2, ...])
    /// e.g. ((B f) g) x → (B, [f, g, x])
    fn uncurry(&self) -> (&LambdaTerm, Vec<&LambdaTerm>) {
        let mut args = Vec::new();
        let mut current = self;
        while let LambdaTerm::Application { function, argument } = current {
            args.push(argument.as_ref());
            current = function.as_ref();
        }
        args.reverse();
        (current, args)
    }

    // --- Type inference ---

    /// Infer the semantic type of this term
    pub fn infer_type(
        &self,
        context: &HashMap<String, SemanticType>,
    ) -> Result<SemanticType, String> {
        match self {
            LambdaTerm::Variable(name) => context
                .get(name)
                .ok_or_else(|| format!("Unknown variable: {}", name))
                .map(|t| t.clone()),
            LambdaTerm::Constant(pos) => {
                Ok(crate::semantic_types::pos_to_semantic_type(pos))
            }
            LambdaTerm::Combinator(c) => {
                // Combinators have polymorphic types — we can't give a monomorphic
                // type without knowing the arguments. Return an error suggesting
                // the combinator should be applied before type inference.
                Err(format!(
                    "Cannot infer monomorphic type for bare combinator {}. \
                     Apply it to arguments first, or use infer_combinator_app_type.",
                    c
                ))
            }
            LambdaTerm::Application { function, argument } => {
                let func_type = function.infer_type(context)?;
                let arg_type = argument.infer_type(context)?;
                func_type.can_apply_to(&arg_type).ok_or_else(|| {
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

    // --- POS sequence extraction ---

    /// Generate POS sequence from a lambda term.
    ///
    /// The derivation tree determines word order:
    /// - Application f(a): function POS first, then argument POS (head-initial/SVO)
    /// - Use C combinator to flip when needed (SOV, etc.)
    ///
    /// For a fully-reduced term, this extracts the POS tags in linearization order.
    pub fn to_pos_sequence(&self) -> Vec<Pos> {
        match self {
            LambdaTerm::Constant(pos) => vec![*pos],
            LambdaTerm::Combinator(_) => vec![], // Unapplied combinators produce no POS
            LambdaTerm::Application { function, argument } => {
                // Head-initial linearization: function before argument
                // The C combinator handles reordering at the term level,
                // so by the time we extract POS, the tree structure is correct.
                let mut result = function.to_pos_sequence();
                result.extend(argument.to_pos_sequence());
                result
            }
            LambdaTerm::Abstraction { body, .. } => {
                // Abstraction is transparent — just traverse the body
                body.to_pos_sequence()
            }
            LambdaTerm::Variable(_) => {
                // Variables don't generate POS directly
                vec![]
            }
        }
    }

    /// Check if this term is a normal form (no further reductions possible)
    pub fn is_normal_form(&self) -> bool {
        match self {
            LambdaTerm::Variable(_) | LambdaTerm::Constant(_) | LambdaTerm::Combinator(_) => true,
            LambdaTerm::Application { function, argument } => {
                // Check if this is a beta-redex
                if matches!(function.as_ref(), LambdaTerm::Abstraction { .. }) {
                    return false;
                }
                // Check if this is a fully-saturated combinator
                if self.reduce_combinator().is_some() {
                    return false;
                }
                function.is_normal_form() && argument.is_normal_form()
            }
            LambdaTerm::Abstraction { body, .. } => body.is_normal_form(),
        }
    }
}

impl fmt::Display for LambdaTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LambdaTerm::Variable(name) => write!(f, "{}", name),
            LambdaTerm::Constant(pos) => write!(f, "{:?}", pos),
            LambdaTerm::Combinator(c) => write!(f, "{}", c),
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
        assert_eq!(pos_seq, vec![Pos::V, Pos::N]);
    }

    // --- Substitution tests ---

    #[test]
    fn test_substitute_variable() {
        // x[N/x] = N
        let var = LambdaTerm::variable("x".into());
        let replacement = LambdaTerm::constant(Pos::N);
        let result = var.substitute("x", &replacement);
        assert!(matches!(result, LambdaTerm::Constant(Pos::N)));
    }

    #[test]
    fn test_substitute_different_variable() {
        // y[N/x] = y
        let var = LambdaTerm::variable("y".into());
        let replacement = LambdaTerm::constant(Pos::N);
        let result = var.substitute("x", &replacement);
        assert!(matches!(result, LambdaTerm::Variable(ref name) if name == "y"));
    }

    #[test]
    fn test_substitute_shadowed() {
        // (λx:e. x)[N/x] = (λx:e. x) — binding shadows, no substitution
        let term = LambdaTerm::abstract_var(
            "x".into(),
            SemanticType::Entity,
            LambdaTerm::variable("x".into()),
        );
        let replacement = LambdaTerm::constant(Pos::N);
        let result = term.substitute("x", &replacement);
        // Body should still be Variable("x"), not Constant(N)
        if let LambdaTerm::Abstraction { body, .. } = &result {
            assert!(matches!(body.as_ref(), LambdaTerm::Variable(ref n) if n == "x"));
        } else {
            panic!("Expected Abstraction");
        }
    }

    #[test]
    fn test_substitute_capture_avoiding() {
        // (λy:e. x)[y/x] should rename y to avoid capture
        // Result: (λy0:e. y) — NOT (λy:e. y) which would capture
        let term = LambdaTerm::abstract_var(
            "y".into(),
            SemanticType::Entity,
            LambdaTerm::variable("x".into()),
        );
        let replacement = LambdaTerm::variable("y".into());
        let result = term.substitute("x", &replacement);
        if let LambdaTerm::Abstraction {
            var_name, body, ..
        } = &result
        {
            assert_ne!(var_name, "y", "Should have renamed to avoid capture");
            // Body should reference the replacement "y", not the bound variable
            assert!(matches!(body.as_ref(), LambdaTerm::Variable(ref n) if n == "y"));
        } else {
            panic!("Expected Abstraction");
        }
    }

    // --- Beta reduction tests ---

    #[test]
    fn test_beta_reduce_simple() {
        // (λx:e. x)(N) → N
        let id = LambdaTerm::abstract_var(
            "x".into(),
            SemanticType::Entity,
            LambdaTerm::variable("x".into()),
        );
        let app = LambdaTerm::apply(id, LambdaTerm::constant(Pos::N));
        let result = app.beta_reduce();
        assert!(matches!(result, LambdaTerm::Constant(Pos::N)));
    }

    #[test]
    fn test_beta_reduce_nested() {
        // (λf:(e->t). λx:e. f(x))(V)(N) → V(N)
        let inner = LambdaTerm::abstract_var(
            "x".into(),
            SemanticType::Entity,
            LambdaTerm::apply(
                LambdaTerm::variable("f".into()),
                LambdaTerm::variable("x".into()),
            ),
        );
        let outer = LambdaTerm::abstract_var(
            "f".into(),
            SemanticType::Function {
                domain: Box::new(SemanticType::Entity),
                codomain: Box::new(SemanticType::Truth),
            },
            inner,
        );
        let app1 = LambdaTerm::apply(outer, LambdaTerm::constant(Pos::V));
        let app2 = LambdaTerm::apply(app1, LambdaTerm::constant(Pos::N));
        let result = app2.normalize(10);
        let pos = result.to_pos_sequence();
        assert_eq!(pos, vec![Pos::V, Pos::N]);
    }

    // --- Combinator reduction tests ---

    #[test]
    fn test_identity_combinator() {
        // I(N) = N
        let term = LambdaTerm::apply(
            LambdaTerm::combinator(Combinator::I),
            LambdaTerm::constant(Pos::N),
        );
        let result = term.beta_reduce();
        assert!(matches!(result, LambdaTerm::Constant(Pos::N)));
    }

    #[test]
    fn test_type_raise_combinator() {
        // T(N)(V) = V(N)
        let term = LambdaTerm::apply(
            LambdaTerm::apply(
                LambdaTerm::combinator(Combinator::T),
                LambdaTerm::constant(Pos::N),
            ),
            LambdaTerm::constant(Pos::V),
        );
        let result = term.beta_reduce();
        // Should be V(N)
        if let LambdaTerm::Application { function, argument } = &result {
            assert!(matches!(function.as_ref(), LambdaTerm::Constant(Pos::V)));
            assert!(matches!(argument.as_ref(), LambdaTerm::Constant(Pos::N)));
        } else {
            panic!("Expected Application, got: {}", result);
        }
    }

    #[test]
    fn test_compose_combinator() {
        // B(Det)(N)(x) = Det(N(x))
        let term = LambdaTerm::apply(
            LambdaTerm::apply(
                LambdaTerm::apply(
                    LambdaTerm::combinator(Combinator::B),
                    LambdaTerm::constant(Pos::Det),
                ),
                LambdaTerm::constant(Pos::N),
            ),
            LambdaTerm::variable("x".into()),
        );
        let result = term.beta_reduce();
        // Should be Det(N(x))
        if let LambdaTerm::Application { function, argument } = &result {
            assert!(matches!(function.as_ref(), LambdaTerm::Constant(Pos::Det)));
            if let LambdaTerm::Application {
                function: inner_f,
                argument: inner_a,
            } = argument.as_ref()
            {
                assert!(matches!(inner_f.as_ref(), LambdaTerm::Constant(Pos::N)));
                assert!(matches!(inner_a.as_ref(), LambdaTerm::Variable(ref n) if n == "x"));
            } else {
                panic!("Expected inner Application");
            }
        } else {
            panic!("Expected Application, got: {}", result);
        }
    }

    #[test]
    fn test_flip_combinator() {
        // C(V)(N)(Adj) = V(Adj)(N)
        let term = LambdaTerm::apply(
            LambdaTerm::apply(
                LambdaTerm::apply(
                    LambdaTerm::combinator(Combinator::C),
                    LambdaTerm::constant(Pos::V),
                ),
                LambdaTerm::constant(Pos::N),
            ),
            LambdaTerm::constant(Pos::Adj),
        );
        let result = term.beta_reduce();
        // Should be V(Adj)(N)
        if let LambdaTerm::Application { function, argument } = &result {
            // outer argument is N
            assert!(matches!(argument.as_ref(), LambdaTerm::Constant(Pos::N)));
            // function is V(Adj)
            if let LambdaTerm::Application {
                function: inner_f,
                argument: inner_a,
            } = function.as_ref()
            {
                assert!(matches!(inner_f.as_ref(), LambdaTerm::Constant(Pos::V)));
                assert!(matches!(inner_a.as_ref(), LambdaTerm::Constant(Pos::Adj)));
            } else {
                panic!("Expected inner Application");
            }
        } else {
            panic!("Expected Application, got: {}", result);
        }
    }

    #[test]
    fn test_s_combinator() {
        // S(V)(N)(x) = V(x)(N(x))
        let term = LambdaTerm::apply(
            LambdaTerm::apply(
                LambdaTerm::apply(
                    LambdaTerm::combinator(Combinator::S),
                    LambdaTerm::constant(Pos::V),
                ),
                LambdaTerm::constant(Pos::N),
            ),
            LambdaTerm::variable("x".into()),
        );
        let result = term.beta_reduce();
        // Should be V(x)(N(x))
        if let LambdaTerm::Application { function, argument } = &result {
            // argument is N(x)
            if let LambdaTerm::Application {
                function: n_f,
                argument: n_a,
            } = argument.as_ref()
            {
                assert!(matches!(n_f.as_ref(), LambdaTerm::Constant(Pos::N)));
                assert!(matches!(n_a.as_ref(), LambdaTerm::Variable(ref n) if n == "x"));
            } else {
                panic!("Expected N(x)");
            }
            // function is V(x)
            if let LambdaTerm::Application {
                function: v_f,
                argument: v_a,
            } = function.as_ref()
            {
                assert!(matches!(v_f.as_ref(), LambdaTerm::Constant(Pos::V)));
                assert!(matches!(v_a.as_ref(), LambdaTerm::Variable(ref n) if n == "x"));
            } else {
                panic!("Expected V(x)");
            }
        } else {
            panic!("Expected Application, got: {}", result);
        }
    }

    #[test]
    fn test_partial_application_no_reduce() {
        // B(Det) — only 1 argument, B needs 3. Should not reduce.
        let term = LambdaTerm::apply(
            LambdaTerm::combinator(Combinator::B),
            LambdaTerm::constant(Pos::Det),
        );
        assert!(term.reduce_combinator().is_none());
    }

    #[test]
    fn test_is_normal_form() {
        assert!(LambdaTerm::constant(Pos::N).is_normal_form());
        assert!(LambdaTerm::combinator(Combinator::B).is_normal_form());

        // I(N) is not normal — it reduces to N
        let redex = LambdaTerm::apply(
            LambdaTerm::combinator(Combinator::I),
            LambdaTerm::constant(Pos::N),
        );
        assert!(!redex.is_normal_form());
    }

    // --- Convenience constructor tests ---

    #[test]
    fn test_compose_convenience() {
        // LambdaTerm::compose(Det, N) applied to x should give Det(N(x))
        let composed = LambdaTerm::compose(
            LambdaTerm::constant(Pos::Det),
            LambdaTerm::constant(Pos::N),
        );
        let applied = LambdaTerm::apply(composed, LambdaTerm::variable("x".into()));
        let result = applied.normalize(10);
        let display = format!("{}", result);
        assert!(
            display.contains("Det"),
            "Expected Det in result: {}",
            display
        );
        assert!(display.contains("N"), "Expected N in result: {}", display);
    }

    #[test]
    fn test_free_vars() {
        // λx:e. f(x) has free var {f}
        let term = LambdaTerm::abstract_var(
            "x".into(),
            SemanticType::Entity,
            LambdaTerm::apply(
                LambdaTerm::variable("f".into()),
                LambdaTerm::variable("x".into()),
            ),
        );
        let fv = term.free_vars();
        assert!(fv.contains("f"));
        assert!(!fv.contains("x"));
    }
}
