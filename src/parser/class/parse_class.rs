use nom::{
    bytes::complete::tag,
    character::complete::{multispace0, space1},
    combinator::opt,
    IResult, Parser,
};

use super::method::parse_method;
use super::{ClassDef, ClassField, MethodDef, parse_identifier};
use crate::parser::ty::{parse_type, parse_type_params};

pub fn parse_class(input: &str) -> IResult<&str, ClassDef> {
    let (input, _) = tag("class")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, type_params) = parse_type_params(input)?;
    let (input, parent) = opt(parse_inheritance).parse(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;

    // Parse fields and methods
    let (input, (fields, methods)) = parse_class_body(input)?;

    // Check if class has a constructor (fn new)
    let has_constructor = methods.iter().any(|m| m.name == "new");

    Ok((
        input,
        ClassDef {
            name: name.to_string(),
            type_params,
            parent,
            fields,
            methods,
            packed: false,  // Default: not packed
            has_constructor,  // Set based on whether fn new exists
        },
    ))
}

/// Parse packed class definition
/// Syntax: packed class Name:
pub fn parse_packed_class(input: &str) -> IResult<&str, ClassDef> {
    let (input, _) = tag("packed")(input)?;
    let (input, _) = space1(input)?;
    let (input, _) = tag("class")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, type_params) = parse_type_params(input)?;
    let (input, parent) = opt(parse_inheritance).parse(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;

    // Parse fields and methods
    let (input, (fields, methods)) = parse_class_body(input)?;

    // Check if class has a constructor (fn new)
    let has_constructor = methods.iter().any(|m| m.name == "new");

    Ok((
        input,
        ClassDef {
            name: name.to_string(),
            type_params,
            parent,
            fields,
            methods,
            packed: true,  // Packed layout
            has_constructor,  // Set based on whether fn new exists
        },
    ))
}

fn parse_class_body(input: &str) -> IResult<&str, (Vec<ClassField>, Vec<MethodDef>)> {
    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut first_method_indent = None;  // Track the indentation of the first method

    // Collect the starting position of all lines
    let mut line_starts = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        line_starts.push(pos);
        // Find the next newline character
        if let Some(nl_pos) = input[pos..].find('\n') {
            pos += nl_pos + 1;
        } else {
            break;
        }
    }
    // Add the last line (if there's no newline at the end)
    if pos < input.len() {
        line_starts.push(pos);
    }

    let mut i = 0;
    while i < line_starts.len() {
        let line_start = line_starts[i];
        let line_end = if i + 1 < line_starts.len() {
            line_starts[i + 1]
        } else {
            input.len()
        };

        let line = &input[line_start..line_end];
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        let current_indent = line.len() - trimmed.len();

        // Left the class: following top-level items (`fn main`, another class, …).
        if current_indent == 0
            && (trimmed.starts_with("class ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("c ")
                || trimmed.starts_with("main("))
        {
            break;
        }

        // Check if this is a method definition (fn)
        if trimmed.starts_with("fn ") {
            // Dedented `fn` is not a class method.
            if let Some(first_indent) = first_method_indent {
                if current_indent < first_indent {
                    break;
                }
                if current_indent != first_indent {
                    return Err(nom::Err::Error(nom::error::Error {
                        input: line,
                        code: nom::error::ErrorKind::Tag,
                    }));
                }
            } else {
                // Record the first method's indentation
                first_method_indent = Some(current_indent);
            }

            // Parse the method
            let method_input = &input[line_start..];
            match parse_method(method_input) {
                Ok((new_remaining, method)) => {
                    methods.push(method);

                    // Calculate the position where new_remaining starts in the original input
                    let new_remaining_start = input.len() - new_remaining.len();
                    
                    // Find which line we should continue from
                    let mut new_i = i;
                    while new_i < line_starts.len() && line_starts[new_i] < new_remaining_start {
                        new_i += 1;
                    }
                    i = new_i;
                    continue;
                }
                Err(_e) => {
                    break;
                }
            }
        }

        // Check if this is a field definition name:type or name:type:bitwidth
        // Only lines that don't start with 'fn' and don't contain '=>' could be field definitions
        if !trimmed.starts_with("fn ") && !trimmed.contains("=>") {
            if let Some(colon_pos) = trimmed.find(':') {
                let name = trimmed[..colon_pos].trim();
                let rest = &trimmed[colon_pos + 1..];

                // First skip leading whitespace
                let rest_trimmed = rest.trim_start();

                // Check for bit field syntax: type:bitwidth (e.g., int(1):3)
                // Look for second colon that indicates bit width
                let (field_type, bit_width) = parse_field_type_and_bitwidth(rest_trimmed);

                if !name.is_empty() && !field_type.is_empty() && !name.contains(' ') && !name.contains('(') {
                    // Check if field name is 'assign' (reserved for built-in method)
                    if name == "assign" {
                        return Err(nom::Err::Error(nom::error::Error {
                            input: line,
                            code: nom::error::ErrorKind::Verify,
                        }));
                    }
                    
                    fields.push(ClassField {
                        name: name.to_string(),
                        field_type,
                        bit_width,
                    });
                }
            }
        }

        i += 1;
    }

    Ok(("", (fields, methods)))
}

