//! Parser Module for Coffee Language
//! 
//! This module provides the complete parser for the Coffee programming language,
//! handling all language constructs including functions, classes, enums, control flow,
//! expressions, and more. The parser is designed to handle both single-line statements
//! and complex multiline structures, using a combination of line-based and block-based
//! parsing approaches.
//! 
//! The parser system features:
//! - Line-based parsing with indentation-aware block structure detection
//! - Multiline statement parsing for functions, classes, control flow blocks, etc.
//! - Comprehensive error detection and reporting
//! - Support for Coffee's unique syntax features including:
//!   - Indentation-based block structure
//!   - Memory management operations (mv, copy, clone, rm, clean)
//!   - Pattern matching with match expressions
//!   - Class and enum definitions
//!   - Import statements with various formats
//! - Block tracking to correctly identify nested structures
//! 
//! The top-level `parse_program` function orchestrates the parsing process,
//! combining single-line and multiline parsing strategies to handle complete
//! Coffee source files.

#![allow(dead_code)]

use crate::coffee_debug;
use rayon::prelude::*;

pub mod import;
pub mod function;
pub mod main;
pub mod r#if;
pub mod r#while;
pub mod r#match;
pub mod pattern;
pub mod r#for;
pub mod var;
pub mod comment;
pub mod class;
pub mod memory;
pub mod expr;
pub mod error;
pub mod tracker;
pub mod indent;
pub mod raise;

pub use error::ParseError;
pub use tracker::{BlockTracker, BlockType};
pub use raise::RaiseStmt;

pub use import::Import;
pub use function::{Function, FunctionBody};
pub use main::MainEntry;

pub use r#if::IfExpr;
pub use r#while::WhileLoop;
pub use r#match::MatchExpr;
pub use pattern::Pattern;
pub use r#for::{ForLoop, ForIterator};
pub use var::{VariableDecl, ReturnStmt, BreakStmt, ContinueStmt};
pub use comment::{SingleLineComment, MultiLineComment};
pub use class::{ClassDef, EnumDef};
pub use memory::MemoryOp;

#[derive(Debug, PartialEq, Clone)]
#[allow(dead_code)]
pub struct Program {
    /// List of statements that make up the parsed Coffee program
    /// These statements represent the complete AST of the parsed source code
    pub statements: Vec<Statement>,
}

impl Program {
    /// Create a new Program with the given statements
    /// 
    /// # Arguments
    /// 
    /// * `statements` - A vector of Statement nodes representing the AST
    /// 
    /// # Returns
    /// 
    /// A new Program instance containing the provided statements
    pub fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }

    /// Create an empty Program
    /// 
    /// # Returns
    /// 
    /// A new Program instance with an empty statement list
    pub fn empty() -> Self {
        Self { statements: Vec::new() }
    }
}

/// Parse complete program
/// 
/// This function parses an entire Coffee source file into an Abstract Syntax Tree (AST).
/// It handles both single-line statements and complex multiline constructs like functions,
/// classes, control flow blocks, and comments. The function processes the input line by line,
/// identifying the appropriate parsing strategy for each statement based on its structure
/// and content.
/// 
/// The parser first checks for multiline comments, then attempts to parse multiline
/// structures (functions, classes, enums, control flow), and finally tries to parse
/// single-line statements. Error recovery is implemented to continue parsing even
/// when encountering syntax errors, collecting all errors for reporting.
/// 
/// # Arguments
/// 
/// * `input` - The Coffee source code to parse as a string slice
/// 
/// # Returns
/// 
/// * `Ok(Program)` - Successfully parsed program if no errors occurred
/// * `Err(Vec<ParseError>)` - List of parsing errors if any syntax errors were found
pub fn parse_program(input: &str) -> Result<Program, Vec<ParseError>> {
    let mut statements = Vec::new();
    let mut errors = Vec::new();

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut iterations = 0;
    const MAX_ITER: usize = 1_000_000;
    const PARALLEL_MIN_LINES: usize = 200;
    const PARALLEL_MIN_UNITS: usize = 8;

    // Sequential indent scan first: locate independent top-level fn/class/enum/c fn spans.
    let independent = scan_independent_top_level_spans(&lines);
    let parallelize =
        lines.len() >= PARALLEL_MIN_LINES || independent.len() >= PARALLEL_MIN_UNITS;
    let parsed_units: Vec<(Vec<Statement>, Vec<ParseError>)> = if parallelize {
        independent
            .par_iter()
            .map(|&(start, end)| parse_independent_unit(&lines, start, end))
            .collect()
    } else {
        Vec::new()
    };
    let mut unit_idx = 0;

    while i < lines.len() {
        iterations += 1;
        if iterations > MAX_ITER {
            errors.push(ParseError::GenericSyntaxError {
                line: i + 1,
                context: lines.get(i).copied().unwrap_or("").to_string(),
                hint: "Parser safety limit reached; remaining input was not parsed".to_string(),
            });
            break;
        }

        if parallelize && unit_idx < independent.len() && i == independent[unit_idx].0 {
            let (unit_stmts, unit_errs) = &parsed_units[unit_idx];
            statements.extend_from_slice(unit_stmts);
            errors.extend_from_slice(unit_errs);
            i = independent[unit_idx].1;
            unit_idx += 1;
            continue;
        }

        let line = lines[i].trim();

        // Skip empty lines
        if line.is_empty() {
            i += 1;
            continue;
        }

        // Skip whole-line comments
        if line.starts_with("/#/") {
            i += 1;
            continue;
        }

        // First check for multiline comments
        if line.starts_with("/#*") {
            if let Some((comment_content, lines_consumed)) = collect_multiline_comment(&lines[i..]) {
                if let Ok((_, comment)) = crate::parser::comment::parse_multi_line_comment(&comment_content) {
                    statements.push(Statement::MultiLineComment(comment));
                    i += lines_consumed;
                    continue;
                } else {
                    errors.push(ParseError::GenericSyntaxError {
                        line: i + 1,
                        context: line.to_string(),
                        hint: "Check multiline comment syntax: /#* ... *#/".to_string(),
                    });
                }
            }
        }

        // Try to parse multiline structures (functions, classes, enums)
        if let Some((stmt, lines_consumed)) = parse_multiline_statement(&lines[i..]) {
            statements.push(stmt);
            // SAFETY: Ensure we always advance at least 1 line to prevent infinite loops
            i += lines_consumed.max(1);
            continue;
        }

        // Failed block recovery: do not parse an indented body as top-level statements
        if is_block_starter(line) {
            errors.push(block_parse_error(i + 1, lines[i]));
            i += skip_failed_block(&lines, i);
            continue;
        }

        // Check for multiline variable declarations (struct literals, etc.)
        let trimmed = line.trim();
        if trimmed.starts_with("let ") {
            // Collect multiline content for variable declarations
            if let Some((multiline_content, lines_consumed)) = collect_multiline_variable_decl(&lines[i..]) {
                if let Ok((remaining, var_decl)) = crate::parser::var::parse_variable_decl(&multiline_content) {
                    if remaining.trim().is_empty() {
                        statements.push(Statement::VariableDecl(var_decl));
                        i += lines_consumed;
                        continue;
                    }
                }
            }
        }

        // Try to parse single-line statements
        if let Some(stmt) = parse_single_line_statement(line) {
            statements.push(stmt);
            i += 1;
        } else {
            // Smart error detection: analyze specific error types
            errors.push(ParseError::detect_error(i + 1, lines[i]));
            i += 1;
        }
    }

    if errors.is_empty() {
        Ok(Program::new(statements))
    } else {
        Err(errors)
    }
}

