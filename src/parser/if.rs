//! If Expression Parser for Coffee Language
//! 
//! This module handles parsing of if expressions in the Coffee programming language.
//! Coffee supports conditional branches with if, elif, and else statements, with
//! support for both single-line and multi-line bodies. The parser handles:
//! 
//! - Basic if statements with condition and body
//! - Multiple elif branches with conditions and bodies
//! - Optional else branch with body
//! - Both single-line and multi-line block bodies
//! - Proper indentation-based block detection

use nom::{
    bytes::complete::tag,
    character::complete::{char, multispace0, space1},
    combinator::opt,
    multi::many0,
    IResult, Parser,
};

// Import Statement for if body
use super::Statement;

/// Represents an if expression with condition, body, elif branches, and else body
/// 
/// This structure captures the complete structure of a Coffee if expression,
/// including the main condition and body, any number of elif branches with their
/// conditions and bodies, and an optional else branch with its body.
#[derive(Debug, PartialEq, Clone)]
pub struct IfExpr {
    /// The condition expression for the main if branch
    pub condition: String,
    /// The body statements for the main if branch
    pub body: Vec<Statement>,  // Changed from String to Vec<Statement>
    /// A list of elif branches, each with a condition and body
    pub elifs: Vec<ElifBranch>,
    /// The optional else branch body statements
    pub else_body: Option<Vec<Statement>>,  // Changed from Option<String> to Option<Vec<Statement>>
}

/// Represents a single elif branch with condition and body
/// 
/// This structure captures a single elif branch in an if expression,
/// containing the condition to check and the body statements to execute
/// if the condition is true.
#[derive(Debug, PartialEq, Clone)]
pub struct ElifBranch {
    /// The condition expression for this elif branch
    pub condition: String,
    /// The body statements for this elif branch
    pub body: Vec<Statement>,  // Changed from String to Vec<Statement>
}

/// Parse a complete Coffee if expression
/// 
/// This function parses the complete if expression syntax in Coffee, which includes:
/// - The main if branch with condition and body
/// - Zero or more elif branches with conditions and bodies
/// - An optional else branch with body
/// 
/// The parser handles both single-line and multi-line bodies. For multi-line bodies,
/// it uses indentation-based block detection to identify the statements that belong
/// to each branch of the if expression.
/// 
/// # Arguments
/// 
/// * `input` - The input string containing an if expression to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, IfExpr))` - Successfully parsed if expression and remaining input
/// * `Err(nom::Err)` - If the input does not match the if expression pattern
pub fn parse_if(input: &str) -> IResult<&str, IfExpr> {
    let (input, _) = tag("if")(input)?;
    let (input, _) = space1(input)?;
    let (input, condition) = take_until_colon(input)?;
    let (input, _) = char(':')(input)?;

    // Check if there's a newline (multiline body) or just whitespace (single line)
    let has_newline = input.starts_with('\n');
    let (input, body) = if has_newline {
        // Multiline body - pass input with whitespace to parse_statement_block
        parse_statement_block(input, false)?
    } else {
        // Single line body - parse inline
        let (tmp_input, _) = multispace0(input)?;
        if tmp_input.is_empty() {
            (input, Vec::new())
        } else {
            let (remaining, line) = take_until_newline(tmp_input)?;
            let line = line.trim();
            if line.is_empty() {
                (remaining, Vec::new())
            } else if let Some(stmt) = super::parse_single_line_statement(line) {
                (remaining, vec![stmt])
            } else {
                (remaining, Vec::new())
            }
        }
    };

    // 解析 elif 分支
    let (input, elifs) = many0(parse_elif).parse(input)?;

    // 解析 else 分支
    let (input, else_body) = opt(parse_else).parse(input)?;

    Ok((
        input,
        IfExpr {
            condition: condition.trim().to_string(),
            body,
            elifs,
            else_body,
        },
    ))
}

