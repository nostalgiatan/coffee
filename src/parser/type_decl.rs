use nom::{
    bytes::complete::tag,
    character::complete::{multispace0, space0, space1},
    IResult,
};

/// Nominal newtype declaration: `type Name: Source`.
#[derive(Debug, PartialEq, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub source: String,
}

pub fn parse_type_decl(input: &str) -> IResult<&str, TypeDecl> {
    let (input, _) = tag("type")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = space0(input)?;
    let (input, source) = parse_identifier(input)?;
    if source.is_empty() {
        return Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Fail,
        }));
    }
    Ok((
        input,
        TypeDecl {
            name: name.to_string(),
            source: source.to_string(),
        },
    ))
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
    let ident = &input[..len];
    if ident.is_empty() {
        return Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Fail,
        }));
    }
    Ok((&input[len..], ident))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_program, parse_single_line_statement, Statement};

    #[test]
    fn parse_type_file_object() {
        let (rest, decl) = parse_type_decl("type FILE: object").expect("parse");
        assert_eq!(rest, "");
        assert_eq!(
            decl,
            TypeDecl {
                name: "FILE".into(),
                source: "object".into(),
            }
        );
    }

    #[test]
    fn parse_type_decl_via_single_line() {
        let stmt = parse_single_line_statement("type FILE: object").expect("stmt");
        assert_eq!(
            stmt,
            Statement::TypeDecl(TypeDecl {
                name: "FILE".into(),
                source: "object".into(),
            })
        );
    }

    #[test]
    fn parse_type_decl_then_function() {
        let src = "type FILE: object\nfn main() => int:\n    return 0\n";
        let program = parse_program(src).expect("parse type then fn");
        assert!(matches!(program.statements[0], Statement::TypeDecl(_)));
        assert!(matches!(program.statements[1], Statement::Function(_)));
    }

    #[test]
    fn parse_type_decl_in_program() {
        let program = parse_program("type FILE: object\n").expect("program");
        assert_eq!(program.statements.len(), 1);
        assert_eq!(
            program.statements[0],
            Statement::TypeDecl(TypeDecl {
                name: "FILE".into(),
                source: "object".into(),
            })
        );
    }

    #[test]
    fn reject_type_file_object_without_colon() {
        assert!(parse_type_decl("type FILE object").is_err());
        assert!(parse_single_line_statement("type FILE object").is_none());
    }

    #[test]
    fn reject_type_file_missing_source() {
        assert!(parse_type_decl("type FILE:").is_err());
        assert!(parse_single_line_statement("type FILE:").is_none());
    }

    #[test]
    fn reject_type_decl_leftover_junk() {
        assert!(parse_single_line_statement("type FILE: object extra").is_none());
    }

    #[test]
    fn reject_type_empty_name() {
        assert!(parse_type_decl("type : object").is_err());
        assert!(parse_single_line_statement("type : object").is_none());
    }
}
