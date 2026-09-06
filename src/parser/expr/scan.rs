use super::ast::Expression;
use super::parse::parse_expression_at;

/// Split a comma-separated argument list, respecting nested `()`, `[]`, `{}`, and strings.
pub(super) fn parse_arg_list(args_str: &str, base: usize) -> Result<Vec<Expression>, String> {
    let mut args = Vec::new();
    if args_str.trim().is_empty() {
        return Ok(args);
    }
    let mut start = 0;
    let mut paren_depth = 0;
    let mut brace_depth = 0;
    let mut bracket_depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in args_str.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' => {
                escape_next = true;
            }
            '"' if !in_string => {
                in_string = true;
            }
            '"' if in_string => {
                in_string = false;
            }
            '(' if !in_string => {
                paren_depth += 1;
            }
            ')' if !in_string => {
                paren_depth -= 1;
            }
            '{' if !in_string => {
                brace_depth += 1;
            }
            '}' if !in_string => {
                brace_depth -= 1;
            }
            '[' if !in_string => {
                bracket_depth += 1;
            }
            ']' if !in_string => {
                bracket_depth -= 1;
            }
            ',' if !in_string && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                let slice = &args_str[start..i];
                if !slice.trim().is_empty() {
                    args.push(parse_expression_at(slice, base + start)?);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let slice = &args_str[start..];
    if !slice.trim().is_empty() {
        args.push(parse_expression_at(slice, base + start)?);
    }
    Ok(args)
}

pub(super) fn matching_close_paren(input: &str, open_pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    if open_pos >= bytes.len() || bytes[open_pos] != b'(' {
        return None;
    }
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    for i in open_pos..bytes.len() {
        let ch = bytes[i] as char;
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

pub(super) fn find_last_dot_outside_parens(input: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    let mut last = None;
    for (i, ch) in input.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => depth -= 1,
            '.' if !in_string && depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}
