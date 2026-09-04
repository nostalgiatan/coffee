use nom::{
    bytes::complete::tag,
    character::complete::{char, space0, space1},
    multi::many1,
    IResult, Parser,
};

/// Match 表达式
/// match value:
///     pattern1 => result1
///     pattern2 => result2
///     _ => default
#[derive(Debug, PartialEq, Clone)]
pub struct MatchExpr {
    pub value: String,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct MatchArm {
    pub pattern: String,
    pub result: String,
}

pub fn parse_match(input: &str) -> IResult<&str, MatchExpr> {
    let (input, _) = tag("match")(input)?;
    let (input, _) = space1(input)?;
    let (input, value) = take_until_colon(input)?;
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;

    // 解析 match arms
    let (input, arms) = {
        let mut parser = many1(parse_match_arm);
        Parser::parse(&mut parser, input)?
    };

    Ok((
        input,
        MatchExpr {
            value: value.trim().to_string(),
            arms,
        },
    ))
}

fn parse_match_arm(input: &str) -> IResult<&str, MatchArm> {
    let (input, _) = space0(input)?;
    let (input, pattern) = take_until_double_arrow(input)?;
    let (input, _) = tag("=>")(input)?;
    let (input, _) = space0(input)?;
    let (input, result) = parse_single_line(input)?;

    Ok((
        input,
        MatchArm {
            pattern: pattern.trim().to_string(),
            result: result.trim().to_string(),
        },
    ))
}

fn take_until_colon(input: &str) -> IResult<&str, &str> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ':' {
            // Check if it's :: (skip both colons)
            if i + 1 < chars.len() && chars[i + 1] == ':' {
                i += 2;  // Skip both : characters in ::
                continue;
            }
            // Found a single :
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
