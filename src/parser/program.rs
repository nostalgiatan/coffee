use rayon::prelude::*;

use super::line::parse_single_line_statement_at;
use super::multiline::{
    block_parse_error, collect_multiline_comment, collect_multiline_variable_decl, is_block_starter,
    parse_multiline_statement_at, skip_failed_block, with_source_origin,
};
use super::{BlockTracker, BlockType, ParseError, Statement};
use crate::types::definition::Span;

#[derive(Debug, PartialEq, Clone)]
pub struct Program {
    /// List of statements that make up the parsed Coffee program
    /// These statements represent the complete AST of the parsed source code
    pub statements: Vec<Statement>,
    /// Byte ranges in the original source for each `statements` entry (same length).
    /// `parse_program` fills these from the first character of the statement’s first
    /// line through the end of its last consumed line (including that line’s newline).
    /// `Span::new(0, 0)` is only for synthesized programs (tests / `Program::new`).
    pub stmt_spans: Vec<Span>,
}

impl Program {
    /// Build a program whose statements were not parsed from a source buffer.
    ///
    /// Each statement gets a dummy `Span::new(0, 0)` so `stmt_spans.len()` matches
    /// `statements.len()`. Prefer this over a struct literal. Real offsets come
    /// only from `parse_program`.
    pub fn new(statements: Vec<Statement>) -> Self {
        let stmt_spans = vec![Span::new(0, 0); statements.len()];
        Self {
            statements,
            stmt_spans,
        }
    }
}

/// Byte range of each `str::lines()` entry in `input`: first character of the
/// line through the end of that line (including its trailing newline if any).
fn line_byte_ranges(input: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut rest = input;
    let mut offset = 0usize;
    while !rest.is_empty() {
        let start = offset;
        match rest.find(['\n', '\r']) {
            None => {
                ranges.push((start, start + rest.len()));
                break;
            }
            Some(rel) => {
                let nl_len = if rest.as_bytes()[rel] == b'\r'
                    && rest.as_bytes().get(rel + 1) == Some(&b'\n')
                {
                    2
                } else {
                    1
                };
                let end = start + rel + nl_len;
                ranges.push((start, end));
                rest = &rest[(rel + nl_len)..];
                offset = end;
            }
        }
    }
    ranges
}

fn span_for_consumed_lines(
    line_ranges: &[(usize, usize)],
    start_line: usize,
    lines_consumed: usize,
) -> Span {
    let n = lines_consumed.max(1);
    let start = line_ranges.get(start_line).map(|r| r.0).unwrap_or(0);
    let last = start_line.saturating_add(n.saturating_sub(1));
    let end = line_ranges.get(last).map(|r| r.1).unwrap_or(start);
    Span::new(start, end)
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
    with_source_origin(input, || parse_program_inner(input))
}

fn parse_program_inner(input: &str) -> Result<Program, Vec<ParseError>> {
    let mut statements = Vec::new();
    let mut stmt_spans = Vec::new();
    let mut errors = Vec::new();

    let lines: Vec<&str> = input.lines().collect();
    let line_ranges = line_byte_ranges(input);
    debug_assert_eq!(line_ranges.len(), lines.len());
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
            .map(|&(start, end)| {
                with_source_origin(input, || {
                    let byte_start = line_ranges.get(start).map(|r| r.0).unwrap_or(0);
                    parse_independent_unit(&lines, start, end, byte_start)
                })
            })
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
            let (unit_start, unit_end) = independent[unit_idx];
            let (unit_stmts, unit_errs) = &parsed_units[unit_idx];
            let unit_span = span_for_consumed_lines(
                &line_ranges,
                unit_start,
                unit_end.saturating_sub(unit_start),
            );
            for _ in unit_stmts {
                stmt_spans.push(unit_span);
            }
            statements.extend_from_slice(unit_stmts);
            errors.extend_from_slice(unit_errs);
            i = unit_end;
            unit_idx += 1;
            continue;
        }

        let raw = lines[i];
        let lead = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        let expr_base = line_ranges.get(i).map(|r| r.0).unwrap_or(0) + lead;

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
                    stmt_spans.push(span_for_consumed_lines(&line_ranges, i, lines_consumed));
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
        if let Some((stmt, lines_consumed)) =
            parse_multiline_statement_at(&lines[i..], line_ranges.get(i).map(|r| r.0).unwrap_or(0))
        {
            statements.push(stmt);
            stmt_spans.push(span_for_consumed_lines(&line_ranges, i, lines_consumed));
            // SAFETY: Ensure we always advance at least 1 line to prevent infinite loops
            i += lines_consumed.max(1);
            continue;
        }

        // Failed block recovery: do not parse an indented body as top-level statements
        if is_block_starter(line) {
            let consumed = skip_failed_block(&lines, i);
            errors.push(block_parse_error(i + 1, &lines[i..i + consumed]));
            i += consumed;
            continue;
        }

        // Check for multiline variable declarations (struct literals, etc.)
        let trimmed = line.trim();
        if trimmed.starts_with("let ") {
            // Collect multiline content for variable declarations
            if let Some((multiline_content, lines_consumed)) = collect_multiline_variable_decl(&lines[i..]) {
                if let Ok((remaining, var_decl)) = crate::parser::var::parse_variable_decl_at(
                    &multiline_content,
                    line_ranges.get(i).map(|r| r.0).unwrap_or(0),
                ) {
                    if remaining.trim().is_empty() {
                        statements.push(Statement::VariableDecl(var_decl));
                        stmt_spans.push(span_for_consumed_lines(&line_ranges, i, lines_consumed));
                        i += lines_consumed;
                        continue;
                    }
                }
            }
        }

        // Try to parse single-line statements
        if let Some(stmt) = parse_single_line_statement_at(line, expr_base) {
            statements.push(stmt);
            stmt_spans.push(span_for_consumed_lines(&line_ranges, i, 1));
            i += 1;
        } else {
            // Smart error detection: analyze specific error types
            errors.push(ParseError::detect_error(i + 1, lines[i]));
            i += 1;
        }
    }

    if errors.is_empty() {
        debug_assert_eq!(statements.len(), stmt_spans.len());
        Ok(Program {
            statements,
            stmt_spans,
        })
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
    line_byte_start: usize,
) -> (Vec<Statement>, Vec<ParseError>) {
    let end = end.min(lines.len());
    if start >= end {
        return (Vec::new(), Vec::new());
    }
    let slice = &lines[start..end];

    if let Some((stmt, _)) = parse_multiline_statement_at(slice, line_byte_start) {
        return (vec![stmt], Vec::new());
    }

    if is_block_starter(slice[0].trim()) {
        return (Vec::new(), vec![block_parse_error(start + 1, slice)]);
    }

    let raw = slice[0];
    let lead = raw.len() - raw.trim_start().len();
    match parse_single_line_statement_at(raw.trim(), line_byte_start + lead) {
        Some(stmt) => (vec![stmt], Vec::new()),
        None => (
            Vec::new(),
            vec![ParseError::detect_error(start + 1, slice[0])],
        ),
    }
}
