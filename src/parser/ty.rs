//! Unified type substring scanner for params, returns, `let`, fields, and methods.

use nom::{
    bytes::complete::tag,
    character::complete::{char, space0},
    IResult,
};

fn type_error(input: &str) -> nom::Err<nom::error::Error<&str>> {
    nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::Fail,
    })
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    let mut chars = input.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => {
            return Err(nom::Err::Error(nom::error::Error {
                input,
                code: nom::error::ErrorKind::Alpha,
            }))
        }
    }
    let len = chars
        .take_while(|&(_, c)| c.is_ascii_alphanumeric() || c == '_')
        .map(|(_, c)| c.len_utf8())
        .sum::<usize>()
        + 1;
    Ok((&input[len..], &input[..len]))
}

fn looks_like_fn_type(input: &str) -> bool {
    let rest = match input.strip_prefix("fn") {
        Some(r) => r,
        None => return false,
    };
    rest.trim_start().starts_with('(')
}

fn parse_fn_type_annotation(input: &str) -> IResult<&str, &str> {
    let origin = input;
    let (input, _) = tag("fn")(input)?;
    let (input, _) = space0(input)?;
    if !input.starts_with('(') {
        return Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Char,
        }));
    }
    let mut depth = 0i32;
    let mut idx = 0usize;
    let mut in_string = false;
    for (i, c) in input.char_indices() {
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    idx = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 || idx == 0 {
        return Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Char,
        }));
    }
    let input = &input[idx..];
    let (input, _) = space0(input)?;
    let (input, _) = tag("=>")(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = parse_type(input)?;
    let consumed = origin.len() - input.len();
    Ok((input, &origin[..consumed]))
}

fn scan_balanced_type(input: &str) -> IResult<&str, &str> {
    if input.is_empty() {
        return Err(type_error(input));
    }
    let first = input.chars().next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_' || first == '(' || first == '[') {
        return Err(type_error(input));
    }

    let mut angle = 0i32;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut in_string = false;
    let mut end = 0usize;

    for (i, c) in input.char_indices() {
        if in_string {
            if c == '"' {
                in_string = false;
            }
            end = i + c.len_utf8();
            continue;
        }
        if c == '"' {
            in_string = true;
            end = i + c.len_utf8();
            continue;
        }

        if angle == 0 && paren == 0 && bracket == 0 {
            if c == ',' || c == ':' || c == '\n' || c == '\r' {
                break;
            }
            if c == '=' {
                break;
            }
            if c == ')' || c == ']' || c == '}' {
                break;
            }
            if c == '>' {
                return Err(type_error(&input[i..]));
            }
        }

        match c {
            '<' => angle += 1,
            '>' => {
                if angle == 0 {
                    return Err(type_error(&input[i..]));
                }
                angle -= 1;
            }
            '(' => paren += 1,
            ')' => {
                if paren == 0 {
                    break;
                }
                paren -= 1;
            }
            '[' => bracket += 1,
            ']' => {
                if bracket == 0 {
                    return Err(type_error(&input[i..]));
                }
                bracket -= 1;
            }
            _ => {}
        }

        end = i + c.len_utf8();
    }

    if angle != 0 || paren != 0 || bracket != 0 {
        return Err(type_error(input));
    }
    if end == 0 {
        return Err(type_error(input));
    }
    Ok((&input[end..], &input[..end]))
}

/// Parse a Coffee type as a complete source substring (not a type tree).
pub fn parse_type(input: &str) -> IResult<&str, &str> {
    if let Ok((rest, _)) = tag::<&str, &str, nom::error::Error<&str>>("&mut ")(input) {
        let (rest, _) = parse_type(rest)?;
        let consumed = input.len() - rest.len();
        return Ok((rest, &input[..consumed]));
    }
    if let Some(rest) = input.strip_prefix('&') {
        if !rest.starts_with('&') {
            let (rest, _) = parse_type(rest)?;
            let consumed = input.len() - rest.len();
            return Ok((rest, &input[..consumed]));
        }
    }
    if looks_like_fn_type(input) {
        return parse_fn_type_annotation(input);
    }
    scan_balanced_type(input)
}

/// Optional `<T, U>` after a class or function name. Missing list is empty.
/// Empty `<>` or a non-identifier is a parse error.
/// Do not consume the space before `of Parent` when there is no `<...>`.
pub fn parse_type_params(input: &str) -> IResult<&str, Vec<String>> {
    let (after_space, _) = space0(input)?;
    if !after_space.starts_with('<') {
        return Ok((input, Vec::new()));
    }
    let input = after_space;
    let (input, _) = char('<')(input)?;
    let (input, _) = space0(input)?;
    if input.starts_with('>') {
        return Err(type_error(input));
    }
    let mut params = Vec::new();
    let mut rest = input;
    loop {
        let (r, ident) = parse_identifier(rest)?;
        params.push(ident.to_string());
        let (r, _) = space0(r)?;
        if r.starts_with(',') {
            let r = &r[1..];
            let (r, _) = space0(r)?;
            if r.starts_with('>') {
                return Err(type_error(r));
            }
            rest = r;
            continue;
        }
        if r.starts_with('>') {
            return Ok((&r[1..], params));
        }
        return Err(type_error(r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_list_type_is_complete() {
        let (rest, ty) = parse_type("List<List<int>>, y").expect("parse");
        assert_eq!(ty, "List<List<int>>");
        assert!(rest.starts_with(','), "rest={rest:?}");
    }

    #[test]
    fn nested_list_rest_does_not_start_with_gt() {
        let (rest, ty) = parse_type("List<List<int>>)").expect("parse");
        assert_eq!(ty, "List<List<int>>");
        assert!(!rest.starts_with('>'), "rest={rest:?}");
        assert!(rest.starts_with(')'), "rest={rest:?}");
    }

    #[test]
    fn map_str_list_int_not_truncated() {
        let (rest, ty) = parse_type("Map<str, List<int>>)").expect("parse");
        assert_eq!(ty, "Map<str, List<int>>");
        assert!(rest.starts_with(')'), "rest={rest:?}");
    }

    #[test]
    fn array_tuple_intn_ref_and_fn_types() {
        let (_, ty) = parse_type("[int; 2]").expect("array");
        assert_eq!(ty, "[int; 2]");
        let (_, ty) = parse_type("(int, str)").expect("tuple");
        assert_eq!(ty, "(int, str)");
        let (_, ty) = parse_type("int(4)+").expect("intn");
        assert_eq!(ty, "int(4)+");
        let (_, ty) = parse_type("&mut int").expect("mut ref");
        assert_eq!(ty, "&mut int");
        let (_, ty) = parse_type("fn(int) => int").expect("fn type");
        assert_eq!(ty.trim(), "fn(int) => int");
    }

    #[test]
    fn empty_type_params_is_error() {
        assert!(parse_type_params("<>").is_err());
    }

    #[test]
    fn type_params_t_u() {
        let (rest, params) = parse_type_params("<T, U>:").expect("params");
        assert_eq!(params, vec!["T".to_string(), "U".to_string()]);
        assert!(rest.starts_with(':'), "rest={rest:?}");
    }

    #[test]
    fn type_params_do_not_eat_space_before_of() {
        let (rest, params) = parse_type_params(" of Base:").expect("no type params");
        assert!(params.is_empty());
        assert!(rest.starts_with(" of "), "rest={rest:?}");
    }
}