fn parse_field_type_and_bitwidth(rest: &str) -> (String, Option<u8>) {
    let rest = rest.trim_start();
    match parse_type(rest) {
        Ok((after_ty, ty)) => {
            let field_type = ty.trim().to_string();
            let after = after_ty.trim_start();
            let bit_width = if let Some(stripped) = after.strip_prefix(':') {
                let width_part = stripped.trim_start();
                let width_end = width_part
                    .find(|c: char| c.is_whitespace() || c == '=')
                    .unwrap_or(width_part.len());
                width_part[..width_end].trim().parse::<u8>().ok()
            } else {
                None
            };
            (field_type, bit_width)
        }
        Err(_) => (String::new(), None),
    }
}

fn parse_inheritance(input: &str) -> IResult<&str, String> {
    let (input, _) = space1(input)?;
    let (input, _) = tag("of")(input)?;
    let (input, _) = space1(input)?;
    let (input, parent) = parse_identifier(input)?;
    Ok((input, parent.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_fields_keep_spaces_inside_array_and_tuple_types() {
        let src = "class Matrix:\n    data: [int; 9]\n    pair: (int, int)\n";
        let (_, class) = parse_class(src).expect("parse");
        assert_eq!(class.fields[0].field_type, "[int; 9]");
        assert_eq!(class.fields[1].field_type, "(int, int)");
    }

    #[test]
    fn class_method_with_while_does_not_eat_following_main() {
        let src = "\
class Counter:
    value: int
    fn new() => Counter:
        Counter { value: 0 }
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
        let (_, class) = parse_class(src).expect("class");
        let names: Vec<&str> = class.methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["new", "add_multiple"]);
    }

    #[test]
    fn class_field_ref_int_type() {
        let src = "class Holder:\n    p: &int\n";
        let (_, class) = parse_class(src).expect("parse");
        assert_eq!(class.fields[0].field_type, "&int");
    }

    #[test]
    fn class_list_t_type_params() {
        let src = "class List<T>:\n    head: T\n";
        let (_, class) = parse_class(src).expect("parse");
        assert_eq!(class.type_params, vec!["T".to_string()]);
        assert_eq!(class.fields[0].field_type, "T");
    }

    #[test]
    fn class_child_t_of_parent_type_params() {
        let src = "class Child<T> of Parent:\n    x: T\n";
        let (_, class) = parse_class(src).expect("parse");
        assert_eq!(class.name, "Child");
        assert_eq!(class.type_params, vec!["T".to_string()]);
        assert_eq!(class.parent.as_deref(), Some("Parent"));
    }

    #[test]
    fn class_of_parent_without_type_params() {
        let src = "class Child of Base:\n    y: int\n";
        let (_, class) = parse_class(src).expect("parse");
        assert_eq!(class.name, "Child");
        assert_eq!(class.parent.as_deref(), Some("Base"));
        assert_eq!(class.fields[0].name, "y");
    }

    #[test]
    fn class_of_error_empty_body() {
        let src = "class E of Error:\n";
        let (_, class) = parse_class(src).expect("parse empty of Error");
        assert_eq!(class.name, "E");
        assert_eq!(class.parent.as_deref(), Some("Error"));
        assert!(class.fields.is_empty());
        assert!(class.methods.is_empty());
    }

    #[test]
    fn class_of_error_with_method() {
        let src = "\
class Boom of Error:
    extra: int

    fn ping(self) => int:
        return self.extra
";
        let (_, class) = parse_class(src).expect("parse of Error with method");
        assert_eq!(class.methods.len(), 1);
        assert_eq!(class.methods[0].name, "ping");
    }

    #[test]
    fn packed_class_list_t_type_params() {
        let src = "packed class List<T>:\n    head: T\n";
        let (_, class) = parse_packed_class(src).expect("parse");
        assert!(class.packed);
        assert_eq!(class.type_params, vec!["T".to_string()]);
    }

    #[test]
    fn increment_and_get_value_both_keep_bodies() {
        let src = "\
class Counter:
    value: int
    fn new() => Counter:
        Counter { value: 0 }
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self
    fn get_value(self) => int:
        let result: int = self.value
        return result
";
        let (_, class) = parse_class(src).expect("class");
        let inc = class.methods.iter().find(|m| m.name == "increment").unwrap();
        let get = class.methods.iter().find(|m| m.name == "get_value").unwrap();
        assert!(
            inc.body.len() >= 2,
            "increment body: {:?}",
            inc.body
        );
        assert!(
            get.body.len() >= 2,
            "get_value body: {:?}",
            get.body
        );
    }
}
