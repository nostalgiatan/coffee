use nom::IResult;

mod def;
mod method;
mod parse_class;
mod parse_enum;

pub use def::{
    ClassDef, ClassField, EnumDef, EnumVariant, MethodDef, MethodParameter, VariantField,
};
pub use parse_class::{parse_class, parse_packed_class};
pub use parse_enum::parse_enum;

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

pub(super) use crate::parser::ty::parse_type as parse_type_identifier;

fn take_until_line_end(input: &str) -> IResult<&str, &str> {
    if let Some(pos) = input.find('\n') {
        Ok((&input[pos..], &input[..pos]))
    } else {
        Ok(("", input))
    }
}
