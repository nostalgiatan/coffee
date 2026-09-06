use std::borrow::Cow;
use std::cell::Cell;

use crate::coffee_debug;

use super::line::{is_broken_statement_keyword, is_top_level_keyword, try_parse_assignment};
use super::{BlockTracker, BlockType, ParseError, Statement};

thread_local! {
    /// File origin for the source currently being parsed by `parse_program`.
    /// `(ptr, len)` of the original buffer so nested parsers can recover file offsets
    /// from substrings even when they were not passed an explicit `base`.
    static SOURCE_ORIGIN: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}

pub(super) fn with_source_origin<T>(source: &str, f: impl FnOnce() -> T) -> T {
    SOURCE_ORIGIN.with(|cell| {
        let prev = cell.replace(Some((source.as_ptr() as usize, source.len())));
        let _guard = scopeguard::guard((), |_| {
            cell.set(prev);
        });
        f()
    })
}

/// Byte offset of `s[0]` in the `parse_program` source, or `fallback`.
pub(super) fn slice_base(s: &str, fallback: usize) -> usize {
    SOURCE_ORIGIN.with(|cell| match cell.get() {
        Some((ptr, len)) => {
            let p = s.as_ptr() as usize;
            if p >= ptr && p + s.len() <= ptr + len {
                p - ptr
            } else {
                fallback
            }
        }
        None => fallback,
    })
}

fn subslice_offset(parent: &str, child: &str) -> Option<usize> {
    let p = parent.as_ptr() as usize;
    let c = child.as_ptr() as usize;
    if c >= p && c + child.len() <= p + parent.len() {
        Some(c - p)
    } else {
        None
    }
}

/// File (or parent) offset of `raw[0]` before trim.
pub(super) fn expr_base(parent: &str, parent_base: usize, raw: &str) -> usize {
    let fallback = match subslice_offset(parent, raw) {
        Some(off) => parent_base + off,
        None => parent_base,
    };
    slice_base(raw, fallback)
}

pub(super) fn parse_expr_at<'a>(
    parent: &str,
    parent_base: usize,
    raw: &'a str,
) -> Result<crate::parser::expr::Expression, nom::Err<nom::error::Error<&'a str>>> {
    crate::parser::expr::parse_expression_at(raw, expr_base(parent, parent_base, raw)).map_err(
        |_| {
            nom::Err::Error(nom::error::Error {
                input: raw,
                code: nom::error::ErrorKind::Fail,
            })
        },
    )
}

