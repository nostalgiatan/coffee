use nom::{
    bytes::complete::tag,
    character::complete::{char, space0, space1},
    error::ErrorKind,
    multi::many1,
    IResult, Parser,
};

use super::expr::Expression;
use super::pattern::Pattern;
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
    pub pattern: Pattern,
    pub guard: Option<Expression>,
    pub body: Vec<Statement>,
}

#[cfg(test)]
pub fn parse_match(input: &str) -> IResult<&str, MatchExpr> {
    parse_match_at(input, 0)
}

pub fn parse_match_at(input: &str, base: usize) -> IResult<&str, MatchExpr> {
    let src = input;
    let (input, _) = tag("match")(input)?;
    let (input, _) = space1(input)?;
    let (input, value_raw) = super::take_until_header_colon(input)?;
    let value = super::multiline::parse_expr_at(src, base, value_raw.trim())?;
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;

    let (input, arms) = {
        let mut parser = many1(parse_match_arm);
        Parser::parse(&mut parser, input)?
    };

    if remaining_has_non_trivia(input) {
        return Err(nom::Err::Error(nom::error::Error {
            input,
            code: ErrorKind::Fail,
        }));
    }

    Ok((
        input,
        MatchExpr {
            value,
            arms,
        },
    ))
}

fn parse_match_arm(input: &str) -> IResult<&str, MatchArm> {
    let src = input;
    let (input, _) = space0(input)?;
    let (input, pattern_raw) = take_until_double_arrow(input)?;
    let (input, _) = tag("=>")(input)?;
    let (input, _) = space0(input)?;
    let (input, result) = parse_single_line(input)?;

    let pattern_raw = pattern_raw.trim();
    let (pattern, guard) = if let Some(pos) = pattern_raw.find(" if ") {
        let base = pattern_raw[..pos].trim();
        let guard_s = pattern_raw[pos + 4..].trim();
        (parse_pattern(src, base)?, Some(super::multiline::parse_expr_at(src, super::multiline::slice_base(src, 0), guard_s)?))
    } else {
        (parse_pattern(src, pattern_raw)?, None)
    };

    let line = result.trim();
    let body = if line.is_empty() {
        Vec::new()
    } else if let Some(stmt) = super::line::parse_single_line_statement_at(
        line,
        super::multiline::slice_base(line, 0),
    ) {
        vec![stmt]
    } else {
        return Err(nom::Err::Error(nom::error::Error {
            input: line,
            code: ErrorKind::Fail,
        }));
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

fn parse_pattern<'a>(parent: &str, raw: &'a str) -> Result<Pattern, nom::Err<nom::error::Error<&'a str>>> {
    let raw = raw.trim();
    if raw == "_" {
        return Ok(Pattern::Wildcard);
    }
    Pattern::from_expr(super::multiline::parse_expr_at(parent, super::multiline::slice_base(parent, 0), raw)?).map_err(|_| {
        nom::Err::Error(nom::error::Error {
            input: raw,
            code: ErrorKind::Fail,
        })
    })
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

fn remaining_has_non_trivia(input: &str) -> bool {
    input.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with("/#")
    })
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
        match m.value.kind() {
            Expression::Variable(n) => assert_eq!(n, "x"),
            other => panic!("{:?}", other),
        }
        assert!(matches!(m.arms[0].pattern, Pattern::Literal(_)));
        assert!(matches!(m.arms[1].pattern, Pattern::Wildcard));
        assert!(matches!(m.arms[0].body[0], Statement::Expr(_)));
        assert!(m.arms[0].guard.is_none());
    }

    #[test]
    fn match_guard_is_binary() {
        let src = "match x:\n    n if n > 5 => 1\n    _ => 0\n";
        let (_, m) = parse_match(src).expect("parse");
        match &m.arms[0].pattern {
            Pattern::Ident(n) => assert_eq!(n, "n"),
            other => panic!("{:?}", other),
        }
        match m.arms[0].guard.as_ref().map(|e| e.kind()) {
            Some(Expression::Binary { op, .. }) => assert_eq!(op, ">"),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn match_tuple_struct_enum_or_patterns() {
        let src = "match v:\n    (a, _) => 1\n    Point { x: a, y: b } => 2\n    Color.Red => 3\n    1 | 2 => 4\n    _ => 0\n";
        let (_, m) = parse_match(src).expect("parse");
        assert!(matches!(m.arms[0].pattern, Pattern::Tuple(_)));
        assert!(matches!(m.arms[1].pattern, Pattern::Struct { .. }));
        assert!(matches!(m.arms[2].pattern, Pattern::EnumVariant { .. }));
        assert!(matches!(m.arms[3].pattern, Pattern::Or(_)));
        assert!(matches!(m.arms[4].pattern, Pattern::Wildcard));
    }

    #[test]
    fn invalid_match_scrutinee_is_error_not_literal() {
        match parse_match("match @@@:\n    _ => 0\n") {
            Err(_) => {}
            Ok((_, m)) => panic!(
                "invalid match scrutinee must not parse; got {:?}",
                m.value
            ),
        }
    }

    #[test]
    fn invalid_match_pattern_is_error_not_literal() {
        match parse_match("match x:\n    @@@ => 0\n") {
            Err(_) => {}
            Ok((_, m)) => panic!(
                "invalid match pattern must not parse; got {:?}",
                m.arms[0].pattern
            ),
        }
    }

    #[test]
    fn binary_plus_pattern_is_error_not_literal() {
        match parse_match("match x:\n    1 + 2 => 0\n") {
            Err(_) => {}
            Ok((_, m)) => panic!(
                "binary + pattern must not stringify as Literal; got {:?}",
                m.arms[0].pattern
            ),
        }
    }

    #[test]
    fn unary_minus_of_non_literal_pattern_is_error() {
        match parse_match("match x:\n    -n => 0\n") {
            Err(_) => {}
            Ok((_, m)) => panic!(
                "unary minus of non-literal must not stringify as Literal; got {:?}",
                m.arms[0].pattern
            ),
        }
    }

    #[test]
    fn invalid_match_arm_body_is_error_not_empty() {
        match parse_match("match x:\n    _ => @@@\n") {
            Err(_) => {}
            Ok((_, m)) => panic!(
                "invalid match arm body must not parse; got {:?}",
                m.arms[0].body
            ),
        }
    }

    #[test]
    fn leftover_garbage_after_match_arms_is_error() {
        match parse_match("match x:\n    _ => 0\n    @@@\n") {
            Err(_) => {}
            Ok((_, m)) => panic!(
                "garbage after last match arm must not parse; got {:?}",
                m.arms
            ),
        }
    }

    #[test]
    fn broken_let_match_arm_body_is_error_not_expr() {
        match parse_match("match x:\n    _ => let\n") {
            Err(_) => {}
            Ok((_, m)) => panic!(
                "bare let in match arm must not parse as expr; got {:?}",
                m.arms[0].body
            ),
        }
    }
}
