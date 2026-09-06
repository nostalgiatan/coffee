use crate::coffee_debug;

use super::Statement;

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
                let left = &trimmed[..i];
                let right = &trimmed[i + 1..];
                if left.trim().is_empty() || right.trim().is_empty() {
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
    match expr.kind() {
        Expression::Variable(name) if !name.is_empty() => Some(name.clone()),
        Expression::Member { object, field, args } if args.is_empty() && !field.is_empty() => {
            let prefix = assignment_lhs_name(object)?;
            Some(format!("{}.{}", prefix, field))
        }
        _ => None,
    }
}

pub(super) fn try_parse_assignment(trimmed: &str) -> Option<(String, crate::parser::expr::Expression)> {
    try_parse_assignment_at(trimmed, 0)
}

fn try_parse_assignment_at(
    trimmed: &str,
    base: usize,
) -> Option<(String, crate::parser::expr::Expression)> {
    if trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("let ")
    {
        return None;
    }
    let (lhs_src, rhs_src) = split_assignment(trimmed)?;
    let rhs_off = trimmed.len() - rhs_src.len();
    let lhs_expr = crate::parser::expr::parse_expression_at(lhs_src, base).ok()?;
    let var_name = assignment_lhs_name(&lhs_expr)?;
    let rhs = crate::parser::expr::parse_expression_at(rhs_src, base + rhs_off).ok()?;
    Some((var_name, rhs))
}

pub(super) fn is_broken_statement_keyword(trimmed: &str) -> bool {
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
            | "type"
            | "elif"
            | "else"
            | "return"
            | "raise"
            | "use"
            | "c"
            | "packed"
    )
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
pub(super) fn is_top_level_keyword(line: &str) -> bool {
    // Quick check: get the first word
    let first_word = match line.split_whitespace().next() {
        Some(word) => word,
        None => return false,
    };

    // Use match instead of multiple starts_with, compiler will optimize to lookup table
    match first_word {
        "fn" | "c" | "class" | "packed" | "enum" | "type" | "use" | "let" | "if" | "while" | "for" | "match" | "return" | "break" | "continue" => true,
        "main(" => true,  // Special case: main(
        "/#/" | "/#*" => true,  // Comments
        _ => false,
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
    parse_single_line_statement_at(line, 0)
}

pub(super) fn parse_single_line_statement_at(line: &str, expr_base: usize) -> Option<Statement> {
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

    // Type declaration: type Name: Source
    if trimmed.starts_with("type ") {
        if let Ok((remaining, type_decl)) = crate::parser::type_decl::parse_type_decl(line) {
            if remaining.trim().is_empty() {
                return Some(Statement::TypeDecl(type_decl));
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

    if let Some((var_name, value_expr)) = try_parse_assignment_at(trimmed, expr_base) {
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
    match crate::parser::expr::parse_expression_at(line, expr_base) {
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

#[cfg(test)]
mod tests {
    use super::try_parse_assignment;

    #[test]
    fn try_parse_assignment_lhs_is_x() {
        let (lhs, _) = try_parse_assignment("x = 1").expect("x = 1");
        assert_eq!(lhs, "x");
    }
}