/// Reconstruct a contiguous `&str` covering `lines[..n]` when they share one allocation.
fn contiguous_source<'a>(lines: &'a [&str], n: usize) -> Option<&'a str> {
    if n == 0 || n > lines.len() {
        return None;
    }
    let first = lines[0];
    let last = lines[n - 1];
    let start = first.as_ptr() as usize;
    let end = last.as_ptr() as usize + last.len();
    if end < start {
        return None;
    }
    let len = end - start;
    for line in &lines[..n] {
        let p = line.as_ptr() as usize;
        if p < start || p + line.len() > start + len {
            return None;
        }
    }
    // SAFETY: `lines[..n]` are subslices of one UTF-8 string; `[first, last]` is that span.
    Some(unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(first.as_ptr(), len)) })
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
pub(super) fn collect_multiline_comment(lines: &[&str]) -> Option<(String, usize)> {
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

pub(super) fn is_block_starter(trimmed: &str) -> bool {
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

pub(super) fn block_parse_error(line_num: usize, lines: &[&str]) -> ParseError {
    let first = lines.first().map(|s| s.trim()).unwrap_or("");
    let detected = ParseError::detect_error_in_block(line_num, lines);
    if detected.line() == line_num {
        if matches!(detected, ParseError::GenericSyntaxError { .. }) {
            return ParseError::MissingBlockBody {
                line: line_num,
                statement_type: block_starter_name(first).to_string(),
            };
        }
    }
    detected
}

/// Consume the starter line plus any following indented body so it is not reparsed top-level.
pub(super) fn skip_failed_block(lines: &[&str], start: usize) -> usize {
    let mut cache = crate::parser::indent::IndentCache::new();
    cache.indented_block_end(lines, start).saturating_sub(start).max(1)
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
    parse_multiline_statement_at(lines, 0)
}

/// `base` is the file offset of `lines[0][0]` (before trim). Nested callers may pass `0`;
/// when `parse_program` has set the source origin, offsets are recovered from the slice.
pub fn parse_multiline_statement_at(lines: &[&str], base: usize) -> Option<(Statement, usize)> {
    if lines.is_empty() {
        return None;
    }

    let stmt_base = slice_base(lines[0], base);
    let first_line = lines[0].trim();

    // Try to parse function (supports fn and c fn)
    if first_line.starts_with("fn ") || first_line.starts_with("c fn ") {
        coffee_debug!("DEBUG: parse_multiline_statement: found function definition, first_line='{}'", first_line);
        // Try to parse current line and following lines directly
        if let Some(multiline_content) = collect_multiline_content(lines, "fn") {
            coffee_debug!("DEBUG: parse_multiline_statement: collected multiline_content='{}'", multiline_content.0);
            // Try direct parsing - parser will handle c fn prefix
            if let Ok((_, func)) = crate::parser::function::parse_function(multiline_content.0.as_ref()) {
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
            if let Ok((_, class)) = crate::parser::class::parse_packed_class(multiline_content.0.as_ref()) {
                return Some((Statement::Class(class), multiline_content.1));
            }
        }
    }

    // Try to parse normal class
    if first_line.starts_with("class ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "class") {
            if let Ok((_, class)) = crate::parser::class::parse_class(multiline_content.0.as_ref()) {
                return Some((Statement::Class(class), multiline_content.1));
            } else {
            }
        } else {
        }
    }

    // Try to parse enum
    if first_line.starts_with("enum ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "enum") {
            if let Ok((_, enum_def)) = crate::parser::class::parse_enum(multiline_content.0.as_ref()) {
                return Some((Statement::Enum(enum_def), multiline_content.1));
            }
        }
    }

    // Try to parse if expression
    if first_line.starts_with("if ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "if") {
            let src = multiline_content.0.as_ref();
            if let Ok((_, if_expr)) = crate::parser::r#if::parse_if_at(src, slice_base(src, stmt_base)) {
                return Some((Statement::If(if_expr), multiline_content.1));
            }
        }
    }

    // Try to parse while loop
    if first_line.starts_with("while ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "while") {
            let src = multiline_content.0.as_ref();
            if let Ok((_, while_loop)) =
                crate::parser::r#while::parse_while_at(src, slice_base(src, stmt_base))
            {
                return Some((Statement::While(while_loop), multiline_content.1));
            }
        }
    }

    // Try to parse for loop
    if first_line.starts_with("for ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "for") {
            let src = multiline_content.0.as_ref();
            if let Ok((_, for_loop)) =
                crate::parser::r#for::parse_for_at(src, slice_base(src, stmt_base))
            {
                return Some((Statement::For(for_loop), multiline_content.1));
            }
        }
    }

    // Try to parse match expression
    if first_line.starts_with("match ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "match") {
            let src = multiline_content.0.as_ref();
            if let Ok((_, match_expr)) =
                crate::parser::r#match::parse_match_at(src, slice_base(src, stmt_base))
            {
                return Some((Statement::Match(match_expr), multiline_content.1));
            }
        }
    }

    // Try to parse let variable declaration
    if first_line.starts_with("let ") {
        if let Some(multiline_content) = collect_multiline_content(lines, "let") {
            let src = multiline_content.0.as_ref();
            if let Ok((_, var_decl)) =
                crate::parser::var::parse_variable_decl_at(src, slice_base(src, stmt_base))
            {
                return Some((Statement::VariableDecl(var_decl), multiline_content.1));
            }
        }
    }

    // Try to parse return statement: return value
    if first_line.starts_with("return ") {
        coffee_debug!("DEBUG: parse_multiline_statement: found return statement, first_line='{}'", first_line);
        if let Ok((remaining, return_stmt)) =
            crate::parser::var::parse_return_at(first_line, slice_base(first_line, stmt_base))
        {
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
        let raise_owned;
        let raise_src: &str = if first_line.contains('{') {
            raise_owned = collect_balanced_brace_prefix(lines).unwrap_or_else(|| first_line.to_string());
            &raise_owned
        } else {
            first_line
        };
        let lines_used = raise_src.lines().count().max(1);
        if let Ok((remaining, raise_stmt)) =
            crate::parser::raise::parse_raise_at(raise_src, slice_base(raise_src, stmt_base))
        {
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
        if let Ok(expr) =
            crate::parser::expr::parse_expression_at(first_line, slice_base(first_line, stmt_base))
        {
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
        if let Ok(expr) =
            crate::parser::expr::parse_expression_at(first_line, slice_base(first_line, stmt_base))
        {
            return Some((Statement::Expr(Box::new(expr)), 1));
        }
    }
    
    if !expr_lines.is_empty() {
        let combined_expr = expr_lines.join("\n");
        if let Ok(expr) =
            crate::parser::expr::parse_expression_at(&combined_expr, slice_base(first_line, stmt_base))
        {
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
fn collect_multiline_content<'a>(lines: &'a [&str], keyword: &str) -> Option<(Cow<'a, str>, usize)> {
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
            let is_raise = trimmed.starts_with("raise ");
            let should_end_for_memory_op = (is_memory_op || is_return || is_raise) &&
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

    let collected = contiguous_source(lines, lines_consumed)
        .map(Cow::Borrowed)
        .unwrap_or(Cow::Owned(content));
    Some((collected, lines_consumed))
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
pub(crate) fn collect_multiline_variable_decl(lines: &[&str]) -> Option<(String, usize)> {
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