/// Sequential indent scan: complete top-level `fn` / `class` / `enum` / `c fn` spans.
/// Nested bodies stay inside the parent span and are never listed separately.
fn scan_independent_top_level_spans(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut cache = crate::parser::indent::IndentCache::new();
    let mut i = 0;
    let mut iterations = 0;
    const MAX_ITER: usize = 1_000_000;

    while i < lines.len() {
        iterations += 1;
        if iterations > MAX_ITER {
            break;
        }

        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with("/#/") {
            i += 1;
            continue;
        }
        if trimmed.starts_with("/#*") {
            i += collect_multiline_comment(&lines[i..])
                .map(|(_, n)| n.max(1))
                .unwrap_or(1);
            continue;
        }

        if BlockTracker::detect_block_type(lines[i])
            .map(BlockType::is_independent_top_level)
            .unwrap_or(false)
        {
            let end = cache.indented_block_end(lines, i).max(i + 1);
            spans.push((i, end));
            i = end;
            continue;
        }

        if is_block_starter(trimmed) {
            i = cache.indented_block_end(lines, i).max(i + 1);
            continue;
        }

        if trimmed.starts_with("let ") {
            if let Some((_, n)) = collect_multiline_variable_decl(&lines[i..]) {
                i += n.max(1);
                continue;
            }
        }

        i += 1;
    }

    spans
}

/// Parse one already-identified independent top-level span. Line numbers stay absolute.
fn parse_independent_unit(
    lines: &[&str],
    start: usize,
    end: usize,
) -> (Vec<Statement>, Vec<ParseError>) {
    let end = end.min(lines.len());
    if start >= end {
        return (Vec::new(), Vec::new());
    }
    let slice = &lines[start..end];

    if let Some((stmt, _)) = parse_multiline_statement(slice) {
        return (vec![stmt], Vec::new());
    }

    if is_block_starter(slice[0].trim()) {
        return (Vec::new(), vec![block_parse_error(start + 1, slice[0])]);
    }

    match parse_single_line_statement(slice[0].trim()) {
        Some(stmt) => (vec![stmt], Vec::new()),
        None => (
            Vec::new(),
            vec![ParseError::detect_error(start + 1, slice[0])],
        ),
    }
}

/// Collect multiline comment content
/// 
/// This internal function extracts the content of a multiline comment block
/// that starts with `/#*` and ends with `*#/`. It processes an array of lines
/// and returns the complete comment content along with the number of lines
/// consumed from the input.
/// 
/// # Arguments
/// 
/// * `lines` - A slice of string slices representing the lines to process
/// 
/// # Returns
/// 
/// * `Some((String, usize))` - The comment content and number of lines consumed if a multiline comment was found
/// * `None` - If the first line doesn't start with `/#*`
fn collect_multiline_comment(lines: &[&str]) -> Option<(String, usize)> {
    if lines.is_empty() || !lines[0].trim().starts_with("/#*") {
        return None;
    }

    let mut content = String::new();
    let mut lines_consumed = 0;

    for line in lines {
        content.push_str(line);
        content.push('\n');
        lines_consumed += 1;

        if line.trim().ends_with("*#/") {
            break;
        }
    }

    Some((content, lines_consumed))
}

fn is_block_starter(trimmed: &str) -> bool {
    trimmed.starts_with("fn ")
        || trimmed.starts_with("c fn ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("packed class ")
        || matches!(
            trimmed,
            "fn" | "if" | "while" | "for" | "match" | "enum" | "class"
        )
}

fn block_starter_name(trimmed: &str) -> &'static str {
    if trimmed.starts_with("c fn ") || trimmed.starts_with("fn ") || trimmed == "fn" {
        "fn"
    } else if trimmed.starts_with("packed class ") || trimmed.starts_with("class ") || trimmed == "class" {
        "class"
    } else if trimmed.starts_with("if ") || trimmed == "if" {
        "if"
    } else if trimmed.starts_with("while ") || trimmed == "while" {
        "while"
    } else if trimmed.starts_with("for ") || trimmed == "for" {
        "for"
    } else if trimmed.starts_with("match ") || trimmed == "match" {
        "match"
    } else if trimmed.starts_with("enum ") || trimmed == "enum" {
        "enum"
    } else {
        "block"
    }
}

fn block_parse_error(line_num: usize, line: &str) -> ParseError {
    let detected = ParseError::detect_error(line_num, line);
    match detected {
        ParseError::GenericSyntaxError { .. } => ParseError::MissingBlockBody {
            line: line_num,
            statement_type: block_starter_name(line.trim()).to_string(),
        },
        other => other,
    }
}

/// Consume the starter line plus any following indented body so it is not reparsed top-level.
fn skip_failed_block(lines: &[&str], start: usize) -> usize {
    let mut cache = crate::parser::indent::IndentCache::new();
    cache.indented_block_end(lines, start).saturating_sub(start).max(1)
}

/// Split `lhs = rhs` without requiring spaces. Ignores `==`/`!=`/`<=`/`>=` and quoted `=`.
fn split_assignment(trimmed: &str) -> Option<(&str, &str)> {
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    while i < bytes.len() {
        let c = bytes[i];
        if escape {
            escape = false;
            i += 1;
            continue;
        }
        if in_double {
            if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        if in_single {
            if c == b'\\' {
                escape = true;
            } else if c == b'\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_double = true;
                i += 1;
            }
            b'\'' => {
                in_single = true;
                i += 1;
            }
            b'=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    i += 2;
                    continue;
                }
                if i > 0 {
                    let prev = bytes[i - 1];
                    if matches!(prev, b'!' | b'<' | b'>' | b'=' | b'+' | b'-' | b'*' | b'/' | b'%') {
                        i += 1;
                        continue;
                    }
                }
                let left = trimmed[..i].trim();
                let right = trimmed[i + 1..].trim();
                if left.is_empty() || right.is_empty() {
                    return None;
                }
                return Some((left, right));
            }
            _ => i += 1,
        }
    }
    None
}

