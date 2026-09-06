//! Function Parser for Coffee Language
//! 
//! This module handles parsing of function definitions in the Coffee programming language.
//! Coffee supports various function syntaxes including regular functions, C ABI functions,
//! error-handling functions, and external function declarations. The parser handles:
//! 
//! - Function signatures with parameters and return types
//! - Function bodies (both single-expression and multi-line blocks)
//! - C ABI linkage with the 'c fn' syntax
//! - Error handler functions
//! - External function declarations (no body)
//! - Various type annotations including parametric types (int(4)+, float(8), etc.)

use crate::coffee_debug;
use crate::parser::ty::{parse_type, parse_type_params};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{char, space0, space1},
    combinator::{map, opt},
    multi::separated_list1,
    sequence::{delimited, preceded},
    IResult, Parser,
};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ANON_ID: AtomicU64 = AtomicU64::new(0);

fn next_anon_name() -> String {
    format!("__anon_{}", NEXT_ANON_ID.fetch_add(1, Ordering::Relaxed))
}

// Import Statement for function body
use super::Statement;

/// Represents a function parameter with its name, type, and variadic status
/// 
/// This structure captures the details of a parameter in a function signature.
/// Parameters can be regular typed parameters or C varargs (`...` only).
#[derive(Debug, PartialEq, Clone)]
pub struct Parameter {
    /// The name of the parameter
    pub name: String,
    /// The type of the parameter (e.g., "int", "string", "int(4)+", etc.)
    pub param_type: String,
    /// Is this a variadic parameter (...)
    pub is_variadic: bool,
}

/// Represents the body of a function, which can be an expression, a block of statements, or no body (external)
/// 
/// Function bodies in Coffee can take several forms:
/// - A single expression for simple functions
/// - A block of statements for complex functions
/// - No body for external function declarations (like C function declarations)
#[derive(Debug, PartialEq, Clone)]
pub enum FunctionBody {
    /// Function body with a single expression
    Expression(crate::parser::expr::Expression),
    /// Function body with a block of statements
    Block(Vec<Statement>),
    /// External declaration (no body, e.g., C function declaration)
    External,
}

/// Represents a function definition in Coffee
/// 
/// This structure captures all aspects of a function definition in Coffee,
/// including its name, parameters, return type, body, and special attributes
/// like C ABI linkage and error handling.
#[derive(Debug, PartialEq, Clone)]
pub struct Function {
    /// The name of the function
    pub name: String,
    /// Generic type parameters (`fn id<T>`). Empty when the function is not generic.
    pub type_params: Vec<String>,
    /// The list of parameters for the function
    pub parameters: Vec<Parameter>,
    /// The return type of the function
    pub return_type: String,
    /// Optional error handler associated with the function
    pub error_handler: Option<String>,
    /// The body of the function (expression, block, or external)
    pub body: FunctionBody,
    /// C ABI linkage (true = C function, false = Coffee function)
    pub is_c: bool,
}

/// Parse an identifier token
/// 
/// This function parses a valid Coffee identifier, which consists of one or more
/// alphanumeric characters or underscores. Identifiers are used for function names,
/// parameter names, and other named entities in Coffee.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, identifier))` - Successfully parsed identifier and remaining input
/// * `Err(nom::Err)` - If the input does not start with a valid identifier
fn parse_identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)
}

/// Parse a function parameter
/// 
/// This function parses a single parameter in a function signature. The expected
/// format is `name: type` where name is an identifier and type is any valid
/// Coffee type expression. Only `...` is a C varargs marker. `object` is an
/// opaque pointer (`void*`), not varargs.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Parameter))` - Successfully parsed parameter and remaining input
/// * `Err(nom::Err)` - If the input does not match the parameter pattern
fn parse_parameter(input: &str) -> IResult<&str, Parameter> {
    let (input, _) = space0(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;
    let (input, param_type) = parse_type(input)?;
    let (input, _) = space0(input)?;

    let is_variadic = param_type == "...";

    Ok((
        input,
        Parameter {
            name: name.to_string(),
            param_type: param_type.to_string(),
            is_variadic,
        },
    ))
}

/// Parse function parameters list
/// 
/// This function parses the complete parameter list of a function signature,
/// which is enclosed in parentheses and contains zero or more parameters
/// separated by commas. The function handles optional parameters (empty list)
/// and properly deals with whitespace around commas.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse (starting with the opening parenthesis)
/// 
/// # Returns
/// 
/// * `Ok((remaining, Vec<Parameter>))` - Successfully parsed parameters and remaining input
/// * `Err(nom::Err)` - If the input does not match the parameters pattern
fn parse_parameters(input: &str) -> IResult<&str, Vec<Parameter>> {
    let mut parser = delimited(
        char('('),
        opt(separated_list1(
            delimited(space0, char(','), space0), // Improved comma separator handling, supports spaces
            parse_parameter
        )),
        char(')'),
    );
    let (input, params) = Parser::parse(&mut parser, input)?;
    Ok((input, params.unwrap_or_default()))
}

fn parse_indent(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c == ' ' || c == '\t')(input)
}

