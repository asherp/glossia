// Lambda expression parser using pest

use crate::lambda_terms::LambdaTerm;
use crate::semantic_types::SemanticType;
use crate::types::Pos;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "lambda_parser.pest"]
pub struct LambdaExprParser;

/// Parse lambda expression from string
pub fn parse_lambda_expr(s: &str) -> Result<LambdaTerm, String> {
    let pairs = LambdaExprParser::parse(Rule::expr, s)
        .map_err(|e| format!("Parse error: {}", e))?;
    
    parse_expr(pairs)
}

fn parse_expr(mut pairs: pest::iterators::Pairs<'_, Rule>) -> Result<LambdaTerm, String> {
    let pair = pairs.next().ok_or("Empty expression")?;
    
    match pair.as_rule() {
        Rule::abstraction => parse_abstraction(pair),
        Rule::application => parse_application(pair),
        Rule::atom => parse_atom(pair),
        Rule::expr => {
            // Handle nested expr - recurse into inner pairs
            parse_expr(pair.into_inner())
        }
        _ => Err(format!("Unexpected rule: {:?}", pair.as_rule())),
    }
}

fn parse_abstraction(pair: pest::iterators::Pair<'_, Rule>) -> Result<LambdaTerm, String> {
    let inner = pair.into_inner();
    
    // Find components by rule type instead of assuming order
    let mut var_name = None;
    let mut var_type_str = None;
    let mut body_pair = None;
    
    for item in inner {
        match item.as_rule() {
            Rule::lambda_keyword => {
                // Skip
            }
            Rule::identifier => {
                var_name = Some(item.as_str().to_string());
            }
            Rule::type_expr => {
                var_type_str = Some(item.as_str().to_string());
            }
            Rule::expr => {
                body_pair = Some(item);
            }
            _ => {
                // Skip punctuation like ":" and "."
            }
        }
    }
    
    let var_name = var_name.ok_or("Missing variable name")?;
    let var_type_str = var_type_str.ok_or("Missing variable type")?;
    let var_type = SemanticType::from_str(&var_type_str)?;
    let body_pair = body_pair.ok_or("Missing abstraction body")?;
    let body = parse_expr(body_pair.into_inner())?;
    
    Ok(LambdaTerm::abstract_var(var_name, var_type, body))
}

fn parse_application(pair: pest::iterators::Pair<'_, Rule>) -> Result<LambdaTerm, String> {
    let inner = pair.into_inner();
    
    // Find components by rule type instead of assuming order
    let mut function_pair = None;
    let mut argument_pair = None;
    
    for item in inner {
        match item.as_rule() {
            Rule::atom => {
                function_pair = Some(item);
            }
            Rule::expr => {
                argument_pair = Some(item);
            }
            _ => {
                // Skip punctuation like "(" and ")"
            }
        }
    }
    
    let function_pair = function_pair.ok_or("Missing function in application")?;
    let function = parse_atom(function_pair)?;
    
    let argument_pair = argument_pair.ok_or("Missing argument in application")?;
    let argument = parse_expr(argument_pair.into_inner())?;
    
    Ok(LambdaTerm::apply(function, argument))
}

fn parse_atom(pair: pest::iterators::Pair<'_, Rule>) -> Result<LambdaTerm, String> {
    let inner_pair = pair.into_inner().next()
        .ok_or("Empty atom")?;
    
    match inner_pair.as_rule() {
        Rule::identifier => {
            let name = inner_pair.as_str().to_string();
            Ok(LambdaTerm::variable(name))
        }
        Rule::constant => {
            let pos = parse_pos_constant(inner_pair.as_str())?;
            Ok(LambdaTerm::constant(pos))
        }
        Rule::expr => {
            // Recurse into expr
            let inner = inner_pair.into_inner();
            parse_expr(inner)
        }
        _ => Err(format!("Unexpected atom rule: {:?}", inner_pair.as_rule())),
    }
}

fn parse_pos_constant(s: &str) -> Result<Pos, String> {
    Pos::from_str(s).ok_or_else(|| format!("Unknown POS constant: {}", s))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_constant() {
        let term = parse_lambda_expr("N").unwrap();
        assert!(matches!(term, LambdaTerm::Constant(Pos::N)));
    }
    
    #[test]
    fn test_parse_application() {
        let term = parse_lambda_expr("V(N)").unwrap();
        assert!(matches!(term, LambdaTerm::Application { .. }));
    }
    
    #[test]
    fn test_parse_abstraction() {
        let term = parse_lambda_expr("λx: e -> t. x").unwrap();
        assert!(matches!(term, LambdaTerm::Abstraction { .. }));
    }
}