fn assignment_lhs_name(expr: &crate::parser::expr::Expression) -> Option<String> {
    use crate::parser::expr::Expression;
    match expr {
        Expression::Variable(name) if !name.is_empty() => Some(name.clone()),
        Expression::Member { object, field, args } if args.is_empty() && !field.is_empty() => {
            let prefix = assignment_lhs_name(object)?;
            Some(format!("{}.{}", prefix, field))
        }
        _ => None,
    }
}

fn try_parse_assignment(trimmed: &str) -> Option<(String, crate::parser::expr::Expression)> {
    if trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("let ")
    {
        return None;
    }
    let (lhs_src, rhs_src) = split_assignment(trimmed)?;
    let lhs_expr = crate::parser::expr::parse_expression(lhs_src).ok()?;
    let var_name = assignment_lhs_name(&lhs_expr)?;
    Some((var_name, crate::parser::expr::assignment_rhs(rhs_src)))
}

fn is_broken_statement_keyword(trimmed: &str) -> bool {
    let word = trimmed
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("");
    matches!(
        word,
        "if" | "fn"
            | "let"
            | "while"
            | "for"
            | "match"
            | "enum"
            | "class"
            | "elif"
            | "else"
            | "return"
            | "raise"
            | "use"
            | "c"
            | "packed"
    )
}

/// Parse multiline statement (functions, classes, enums, control flows, etc.)
/// 
/// This function attempts to parse complex multiline constructs in Coffee source code.
/// It identifies the type of multiline statement by examining the first line and then
/// collects the appropriate content using the block tracker before passing it to
/// the specific parser for that construct.
/// 
/// The function supports parsing of functions (including C functions), classes (packed
/// and normal), enums, and control flow statements (if, while, for, match).
/// 
/// # Arguments
/// 
/// * `lines` - A slice of string slices representing the lines to parse, starting with the first line of the potential multiline statement
/// 
/// # Returns
/// 
/// * `Some((Statement, usize))` - The parsed statement and number of lines consumed if successful
/// * `None` - If no multiline statement could be parsed from the given lines
/// Join consecutive lines until `{` / `}` depth returns to zero.
fn collect_balanced_brace_prefix(lines: &[&str]) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    let mut collected = Vec::new();
    let mut brace_depth = 0;
    let mut found_open = false;
    for line in lines {
        collected.push((*line).to_string());
        for ch in line.chars() {
            if ch == '{' {
                brace_depth += 1;
                found_open = true;
            } else if ch == '}' {
                brace_depth -= 1;
            }
        }
        if found_open && brace_depth == 0 {
            return Some(collected.join("\n"));
        }
    }
    None
}

pub fn parse_multiline_statement(lines: &[&str]) -> Option<(Statement, usize)> {
    if lines.is_empty() {
        return None;
    }

    let first_line = lines[0].trim();

    // Try to parse function (supports fn and c fn)
    if first_line.starts_with("fn ") || first_line.starts_with("c fn ") {
        coffee_debug!("DEBUG: parse_multiline_statement: found function definition, first_line='{}'", first_line);
        // Try to parse current line and following lines directly
        if let Some(multiline_content) = collect_multiline_content(lines, "fn") {
            coffee_debug!("DEBUG: parse_multiline_statement: collected multiline_content='{}'", multiline_content.0);
            // Try direct parsing - parser will handle c fn prefix
            if let Ok((_, func)) = crate::parser::function::parse_function(&multiline_content.0) {
                coffee_debug!("DEBUG: parse_multiline_statement: parsed function '{}'", func.name);
                return Some((Statement::Function(func), multiline_content.1));
            } else {
                coffee_debug!("DEBUG: parse_multiline_statement: failed to parse function");
            }
        } else {
            coffee_debug!("DEBUG: parse_multiline_statement: failed to collect multiline_content");
        }
    }

    // Try to parse packed class
    if first_line.starts_with("packed class ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "packed class") {
            if let Ok((_, class)) = crate::parser::class::parse_packed_class(&multiline_content.0) {
                return Some((Statement::Class(class), multiline_content.1));
            }
        }
    }

    // Try to parse normal class
    if first_line.starts_with("class ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "class") {
            if let Ok((_, class)) = crate::parser::class::parse_class(&multiline_content.0) {
                return Some((Statement::Class(class), multiline_content.1));
            } else {
            }
        } else {
        }
    }

    // Try to parse enum
    if first_line.starts_with("enum ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "enum") {
            if let Ok((_, enum_def)) = crate::parser::class::parse_enum(&multiline_content.0) {
                return Some((Statement::Enum(enum_def), multiline_content.1));
            }
        }
    }

    // Try to parse if expression
    if first_line.starts_with("if ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "if") {
            if let Ok((_, if_expr)) = crate::parser::r#if::parse_if(&multiline_content.0) {
                return Some((Statement::If(if_expr), multiline_content.1));
            }
        }
    }

    // Try to parse while loop
    if first_line.starts_with("while ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "while") {
            if let Ok((_, while_loop)) = crate::parser::r#while::parse_while(&multiline_content.0) {
                return Some((Statement::While(while_loop), multiline_content.1));
            }
        }
    }

    // Try to parse for loop
    if first_line.starts_with("for ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "for") {
            if let Ok((_, for_loop)) = crate::parser::r#for::parse_for(&multiline_content.0) {
                return Some((Statement::For(for_loop), multiline_content.1));
            }
        }
    }

    // Try to parse match expression
    if first_line.starts_with("match ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "match") {
            if let Ok((_, match_expr)) = crate::parser::r#match::parse_match(&multiline_content.0) {
                return Some((Statement::Match(match_expr), multiline_content.1));
            }
        }
    }

    // Try to parse let variable declaration
    if first_line.starts_with("let ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "let") {
            if let Ok((_, var_decl)) = crate::parser::var::parse_variable_decl(&multiline_content.0) {
                return Some((Statement::VariableDecl(var_decl), multiline_content.1));
            }
        }
    }

    // Try to parse return statement: return value
    if first_line.starts_with("return ") {
        coffee_debug!("DEBUG: parse_multiline_statement: found return statement, first_line='{}'", first_line);
        if let Ok((remaining, return_stmt)) = crate::parser::var::parse_return(first_line) {
            if remaining.trim().is_empty() {
                return Some((Statement::Return(return_stmt), 1));
            }
        }
        coffee_debug!("DEBUG: parse_multiline_statement: return parsing failed");
        return None;
    }

    // Try to parse raise statement: raise Error(...) or raise Error { ... }
    if first_line.starts_with("raise ") {
        coffee_debug!("DEBUG: parse_multiline_statement: found raise statement, first_line='{}'", first_line);
        let raise_src = if first_line.contains('{') {
            collect_balanced_brace_prefix(lines).unwrap_or_else(|| first_line.to_string())
        } else {
            first_line.to_string()
        };
        let lines_used = raise_src.lines().count().max(1);
        if let Ok((remaining, raise_stmt)) = crate::parser::raise::parse_raise(&raise_src) {
            if remaining.trim().is_empty() {
                return Some((Statement::Raise(raise_stmt), lines_used));
            }
        }
        coffee_debug!("DEBUG: parse_multiline_statement: raise parsing failed");
        return None;
    }

    // Try to parse break statement
    if first_line == "break" {
        coffee_debug!("DEBUG: parse_multiline_statement: found break statement, first_line='{}'", first_line);
        if let Ok((remaining, break_stmt)) = crate::parser::var::parse_break(first_line) {
            if remaining.trim().is_empty() {
                return Some((Statement::Break(break_stmt), 1));
            }
        }
        coffee_debug!("DEBUG: parse_multiline_statement: break parsing failed");
        return None;
    }

    // Try to parse continue statement
    if first_line == "continue" {
        coffee_debug!("DEBUG: parse_multiline_statement: found continue statement, first_line='{}'", first_line);
        if let Ok((remaining, continue_stmt)) = crate::parser::var::parse_continue(first_line) {
            if remaining.trim().is_empty() {
                return Some((Statement::Continue(continue_stmt), 1));
            }
        }
        coffee_debug!("DEBUG: parse_multiline_statement: continue parsing failed");
        return None;
    }

    // Try to parse main entry point: main(function(args))
    if first_line == "main()" || first_line.starts_with("main(") {
        coffee_debug!("DEBUG: parse_multiline_statement: found main() statement, first_line='{}'", first_line);
        match crate::parser::main::parse_main_entry(first_line) {
            Ok((remaining, main_entry)) if remaining.trim().is_empty() => {
                coffee_debug!("DEBUG: parse_multiline_statement: parsed main entry, entry_function='{}', args={:?}", main_entry.entry_function, main_entry.args);
                return Some((Statement::Main(main_entry), 1));
            }
            _ => {
                // Main parsing failed - return None to trigger error detection
                coffee_debug!("DEBUG: parse_multiline_statement: main parsing failed");
                return None;
            }
        }
    }

    // Try to parse struct literal (expression statement)
    if let Some((var_name, value_expr)) = try_parse_assignment(first_line) {
        return Some((Statement::Assignment(var_name, value_expr), 1));
    }

    // Check if first line contains '{' and ends with '}'
    if first_line.contains('{') && first_line.ends_with('}') {
        if let Ok(expr) = crate::parser::expr::parse_expression(first_line) {
            return Some((Statement::Expr(Box::new(expr)), 1));
        }
    }

    // Try to parse multiline struct literal or expression
    // Collect lines until we find a complete expression
    let mut expr_lines = Vec::new();
    let mut brace_depth = 0;
    let mut found_open_brace = false;

    // Only collect multiline expression if the first line contains an opening brace
    if first_line.contains('{') {
        for line in lines.iter() {
            let trimmed = line.trim();
            expr_lines.push(line.to_string());

            // Track brace depth
            for ch in trimmed.chars() {
                if ch == '{' {
                    brace_depth += 1;
                    found_open_brace = true;
                } else if ch == '}' {
                    brace_depth -= 1;
                }
            }

            // If we found an opening brace and the brace depth is back to 0, we have a complete expression
            if found_open_brace && brace_depth == 0 {
                break;
            }

            // Also break if we encounter a new top-level keyword
        if is_top_level_keyword(trimmed) && !trimmed.starts_with("return") {
            break;
        }
    }
    } else if !is_broken_statement_keyword(first_line) && !is_block_starter(first_line) {
        // If first line doesn't contain an opening brace, just parse it as a single-line expression
        if let Ok(expr) = crate::parser::expr::parse_expression(first_line) {
            return Some((Statement::Expr(Box::new(expr)), 1));
        }
    }
    
    if !expr_lines.is_empty() {
        let combined_expr = expr_lines.join("\n");
        if let Ok(expr) = crate::parser::expr::parse_expression(&combined_expr) {
            return Some((Statement::Expr(Box::new(expr)), expr_lines.len()));
        }
    }

    None
}

