use super::ast::Expression;
use super::parse::parse_expression_at;

/// Handle unary operators (prefix `&`, `&mut`, `*`, `!`, `-`, `~`, `clone `).
pub(super) fn try_unary(input: &str, start: usize) -> Result<Option<Expression>, String> {
    if let Some(rest) = input.strip_prefix("&mut ") {
        let operand = parse_expression_at(rest, start + 5)?;
        return Ok(Some(Expression::Unary {
            op: "&mut".to_string(),
            operand: Box::new(operand),
        }));
    }
    if input.starts_with('&') && !input.starts_with("&&") {
        let operand = parse_expression_at(&input[1..], start + 1)?;
        return Ok(Some(Expression::Unary {
            op: "&".to_string(),
            operand: Box::new(operand),
        }));
    }
    if input.starts_with('*') {
        let rest = &input[1..];
        if !rest.trim().is_empty() {
            let operand = parse_expression_at(rest, start + 1)?;
            return Ok(Some(Expression::Unary {
                op: "*".to_string(),
                operand: Box::new(operand),
            }));
        }
    }
    if input.starts_with('!') || input.starts_with('-') || input.starts_with('~') {
        let op = &input[..1];
        let operand = parse_expression_at(&input[1..], start + 1)?;
        return Ok(Some(Expression::Unary {
            op: op.to_string(),
            operand: Box::new(operand),
        }));
    }

    if let Some(rest) = input.strip_prefix("clone ") {
        let operand = parse_expression_at(rest, start + 6)?;
        return Ok(Some(Expression::Unary {
            op: "clone".to_string(),
            operand: Box::new(operand),
        }));
    }

    Ok(None)
}
