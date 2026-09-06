use nom::{
    bytes::complete::tag,
    character::complete::{char, multispace0, space0, space1},
    combinator::opt,
    sequence::preceded,
    IResult, Parser,
};

use super::super::var::ReturnStmt;
use super::super::Statement;
use super::{
    parse_identifier, parse_type_identifier, take_until_line_end, MethodDef, MethodParameter,
};

pub(super) fn parse_method(input: &str) -> IResult<&str, MethodDef> {
    // Calculate the method signature's indentation
    let method_sig_indent = input.len() - input.trim_start().len();
    
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("fn")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;

        // Parse parameters

        let (input, parameters) = opt(parse_method_params).parse(input)?;

        let parameters = parameters.unwrap_or_default();

        // Optional `#on_err` listener name (same placement as free functions).
        let (input, _) = space0(input)?;
        let (input, _) = {
            let mut parser = opt(preceded(char('#'), parse_identifier));
            Parser::parse(&mut parser, input)?
        };

        // Parse return type => type

        let (input, _) = multispace0(input)?;

        let (input, _) = tag("=>")(input)?;

        let (input, _) = multispace0(input)?;

        let (input, return_type) = parse_type_identifier(input)?;

    

        // Parse method body

        let (input, _) = multispace0(input)?;

        let (input, _) = tag(":")(input)?;
        // Same-line spaces only. `multispace0` would strip the first body line's
        // indent, leave later `while`/`if` indented, and `parse_while` would miss `while`.
        let (input, _) = space0(input)?;

    

        let (input, body) = if input.contains('\n') {
            // Multi-line method body - collect until encountering a new top-level definition
            let lines: Vec<&str> = input.lines().collect();
            let mut body_lines = Vec::new();
            let mut consumed = 0;

            for line in lines.iter() {
                let trimmed = line.trim();
                let current_indent = line.len() - trimmed.len();

                if !trimmed.is_empty() {
                    let is_new_definition = trimmed.starts_with("fn ")
                        || trimmed.starts_with("class ")
                        || trimmed.starts_with("enum ")
                        || trimmed.starts_with("main(")
                        || trimmed.starts_with("c fn ");
                    // `fn main` / next class at column 0. Do not treat a first body
                    // line as ended just because `multispace0` after `:` stripped its indent.
                    if is_new_definition
                        && (current_indent < method_sig_indent
                            || current_indent == method_sig_indent)
                    {
                        break;
                    }
                }

                // Keep original indentation; empty lines stay so block parsers see structure
                body_lines.push(*line);

                // Update consumed (include the newline character)
                consumed += line.len() + 1;
            }

            // Adjust consumed (remove the last newline)
            if consumed > 0 {
                consumed = consumed.saturating_sub(1);
            }

            // Ensure consumed doesn't exceed input length
            consumed = consumed.min(input.len());

            let remaining = &input[consumed..];
            let body = parse_method_body_lines(&body_lines, false)?;
            (remaining, body)
        } else {
            // Single-line method body
            let (input, body) = take_until_line_end(input)?;
            let trimmed = body.trim();
            let body = if trimmed.is_empty() {
                Vec::new()
            } else {
                parse_method_body_lines(&[trimmed], true)?
            };
            (input, body)
        };

    Ok((
        input,
        MethodDef {
            name: name.to_string(),
            parameters,
            return_type: return_type.to_string(),
            body,
        },
    ))
}

fn strip_inline_comment(s: &str) -> &str {
    if let Some(start_pos) = s.find("/#/") {
        let before = &s[..start_pos];
        let quote_count = before.matches('"').count() + before.matches('\'').count();
        if quote_count % 2 == 0 {
            return before;
        }
    }
    s
}