/// Collect multiline content (refactored version - using stack tracking)
/// 
/// This internal function collects the content of a multiline construct by tracking
/// indentation levels and block structure. It uses a BlockTracker to correctly
/// determine when a block ends, handling nested structures and special cases like
/// elif/else clauses in if statements.
/// 
/// The function is designed to handle Coffee's indentation-based block structure,
/// ensuring that only properly nested content is included in the result.
/// 
/// # Arguments
/// 
/// * `lines` - A slice of string slices representing the lines to process, starting with the first line of the multiline construct
/// * `keyword` - The keyword that started the multiline construct (e.g., "fn", "class", "if") used to detect when a new construct of the same type begins
/// 
/// # Returns
/// 
/// * `Some((String, usize))` - The collected content and number of lines consumed if successful
/// * `None` - If no lines were provided
fn collect_multiline_content(lines: &[&str], keyword: &str) -> Option<(String, usize)> {
    if lines.is_empty() {
        return None;
    }

    coffee_debug!("DEBUG: collect_multiline_content: keyword='{}', lines.len()={}", keyword, lines.len());

    // Pre-allocate capacity: estimate total size (average 60 chars per line)
    let estimated_size = lines.iter().take(20).map(|l| l.len() + 1).sum::<usize>() * 2;
    let mut content = String::with_capacity(estimated_size.max(256));
    let mut lines_consumed = 0;

    // Initialize block tracker
    let mut tracker = BlockTracker::new();
    let mut base_indent = None;
    const MAX_ITER: usize = 1_000_000;

    for (line_idx, line) in lines.iter().enumerate() {
        if line_idx >= MAX_ITER {
            break;
        }

        let line_num = line_idx + 1;
        let trimmed = line.trim();
        let current_indent = line.len() - trimmed.len();

        if lines_consumed == 0 {
            // First line - establish base indent level
            content.push_str(line);
            content.push('\n');
            lines_consumed += 1;
            base_indent = Some(current_indent);

            // Check if this line contains an unclosed brace, bracket, or parenthesis
            let mut brace_depth = 0;
            let mut bracket_depth = 0;
            let mut paren_depth = 0;
            let mut in_string = false;
            let mut string_char = '\0';

            for c in line.chars() {
                if !in_string && (c == '"' || c == '\'') {
                    in_string = true;
                    string_char = c;
                    continue;
                }

                if in_string {
                    if c == '\\' {
                        // Skip the next character (escape sequence)
                        continue;
                    } else if c == string_char {
                        in_string = false;
                    }
                    continue;
                }

                match c {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    '[' => bracket_depth += 1,
                    ']' => bracket_depth -= 1,
                    '(' => paren_depth += 1,
                    ')' => paren_depth -= 1,
                    _ => {}
                }
            }

            // If the line ends with ':', it's a block start
            // If the line has unclosed braces/brackets/parens, it's a multiline expression
            if trimmed.ends_with(':') {
                // Start tracking this block
                if let Some(block_type) = BlockTracker::detect_block_type(line) {
                    tracker.push_block(block_type, current_indent, line_num, true);
                    coffee_debug!("DEBUG: collect_multiline_content: first line, pushed block {:?}, indent={}", block_type, current_indent);
                } else {
                    coffee_debug!("DEBUG: collect_multiline_content: first line, no block type detected");
                }
            } else if brace_depth > 0 || bracket_depth > 0 || paren_depth > 0 {
                // This is a multiline expression (e.g., struct literal)
                // Track the brace/bracket/paren depth
                coffee_debug!("DEBUG: collect_multiline_content: first line, multiline expression, brace_depth={}, bracket_depth={}, paren_depth={}", brace_depth, bracket_depth, paren_depth);
                // Store the depth in the tracker for later use
                tracker.push_block(BlockType::Expression, current_indent, line_num, true);
            } else {
                break; // Single-line statement, no need to collect multiple lines
            }
        } else {
            let base_level = base_indent.unwrap_or(0);

            // Empty lines are always included
            if trimmed.is_empty() {
                content.push_str(line);
                content.push('\n');
                lines_consumed += 1;
                continue;
            }

            // Process this line with the tracker
            tracker.process_line(line, line_num);

            // Check if we should end the block
            let should_end = if current_indent <= base_level {
                // Special check: main( indicates entry point, always end current block
                if trimmed.starts_with("main(") {
                    true
                // Check if this is a continuation of the current block (elif/else for if)
                } else if tracker.is_if_continuation(trimmed) {
                    false // Don't end, this is part of the if block
                } else if tracker.current_block().map_or(false, |b| {
                    // If we're directly inside an if and this is elif/else at same level
                    b.block_type == BlockType::IfStatement &&
                    (trimmed.starts_with("elif ") || trimmed.starts_with("else:")) &&
                    current_indent == b.indent_level
                }) {
                    false // Part of the if block
                } else if is_top_level_keyword(trimmed) && current_indent == base_level {
                    // Sibling construct at the same indent (including another `fn`/`for`/`if`).
                    // elif/else are handled above as if-continuations.
                    coffee_debug!(
                        "DEBUG: should_end = true (sibling keyword at base indent): line='{}', keyword='{}'",
                        trimmed,
                        keyword
                    );
                    true
                } else if current_indent < base_level {
                    true // Dedented past the base level
                } else {
                    false // Continue collecting
                }
            } else {
                false
            };
            
            // Special check: if we're collecting a multiline expression and encounter a memory operation (rm, mv, etc.)
            // or return statement at the base level, end the collection
            let is_memory_op = trimmed.starts_with("rm ") || trimmed.starts_with("mv ") ||
                             trimmed.starts_with("clone ") || trimmed.starts_with("copy ") ||
                             trimmed.starts_with("clean ");
            let is_return = trimmed.starts_with("return ");
            let should_end_for_memory_op = (is_memory_op || is_return) &&
                                           current_indent == base_level;

            coffee_debug!("DEBUG: line='{}', current_indent={}, base_level={:?}, should_end={}, tracker.depth={}, is_memory_op={}, should_end_for_memory_op={}", trimmed, current_indent, base_level, should_end, tracker.depth(), is_memory_op, should_end_for_memory_op);

            if should_end || should_end_for_memory_op {
                break;
            }

            // Include this line
            coffee_debug!("[DEBUG] collect_multiline_content: adding line '{}' to content (current content length = {})", line, content.len());
            content.push_str(line);
            content.push('\n');
            lines_consumed += 1;
        }
    }

    coffee_debug!("[DEBUG] collect_multiline_content: finished collecting, content length = {}, lines_consumed = {}", content.len(), lines_consumed);

    // Validate closure (check for unclosed blocks)
    let _ = tracker.validate_closure();

    Some((content, lines_consumed))
}

