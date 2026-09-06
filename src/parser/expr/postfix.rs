use crate::coffee_debug;
use super::ast::Expression;
use super::ident::is_valid_identifier;
use super::parse::parse_expression_at;
use super::scan::{find_last_dot_outside_parens, matching_close_paren, parse_arg_list};

/// Array indexing: `array[index]`
pub(super) fn try_index(input: &str, start: usize) -> Result<Option<Expression>, String> {
    if let Some(pos) = input.find('[') {
        if input.ends_with(']') {
            let array_expr = parse_expression_at(&input[..pos], start)?;
            let index_expr =
                parse_expression_at(&input[pos + 1..input.len() - 1], start + pos + 1)?;

            return Ok(Some(Expression::Index {
                array: Box::new(array_expr),
                index: Box::new(index_expr),
            }));
        }
    }
    Ok(None)
}

/// Function calls: `name(args)` or `class::new(args)` or `Enum::Variant(args)`
pub(super) fn try_call(input: &str, start: usize) -> Result<Option<Expression>, String> {
        if let Some(pos) = input.find('(') {
            if matching_close_paren(input, pos) == Some(input.len() - 1) {
                let func_name = input[..pos].trim().to_string();
                let args_raw = &input[pos + 1..input.len() - 1];
                let args_str = args_raw.trim();
                coffee_debug!("DEBUG: parse_expression: input='{}', pos={}, func_name='{}', args_str='{}'", input, pos, func_name, args_str);
    
                if func_name.contains('.') || func_name.contains('+') || func_name.contains('-') || func_name.contains('*') || func_name.contains('/') || func_name.contains('%') || func_name.contains('&') || func_name.contains('|') || func_name.contains('^') || func_name.contains('<') || func_name.contains('>') || func_name.contains('=') || func_name.contains('!') {
                    coffee_debug!("DEBUG: parse_expression: func_name contains operator or '.', skipping function call parsing");
                } else {
                if matches!(func_name.as_str(), "int" | "float" | "bool") {
                    coffee_debug!("DEBUG: parse_expression: type conversion function '{}'", func_name);
                    let arg_expr = parse_expression_at(args_raw, start + pos + 1)?;
                    return Ok(Some(Expression::TypeCast {
                        target_type: func_name.clone(),
                        value: Box::new(arg_expr),
                    }));
                }
                
                coffee_debug!("DEBUG: parse_expression: func_name.contains(\"::new\")={}", func_name.contains("::new"));
                if func_name.contains("::new") {
                    let parts: Vec<&str> = func_name.split("::").collect();
                    if parts.len() == 2 && parts[1] == "new" {
                        let class_name = parts[0].to_string();
                        let args = parse_arg_list(args_raw, start + pos + 1)?;
    
                        return Ok(Some(Expression::ConstructorCall {
                            class_name,
                            args,
                        }));
                    }
                }
                
                if func_name.contains("::") {
                    let parts: Vec<&str> = func_name.split("::").collect();
                    if parts.len() == 2 {
                        let enum_name = parts[0].to_string();
                        let variant_name = parts[1].to_string();
                        let args = parse_arg_list(args_raw, start + pos + 1)?;
                        return Ok(Some(Expression::Call {
                            function: Box::new(Expression::var(&format!("{}::{}", enum_name, variant_name))),
                            args,
                        }));
                    }
                }

                let args = parse_arg_list(args_raw, start + pos + 1)?;
                return Ok(Some(Expression::Call {
                    function: Box::new(Expression::var(func_name)),
                    args,
                }));
            }
        }    }
    Ok(None)
}

/// Member access: `object.method(args)` or `object.field`
pub(super) fn try_member(input: &str, start: usize) -> Result<Option<Expression>, String> {
    if !input.starts_with('"') && !input.ends_with('"') && input.contains('.') {
        if let Some(pos) = find_last_dot_outside_parens(input) {
            let object_raw = &input[..pos];
            let rest_raw = &input[pos + 1..];
            let object_str = object_raw.trim();
            let rest = rest_raw.trim();
            let object_ok = !object_str.is_empty()
                && object_str.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
            if object_ok && !object_str.chars().all(|c| c.is_ascii_digit() || c == '.') {
                let is_float_literal = object_str.parse::<f64>().is_ok() && rest.parse::<u64>().is_ok();
                if !is_float_literal {
                    if rest.contains('(') && rest.ends_with(')') {
                        let method_name = &rest[..rest.find('(').unwrap()];
                        if is_valid_identifier(method_name) {
                            let paren_in_rest = rest.find('(').unwrap();
                            let rest_lead = rest_raw.len() - rest_raw.trim_start().len();
                            let args_base = start + pos + 1 + rest_lead + paren_in_rest + 1;
                            let args_raw = &rest[method_name.len() + 1..rest.len() - 1];
                            let args = parse_arg_list(args_raw, args_base)?;
                            if method_name == "assign" {
                                if let Some(assign) = try_parse_assign_expr(object_raw, start, &args)? {
                                    return Ok(Some(assign));
                                }
                            }
                            return Ok(Some(Expression::Member {
                                object: Box::new(parse_expression_at(object_raw, start)?),
                                field: method_name.to_string(),
                                args,
                            }));
                        }
                    } else if is_valid_identifier(rest) {
                        return Ok(Some(Expression::Member {
                            object: Box::new(parse_expression_at(object_raw, start)?),
                            field: rest.to_string(),
                            args: Vec::new(),
                        }));
                    }
                }
            }
        }
    }
    Ok(None)
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
