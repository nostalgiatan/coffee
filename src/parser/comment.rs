use nom::bytes::complete::tag;
use nom::IResult;

/// 单行注释 /#/ ... /#/
#[derive(Debug, PartialEq, Clone)]
pub struct SingleLineComment {
    pub content: String,
}

/// 多行注释 /#* ... *#/
#[derive(Debug, PartialEq, Clone)]
pub struct MultiLineComment {
    pub content: String,
}

pub fn parse_single_line_comment(input: &str) -> IResult<&str, SingleLineComment> {
    let (input, _) = tag("/#/")(input)?;

    // 找到结束的 /#/ 或行尾
    if let Some(pos) = input.find("/#/") {
        let content = &input[..pos];
        let remaining = &input[pos + 3..];
        return Ok((
            remaining,
            SingleLineComment {
                content: content.to_string(),
            },
        ));
    }

    // 如果没有找到结束标记，取到行尾
    if let Some(pos) = input.find('\n') {
        let content = &input[..pos];
        let remaining = &input[pos..];
        return Ok((
            remaining,
            SingleLineComment {
                content: content.to_string(),
            },
        ));
    }

    // 整个剩余部分都是注释
    Ok((
        "",
        SingleLineComment {
            content: input.to_string(),
        },
    ))
}

pub fn parse_multi_line_comment(input: &str) -> IResult<&str, MultiLineComment> {
    let (input, _) = tag("/#*")(input)?;

    // 找到结束的 *#/
    if let Some(pos) = input.find("*#/") {
        let content = &input[..pos];
        let remaining = &input[pos + 3..];
        return Ok((
            remaining,
            MultiLineComment {
                content: content.to_string(),
            },
        ));
    }

    // 如果没有找到结束标记，返回错误
    Err(nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::TakeUntil,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_line_comment() {
        let input = "/#/ this is a comment\nnext line";
        let result = parse_single_line_comment(input);
        assert!(result.is_ok());
        let (remaining, comment) = result.unwrap();
        assert_eq!(comment.content, " this is a comment");
        assert_eq!(remaining, "\nnext line");
    }

    #[test]
    fn test_multi_line_comment() {
        let input = "/#* this is a\nmulti-line comment *#/ next";
        let result = parse_multi_line_comment(input);
        assert!(result.is_ok());
        let (remaining, comment) = result.unwrap();
        assert_eq!(comment.content, " this is a\nmulti-line comment ");
        assert_eq!(remaining, " next");
    }
}
