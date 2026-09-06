//! For Loop Parser for Coffee Language
//! 
//! This module handles parsing of for loops in the Coffee programming language.
//! Coffee supports multiple forms of for loops:
//! 
//! - Collection iteration: `for item in collection: body`
//! - Range iteration: `for i in range(start, end): body` or `for i in start..end: body`
//! 
//! The parser handles both single-line and multi-line loop bodies, and supports
//! different iteration patterns with appropriate parsing for each.

use nom::{
    bytes::complete::tag,
    character::complete::{char, space0, space1},
    IResult,
};

/// Represents a for loop with variable, iterator, and body
/// 
/// This structure captures the complete structure of a Coffee for loop,
/// containing the iteration variable, the iterator specification, and the
/// loop body. The iterator can be either a collection or a range.
#[derive(Debug, PartialEq, Clone)]
pub struct ForLoop {
    /// The variable name used for iteration
    pub variable: String,
    /// The iterator specification (collection or range)
    pub iterator: ForIterator,
    /// The body of the for loop as a vector of statements
    pub body: Vec<crate::parser::Statement>,
}

/// Represents different types of iterators for for loops
/// 
/// For loops in Coffee can iterate over collections or ranges, and this
/// enum captures both possibilities with appropriate parameterization.
#[derive(Debug, PartialEq, Clone)]
pub enum ForIterator {
    /// Collection iterator - iterates over a collection
    Collection(crate::parser::expr::Expression),
    /// Range iterator - iterates from start to end
    Range { start: crate::parser::expr::Expression, end: crate::parser::expr::Expression },
}

/// Parse a complete Coffee for loop
/// 
/// This function parses the for loop syntax in Coffee, which includes:
/// - The 'for' keyword
/// - A variable name for iteration
/// - The 'in' keyword
/// - An iterator specification (collection name, range(start, end), or start..end)
/// - A colon
/// - A body (either single-line or multi-line indented block)
/// 
/// The function supports different iteration patterns:
/// - Collection iteration: `for item in collection: body`
/// - Range iteration with function syntax: `for i in range(start, end): body`
/// - Range iteration with operator syntax: `for i in start..end: body`
/// 
/// # Arguments
/// 
/// * `input` - The input string containing a for loop to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, ForLoop))` - Successfully parsed for loop and remaining input
/// * `Err(nom::Err)` - If the input does not match the for loop pattern
#[cfg(test)]
pub fn parse_for(input: &str) -> IResult<&str, ForLoop> {
    parse_for_at(input, 0)
}

pub fn parse_for_at(input: &str, base: usize) -> IResult<&str, ForLoop> {
    let src = input;
    let (input, _) = tag("for")(input)?;
    let (input, _) = space1(input)?;
    let (input, variable) = take_until_space(input)?;
    let (input, _) = space1(input)?;
    let (input, _) = tag("in")(input)?;
    let (input, _) = space1(input)?;
    let (input, iterator) = crate::parser::take_until_header_colon(input)?;
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;

    // 解析 body
    let (input, body_str) = if input.starts_with('\n') {
        // 多行块
        parse_indented_block(input)?
    } else {
        // 单行
        let (input, body) = take_until_newline(input)?;
        (input, body.trim().to_string())
    };

    // Parse iterator: supports range(start, end), start..end, or collection name
    let iterator = if let Some(after_range) = iterator.strip_prefix("range") {
        if after_range.starts_with('(') {
            if let Some(close) = crate::parser::index_of_matching_close_paren(after_range) {
                let range_content = &after_range[1..close];
                let parts = crate::parser::split_top_level_commas(range_content);
                if parts.len() == 2 {
                    ForIterator::Range {
                        start: parse_range_bound(src, base, parts[0])?,
                        end: parse_range_bound(src, base, parts[1])?,
                    }
                } else {
                    ForIterator::Collection(parse_range_bound(src, base, iterator.trim())?)
                }
            } else {
                ForIterator::Collection(parse_range_bound(src, base, iterator.trim())?)
            }
        } else {
            ForIterator::Collection(parse_range_bound(src, base, iterator.trim())?)
        }
    } else if iterator.contains("..") {
        // start..end syntax (Rust-style range)
        let parts: Vec<&str> = iterator.split("..").collect();
        if parts.len() == 2 {
            ForIterator::Range {
                start: parse_range_bound(src, base, parts[0].trim())?,
                end: parse_range_bound(src, base, parts[1].trim())?,
            }
        } else {
            ForIterator::Collection(parse_range_bound(src, base, iterator.trim())?)
        }
    } else {
        ForIterator::Collection(parse_range_bound(src, base, iterator.trim())?)
    };

    // 递归解析 body 中的语句
    let body = if body_str.is_empty() {
        Vec::new()
    } else {
        // 将 body_str 按行分割
        let body_lines: Vec<&str> = body_str.lines().collect();
        let mut statements = Vec::new();
        let mut idx = 0;

        while idx < body_lines.len() {
            let remaining_lines = &body_lines[idx..];
            if let Some((stmt, consumed)) = crate::parser::multiline::parse_multiline_statement_at(
                remaining_lines,
                crate::parser::multiline::slice_base(remaining_lines[0], 0),
            ) {
                statements.push(stmt);
                idx += consumed;
            } else {
                let line = body_lines[idx].trim();
                if line.is_empty() {
                    idx += 1;
                    continue;
                }
                if let Some(stmt) = crate::parser::line::parse_single_line_statement_at(
                    line,
                    crate::parser::multiline::slice_base(line, 0),
                ) {
                    statements.push(stmt);
                    idx += 1;
                    continue;
                }
                if line.contains(" = ") && !line.starts_with("if ") && !line.starts_with("for ") && !line.starts_with("while ") {
                    let parts: Vec<&str> = line.splitn(2, " = ").collect();
                    if parts.len() == 2 {
                        let var_name = parts[0].trim();
                        let value_expr = parts[1].trim();
                        if var_name.chars().all(|c| c.is_alphanumeric() || c == '_') && !var_name.is_empty()
                            && !value_expr.is_empty()
                        {
                            if let Ok(rhs) = crate::parser::expr::assignment_rhs(value_expr) {
                                statements.push(crate::parser::Statement::Assignment(var_name.to_string(), rhs));
                                idx += 1;
                                continue;
                            }
                        }
                    }
                }
                if let Ok(expr) = crate::parser::expr::parse_expression_at(
                    line,
                    crate::parser::multiline::slice_base(line, 0),
                ) {
                    statements.push(crate::parser::Statement::Expr(Box::new(expr)));
                    idx += 1;
                    continue;
                }
                return Err(nom::Err::Error(nom::error::Error {
                    input,
                    code: nom::error::ErrorKind::Fail,
                }));
            }
        }

        statements
    };

    Ok((
        input,
        ForLoop {
            variable: variable.trim().to_string(),
            iterator,
            body,
        },
    ))
}