/// Collect multiline variable declaration (for struct literals, etc.)
/// 
/// This internal function collects the content of a multiline variable declaration
/// that may contain struct literals or other complex expressions spanning multiple lines.
/// It tracks brace, bracket, and parenthesis depth to correctly determine when the
/// expression ends.
/// 
/// # Arguments
/// 
/// * `lines` - A slice of string slices representing the lines to process, starting with the first line of the variable declaration
/// 
/// # Returns
/// 
/// * `Some((String, usize))` - The collected content and number of lines consumed if successful
/// * `None` - If no lines were provided or the variable declaration is single-line
fn collect_multiline_variable_decl(lines: &[&str]) -> Option<(String, usize)> {
    if lines.is_empty() {
        return None;
    }

    let first_line = lines[0].trim();
    
    // Check if the first line contains an unclosed brace, bracket, or parenthesis
    let mut brace_depth = 0;
    let mut bracket_depth = 0;
    let mut paren_depth = 0;
    let mut in_string = false;
    let mut string_char = '\0';

    for c in first_line.chars() {
        if !in_string && (c == '"' || c == '\'') {
            in_string = true;
            string_char = c;
            continue;
        }
        
        if in_string {
            if c == string_char {
                if brace_depth > 0 || bracket_depth > 0 || paren_depth > 0 {
                    // Don't end string if we're inside a nested structure
                } else {
                    in_string = false;
                }
            }
            continue;
        }
        
        match c {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            _ => {}
        }
    }

    // If all depths are zero, it's a single-line declaration
    if brace_depth == 0 && bracket_depth == 0 && paren_depth == 0 {
        return None;
    }

    // Collect multiline content
    let mut content = String::new();
    let mut lines_consumed = 0;
    const MAX_ITER: usize = 1_000_000;

    for (line_idx, line) in lines.iter().enumerate() {
        if line_idx >= MAX_ITER {
            break;
        }

        let trimmed = line.trim();
        
        // Check if this is a new statement (not part of the variable declaration)
        if line_idx > 0 && !trimmed.is_empty() {
            // Check if it's a keyword that starts a new statement
            let first_word = trimmed.split_whitespace().next().unwrap_or("");
            if matches!(first_word, "let" | "rm" | "mv" | "clone" | "copy" | "clean" | "return" | "break" | "continue" | "if" | "while" | "for" | "match" | "raise") {
                // This is a new statement, stop collecting
                break;
            }
        }

        content.push_str(line);
        lines_consumed += 1;

        // Update depths
        for c in line.chars() {
            if !in_string && (c == '"' || c == '\'') {
                in_string = true;
                string_char = c;
                continue;
            }
            
            if in_string {
                if c == string_char {
                    if brace_depth > 0 || bracket_depth > 0 || paren_depth > 0 {
                        // Don't end string if we're inside a nested structure
                    } else {
                        in_string = false;
                    }
                }
                continue;
            }
            
            match c {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                '[' => bracket_depth += 1,
                ']' => bracket_depth -= 1,
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                _ => {}
            }
        }

        // Check if all depths are zero (expression is complete)
        if brace_depth == 0 && bracket_depth == 0 && paren_depth == 0 && !in_string {
            break;
        }
    }

    Some((content, lines_consumed))
}

