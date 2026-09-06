use crate::coffee_debug;
use super::ast::Expression;
use super::ident::is_valid_identifier;
use super::parse::parse_expression_at;
use super::scan::matching_close_paren;

pub(super) fn try_fstring(input: &str) -> Option<Result<Expression, String>> {
    if input.starts_with("f\"") && input.ends_with('"') {
        let template = &input[2..input.len()-1];
        return Some(Expression::fstring(template));
    }
    None
}

/// Parentheses: tuple literal or grouping.
pub(super) fn try_paren_or_tuple(input: &str, start: usize) -> Result<Option<Expression>, String> {
    if input.starts_with('(') && matching_close_paren(input, 0) == Some(input.len() - 1) {
        let inner_raw = &input[1..input.len() - 1];
        let inner = inner_raw.trim();

        // Check if it's a tuple (contains comma)
        if inner.contains(',') {
            // It's a tuple literal
            let mut elements = Vec::new();
            let mut current_element = String::new();
            let mut paren_depth = 0;
            let mut brace_depth = 0;
            let mut bracket_depth = 0;
            let mut in_string = false;

            for ch in inner.chars() {
                match ch {
                    '"' if !in_string => {
                        in_string = true;
                        current_element.push(ch);
                    },
                    '"' if in_string => {
                        in_string = false;
                        current_element.push(ch);
                    },
                    '(' if !in_string => {
                        paren_depth += 1;
                        current_element.push(ch);
                    },
                    ')' if !in_string => {
                        paren_depth -= 1;
                        current_element.push(ch);
                    },
                    '{' if !in_string => {
                        brace_depth += 1;
                        current_element.push(ch);
                    },
                    '}' if !in_string => {
                        brace_depth -= 1;
                        current_element.push(ch);
                    },
                    '[' if !in_string => {
                        bracket_depth += 1;
                        current_element.push(ch);
                    },
                    ']' if !in_string => {
                        bracket_depth -= 1;
                        current_element.push(ch);
                    },
                    ',' if !in_string && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                        if !current_element.trim().is_empty() {
                            elements.push(parse_expression_at(current_element.trim(), start + 1)?);
                        }
                        current_element.clear();
                    },
                    _ => current_element.push(ch),
                }
            }

            if !current_element.trim().is_empty() {
                elements.push(parse_expression_at(current_element.trim(), start + 1)?);
            }

            return Ok(Some(Expression::TupleLiteral { elements }));
        } else {
            // It's just grouping parentheses
            return Ok(Some(parse_expression_at(inner_raw, start + 1)?));
        }
    }
    Ok(None)
}

/// Array literals: `[1, 2, 3]`
pub(super) fn try_array_literal(input: &str, start: usize) -> Result<Option<Expression>, String> {
    if input.starts_with('[') && input.ends_with(']') {
        let elements_str = &input[1..input.len()-1].trim();
        let mut elements = Vec::new();

        if !elements_str.is_empty() {
            let mut current_element = String::new();
            let mut paren_depth = 0;
            let mut brace_depth = 0;
            let mut bracket_depth = 0;
            let mut in_string = false;

            for ch in elements_str.chars() {
                match ch {
                    '"' if !in_string => {
                        in_string = true;
                        current_element.push(ch);
                    },
                    '"' if in_string => {
                        in_string = false;
                        current_element.push(ch);
                    },
                    '(' if !in_string => {
                        paren_depth += 1;
                        current_element.push(ch);
                    },
                    ')' if !in_string => {
                        paren_depth -= 1;
                        current_element.push(ch);
                    },
                    '{' if !in_string => {
                        brace_depth += 1;
                        current_element.push(ch);
                    },
                    '}' if !in_string => {
                        brace_depth -= 1;
                        current_element.push(ch);
                    },
                    '[' if !in_string => {
                        bracket_depth += 1;
                        current_element.push(ch);
                    },
                    ']' if !in_string => {
                        bracket_depth -= 1;
                        current_element.push(ch);
                    },
                    ',' if !in_string && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                        if !current_element.trim().is_empty() {
                            elements.push(parse_expression_at(current_element.trim(), start + 1)?);
                        }
                        current_element.clear();
                    },
                    _ => current_element.push(ch),
                }
            }

            if !current_element.trim().is_empty() {
                elements.push(parse_expression_at(current_element.trim(), start + 1)?);
            }
        }

        return Ok(Some(Expression::ArrayLiteral { elements }));
    }
    Ok(None)
}