/// Dedent method body lines by the indent of the first non-empty line, then parse statements.
fn parse_method_body_lines<'a>(
    raw_lines: &[&'a str],
    implicit_return: bool,
) -> Result<Vec<Statement>, nom::Err<nom::error::Error<&'a str>>> {
    let first_indent = raw_lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    let dedented: Vec<&str> = raw_lines
        .iter()
        .map(|line| {
            if line.len() >= first_indent
                && line[..first_indent]
                    .chars()
                    .all(|c| c == ' ' || c == '\t')
            {
                &line[first_indent..]
            } else {
                line.trim_start()
            }
        })
        .collect();

    let mut statements = Vec::new();
    let mut i = 0;
    while i < dedented.len() {
        let line_original = dedented[i];
        let line = strip_inline_comment(line_original).trim();

        if line.is_empty() || line.starts_with("/#") {
            i += 1;
            continue;
        }

        if let Some((stmt, lines_consumed)) = super::super::parse_multiline_statement(&dedented[i..]) {
            statements.push(stmt);
            i += lines_consumed;
        } else if let Some(stmt) = super::super::parse_single_line_statement(line) {
            statements.push(stmt);
            i += 1;
        } else {
            return Err(nom::Err::Error(nom::error::Error {
                input: raw_lines[i],
                code: nom::error::ErrorKind::Fail,
            }));
        }
    }

    // Preserve single-line method implicit return (bare expression => return)
    if implicit_return && statements.len() == 1 {
        if let Statement::Expr(expr) = statements.remove(0) {
            return Ok(vec![Statement::Return(ReturnStmt {
                value: Some(*expr),
            })]);
        }
    }

    Ok(statements)
}

fn parse_method_params(input: &str) -> IResult<&str, Vec<MethodParameter>> {
    let (input, _) = tag("(")(input)?;
    let mut parameters = Vec::new();
    let mut rest = input;
    loop {
        let (r, _) = space0(rest)?;
        if r.starts_with(')') {
            let r = &r[1..];
            return Ok((r, parameters));
        }
        if !parameters.is_empty() {
            let (r, _) = char(',')(r)?;
            let (r, _) = space0(r)?;
            rest = r;
            if rest.starts_with(')') {
                let rest = &rest[1..];
                return Ok((rest, parameters));
            }
        }
        let (r, name) = parse_identifier(rest)?;
        let (r, _) = space0(r)?;
        if name == "self" && !r.starts_with(':') {
            parameters.push(MethodParameter {
                name: "self".to_string(),
                param_type: String::new(),
            });
            rest = r;
            continue;
        }
        let (r, _) = char(':')(r)?;
        let (r, _) = space0(r)?;
        let (r, param_type) = parse_type_identifier(r)?;
        parameters.push(MethodParameter {
            name: name.to_string(),
            param_type: param_type.trim().to_string(),
        });
        rest = r;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_self_without_type_is_receiver() {
        let src = "fn move(self, dx: int, dy: int) => Point:\n        return self\n";
        let (_, m) = parse_method(src).expect("parse");
        assert_eq!(m.parameters[0].name, "self");
        assert_eq!(m.parameters[1].name, "dx");
    }

    #[test]
    fn method_tuple_param_is_one_parameter() {
        let src = "fn wrap(self, pair: (int, int)) => int:\n        return 0\n";
        let (_, m) = parse_method(src).expect("parse");
        assert_eq!(m.parameters.len(), 2);
        assert_eq!(m.parameters[0].name, "self");
        assert_eq!(m.parameters[1].name, "pair");
        assert_eq!(m.parameters[1].param_type, "(int, int)");
    }

    #[test]
    fn parse_method_rejects_unparseable_body_line() {
        let src = "fn move(self, dx: int) => Point:\n        @@@\n        return self\n";
        assert!(parse_method(src).is_err());
    }

    #[test]
    fn parse_method_with_while_stops_before_main() {
        let src = "\
    fn add_multiple(self, n: int, times: int) => Counter:
        let i: int = 0
        while i < times:
            self.value = self.value + n
            i = i + 1
        rm i
        return self
fn main() => int:
    return 0
";
        let (rest, m) = parse_method(src).unwrap_or_else(|e| panic!("parse_method: {:?}", e));
        assert_eq!(m.name, "add_multiple");
        assert!(m.body.len() >= 3, "body={:?}", m.body);
        assert!(rest.trim_start().starts_with("fn main"), "rest={rest:?}");
    }
}