/// Check if it's a top-level keyword (optimized version - using first word check)
/// 
/// This internal function quickly determines if a line starts with a keyword that
/// indicates a top-level construct in Coffee syntax. This is used during block
/// parsing to determine when a block should end (e.g., when encountering a new
/// function definition at the same indentation level).
/// 
/// # Arguments
/// 
/// * `line` - A string slice representing the line to check
/// 
/// # Returns
/// 
/// * `true` if the line starts with a top-level keyword
/// * `false` otherwise
fn is_top_level_keyword(line: &str) -> bool {
    // Quick check: get the first word
    let first_word = match line.split_whitespace().next() {
        Some(word) => word,
        None => return false,
    };

    // Use match instead of multiple starts_with, compiler will optimize to lookup table
    match first_word {
        "fn" | "c" | "class" | "packed" | "enum" | "use" | "let" | "if" | "while" | "for" | "match" | "return" | "break" | "continue" => true,
        "main(" => true,  // Special case: main(
        "/#/" | "/#*" => true,  // Comments
        _ => false,
    }
}

/// Get statement type keyword (for quick dispatch)
/// 
/// This enumeration represents the different types of statements that can appear
/// in Coffee source code. It provides a quick way to categorize statements based
/// on their syntax, enabling optimized parsing and processing.
/// 
/// The statement type is used to quickly determine parsing strategies and to
/// categorize statements for analysis and code generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementType {
    /// Function definition (fn or c fn)
    Function,
    /// Class definition (class or packed class)
    Class,
    /// Enum definition
    Enum,
    /// Import statement (use ...)
    Import,
    /// Variable declaration (let ...)
    VariableDecl,
    /// If expression/block
    If,
    /// While loop
    While,
    /// For loop
    For,
    /// Match expression
    Match,
    /// Return statement
    Return,
    /// Break statement
    Break,
    /// Continue statement
    Continue,
    /// Comment (single line or multi-line)
    Comment,
    /// Main entry point (main(...))
    Main,
    /// Memory operation (alloc/free/load/store)
    MemoryOp,    // alloc/free/load/store
    /// Unknown or unrecognized statement type
    Unknown,
}

impl StatementType {
    /// Detect statement type from the beginning of the line
    /// 
    /// This method analyzes the beginning of a line to quickly determine what type
    /// of statement it likely contains. This enables the parser to choose the
    /// appropriate parsing strategy without attempting all possible parsers.
    /// 
    /// # Arguments
    /// 
    /// * `line` - A string slice representing the line to analyze
    /// 
    /// # Returns
    /// 
    /// The detected statement type
    pub fn from_line(line: &str) -> Self {
            let trimmed = line.trim();
        
            // Quick keyword detection
            if trimmed.starts_with("fn ") {
                return StatementType::Function;
            }
            if trimmed.starts_with("packed class ") {
                return StatementType::Class;
            }
            if trimmed.starts_with("class ") {
                return StatementType::Class;
            }
            if trimmed.starts_with("enum ") {
                return StatementType::Enum;
            }
            if trimmed.starts_with("use ") {
                return StatementType::Import;
            }
            if trimmed.starts_with("let ") {
                return StatementType::VariableDecl;
            }
            if trimmed.starts_with("if ") {
                return StatementType::If;
            }
            if trimmed.starts_with("while ") {
                return StatementType::While;
            }
            if trimmed.starts_with("for ") {
                return StatementType::For;
            }
            if trimmed.starts_with("match ") {
                return StatementType::Match;
            }
            if trimmed.starts_with("return ") {
                return StatementType::Return;
            }
            if trimmed == "break" {
                return StatementType::Break;
            }
            if trimmed == "continue" {
                return StatementType::Continue;
            }
            if trimmed.starts_with("/#/") || trimmed.starts_with("/#*") {
                return StatementType::Comment;
            }
            if trimmed.starts_with("main(") {
                return StatementType::Main;
            }
        
            // Memory operations
            if trimmed.starts_with("alloc") || trimmed.starts_with("free") ||
               trimmed.starts_with("load") || trimmed.starts_with("store") {
                return StatementType::MemoryOp;
            }
        
            StatementType::Unknown    }

    /// Check if it's a multiline statement
    /// 
    /// Determines whether statements of this type typically span multiple lines
    /// in Coffee syntax. This is used by the parser to decide whether to use
    /// multiline or single-line parsing strategies.
    /// 
    /// # Returns
    /// 
    /// * `true` if statements of this type are typically multiline
    /// * `false` otherwise
    pub fn is_multiline(&self) -> bool {
        matches!(self,
            StatementType::Function |
            StatementType::Class |
            StatementType::Enum |
            StatementType::If |
            StatementType::While |
            StatementType::For |
            StatementType::Match
        )
    }

    /// Check if it's a control flow statement
    /// 
    /// Determines whether statements of this type are control flow constructs
    /// that affect the execution path of the program.
    /// 
    /// # Returns
    /// 
    /// * `true` if statements of this type are control flow statements
    /// * `false` otherwise
    pub fn is_control_flow(&self) -> bool {
        matches!(self,
            StatementType::If |
            StatementType::While |
            StatementType::For |
            StatementType::Match
        )
    }
}

