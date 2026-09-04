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
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till, take_while1},
    character::complete::{char, space0, space1},
    combinator::{map, opt, recognize},
    multi::separated_list1,
    sequence::{delimited, preceded},
    IResult, Parser,
};

// Import Statement for function body
use super::Statement;

/// Represents a function parameter with its name, type, and variadic status
/// 
/// This structure captures the details of a parameter in a function signature.
/// Parameters can be regular typed parameters or variadic parameters (like ... or object).
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

/// Represents a function call in Coffee
/// 
/// This structure captures the details of a function call, including the function name
/// and its arguments. Currently, arguments are stored as string representations.
#[derive(Debug, PartialEq, Clone)]
#[allow(dead_code)]
pub struct FunctionCall {
    /// The name of the function being called
    pub name: String,
    /// The list of arguments to the function (as string representations)
    pub args: Vec<String>, // For now, using string representations of arguments
}

impl Function {
    /// Check if this function is an error handler
    /// 
    /// Error handlers in Coffee have a specific signature:
    /// - First parameter must be of type "Error"
    /// - Second parameter must be a tuple type (starts with '(' and ends with ')')
    /// 
    /// This signature allows the function to handle errors and potentially return
    /// a result in a tuple format.
    /// 
    /// # Returns
    /// 
    /// * `true` if the function matches the error handler signature
    /// * `false` otherwise
    pub fn is_error_handler(&self) -> bool {
        self.parameters.len() >= 2
            && self.parameters[0].param_type == "Error"
            && self.parameters[1].param_type.starts_with('(')
            && self.parameters[1].param_type.ends_with(')')
    }

    /// Check if this is a C function (uses C ABI)
    /// 
    /// This method checks if the function uses C ABI linkage, which means it can be
    /// called from C code or can call C functions. C functions are declared using
    /// the 'c fn' syntax in Coffee.
    /// 
    /// # Returns
    /// 
    /// * `true` if the function uses C ABI linkage
    /// * `false` otherwise
    pub fn is_extern_c(&self) -> bool {
        self.is_c
    }

    /// Normalize types in the function signature
    /// 
    /// This method converts common type names to their more specific equivalents:
    /// - "int" becomes "int(4)+" (4-byte signed integer)
    /// - "float" becomes "float(8)" (8-byte floating point)
    /// 
    /// The normalization helps ensure consistent type representation throughout
    /// the compilation process.
    /// 
    /// # Returns
    /// 
    /// A new Function instance with normalized types
    pub fn normalize_types(&self) -> Function {
        Function {
            parameters: self.parameters.iter().map(|p| Parameter {
                name: p.name.clone(),
                param_type: normalize_type(&p.param_type),
                is_variadic: p.is_variadic,
            }).collect(),
            return_type: normalize_type(&self.return_type),
            ..self.clone()
        }
    }

    /// Check if the function uses dynamic types
    /// 
    /// This method checks if the function's return type or parameters use dynamic types
    /// like "int" or "float" which have ambiguous sizes. This is important for
    /// type checking and code generation.
    /// 
    /// # Returns
    /// 
    /// * `true` if the function uses dynamic types
    /// * `false` otherwise
    pub fn has_dynamic_types(&self) -> bool {
        self.return_type == "int" || self.return_type == "float"
            || self.parameters.iter().any(|p| p.param_type == "int" || p.param_type == "float")
    }
}

/// Normalize type names to their explicit equivalents
/// 
/// This function converts common type names to their more specific equivalents:
/// - "int" becomes "int(4)+" (4-byte signed integer)
/// - "float" becomes "float(8)" (8-byte floating point)
/// - Other types remain unchanged
/// 
/// # Arguments
/// 
/// * `ty` - The type name to normalize
/// 
/// # Returns
/// 
/// The normalized type name as a String
fn normalize_type(ty: &str) -> String {
    match ty {
        "int" => "int(4)+".to_string(),
        "float" => "float(8)".to_string(),
        _ => ty.to_string(),
    }
}