/// Parse a single elif branch in an if expression
/// 
/// This function parses a single elif branch, which consists of:
/// - The 'elif' keyword
/// - A condition expression
/// - A colon
/// - A body (either single-line or multi-line)
/// 
/// The function handles both single-line and multi-line bodies using the same
/// logic as the main if branch parser.
/// 
/// # Arguments
/// 
/// * `input` - The input string containing an elif branch to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, ElifBranch))` - Successfully parsed elif branch and remaining input
/// * `Err(nom::Err)` - If the input does not match the elif branch pattern
fn parse_elif(input: &str) -> IResult<&str, ElifBranch> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("elif")(input)?;
    let (input, _) = space1(input)?;
    let (input, condition) = take_until_colon(input)?;
    let (input, _) = char(':')(input)?;

    // Check if there's a newline (multiline body)
    let has_newline = input.starts_with('\n');
    let (input, body) = if has_newline {
        parse_statement_block(input, false)?
    } else {
        let (tmp_input, _) = multispace0(input)?;
        if tmp_input.is_empty() {
            (input, Vec::new())
        } else {
            let (remaining, line) = take_until_newline(tmp_input)?;
            let line = line.trim();
            if line.is_empty() {
                (remaining, Vec::new())
            } else if let Some(stmt) = super::parse_single_line_statement(line) {
                (remaining, vec![stmt])
            } else {
                (remaining, Vec::new())
            }
        }
    };

    Ok((
        input,
        ElifBranch {
            condition: condition.trim().to_string(),
            body,
        },
    ))
}

/// Parse an else branch in an if expression
/// 
/// This function parses the else branch of an if expression, which consists of:
/// - The 'else' keyword
/// - A colon
/// - A body (either single-line or multi-line)
/// 
/// Since the else branch is always the last branch in an if expression, the
/// parser is informed that it's the last branch to prevent it from looking
/// for subsequent elif or else branches.
/// 
/// # Arguments
/// 
/// * `input` - The input string containing an else branch to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Vec<Statement>))` - Successfully parsed else body and remaining input
/// * `Err(nom::Err)` - If the input does not match the else branch pattern
fn parse_else(input: &str) -> IResult<&str, Vec<Statement>> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("else")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(':')(input)?;

    // Check if there's a newline (multiline body)
    let has_newline = input.starts_with('\n');
    let (input, body) = if has_newline {
        parse_statement_block(input, true)?
    } else {
        let (input, _) = multispace0(input)?;
        if input.is_empty() {
            return Ok((input, Vec::new()));
        }
        let (remaining, line) = take_until_newline(input)?;
        let line = line.trim();
        if line.is_empty() {
            (remaining, Vec::new())
        } else if let Some(stmt) = super::parse_single_line_statement(line) {
            (remaining, vec![stmt])
        } else {
            (remaining, Vec::new())
        }
    };

    Ok((input, body))
}

/// Take characters from input until a colon is encountered
/// 
/// This utility function scans the input string until it finds a colon character,
/// returning the text before the colon as the parsed content and the colon and
/// everything after as the remaining input.
/// 
/// This function is used to parse the condition part of if, elif, and other
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
/// which are used for the bodies of if expressions, functions, loops, and other
/// control structures. The function:
/// 
/// - Handles both single-line and multi-line blocks
/// - Removes indentation from lines in multi-line blocks
/// - For if/elif branches, can detect when the block should end based on
///   encountering another elif or else at the same indentation level
/// - Parses individual statements within the block
/// 
/// # Arguments
/// 
/// * `input` - The input string containing the block to parse
/// * `is_last_branch` - Flag indicating if this is the last branch in an if expression
///   (for else blocks, where no elif/else can follow)
/// 
/// # Returns
/// 
/// * `Ok((remaining, Vec<Statement>))` - Successfully parsed statements and remaining input
/// * `Err(nom::Err)` - If the block parsing fails
fn parse_statement_block(input: &str, is_last_branch: bool) -> IResult<&str, Vec<Statement>> {
    if !input.starts_with('\n') {
        // Single line - try both multiline and single line parsers
        let (input, line) = take_until_newline(input)?;
        let line = line.trim();
        if line.is_empty() {
            return Ok((input, Vec::new()));
        }
        // Try multiline first (for nested if, while, etc.)
        if let Some((stmt, _)) = super::parse_multiline_statement(&[line]) {
            return Ok((input, vec![stmt]));
        }
        // Then try single line
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

            // For if/elif bodies, check if we encounter elif or else at the SAME indent level
            if !is_last_branch {
                let trimmed = content.trim();
                // Only break if we're back to base indent level (not nested)
                let current_indent = if line.starts_with(indent) {
                    indent.len()
                } else {
                    0
                };
                // Only break if at base indent (not nested)
                if current_indent == indent.len() && (trimmed.starts_with("elif ") || trimmed.starts_with("else:")) {
                    break;
                }
            }

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
