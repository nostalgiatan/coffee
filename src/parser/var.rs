use nom::{
    bytes::complete::tag,
    character::complete::{multispace0, space0, space1},
    combinator::opt,
    IResult, Parser,
};

/// 变量声明
/// let name:type = value
#[derive(Debug, PartialEq, Clone)]
pub struct VariableDecl {
    pub name: String,
    pub var_type: String,
    pub value: String,
}

/// Return 语句
/// return value
#[derive(Debug, PartialEq, Clone)]
pub struct ReturnStmt {
    pub value: Option<String>,
}

/// Break 语句
#[derive(Debug, PartialEq, Clone)]
pub struct BreakStmt;

/// Continue 语句
#[derive(Debug, PartialEq, Clone)]
pub struct ContinueStmt;

pub fn parse_variable_decl(input: &str) -> IResult<&str, VariableDecl> {
    eprintln!("DEBUG: parse_variable_decl: input='{}'", input);
    let (input, _) = tag("let")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = space0(input)?;  // Skip spaces after colon!
    let (input, var_type) = parse_type_identifier(input)?;
    let var_type = var_type.trim();  // Trim whitespace from type
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("=")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = parse_expression(input)?;

    eprintln!("DEBUG: parse_variable_decl: name='{}', var_type='{}', value='{}'", name, var_type, value);

    Ok((
        input,
        VariableDecl {
            name: name.to_string(),
            var_type: var_type.to_string(),
            value: value.trim().to_string(),
        },
    ))
}

pub fn parse_return(input: &str) -> IResult<&str, ReturnStmt> {
    let (input, _) = tag("return")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = opt(parse_expression).parse(input)?;
    eprintln!("DEBUG: parse_return: value={:?}", value);

    Ok((
        input,
        ReturnStmt {
            value: value.map(|v| v.trim().to_string()),
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

fn parse_type_identifier(input: &str) -> IResult<&str, &str> {
    // 类型标识符可以包含字母、数字、下划线、括号、加减号、数组括号、引用符号等
    // 例如: int, int(4)+, float(8), (int, float), Option<int>, [int; 10], [int], &int, &mut int
    let mut chars = input.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() || c == '_' || c == '(' || c == '[' || c == '&' => {}
        _ => return Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Alpha,
        })),
    }

    let mut len = 1;
    let mut _paren_depth = 0;
    let mut _bracket_depth = 0;

    for (_, c) in chars {
        match c {
            '(' => _paren_depth += 1,
            ')' => _paren_depth -= 1,
            '[' => _bracket_depth += 1,
            ']' => {
                _bracket_depth -= 1;
                if _bracket_depth == 0 {
                    len += c.len_utf8();
                    break;  // 完整的数组类型，如 [int; 10]
                }
            }
            c if c.is_ascii_alphanumeric() || c == '_' || c == '(' || c == ')' || c == '+' || c == '-' || c == ',' || c == '<' || c == '>' || c == '[' || c == ']' || c == ';' || c == ' ' || c == '&' => {}
            _ if c.is_whitespace() && _bracket_depth == 0 && _paren_depth == 0 => break,
            _ => break,
        }
        len += c.len_utf8();
    }

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