/// Convert Coffee type names to traditional system types
/// 
/// This function converts Coffee's specific type names to their traditional equivalents
/// used by system APIs and other languages. It handles:
/// - Parametric integer types (int(1)+ → i8, int(4)+ → i32, etc.)
/// - Parametric float types (float(4) → f32, float(8) → f64, etc.)
/// - Basic type aliases (int → i32, float → f64)
/// 
/// The function supports both signed and unsigned integer types, with the '+' suffix
/// indicating signed and '-' indicating unsigned.
/// 
/// # Arguments
/// 
/// * `ty` - The Coffee type name to convert
/// 
/// # Returns
/// 
/// The traditional type name as a String, or the original type name if no conversion is applicable
pub fn type_to_traditional(ty: &str) -> String {
    if let Some(rest) = ty.strip_prefix("int(") {
        if let Some(size_end) = rest.find(')') {
            if let Ok(size) = rest[..size_end].parse::<u32>() {
                let suffix = &rest[size_end + 1..];
                return match (size, suffix) {
                    (1, "+") => "i8".to_string(),
                    (1, "-") => "u8".to_string(),
                    (2, "+") => "i16".to_string(),
                    (2, "-") => "u16".to_string(),
                    (4, "+") => "i32".to_string(),
                    (4, "-") => "u32".to_string(),
                    (8, "+") => "i64".to_string(),
                    (8, "-") => "u64".to_string(),
                    (16, "+") => "i128".to_string(),
                    (16, "-") => "u128".to_string(),
                    _ => ty.to_string(),
                };
            }
        }
    }

    if let Some(rest) = ty.strip_prefix("float(") {
        if let Some(size_end) = rest.find(')') {
            if let Ok(size) = rest[..size_end].parse::<u32>() {
                return match size {
                    4 => "f32".to_string(),
                    8 => "f64".to_string(),
                    _ => ty.to_string(),
                };
            }
        }
    }

    match ty {
        "int" => "i32".to_string(),
        "float" => "f64".to_string(),
        _ => ty.to_string(),
    }
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

/// Parse a parametric type specification
/// 
/// This function parses Coffee's parametric type syntax like `int(4)+`, `int(8)-`, `float(4)`, etc.
/// The syntax follows the pattern: base_type(size)[sign] where:
/// - base_type is either "int" or "float"
/// - size is one or more digits in parentheses
/// - sign is optional '+' for signed or '-' for unsigned (only for integers)
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, type_string))` - Successfully parsed type and remaining input
/// * `Err(nom::Err)` - If the input does not match the parametric type pattern
fn parse_parametric_type(input: &str) -> IResult<&str, &str> {
    let mut parser = alt((
        // int types with optional sign: int(4)+, int(4)-, int(4)
        recognize((
            tag("int"),
            char('('),
            take_while1(|c: char| c.is_ascii_digit()),
            char(')'),
            opt(alt((tag("+"), tag("-")))),
        )),
        // float types without sign: float(4), float(8)
        recognize((
            tag("float"),
            char('('),
            take_while1(|c: char| c.is_ascii_digit()),
            char(')'),
        )),
    ));
    Parser::parse(&mut parser, input)
}

/// Parse any valid Coffee type expression
/// 
/// This function handles parsing all kinds of Coffee type expressions:
/// - Basic types: int, float, string, bool, etc.
/// - Parametric types: int(4)+, float(8), etc.
/// - Generic types: List<int>, HashMap<String, int>, etc.
/// - Tuple types: (int, string), (int, int, bool), etc.
/// 
/// The parser tries each type pattern in order from most complex to simplest.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, type_string))` - Successfully parsed type and remaining input
/// * `Err(nom::Err)` - If the input does not match any type pattern
fn parse_type(input: &str) -> IResult<&str, &str> {
    let mut parser = alt((
        // Generic types, such as List<int>, HashMap<String, int>, etc.
        recognize((
            take_while1(|c: char| c.is_alphanumeric() || c == '_'),
            char('<'),
            take_till(|c| c == '>'),
            char('>'),
        )),
        // Tuple types, such as (int, string)
        recognize((
            char('('),
            take_till(|c| c == ')'),
            char(')'),
        )),
        // Parametric types, such as int(4), float(8)
        parse_parametric_type,
        // Basic types
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
    ));
    Parser::parse(&mut parser, input)
}

/// Parse a function parameter
/// 
/// This function parses a single parameter in a function signature. The expected
/// format is `name: type` where name is an identifier and type is any valid
/// Coffee type expression. The function also handles variadic parameters where
/// the type is "..." or "object".
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

    // Check if this is a variadic parameter
    let is_variadic = param_type == "..." || param_type == "object";

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
fn parse_function_body(input: &str) -> IResult<&str, FunctionBody> {
    let (input_after_colon, _) = char(':')(input)?;
    let (input, _) = space0(input_after_colon)?;

    // Check for external declaration (ends with newline or EOF after colon)
    if input.is_empty() || input.starts_with('\n') {
        // Check if there's actual body content on the next line
        if input.starts_with('\n') {
            let after_newline = &input[1..];
            let trimmed = after_newline.trim_start();
            // If the next non-empty line starts with valid Coffee syntax, it's a block
            // If it's empty or just another declaration, it might be external
            // Comments (/#/) should NOT cause the function to be treated as external
            if trimmed.is_empty() || trimmed.starts_with("fn ") || trimmed.starts_with("c ") ||
               trimmed.starts_with("main") {
                // External declaration - no body
                return Ok((input, FunctionBody::External));
            }
        } else {
            // Just ":" with nothing after - external declaration
            return Ok((input, FunctionBody::External));
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
                        i += 1;
                    }
                } else {
                    coffee_debug!("DEBUG: parse_function_body: line='{}', calling parse_single_line_statement", line);
                    if let Some(stmt) = super::parse_single_line_statement(line) {
                        statements.push(stmt);
                        i += 1;
                    } else {
                        i += 1;
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
        super::expr::assignment_rhs(&expr_str)
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
/// - Functions with error handlers: `fn name(params) #error_handler => return_type: body`
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
    let (input, body) = parse_function_body(input)?;

    Ok((
        input,
        Function {
            name: name.to_string(),
            parameters,
            return_type: return_type.to_string(),
            error_handler: error_handler.map(|s| s.to_string()),
            body,
            is_c,
        },
    ))
}

/// Strip inline Coffee comments from a line
/// 
/// This function removes inline comments from a Coffee source line. Coffee uses
/// '/#/' as the comment delimiter. The function is careful to not remove
/// '/#/' sequences that appear inside string literals, as those are not comments.
/// 
/// # Arguments
/// 
/// * `s` - The source line to strip comments from
/// 
/// # Returns
/// 
/// The line with inline comments removed, or the original line if no comment was found
/// outside of string literals
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