/// Parse single line statement (optimized version - keyword quick dispatch)
/// 
/// This function attempts to parse a single line of Coffee source code into a
/// Statement. It uses a quick dispatch mechanism based on the first word of the
/// line to determine which specific parser to use, avoiding the overhead of
/// trying all possible parsers sequentially.
/// 
/// The function handles various types of single-line statements including:
/// - Main entry point declarations
/// - Import statements
/// - Variable declarations
/// - Return, break, continue, and raise statements
/// - Comments (both single and multi-line syntax on a single line)
/// - Memory operations (mv, copy, clone, rm, clean)
/// - Expression statements (like standalone function calls)
/// 
/// For each statement type, the function validates that the entire line was
/// consumed by the parser (no trailing unrecognized content).
/// 
/// # Arguments
/// 
/// * `line` - A string slice representing the line to parse
/// 
/// # Returns
/// 
/// * `Some(Statement)` - The parsed statement if successful
/// * `None` - If the line could not be parsed as any valid Coffee statement
pub fn parse_single_line_statement(line: &str) -> Option<Statement> {
    // Cache trim result to avoid repeated computation
    let trimmed = line.trim();

    // Quick dispatch: directly dispatch to corresponding parser based on line beginning
    // This avoids the overhead of trying all parsers

    // Main entry point: main(function(args))
    // Only match if it's the entire line (not part of an expression)
    if trimmed == "main()" || trimmed.starts_with("main(") {
        coffee_debug!("DEBUG: parse_multiline_statement: found main() statement, line='{}'", line);
        match crate::parser::main::parse_main_entry(line) {
            Ok((remaining, main_entry)) if remaining.trim().is_empty() => {
                coffee_debug!("DEBUG: parse_multiline_statement: parsed main entry, entry_function='{}', args={:?}", main_entry.entry_function, main_entry.args);
                return Some(Statement::Main(main_entry));
            }
            _ => {
                // Main parsing failed - return None to trigger error detection
                coffee_debug!("DEBUG: parse_multiline_statement: main parsing failed");
                return None;
            }
        }
    }

    // Import statement: use "module"
    if trimmed.starts_with("use ") {
        if let Ok((remaining, import)) = crate::parser::import::parse_import(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::Import(import));
            }
        }
        return None;
    }

    // Variable declaration: let name: Type = value
    if trimmed.starts_with("let ") {
        if let Ok((remaining, var_decl)) = crate::parser::var::parse_variable_decl(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::VariableDecl(var_decl));
            }
        }
        return None;
    }

    if let Some((var_name, value_expr)) = try_parse_assignment(trimmed) {
        return Some(Statement::Assignment(var_name, value_expr));
    }

    // Return statement: return value
    if trimmed.starts_with("return ") {
        coffee_debug!("DEBUG: parse_single_line_statement: found return statement, line='{}'", line);
        if let Ok((remaining, return_stmt)) = crate::parser::var::parse_return(line) {
            coffee_debug!("DEBUG: parse_single_line_statement: parse_return succeeded, remaining='{}'", remaining);
            if remaining.trim().is_empty() {
                return Some(Statement::Return(return_stmt));
            }
        }
        coffee_debug!("DEBUG: parse_single_line_statement: parse_return failed");
        return None;
    }

    // Raise statement: raise Error(...)
    if trimmed.starts_with("raise ") {
        if let Ok((remaining, raise_stmt)) = crate::parser::raise::parse_raise(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::Raise(raise_stmt));
            }
        }
        return None;
    }

    // Break statement
    if trimmed == "break" {
        if let Ok((remaining, break_stmt)) = crate::parser::var::parse_break(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::Break(break_stmt));
            }
        }
        return None;
    }

    // Continue statement
    if trimmed == "continue" {
        if let Ok((remaining, continue_stmt)) = crate::parser::var::parse_continue(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::Continue(continue_stmt));
            }
        }
        return None;
    }

    // Comments: /#/ ... or /#* ... *#/
    if trimmed.starts_with("/#/") || trimmed.starts_with("/#*") {
        // Try single-line comment first
        if let Ok((remaining, comment)) = crate::parser::comment::parse_single_line_comment(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::SingleLineComment(comment));
            }
        }
        // Try multi-line comment
        if let Ok((remaining, comment)) = crate::parser::comment::parse_multi_line_comment(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::MultiLineComment(comment));
            }
        }
        return None;
    }

    // Memory operations (mv/copy/clone/rm/clean)
    coffee_debug!("DEBUG: parse_single_line_statement: checking memory ops, trimmed='{}', starts_with(rm)={}", trimmed, trimmed.starts_with("rm"));
    if trimmed.starts_with("mv") || trimmed.starts_with("copy") ||
       trimmed.starts_with("clone") || trimmed.starts_with("rm") ||
       trimmed.starts_with("clean") {
        coffee_debug!("DEBUG: parse_single_line_statement: trying to parse memory op: '{}'", line);
        if let Ok((remaining, memory_op)) = crate::parser::memory::parse_memory_op(line) {
            coffee_debug!("DEBUG: parse_single_line_statement: parsed memory op successfully, remaining='{}', memory_op={:?}", remaining, memory_op);
            if remaining.trim().is_empty() {
                return Some(Statement::MemoryOp(memory_op));
            }
        } else {
            coffee_debug!("DEBUG: parse_single_line_statement: failed to parse memory op");
        }
    }

    // Expression statement (function calls, etc.): as a last resort try parsing as expression
    // Do not swallow broken statements (bare `if`/`fn`/`let`, leftover keywords) as Expr.
    if is_broken_statement_keyword(trimmed) {
        return None;
    }
    coffee_debug!("DEBUG: parse_single_line_statement: trying to parse as expression, line='{}'", line);
    match crate::parser::expr::parse_expression(line) {
        Ok(expr) => {
            coffee_debug!("DEBUG: parse_single_line_statement: parsed as expression successfully");
            return Some(Statement::Expr(Box::new(expr)));
        }
        Err(e) => {
            coffee_debug!("DEBUG: parse_single_line_statement: failed to parse as expression, error='{}'", e);
        }
    }

    None
}