/// Parse a function body from input
/// 
/// This function handles parsing the body of a Coffee function, which can be:
/// - An external declaration (no body, just a function signature ending with ':')
/// - A single-expression body (function that returns the result of one expression)
/// - A multi-line block body (indented block of statements)
/// 
/// For multi-line blocks, the function handles indentation-based block detection
/// and recursively parses statements within the block, removing the indentation
/// prefix from each line before parsing.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse (starting after the function signature)
/// 
/// # Returns
/// 
/// * `Ok((remaining, FunctionBody))` - Successfully parsed function body and remaining input
/// * `Err(nom::Err)` - If the input does not match any valid function body pattern
fn missing_indented_body(input: &str) -> nom::Err<nom::error::Error<&str>> {
    nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::Fail,
    })
}

fn parse_function_body(input: &str, is_c: bool) -> IResult<&str, FunctionBody> {
    let (input_after_colon, _) = char(':')(input)?;
    let (input, _) = space0(input_after_colon)?;

    // Check for external declaration (ends with newline or EOF after colon)
    if input.is_empty() || input.starts_with('\n') {
        // Check if there's actual body content on the next line
        if input.starts_with('\n') {
            let after_newline = &input[1..];
            let first_content = after_newline.lines().find(|l| !l.trim().is_empty());
            match first_content {
                None => {
                    // `c fn` prototypes may omit a body; Coffee `fn` needs an indented body.
                    if is_c {
                        return Ok((input, FunctionBody::External));
                    }
                    return Err(missing_indented_body(input));
                }
                Some(line) => {
                    // Indented lines are the function body (including nested `fn`).
                    // A following *top-level* `fn` / `c fn` / `main` means this decl is external.
                    let indented = line.starts_with(' ') || line.starts_with('\t');
                    if !indented {
                        let trimmed = line.trim_start();
                        if trimmed.starts_with("fn ")
                            || trimmed.starts_with("c ")
                            || trimmed.starts_with("main")
                        {
                            return Ok((input, FunctionBody::External));
                        }
                    }
                }
            }
        } else {
            // Just ":" with nothing after — external only for `c fn`
            if is_c {
                return Ok((input, FunctionBody::External));
            }
            return Err(missing_indented_body(input));
        }
    }

    // Original block/expression parsing
    if input.starts_with('\n') {
        let after_newline = &input[1..];
        coffee_debug!("DEBUG: parse_function_body: after newline='{}'", after_newline);
        if let Ok((_, indent)) = parse_indent(after_newline) {
            coffee_debug!("DEBUG: parse_function_body: indent='{}'", indent);
            // Collect all body lines WITH indentation removed
            let mut body_lines_vec = Vec::new();
            let mut consumed = 0;

            for line in after_newline.lines() {
                coffee_debug!("DEBUG: parse_function_body: line='{}', starts_with_indent={}", line, line.starts_with(indent));
                if line.is_empty() || line.starts_with(indent) {
                    // Remove the indentation prefix
                    let content = if line.starts_with(indent) {
                        &line[indent.len()..]
                    } else {
                        line
                    };
                    body_lines_vec.push(content);
                    consumed += line.len() + 1;
                } else {
                    break;
                }
            }

            if consumed > 0 {
                consumed = consumed.saturating_sub(1);
            }

            let remaining = if consumed < after_newline.len() {
                &after_newline[consumed..]
            } else {
                ""
            };

            // Parse body lines into Statement vector
            let mut statements = Vec::new();
            let mut i = 0;
            while i < body_lines_vec.len() {
                let line_original = body_lines_vec[i];

                // Strip inline comments before parsing
                let line = strip_inline_comment(line_original).trim();

                // Skip empty lines and comments
                if line.is_empty() || line.starts_with("/#") {
                    i += 1;
                    continue;
                }

                // Try multiline statement first
                coffee_debug!("DEBUG: parse_function_body: trying parse_multiline_statement for line='{}'", line);
                if let Some((stmt, lines_consumed)) = super::parse_multiline_statement(&body_lines_vec[i..]) {
                    coffee_debug!("DEBUG: parse_function_body: parse_multiline_statement succeeded, lines_consumed={}", lines_consumed);
                    statements.push(stmt);
                    i += lines_consumed;
                } else if line.starts_with("let ") {
                    coffee_debug!("DEBUG: parse_function_body: line='{}', starts with let", line);
                    // Check for multiline variable declarations (struct literals, etc.)
                    if let Some((multiline_content, lines_consumed)) = super::collect_multiline_variable_decl(&body_lines_vec[i..]) {
                        if let Ok((remaining, var_decl)) = super::var::parse_variable_decl(&multiline_content) {
                            if remaining.trim().is_empty() {
                                statements.push(Statement::VariableDecl(var_decl));
                                i += lines_consumed;
                                continue;
                            }
                        }
                    }
                    // Fall back to single-line parsing
                    if let Some(stmt) = super::parse_single_line_statement(line) {
                        statements.push(stmt);
                        i += 1;
                    } else {
                        return Err(nom::Err::Error(nom::error::Error {
                            input,
                            code: nom::error::ErrorKind::Fail,
                        }));
                    }
                } else {
                    coffee_debug!("DEBUG: parse_function_body: line='{}', calling parse_single_line_statement", line);
                    if let Some(stmt) = super::parse_single_line_statement(line) {
                        statements.push(stmt);
                        i += 1;
                    } else {
                        return Err(nom::Err::Error(nom::error::Error {
                            input,
                            code: nom::error::ErrorKind::Fail,
                        }));
                    }
                }
            }

            return Ok((
                remaining,
                FunctionBody::Block(statements),
            ));
        }
    }

    let (input, expr_str) = {
        let mut parser = alt((
            map(take_while1(|c: char| !matches!(c, '\n' | '\r')), |s: &str| s.trim().to_string()),
            map(tag(""), |_| String::new()),
        ));
        Parser::parse(&mut parser, input)?
    };

    let expr = if expr_str.is_empty() {
        super::expr::Expression::Literal(String::new())
    } else {
        super::expr::assignment_rhs(&expr_str).map_err(|_| {
            nom::Err::Error(nom::error::Error {
                input,
                code: nom::error::ErrorKind::Fail,
            })
        })?
    };

    Ok((
        input,
        FunctionBody::Expression(expr),
    ))
}