/// Struct literals: `Point { x: 10, y: 20 }`
pub(super) fn try_struct_literal(input: &str, start: usize) -> Result<Option<Expression>, String> {
    if let Some(pos) = input.find('{') {
        if input.ends_with('}') {
            let struct_name = input[..pos].trim();
            let fields_str = &input[pos + 1..input.len() - 1].trim();
            coffee_debug!("DEBUG: parse_expression struct literal: struct_name='{}', fields_str='{}'", struct_name, fields_str);

            // Validate struct_name - must be a valid identifier (only alphanumeric and underscore)
            if !struct_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                coffee_debug!("DEBUG: parse_expression struct literal: struct_name is not a valid identifier, skipping");
            } else {
                // Parse fields: x: 10, y: 20
            // Use smart parsing to handle commas in expressions (like Point::new(x1, y1))
            let mut fields = Vec::new();
            if !fields_str.is_empty() {
                let mut current_field = String::new();
                let mut paren_depth = 0;  // Track parentheses depth
                let mut brace_depth = 0;  // Track braces depth
                let mut bracket_depth = 0;  // Track array `[...]` depth
                let mut in_string = false;

                for ch in fields_str.chars() {
                    match ch {
                        '"' if !in_string => {
                            in_string = true;
                            current_field.push(ch);  // Add opening quote
                        },
                        '"' if in_string => {
                            in_string = false;
                            current_field.push(ch);  // Add closing quote
                        },
                        '(' if !in_string => {
                            paren_depth += 1;
                            current_field.push(ch);
                        },
                        ')' if !in_string => {
                            paren_depth -= 1;
                            current_field.push(ch);
                        },
                        '{' if !in_string => {
                            brace_depth += 1;
                            current_field.push(ch);
                        },
                        '}' if !in_string => {
                            brace_depth -= 1;
                            current_field.push(ch);
                        },
                        '[' if !in_string => {
                            bracket_depth += 1;
                            current_field.push(ch);
                        },
                        ']' if !in_string => {
                            bracket_depth -= 1;
                            current_field.push(ch);
                        },
                        ',' if !in_string && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                            let field_pair = current_field.trim();
                            if !field_pair.is_empty() {
                                // Find the first colon that is not part of :: (double colon)
                                // This handles cases like "field: Point::new(x, y)"
                                let mut colon_pos = None;
                                let mut chars = field_pair.chars().enumerate();
                                while let Some((i, ch)) = chars.next() {
                                    if ch == ':' {
                                        // Check if this is part of ::
                                        if let Some(next_ch) = field_pair.chars().nth(i + 1) {
                                            if next_ch == ':' {
                                                // This is ::, skip both characters
                                                chars.next(); // skip the second colon
                                            } else {
                                                // This is a single colon, this is the field separator
                                                colon_pos = Some(i);
                                                break;
                                            }
                                        } else {
                                            // Single colon at the end
                                            colon_pos = Some(i);
                                            break;
                                        }
                                    }
                                }

                                if let Some(pos) = colon_pos {
                                    let field_name = field_pair[..pos].trim();
                                    let field_value = field_pair[pos + 1..].trim();
                                    coffee_debug!("DEBUG: parse_expression struct field: field_pair='{}', field_name='{}', field_value='{}'", field_pair, field_name, field_value);
                                    fields.push((field_name.to_string(), parse_expression_at(field_value, start)?));
                                }
                            }
                            current_field.clear();
                        }
                        _ => current_field.push(ch),
                    }
                }
                
                // Don't forget the last field
                let field_pair = current_field.trim();
                if !field_pair.is_empty() {
                    // Find the first colon that is not part of :: (double colon)
                    let mut colon_pos = None;
                    let mut chars = field_pair.chars().enumerate();
                    while let Some((i, ch)) = chars.next() {
                        if ch == ':' {
                            // Check if this is part of ::
                            if let Some(next_ch) = field_pair.chars().nth(i + 1) {
                                if next_ch == ':' {
                                    // This is ::, skip both characters
                                    chars.next(); // skip the second colon
                                } else {
                                    // This is a single colon, this is the field separator
                                    colon_pos = Some(i);
                                    break;
                                }
                            } else {
                                // Single colon at the end
                                colon_pos = Some(i);
                                break;
                            }
                        }
                    }

                    if let Some(pos) = colon_pos {
                        let field_name = field_pair[..pos].trim();
                        let field_value = field_pair[pos + 1..].trim();
                        coffee_debug!("DEBUG: parse_expression struct field (last): field_name='{}', field_value='{}'", field_name, field_value);
                        fields.push((field_name.to_string(), parse_expression_at(field_value, start)?));
                    }
                }
            }

            return Ok(Some(Expression::StructLiteral {
                struct_name: struct_name.to_string(),
                fields,
            }));
            }
        }
    }
    Ok(None)
}

/// Literals and variables (final fallback of `parse_expression`).
pub(super) fn parse_atom(input: &str) -> Result<Expression, String> {
    // Check if it's a string literal
    if input.starts_with('"') && input.ends_with('"') {
        return Ok(Expression::literal(input));
    }

    // Check if it's a boolean literal
    if input == "true" || input == "false" {
        return Ok(Expression::literal(input));
    }

    // Check if it's a number
    if input.parse::<i64>().is_ok() || input.parse::<f64>().is_ok() {
        return Ok(Expression::literal(input));
    }

    // Otherwise, treat as variable - but validate it's a valid identifier
    if is_valid_identifier(input) {
        Ok(Expression::var(input))
    } else {
        Err(format!("Invalid identifier '{}': contains invalid characters", input))
    }
}