#[derive(Debug, PartialEq, Clone)]
/// Represents a statement in the Coffee programming language
/// 
/// This enumeration contains all possible statement types that can appear in
/// Coffee source code. Each variant corresponds to a different syntactic
/// construct in the language, from simple expressions to complex control flow
/// structures. The AST (Abstract Syntax Tree) of a parsed Coffee program is
/// composed of these statement nodes.
/// 
/// The Statement enum provides a unified representation of all Coffee constructs,
/// allowing the compiler to process different language elements in a consistent
/// way during semantic analysis, type checking, and code generation phases.
pub enum Statement {
    /// Import statement (e.g., `use printf in libc of c`)
    Import(Import),
    /// Function definition (e.g., `fn add(x: int, y: int) => int:`)
    Function(Function),
    /// Main entry point (e.g., `main(add(1, 2))`)
    Main(MainEntry),
    /// If expression/block (e.g., `if condition: ... elif condition: ... else: ...`)
    If(IfExpr),
    /// While loop (e.g., `while condition: ...`)
    While(WhileLoop),
    /// Match expression (e.g., `match value: pattern1 => result1, pattern2 => result2`)
    Match(MatchExpr),
    /// For loop (e.g., `for item in collection: ...`)
    For(ForLoop),
    /// Variable declaration (e.g., `let x: int = 5`)
    VariableDecl(VariableDecl),
    /// Assignment statement (e.g., `x = x + 1`)
    Assignment(String, crate::parser::expr::Expression),  // (variable_name, value_expression)
    /// Return statement (e.g., `return value`)
    Return(ReturnStmt),
    /// Break statement (e.g., `break`)
    Break(BreakStmt),
    /// Continue statement (e.g., `continue`)
    Continue(ContinueStmt),
    /// Single-line comment (e.g., `/#/ This is a comment`)
    SingleLineComment(SingleLineComment),
    /// Multi-line comment (e.g., `/#* This is a comment *#/`)
    MultiLineComment(MultiLineComment),
    /// Class definition (e.g., `class MyClass: ...`)
    Class(ClassDef),
    /// Enum definition (e.g., `enum Color: Red, Green, Blue`)
    Enum(EnumDef),
    /// Memory operation (e.g., `mv source target`, `clone source target`, `rm var1, var2`)
    MemoryOp(MemoryOp),
    /// Raise statement (e.g., `raise ErrorType(arguments)`)
    Raise(RaiseStmt),
    /// Expression statement (e.g., standalone function call)
    Expr(Box<crate::parser::expr::Expression>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_program_long_file_beyond_old_cap() {
        let mut src = String::new();
        for i in 0..150 {
            src.push_str(&format!("let x{}: int = {}\n", i, i));
        }
        let program = parse_program(&src).expect("files longer than 100 lines must still parse");
        assert_eq!(program.statements.len(), 150);
    }

    #[test]
    fn long_file_several_functions_all_parse() {
        let mut src = String::new();
        let mut expected = Vec::new();
        for f in 0..6 {
            expected.push(format!("func{}", f));
            src.push_str(&format!("fn func{}() => int:\n", f));
            for i in 0..30 {
                src.push_str(&format!("    let a{}: int = {}\n", i, i));
            }
            for i in 0..30 {
                src.push_str(&format!("    rm a{}\n", i));
            }
            src.push_str("    return 0\n\n");
        }
        assert!(src.lines().count() >= 150, "fixture must exceed 150 lines");
        let program = parse_program(&src).expect("150+ line file with several fn must parse");
        let names: Vec<&str> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Function(func) => Some(func.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn parse_assignment_without_spaces() {
        let program = parse_program("x=1\n").expect("x=1 should parse as assignment");
        assert!(matches!(
            program.statements.as_slice(),
            [Statement::Assignment(name, _)] if name == "x"
        ));
    }

    #[test]
    fn parse_nested_member_assignment_flattens_dotted_name() {
        let program = parse_program("p.x.y=1\n").expect("p.x.y=1 should parse as assignment");
        assert!(matches!(
            program.statements.as_slice(),
            [Statement::Assignment(name, _)] if name == "p.x.y"
        ));
    }

    #[test]
    fn parse_assignment_does_not_treat_comparisons() {
        assert!(
            parse_single_line_statement("x==1")
                .is_none()
                || !matches!(parse_single_line_statement("x==1"), Some(Statement::Assignment(_, _))),
            "== must not parse as assignment"
        );
        assert!(!matches!(
            parse_single_line_statement("x!=1"),
            Some(Statement::Assignment(_, _))
        ));
        assert!(!matches!(
            parse_single_line_statement("x<=1"),
            Some(Statement::Assignment(_, _))
        ));
        assert!(!matches!(
            parse_single_line_statement("x>=1"),
            Some(Statement::Assignment(_, _))
        ));
    }

    #[test]
    fn broken_if_does_not_promote_body_to_top_level() {
        let src = "if\n    let x: int = 1\n";
        match parse_program(src) {
            Ok(program) => panic!(
                "broken if must be a parse error, got statements: {:?}",
                program.statements
            ),
            Err(errors) => {
                assert!(
                    errors.iter().any(|e| matches!(
                        e,
                        ParseError::MissingBlockBody { statement_type, .. } if statement_type == "if"
                    ) || matches!(e, ParseError::GenericSyntaxError { .. })),
                    "expected MissingBlockBody, got {:?}",
                    errors
                );
            }
        }
    }

    #[test]
    fn safety_cap_emits_parse_error() {
        // Directly cover the error variant used when the iteration cap is hit.
        let err = ParseError::GenericSyntaxError {
            line: 1,
            context: String::new(),
            hint: "Parser safety limit reached; remaining input was not parsed".to_string(),
        };
        assert!(err.to_message().contains("safety limit"));
    }
}


#[cfg(test)]
mod parse_program_tests {
    use super::*;

    #[test]
    fn long_file_exceeds_old_100_line_cap() {
        let mut src = String::new();
        for i in 0..150 {
            src.push_str(&format!("let x{}: int = {}\n", i, i));
        }
        let program = parse_program(&src).expect("file longer than 100 lines should still parse");
        let lets = program
            .statements
            .iter()
            .filter(|s| matches!(s, Statement::VariableDecl(_)))
            .count();
        assert_eq!(lets, 150);
    }

    #[test]
    fn eight_top_level_functions_preserve_source_order() {
        let mut src = String::new();
        for i in 0..8 {
            src.push_str(&format!("fn f{}() => int:\n    return {}\n\n", i, i));
        }
        let program = parse_program(&src).expect("eight top-level functions should parse");
        let names: Vec<&str> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Function(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7"]);
    }

    #[test]
    fn parallel_unit_error_uses_absolute_line() {
        let mut src = String::new();
        for i in 0..8 {
            if i == 3 {
                src.push_str("fn\n    return 0\n\n");
            } else {
                src.push_str(&format!("fn f{}() => int:\n    return 0\n\n", i));
            }
        }
        let err = parse_program(&src).expect_err("broken fn among eight units should error");
        assert!(
            err.iter().any(|e| matches!(
                e,
                ParseError::MissingBlockBody { line: 10, .. }
                    | ParseError::InvalidFunctionSyntax { line: 10, .. }
                    | ParseError::GenericSyntaxError { line: 10, .. }
            )),
            "error must attach to the broken unit's source line, got {:?}",
            err
        );
    }

    #[test]
    fn assignment_without_spaces() {
        let program = parse_program("x=1\n").expect("x=1 should parse as assignment");
        assert!(matches!(
            &program.statements[..],
            [Statement::Assignment(name, _)] if name == "x"
        ));
    }

    #[test]
    fn comparison_is_not_assignment() {
        let program = parse_program("x==1\n").expect("x==1 should parse as expression");
        assert!(matches!(program.statements[0], Statement::Expr(_)));
    }

    #[test]
    fn broken_if_does_not_promote_indented_let() {
        let src = "if\n    let x: int = 1\n";
        let err = parse_program(src).expect_err("broken if must be a parse error");
        assert!(err.iter().any(|e| matches!(
            e,
            ParseError::MissingBlockBody { statement_type, .. } if statement_type == "if"
        )));
        // Recovery must consume the indented `let` so we do not also report a second success path
        assert!(parse_program(src).is_err());
    }

    #[test]
    fn factorial_parses_two_functions() {
        let src = r#"
fn factorial(n: int) => int:
    if n <= 1:
        return 1
    else:
        let result: int = n * factorial(n - 1)
        rm n
        return result

fn main() => int:
    let result: int = factorial(5)
    rm result
    return 0
"#;
        let program = parse_program(src).expect("factorial fixture should parse");
        let names: Vec<&str> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Function(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["factorial", "main"]);
    }

    #[test]
    fn safety_cap_emits_error() {
        // Directly exercise the error variant used when the cap is hit
        let err = ParseError::GenericSyntaxError {
            line: 1,
            context: String::new(),
            hint: "Parser safety limit reached; remaining input was not parsed".to_string(),
        };
        assert!(err.to_message().contains("safety limit"));
    }
}