/// Parse a complete Coffee function definition
/// 
/// This is the main function parser that handles all Coffee function syntax,
/// including:
/// - Regular Coffee functions: `fn name(params) => return_type: body`
/// - C ABI functions: `c fn name(params) => return_type: body`
/// - Functions with error listeners: `fn name(params) #on_err => return_type: body`
/// - External function declarations: `fn name(params) => return_type:`
/// 
/// The parser identifies the function type based on the 'c' prefix, extracts
/// the name, parameters, return type, optional error handler, and function body.
/// 
/// # Arguments
/// 
/// * `input` - The input string containing a function definition to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Function))` - Successfully parsed function and remaining input
/// * `Err(nom::Err)` - If the input does not match any valid function pattern
pub fn parse_function(input: &str) -> IResult<&str, Function> {
    // Check for C ABI prefix: "c fn"
    let (input, is_c) = if input.starts_with("c ") {
        let (input, _) = tag("c")(input)?;
        let (input, _) = space1(input)?;
        (input, true)
    } else {
        (input, false)
    };

    let (input, _) = tag("fn")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, type_params) = parse_type_params(input)?;
    let (input, _) = space0(input)?;
    let (input, parameters) = parse_parameters(input)?;
    let (input, _) = space0(input)?;

    // Parse optional error handler (after parameters, before =>)
    let (input, error_handler) = {
        let mut parser = opt(preceded(
            char('#'),
            parse_identifier,
        ));
        Parser::parse(&mut parser, input)?
    };

    let (input, _) = space0(input)?;
    let (input, _) = tag("=>")(input)?;
    let (input, _) = space0(input)?;
    let (input, return_type) = parse_type(input)?;
    let (input, _) = space0(input)?;
    let (input, body) = parse_function_body(input, is_c)?;

    Ok((
        input,
        Function {
            name: name.to_string(),
            type_params,
            parameters,
            return_type: return_type.to_string(),
            error_handler: error_handler.map(|s| s.to_string()),
            body,
            is_c,
        },
    ))
}

