// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License

//! Raise statement parsing: `raise Error(...)`

use super::expr::Expression;
use nom::{
    bytes::complete::tag,
    character::complete::space1,
    IResult,
};

/// Raise statement: raise Error(...)
#[derive(Debug, PartialEq, Clone)]
pub struct RaiseStmt {
    pub error_expr: Expression,
}

/// Parse raise statement
/// `raise Error(...)` or `raise Error { field: value, ... }` (possibly multiline).
pub fn parse_raise(input: &str) -> IResult<&str, RaiseStmt> {
    let (input, _) = tag("raise")(input)?;
    let (input, _) = space1(input)?;

    let error_expr_src = input.trim();
    let error_expr = crate::parser::expr::parse_expression(error_expr_src)
        .unwrap_or_else(|_| Expression::Literal(error_expr_src.to_string()));

    Ok((
        "",
        RaiseStmt {
            error_expr,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_raise_simple() {
        let input = "raise DivErr(b)\n";
        let (remaining, stmt) = parse_raise(input).unwrap();
        assert_eq!(stmt.error_expr.to_string(), "DivErr(b)");
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_parse_raise_with_message() {
        let input = "raise DivErr(10)\n";
        let (remaining, stmt) = parse_raise(input).unwrap();
        assert_eq!(stmt.error_expr.to_string(), "DivErr(10)");
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_parse_raise_with_spaces() {
        let input = "raise  DivErr(0)\n";
        let (remaining, stmt) = parse_raise(input).unwrap();
        assert_eq!(stmt.error_expr.to_string(), "DivErr(0)");
        assert_eq!(remaining, "");
    }
}
