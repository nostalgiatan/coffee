use nom::{
    bytes::complete::tag,
    character::complete::{char, space0, space1},
    multi::many1,
    IResult, Parser,
};

use super::expr::Expression;
use super::Statement;

/// Match 表达式
/// match value:
///     pattern1 => result1
///     pattern2 => result2
///     _ => default
#[derive(Debug, PartialEq, Clone)]
pub struct MatchExpr {
    pub value: Expression,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct MatchArm {
    pub pattern: Expression,
    pub guard: Option<Expression>,
    pub body: Vec<Statement>,
}

pub fn parse_match(input: &str) -> IResult<&str, MatchExpr> {
    let (input, _) = tag("match")(input)?;
    let (input, _) = space1(input)?;
    let (input, value_raw) = take_until_colon(input)?;
    let value = parse_expr_or_literal(value_raw.trim());
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;

    let (input, arms) = {
        let mut parser = many1(parse_match_arm);
        Parser::parse(&mut parser, input)?
    };

    Ok((
        input,
        MatchExpr {
            value,
            arms,
        },
    ))
}

fn parse_match_arm(input: &str) -> IResult<&str, MatchArm> {
    let (input, _) = space0(input)?;
    let (input, pattern_raw) = take_until_double_arrow(input)?;
    let (input, _) = tag("=>")(input)?;
    let (input, _) = space0(input)?;
    let (input, result) = parse_single_line(input)?;

    let pattern_raw = pattern_raw.trim();
    let (pattern, guard) = if let Some(pos) = pattern_raw.find(" if ") {
        let base = pattern_raw[..pos].trim();
        let guard_s = pattern_raw[pos + 4..].trim();
        (
            parse_expr_or_literal(base),
            Some(parse_expr_or_literal(guard_s)),
        )
    } else {
        (parse_expr_or_literal(pattern_raw), None)
    };

    let line = result.trim();
    let body = if line.is_empty() {
        Vec::new()
    } else if let Some(stmt) = super::parse_single_line_statement(line) {
        vec![stmt]
    } else if let Ok(expr) = crate::parser::expr::parse_expression(line) {
        vec![Statement::Expr(Box::new(expr))]
    } else {
        Vec::new()
    };

    Ok((
        input,
        MatchArm {
            pattern,
            guard,
            body,
        },
    ))
}

fn parse_expr_or_literal(raw: &str) -> Expression {
    crate::parser::expr::parse_expression(raw)
        .unwrap_or_else(|_| Expression::Literal(raw.to_string()))
}

fn take_until_colon(input: &str) -> IResult<&str, &str> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ':' {
            if i + 1 < chars.len() && chars[i + 1] == ':' {
                i += 2;
                continue;
            }
            let byte_pos = input.char_indices().nth(i).unwrap().0;
            return Ok((&input[byte_pos..], &input[..byte_pos]));
        }
        i += 1;
    }
    Err(nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::TakeUntil,
    }))
}

fn take_until_double_arrow(input: &str) -> IResult<&str, &str> {
    if let Some(pos) = input.find("=>") {
        Ok((&input[pos..], &input[..pos]))
    } else {
        Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::TakeUntil,
        }))
    }
}

fn parse_single_line(input: &str) -> IResult<&str, &str> {
    if let Some(pos) = input.find('\n') {
        Ok((&input[pos..], &input[..pos]))
    } else {
        Ok(("", input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_literal_arm_body_is_stmt() {
        let src = "match x:\n    1 => 10\n    _ => 0\n";
        let (_, m) = parse_match(src).expect("parse");
        match m.value {
            Expression::Variable(n) => assert_eq!(n, "x"),
            other => panic!("{:?}", other),
        }
        assert!(matches!(m.arms[0].pattern, Expression::Literal(_)));
        assert!(matches!(m.arms[0].body[0], Statement::Expr(_)));
        assert!(m.arms[0].guard.is_none());
    }

    #[test]
    fn match_guard_is_binary() {
        let src = "match x:\n    n if n > 5 => 1\n    _ => 0\n";
        let (_, m) = parse_match(src).expect("parse");
        match &m.arms[0].pattern {
            Expression::Variable(n) => assert_eq!(n, "n"),
            other => panic!("{:?}", other),
        }
        match m.arms[0].guard.as_ref() {
            Some(Expression::Binary { op, .. }) => assert_eq!(op, ">"),
            other => panic!("{:?}", other),
        }
    }
}