/// `fn(params) => R:` with an indented body. Synthetic LLVM name `__anon_N`.
pub fn parse_anonymous_function(input: &str) -> IResult<&str, Function> {
    let (input, _) = space0(input)?;
    let (input, _) = tag("fn")(input)?;
    let (input, _) = space0(input)?;
    let (input, parameters) = parse_parameters(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = tag("=>")(input)?;
    let (input, _) = space0(input)?;
    let (input, return_type) = parse_type(input)?;
    let (input, _) = space0(input)?;
    let (input, body) = parse_function_body(input, false)?;
    Ok((
        input,
        Function {
            name: next_anon_name(),
            type_params: vec![],
            parameters,
            return_type: return_type.to_string(),
            error_handler: None,
            body,
            is_c: false,
        },
    ))
}

/// LLVM / `hir_fns` map key for a function declaration.
///
/// If `name` is not taken, it is used as-is (same as `declare_function` inserting
/// `func.name`). On collision, `{parent}_{name}`, then `{parent}_{name}_{i}`.
pub fn unique_function_key(name: &str, parent: Option<&str>, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let base = match parent {
        Some(p) if !p.is_empty() => format!("{}_{}", p, name),
        _ => format!("{}_1", name),
    };
    if !taken(&base) {
        return base;
    }
    let mut i = 2u32;
    loop {
        let cand = format!("{}_{}", base, i);
        if !taken(&cand) {
            return cand;
        }
        i = i.saturating_add(1);
        if i == u32::MAX {
            return cand;
        }
    }
}

/// Strip `/#/` comments that are not inside quotes.
fn strip_inline_comment(s: &str) -> &str {
    if let Some(start_pos) = s.find("/#/") {
        // Check if inside string literal
        let before = &s[..start_pos];
        let quote_count = before.matches('"').count() + before.matches('\'').count();
        if quote_count % 2 == 0 {
            // Not inside string - this is a comment
            return before;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Statement;

    #[test]
    fn unique_function_key_keeps_free_name() {
        assert_eq!(unique_function_key("helper", Some("a"), |_| false), "helper");
    }

    #[test]
    fn unique_function_key_prefixes_parent_on_collision() {
        assert_eq!(
            unique_function_key("helper", Some("b"), |n| n == "helper"),
            "b_helper"
        );
    }

    #[test]
    fn unique_function_key_suffixes_when_parent_prefix_taken() {
        assert_eq!(
            unique_function_key("helper", Some("b"), |n| n == "helper" || n == "b_helper"),
            "b_helper_2"
        );
    }

    #[test]
    fn indented_nested_fn_is_block_not_external() {
        let src = "fn main() => int:\n    fn helper() => int:\n        return 1\n    return 0\n";
        let (_, f) = parse_function(src).expect("parse outer");
        match f.body {
            FunctionBody::Block(stmts) => {
                assert!(
                    stmts.iter().any(|s| matches!(s, Statement::Function(inner) if inner.name == "helper")),
                    "nested helper missing: {:?}",
                    stmts
                );
            }
            other => panic!("expected block body, got {:?}", other),
        }
    }

    #[test]
    fn following_top_level_fn_is_still_external() {
        let src = "fn proto() => int:\nfn other() => int:\n    return 0\n";
        let (_, f) = parse_function(src).expect("parse proto");
        assert!(
            matches!(f.body, FunctionBody::External),
            "{:?}",
            f.body
        );
    }

    #[test]
    fn coffee_fn_without_indented_body_is_error() {
        assert!(parse_function("fn main() => int:\n").is_err());
        assert!(parse_function("fn main() => int:").is_err());
    }

    #[test]
    fn c_fn_without_body_is_external() {
        let (_, f) = parse_function("c fn foo() => int:\n").expect("parse c fn proto");
        assert!(matches!(f.body, FunctionBody::External), "{:?}", f.body);
    }

    #[test]
    fn array_parameter_type_is_parsed() {
        let src = "fn find_value(arr: [int; 5], target: int) => int:\n    return 0\n";
        let (_, f) = parse_function(src).expect("parse");
        assert_eq!(f.parameters.len(), 2);
        assert_eq!(f.parameters[0].name, "arr");
        assert_eq!(f.parameters[0].param_type, "[int; 5]");
        assert_eq!(f.parameters[1].name, "target");
        assert_eq!(f.parameters[1].param_type, "int");
    }

    #[test]
    fn unparseable_body_line_is_error() {
        let src = "fn main() => int:\n    @@@\n    return 0\n";
        assert!(parse_function(src).is_err());
    }

    #[test]
    fn nested_generic_parameter_type_is_complete() {
        let src = "fn id(x: List<List<int>>, y: int) => int:\n    return 0\n";
        let (_, f) = parse_function(src).expect("parse");
        assert_eq!(f.parameters[0].param_type, "List<List<int>>");
        assert_eq!(f.parameters[1].param_type, "int");
    }

    #[test]
    fn map_parameter_type_not_truncated() {
        let src = "fn use_map(m: Map<str, List<int>>) => int:\n    return 0\n";
        let (_, f) = parse_function(src).expect("parse");
        assert_eq!(f.parameters[0].param_type, "Map<str, List<int>>");
    }

    #[test]
    fn fn_type_params_after_name() {
        let src = "fn id<T>(x: T) => T:\n    return x\n";
        let (_, f) = parse_function(src).expect("parse");
        assert_eq!(f.type_params, vec!["T".to_string()]);
        assert_eq!(f.parameters[0].param_type, "T");
        assert_eq!(f.return_type, "T");
    }
}