fn parse_range_bound<'a>(
    parent: &str,
    parent_base: usize,
    raw: &'a str,
) -> Result<crate::parser::expr::Expression, nom::Err<nom::error::Error<&'a str>>> {
    crate::parser::multiline::parse_expr_at(parent, parent_base, raw)
}

/// Take characters from input until a space is encountered
/// 
/// This utility function scans the input string until it finds a space character,
/// returning the text before the space as the parsed content and the space and
/// everything after as the remaining input.
/// 
/// This function is used to extract the iteration variable name in for loops
/// (e.g., the 'item' in 'for item in collection').
/// 
/// # Arguments
/// 
/// * `input` - The input string to scan for a space
/// 
/// # Returns
/// 
/// * `Ok((remaining, content))` - The part after the space and the part before the space
/// * `Err(nom::Err)` - If no space is found in the input
fn take_until_space(input: &str) -> IResult<&str, &str> {
    if let Some(pos) = input.find(' ') {
        Ok((&input[pos..], &input[..pos]))
    } else {
        Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::TakeUntil,
        }))
    }
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

/// Parse an indented block of code
/// 
/// This function handles parsing of indented blocks of code in Coffee,
/// which are used for the bodies of for loops, functions, if expressions,
/// and other control structures. The function:
/// 
/// - Determines the indentation level from the first indented line
/// - Collects all lines that match that indentation level or are empty
/// - Removes the indentation prefix from each line
/// - Joins the content lines with newline characters
/// 
/// # Arguments
/// 
/// * `input` - The input string containing the indented block to parse (starting with a newline)
/// 
/// # Returns
/// 
/// * `Ok((remaining, String))` - Successfully parsed block content and remaining input
/// * `Err(nom::Err)` - If the block parsing fails
fn parse_indented_block(input: &str) -> IResult<&str, String> {
    let after_newline = &input[1..];
    let indent = take_while_indent(after_newline);

    if indent.is_empty() {
        let (input, body) = take_until_newline(after_newline)?;
        return Ok((input, body.trim().to_string()));
    }

    let lines: Vec<&str> = after_newline.lines().collect();
    let mut body_lines = Vec::new();
    let mut consumed = 0;

    for line in lines {
        if line.starts_with(indent) || line.trim().is_empty() {
            let content = if line.starts_with(indent) {
                line[indent.len()..].trim_end()
            } else {
                ""
            };
            if !content.is_empty() {
                body_lines.push(content.to_string());
            }
            consumed += line.len() + 1;
        } else {
            break;
        }
    }

    if consumed > 0 {
        consumed = consumed.saturating_sub(1);
    }

    Ok((
        &after_newline[consumed..],
        body_lines.join("\n"),
    ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::expr::Expression;

    #[test]
    fn for_iterator_keeps_double_colon_path() {
        let (rest, before) =
            crate::parser::take_until_header_colon("x in Foo::items:").expect("header colon");
        assert_eq!(before, "x in Foo::items");
        assert_eq!(rest, ":");

        let src = "for x in Foo::items():\n    return 0\n";
        let (_, loop_) = parse_for(src).expect("parse");
        match loop_.iterator {
            ForIterator::Collection(expr) => {
                let s = format!("{:?}", expr);
                assert!(
                    s.contains("Foo") && s.contains("items"),
                    "iterator truncated: {:?}",
                    expr
                );
            }
            other => panic!("expected collection iterator, got {:?}", other),
        }
    }

    #[test]
    fn range_keeps_commas_inside_nested_call() {
        let src = "for i in range(min(1, 2), 10):\n    return 0\n";
        let (_, loop_) = parse_for(src).expect("parse");
        match loop_.iterator {
            ForIterator::Range { start, end } => {
                assert!(
                    matches!(start.kind(), Expression::Call { function: _, args: _ }),
                    "{:?}",
                    start.kind()
                );
                assert!(
                    matches!(end.kind(), Expression::Literal(_)),
                    "{:?}",
                    end.kind()
                );
            }
            other => panic!("expected range, got {:?}", other),
        }
    }

    #[test]
    fn invalid_for_body_line_is_error_not_skipped() {
        let src = "for i in 0..3:\n    @@@\n";
        assert!(parse_for(src).is_err());
    }
}
