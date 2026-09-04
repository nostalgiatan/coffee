// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License

//! Raise statement parsing: `raise Error(...)`

use super::expr::Expression;
use nom::{
    bytes::complete::{tag, take_till},
    character::complete::space1,
    IResult,
};

/// Raise statement: raise Error(...)
#[derive(Debug, PartialEq, Clone)]
pub struct RaiseStmt {
    pub error_expr: Expression,
}

/// Parse raise statement
/// `raise Error(...)`
pub fn parse_raise(input: &str) -> IResult<&str, RaiseStmt> {
    let (input, _) = tag("raise")(input)?;
    let (input, _) = space1(input)?;

    // Parse the error expression (e.g., DivErr(b) or DivisionByZeroError(10))
    // Take until newline or end of input
    let (input, error_expr) = take_till(|c| c == '\n' || c == '\0')(input)?;

    Ok((
        input,
        RaiseStmt {
            error_expr: crate::parser::expr::parse_expression(error_expr.trim())
                .unwrap_or_else(|_| Expression::Literal(error_expr.trim().to_string())),
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
        assert_eq!(remaining, "\n");
    }

    #[test]
    fn test_parse_raise_with_message() {
        let input = "raise DivErr(10)\n";
        let (remaining, stmt) = parse_raise(input).unwrap();
        assert_eq!(stmt.error_expr.to_string(), "DivErr(10)");
        assert_eq!(remaining, "\n");
    }

    #[test]
    fn test_parse_raise_with_spaces() {
        let input = "raise  DivErr(0)\n";
        let (remaining, stmt) = parse_raise(input).unwrap();
        assert_eq!(stmt.error_expr.to_string(), "DivErr(0)");
        assert_eq!(remaining, "\n");
    }
}
