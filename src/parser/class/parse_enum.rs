use nom::{
    bytes::complete::tag,
    character::complete::{multispace0, space1},
    IResult,
};

use super::{EnumDef, EnumVariant, VariantField, parse_identifier};

pub fn parse_enum(input: &str) -> IResult<&str, EnumDef> {
    let (input, _) = tag("enum")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    
    // Manually parse variants using a loop instead of many0
    let mut variants = Vec::new();
    let mut current_input = input;
    
    loop {
        match parse_enum_variant(current_input) {
            Ok((remaining, variant)) => {
                variants.push(variant);
                current_input = remaining;
            }
            Err(_) => {
                break;
            }
        }
    }
    
    let input = current_input;

    Ok((
        input,
        EnumDef {
            name: name.to_string(),
            variants,
        },
    ))
}

fn parse_enum_variant(input: &str) -> IResult<&str, EnumVariant> {
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_identifier(input)?;

    // Check if there are parentheses (for types or named parameters)
    // Only parse as variant with fields if '(' appears immediately after the identifier
    // This prevents mistakenly parsing "Red\n    Green\n    Blue\n    Rgb(...)" as a variant with fields
    if input.starts_with('(') {
        if let Some(closing_pos) = crate::parser::index_of_matching_close_paren(input) {
            let fields_str = &input[1..closing_pos];
            let remaining = &input[closing_pos + 1..];

            // Check if these are named parameters (field:type) or positional parameters (type)
            let mut fields = Vec::new();
            for field_part in crate::parser::split_top_level_commas(fields_str) {
                let field_part = field_part.trim();
                if let Some(colon_pos) = field_part.find(':') {
                    // Named parameter: field:type
                    let name = field_part[..colon_pos].trim();
                    let field_type = field_part[colon_pos + 1..].trim();
                    if !name.is_empty() && !field_type.is_empty() {
                        fields.push(VariantField::Named {
                            name: name.to_string(),
                            field_type: field_type.to_string(),
                        });
                    }
                } else {
                    // Positional parameter: type
                    let field_type = field_part.trim();
                    if !field_type.is_empty() {
                        fields.push(VariantField::Type(field_type.to_string()));
                    }
                }
            }

            return Ok((
                remaining,
                EnumVariant {
                    name: name.to_string(),
                    fields,
                },
            ));
        }
    }

    // Unit variant without fields
    Ok((
        input,
        EnumVariant {
            name: name.to_string(),
            fields: Vec::new(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_nested_tuple_payload_keeps_inner_parens() {
        let src = "enum Wrap:\n    Pair((int, int))\n";
        let (_, e) = parse_enum(src).expect("parse");
        assert_eq!(e.variants.len(), 1);
        assert_eq!(e.variants[0].name, "Pair");
        assert_eq!(e.variants[0].fields.len(), 1);
        match &e.variants[0].fields[0] {
            VariantField::Type(t) => assert_eq!(t, "(int, int)"),
            other => panic!("{:?}", other),
        }
    }
}
