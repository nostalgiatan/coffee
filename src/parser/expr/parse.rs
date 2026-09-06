use super::ast::Expression;
use super::binary::find_lowest_prec_split;
use super::ident;
use super::postfix;
use super::primary;
use super::unary;
use crate::types::definition::Span;

pub use ident::is_valid_identifier;

pub(crate) fn assignment_rhs(s: &str) -> Result<Expression, String> {
    parse_expression(s)
}

/// Parse `input` with spans relative to byte `0` of the (trimmed) slice.
pub fn parse_expression(input: &str) -> Result<Expression, String> {
    parse_expression_at(input.trim(), 0)
}

/// Parse `input` with spans in the same coordinate system as `base`.
///
/// Leading/trailing whitespace is skipped; `base` is the offset of `input[0]`
/// (before trim). The returned node is `Spanned` over the trimmed range.
pub fn parse_expression_at(input: &str, base: usize) -> Result<Expression, String> {
    let lead = input.len() - input.trim_start().len();
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Empty expression".to_string());
    }
    let start = base + lead;
    let inner = parse_expression_kind(trimmed, start)?;
    Ok(Expression::Spanned {
        span: Span::new(start, start + trimmed.len()),
        inner: Box::new(inner),
    })
}

/// Simple recursive descent expression parser (`input` is already trimmed).
fn parse_expression_kind(input: &str, start: usize) -> Result<Expression, String> {
    if let Some(result) = primary::try_fstring(input) {
        return result;
    }

    if let Some(expr) = unary::try_unary(input, start)? {
        return Ok(expr);
    }

    if let Some(expr) = primary::try_paren_or_tuple(input, start)? {
        return Ok(expr);
    }

    if let Some(expr) = primary::try_array_literal(input, start)? {
        return Ok(expr);
    }

    if let Some(expr) = postfix::try_index(input, start)? {
        return Ok(expr);
    }

    if let Some(expr) = primary::try_struct_literal(input, start)? {
        return Ok(expr);
    }

    if let Some(expr) = postfix::try_call(input, start)? {
        return Ok(expr);
    }

    if let Some(expr) = postfix::try_member(input, start)? {
        return Ok(expr);
    }

    if let Some(expr) = postfix::try_as_cast(input, start)? {
        return Ok(expr);
    }

    let mut in_string = false;
    for (i, c) in input.char_indices() {
        if c == '"' && (i == 0 || input.chars().nth(i - 1) != Some('\\')) {
            in_string = !in_string;
        }
    }
    if !in_string {
        if let Some((pos, op)) = find_lowest_prec_split(input) {
            if pos > 0 {
                let left_raw = &input[..pos];
                let right_raw = &input[pos + op.len()..];
                if !left_raw.trim().is_empty() && !right_raw.trim().is_empty() {
                    let left = parse_expression_at(left_raw, start)?;
                    let right = parse_expression_at(right_raw, start + pos + op.len())?;
                    return Ok(Expression::Binary {
                        left: Box::new(left),
                        op: op.to_string(),
                        right: Box::new(right),
                    });
                }
            }
        }
    }

    primary::parse_atom(input)
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
