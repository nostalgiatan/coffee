//! While Loop Parser for Coffee Language
//! 
//! This module handles parsing of while loops in the Coffee programming language.
//! Coffee supports while loops with the syntax `while condition: body`, where
//! the body can be either a single-line statement or a multi-line indented block.
//! The parser handles both forms and converts the body into a vector of statements.

use nom::{
    bytes::complete::tag,
    character::complete::{char, space0, space1},
    IResult,
};

// Import Statement for while body
use super::Statement;
use super::expr::Expression;

/// Represents a while loop with condition and body
/// 
/// This structure captures the complete structure of a Coffee while loop,
/// containing the condition to evaluate and the body statements to execute
/// while the condition is true.
#[derive(Debug, PartialEq, Clone)]
pub struct WhileLoop {
    /// The condition expression for the while loop
    pub condition: Expression,
    /// The body statements for the while loop
    pub body: Vec<Statement>,  // Changed from String to Vec<Statement>
}

/// Parse a complete Coffee while loop
/// 
/// This function parses the while loop syntax in Coffee, which includes:
/// - The 'while' keyword
/// - A condition expression
/// - A colon
/// - A body (either single-line or multi-line indented block)
/// 
/// The parser handles both single-line and multi-line bodies using indentation-based
/// block detection to identify the statements that belong to the loop body.
/// 
/// # Arguments
/// 
/// * `input` - The input string containing a while loop to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, WhileLoop))` - Successfully parsed while loop and remaining input
/// * `Err(nom::Err)` - If the input does not match the while loop pattern
pub fn parse_while(input: &str) -> IResult<&str, WhileLoop> {
    let (input, _) = tag("while")(input)?;
    let (input, _) = space1(input)?;
    let (input, condition_raw) = take_until_colon(input)?;
    let condition = parse_expr_from_slice(condition_raw)?;
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;

    // Parse statement block instead of indented string
    let (input, body) = parse_statement_block(input)?;

    Ok((
        input,
        WhileLoop {
            condition,
            body,
        },
    ))
}

fn parse_expr_from_slice(raw: &str) -> Result<Expression, nom::Err<nom::error::Error<&str>>> {
    crate::parser::expr::parse_expression(raw.trim()).map_err(|_| {
        nom::Err::Error(nom::error::Error {
            input: raw,
            code: nom::error::ErrorKind::Fail,
        })
    })
}

/// Take characters from input until a colon is encountered
/// 
/// This utility function scans the input string until it finds a colon character,
/// returning the text before the colon as the parsed content and the colon and
/// everything after as the remaining input.
/// 
/// This function is used to parse the condition part of while loops and other
/// control flow statements that have the format `keyword condition:`.
/// 
/// # Arguments
/// 
/// * `input` - The input string to scan for a colon
/// 
/// # Returns
/// 
/// * `Ok((remaining, content))` - The part after the colon and the part before the colon
/// * `Err(nom::Err)` - If no colon is found in the input
fn take_until_colon(input: &str) -> IResult<&str, &str> {
    for (i, c) in input.char_indices() {
        if c == ':' {
            return Ok((&input[i..], &input[..i]));
        }
    }
    Err(nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::TakeUntil,
    }))
}

/// Parse an indented block of statements
/// 
/// This function handles parsing of indented blocks of statements in Coffee,
/// which are used for the bodies of while loops, functions, if expressions,
/// and other control structures. The function:
/// 
/// - Handles both single-line and multi-line blocks
/// - Removes indentation from lines in multi-line blocks
/// - Parses individual statements within the block using the main parser
/// - Properly handles nested control structures
/// 
/// # Arguments
/// 
/// * `input` - The input string containing the block to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Vec<Statement>))` - Successfully parsed statements and remaining input
/// * `Err(nom::Err)` - If the block parsing fails
fn parse_statement_block(input: &str) -> IResult<&str, Vec<Statement>> {
    if !input.starts_with('\n') {
        // Single line - parse as single statement
        let (input, line) = take_until_newline(input)?;
        let line = line.trim();
        if line.is_empty() {
            return Ok((input, Vec::new()));
        }
        if let Some(stmt) = super::parse_single_line_statement(line) {
            return Ok((input, vec![stmt]));
        } else {
            return Ok((input, Vec::new()));
        }
    }

    let after_newline = &input[1..];
    let indent = take_while_indent(after_newline);

    if indent.is_empty() {
        // No indentation - single line
        let (input, line) = take_until_newline(after_newline)?;
        let line = line.trim();
        if line.is_empty() {
            return Ok((input, Vec::new()));
        }
        if let Some(stmt) = super::parse_single_line_statement(line) {
            return Ok((input, vec![stmt]));
        } else {
            return Ok((input, Vec::new()));
        }
    }

    // Multi-line block - collect lines with indentation removed
    let mut body_lines_vec = Vec::new();
    let mut consumed = 0;

    for line in after_newline.lines() {
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
        let line = body_lines_vec[i].trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("/#") {
            i += 1;
            continue;
        }

        // Try multiline statement first (for if, while, for, match)
        if let Some((stmt, lines_consumed)) = super::parse_multiline_statement(&body_lines_vec[i..]) {
            statements.push(stmt);
            i += lines_consumed;
        } else if let Some(stmt) = super::parse_single_line_statement(line) {
            statements.push(stmt);
            i += 1;
        } else {
            i += 1;
        }
    }

    Ok((remaining, statements))
}

/// Take whitespace characters from the beginning of input
/// 
/// This utility function extracts all leading whitespace characters (spaces and tabs)
/// from the input string, which is used to determine the indentation level of
/// indented blocks in Coffee code.
/// 
/// # Arguments
/// 
/// * `input` - The input string to extract leading whitespace from
/// 
/// # Returns
/// 
/// A string slice containing only the leading whitespace characters
fn take_while_indent(input: &str) -> &str {
    let len = input.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| c.len_utf8())
        .sum();
    &input[..len]
}

/// Take characters from input until a newline is encountered
/// 
/// This utility function scans the input string until it finds a newline character,
/// returning the text before the newline as the parsed content and the newline and
/// everything after as the remaining input. If no newline is found, it returns
/// the entire input as content with an empty remaining string.
/// 
/// This function is used to extract single-line statements or content from the input.
/// 
/// # Arguments
/// 
/// * `input` - The input string to scan for a newline
/// 
/// # Returns
/// 
/// * `Ok((remaining, content))` - The part after the newline and the part before the newline
/// * `Ok(("", input))` - If no newline is found, returns empty remaining and full input as content
fn take_until_newline(input: &str) -> IResult<&str, &str> {
    if let Some(pos) = input.find('\n') {
        Ok((&input[pos..], &input[..pos]))
    } else {
        Ok(("", input))
    }
}
