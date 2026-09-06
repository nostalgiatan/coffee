use crate::coffee_debug;
use nom::{
    bytes::complete::tag,
    character::complete::{multispace0, space0, space1},
    combinator::opt,
    IResult, Parser,
};

use crate::parser::expr::Expression;

/// 变量声明
/// let name:type = value
#[derive(Debug, PartialEq, Clone)]
pub struct VariableDecl {
    pub name: String,
    pub var_type: String,
    pub value: Expression,
}

/// Return 语句
/// return value
#[derive(Debug, PartialEq, Clone)]
pub struct ReturnStmt {
    pub value: Option<Expression>,
}

/// Break 语句
#[derive(Debug, PartialEq, Clone)]
pub struct BreakStmt;

/// Continue 语句
#[derive(Debug, PartialEq, Clone)]
pub struct ContinueStmt;

pub fn parse_variable_decl(input: &str) -> IResult<&str, VariableDecl> {
    parse_variable_decl_at(input, 0)
}

pub fn parse_variable_decl_at(input: &str, base: usize) -> IResult<&str, VariableDecl> {
    coffee_debug!("DEBUG: parse_variable_decl: input='{}'", input);
    let src = input;
    let (input, _) = tag("let")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = space0(input)?;  // Skip spaces after colon!
    let (input, var_type) = crate::parser::ty::parse_type(input)?;
    let var_type = var_type.trim();  // Trim whitespace from type
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("=")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = parse_let_value(src, base, input)?;

    coffee_debug!("DEBUG: parse_variable_decl: name='{}', var_type='{}', value='{}'", name, var_type, value);

    Ok((
        input,
        VariableDecl {
            name: name.to_string(),
            var_type: var_type.to_string(),
            value,
        },
    ))
}

pub fn parse_return(input: &str) -> IResult<&str, ReturnStmt> {
    parse_return_at(input, 0)
}

pub fn parse_return_at(input: &str, base: usize) -> IResult<&str, ReturnStmt> {
    let src = input;
    let (input, _) = tag("return")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = opt(parse_expression).parse(input)?;
    coffee_debug!("DEBUG: parse_return: value={:?}", value);

    Ok((
        input,
        ReturnStmt {
            value: match value {
                None => None,
                Some(v) => {
                    let v = v.trim();
                    if v.is_empty() {
                        None
                    } else {
                        Some(crate::parser::multiline::parse_expr_at(src, base, v)?)
                    }
                }
            },
        },
    ))
}

pub fn parse_break(input: &str) -> IResult<&str, BreakStmt> {
    let (input, _) = tag("break")(input)?;
    Ok((input, BreakStmt))
}

pub fn parse_continue(input: &str) -> IResult<&str, ContinueStmt> {
    let (input, _) = tag("continue")(input)?;
    Ok((input, ContinueStmt))
}

fn looks_like_anonymous_fn(input: &str) -> bool {
    let rest = input.trim_start();
    let rest = match rest.strip_prefix("fn") {
        Some(r) => r,
        None => return false,
    };
    rest.trim_start().starts_with('(')
}

fn parse_let_value<'a>(
    parent: &str,
    parent_base: usize,
    input: &'a str,
) -> IResult<&'a str, Expression> {
    if looks_like_anonymous_fn(input) {
        if let Ok((rest, func)) = crate::parser::function::parse_anonymous_function(input) {
            return Ok((rest, Expression::AnonymousFunction {
                func: Box::new(func),
            }));
        }
    }
    let (input, value_raw) = parse_expression(input)?;
    let value_raw = value_raw.trim();
    let value = crate::parser::multiline::parse_expr_at(parent, parent_base, value_raw)?;
    Ok((input, value))
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    let mut chars = input.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Alpha,
        })),
    }

    let len = chars
        .take_while(|&(_, c)| c.is_ascii_alphanumeric() || c == '_')
        .map(|(_, c)| c.len_utf8())
        .sum::<usize>() + 1;

    Ok((&input[len..], &input[..len]))
}

fn parse_expression(input: &str) -> IResult<&str, &str> {
    // 表达式解析：支持多行结构体字面量
    // 需要正确处理括号、大括号、中括号的嵌套
    let mut end = 0;
    let mut paren_depth = 0;
    let mut brace_depth = 0;
    let mut bracket_depth = 0;
    let mut in_string = false;
    let mut string_char = '\0';

    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    
    while end < len {
        let c = chars[end];
        
        // 处理字符串字面量
        if !in_string && (c == '"' || c == '\'') {
            in_string = true;
            string_char = c;
            end += 1;
            continue;
        }
        
        if in_string {
            if c == string_char {
                // 检查是否是转义字符
                if end > 0 && chars[end - 1] != '\\' {
                    in_string = false;
                }
            }
            end += 1;
            continue;
        }
        
        // 处理括号、大括号、中括号的嵌套
        match c {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '\n' => {
                // 如果不在任何嵌套中，遇到换行符就停止
                if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        
        // 检查是否遇到注释开始 /#/
        if end + 2 < len && c == '/' && chars[end + 1] == '#' && chars[end + 2] == '/' {
            // 确保不在字符串中
            if !in_string {
                break;
            }
        }
        
        end += 1;
    }

    Ok((&input[end..], &input[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::expr::Expression;

    #[test]
    fn parse_let_valid_int() {
        let (_, decl) = parse_variable_decl("let x: int = 1").expect("valid let");
        assert_eq!(decl.name, "x");
        assert_eq!(decl.var_type, "int");
        assert!(matches!(decl.value.kind(), Expression::Literal(_)));
    }

    #[test]
    fn parse_return_valid_expr() {
        let (_, stmt) = parse_return("return 42").expect("valid return");
        match stmt.value.as_ref().map(|e| e.kind()) {
            Some(Expression::Literal(s)) => assert_eq!(s, "42"),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn parse_let_invalid_value_is_error_not_literal() {
        match parse_variable_decl("let x: int = @@@") {
            Err(_) => {}
            Ok((_, decl)) => panic!(
                "invalid let value must not parse; got {:?}",
                decl.value
            ),
        }
    }

    #[test]
    fn parse_return_invalid_expr_is_error_not_literal() {
        match parse_return("return @@@") {
            Err(_) => {}
            Ok((_, stmt)) => panic!(
                "invalid return expr must not parse; got {:?}",
                stmt.value
            ),
        }
    }
}
