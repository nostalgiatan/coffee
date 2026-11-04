use crate::coffee_debug;
use super::ast::Expression;
use super::ident::is_valid_identifier;
use super::parse::parse_expression_at;
use super::scan::parse_arg_list;

/// Byte mask of positions inside a string literal (quotes included).
fn string_mask(input: &str) -> Vec<bool> {
    let bytes = input.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            mask[i] = true;
            if c == b'\\' && i + 1 < bytes.len() {
                mask[i + 1] = true;
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
            mask[i] = true;
        }
        i += 1;
    }
    mask
}

/// Position of the opening bracket that matches the closing one at `close`, scanning back.
fn matching_open(input: &str, close: usize, open_ch: u8, close_ch: u8, mask: &[bool]) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut depth = 0i32;
    for i in (0..=close).rev() {
        if mask[i] {
            continue;
        }
        if bytes[i] == close_ch {
            depth += 1;
        } else if bytes[i] == open_ch {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Last `.` outside any bracket or string.
fn last_top_level_dot(input: &str, mask: &[bool]) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut depth = 0i32;
    let mut last = None;
    for (i, &c) in bytes.iter().enumerate() {
        if mask[i] {
            continue;
        }
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'.' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// Postfix forms, outermost last: `x[i]`, `f(args)`, `x.method(args)`, `x.field`.
/// The final suffix of the expression is peeled off and the rest is parsed recursively,
/// so chains such as `a[1].x` and `c.me().get()` nest correctly.
pub(super) fn try_postfix(input: &str, start: usize) -> Result<Option<Expression>, String> {
    let mask = string_mask(input);
    let last = input.len() - 1;
    match input.as_bytes()[last] {
        b']' if !mask[last] => {
            let Some(open) = matching_open(input, last, b'[', b']', &mask) else {
                return Ok(None);
            };
            if input[..open].trim().is_empty() {
                return Ok(None);
            }
            let array_expr = parse_expression_at(&input[..open], start)?;
            let index_expr = parse_expression_at(&input[open + 1..last], start + open + 1)?;
            Ok(Some(Expression::Index {
                array: Box::new(array_expr),
                index: Box::new(index_expr),
            }))
        }
        b')' if !mask[last] => {
            let Some(open) = matching_open(input, last, b'(', b')', &mask) else {
                return Ok(None);
            };
            let callee_raw = &input[..open];
            let callee = callee_raw.trim_end();
            if callee.trim().is_empty() {
                return Ok(None);
            }
            let args_raw = &input[open + 1..last];
            let args_base = start + open + 1;
            let callee_mask = &mask[..callee.len()];
            if let Some(dot) = last_top_level_dot(callee, callee_mask) {
                let method = callee[dot + 1..].trim();
                let object_raw = &callee[..dot];
                if object_raw.trim().is_empty() || !is_valid_identifier(method) {
                    return Ok(None);
                }
                // `1.5(…)` style input is not a method call.
                if object_raw.trim().chars().all(|c| c.is_ascii_digit()) {
                    return Ok(None);
                }
                let args = parse_arg_list(args_raw, args_base)?;
                if method == "assign" {
                    if let Some(assign) = try_parse_assign_expr(object_raw, start, &args)? {
                        return Ok(Some(assign));
                    }
                }
                return Ok(Some(Expression::Member {
                    object: Box::new(parse_expression_at(object_raw, start)?),
                    field: method.to_string(),
                    args,
                }));
            }
            let func_name = callee.trim().to_string();
            let name_ok = {
                let mut parts = func_name.split("::");
                let first = parts.next().unwrap_or("");
                let second = parts.next();
                parts.next().is_none()
                    && is_valid_identifier(first)
                    && second.map_or(true, is_valid_identifier)
            };
            if !name_ok {
                return Ok(None);
            }
            coffee_debug!("DEBUG: parse_expression: call '{}'", func_name);
            if matches!(func_name.as_str(), "int" | "float" | "bool") {
                let arg_expr = parse_expression_at(args_raw, args_base)?;
                return Ok(Some(Expression::TypeCast {
                    target_type: func_name,
                    value: Box::new(arg_expr),
                }));
            }
            if let Some((class_name, "new")) = func_name.split_once("::") {
                let args = parse_arg_list(args_raw, args_base)?;
                return Ok(Some(Expression::ConstructorCall {
                    class_name: class_name.to_string(),
                    args,
                }));
            }
            let args = parse_arg_list(args_raw, args_base)?;
            Ok(Some(Expression::Call {
                function: Box::new(Expression::var(func_name)),
                args,
            }))
        }
        _ => {
            if input.starts_with('"') && input.ends_with('"') {
                return Ok(None);
            }
            let Some(dot) = last_top_level_dot(input, &mask) else {
                return Ok(None);
            };
            let object_raw = &input[..dot];
            let field = input[dot + 1..].trim();
            let object_str = object_raw.trim();
            if object_str.is_empty() || !is_valid_identifier(field) {
                return Ok(None);
            }
            // `3.14`: digits on both sides of the dot are a float literal, not a field.
            if object_str.chars().all(|c| c.is_ascii_digit()) {
                return Ok(None);
            }
            Ok(Some(Expression::Member {
                object: Box::new(parse_expression_at(object_raw, start)?),
                field: field.to_string(),
                args: Vec::new(),
            }))
        }
    }
}

/// `value as Type` (newtype wrap/unwrap). Last ` as ` at paren/bracket/brace depth 0.
pub(super) fn try_as_cast(input: &str, start: usize) -> Result<Option<Expression>, String> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut last: Option<usize> = None;
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_string {
            if c == '\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ' ' if depth == 0 && bytes.get(i + 1..i + 4) == Some(b"as ") => {
                last = Some(i);
                i += 4;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let Some(pos) = last else {
        return Ok(None);
    };
    let left = input[..pos].trim();
    let right = input[pos + 4..].trim();
    if left.is_empty() || right.is_empty() {
        return Ok(None);
    }
    if crate::parser::ty::parse_type(right)
        .ok()
        .map(|(rest, _)| rest.trim().is_empty())
        != Some(true)
    {
        return Ok(None);
    }
    let value = parse_expression_at(left, start)?;
    Ok(Some(Expression::TypeCast {
        target_type: right.to_string(),
        value: Box::new(value),
    }))
}

fn try_parse_assign_expr(
    object_raw: &str,
    object_base: usize,
    args: &[Expression],
) -> Result<Option<Expression>, String> {
    if args.len() != 2 {
        return Ok(None);
    }
    let Expression::Literal(lit) = args[0].kind() else {
        return Ok(None);
    };
    let t = lit.trim();
    if t.len() < 2 || !t.starts_with('"') || !t.ends_with('"') {
        return Ok(None);
    }
    Ok(Some(Expression::Assign {
        object: Box::new(parse_expression_at(object_raw, object_base)?),
        field_name: t[1..t.len() - 1].to_string(),
        value: Box::new(args[1].clone()),
    }))
}
