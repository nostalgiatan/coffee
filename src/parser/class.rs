use nom::{
    bytes::complete::tag,
    character::complete::{multispace0, space1},
    combinator::opt,
    multi::many0,
    IResult, Parser,
};

/// Class definition
/// class Name<T> of Parent:
///     field:type
///     field2:type
///
///     fn method(params:type) => return_type:
///         body
#[derive(Debug, PartialEq, Clone)]
pub struct ClassDef {
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<ClassField>,
    pub methods: Vec<MethodDef>,
    pub packed: bool,  // If true, use packed layout (no padding)
    pub has_constructor: bool,  // If true, class has a custom constructor (heap-allocated)
}

/// Class field
#[derive(Debug, PartialEq, Clone)]
pub struct ClassField {
    pub name: String,
    pub field_type: String,
    pub bit_width: Option<u8>,  // Bit width for bit fields (e.g., 3 for a 3-bit field)
}

/// Method definition (inside class)
#[derive(Debug, PartialEq, Clone)]
pub struct MethodDef {
    pub name: String,
    pub parameters: Vec<MethodParameter>,
    pub return_type: String,
    pub body: String,
}

/// Method parameter
#[derive(Debug, PartialEq, Clone)]
pub struct MethodParameter {
    pub name: String,
    pub param_type: String,
}

/// Enum definition
/// enum Name:
///     Variant1
///     Variant2(type1, type2)
///     Variant3(field:type)
#[derive(Debug, PartialEq, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

/// Enum variant
#[derive(Debug, PartialEq, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<VariantField>,
}

/// Variant field
#[derive(Debug, PartialEq, Clone)]
pub enum VariantField {
    /// Positional type: Variant2(type1, type2)
    Type(String),
    /// Named field: Variant3(field:type)
    Named { name: String, field_type: String },
}

pub fn parse_class(input: &str) -> IResult<&str, ClassDef> {
    let (input, _) = tag("class")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;
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
            parent,
            fields,
            methods,
            packed: true,  // Packed layout
            has_constructor,  // Set based on whether fn new exists
        },
    ))
}

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

        // Check if this is a new class/enum/main definition (indicating the current class has ended)
        if trimmed.starts_with("class ") || trimmed.starts_with("enum ") || trimmed.starts_with("main(") {
            break;
        }

        // Check if this is a method definition (fn)
        if trimmed.starts_with("fn ") {
            let current_indent = line.len() - trimmed.len();

            // Check indentation alignment
            if let Some(first_indent) = first_method_indent {
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
                let (field_type, bit_width) = if let Some(second_colon_pos) = rest_trimmed.find(':') {
                    // Potential bit field syntax
                    let type_part = &rest_trimmed[..second_colon_pos];
                    let width_part = &rest_trimmed[second_colon_pos + 1..];

                    // Extract just the type (before any whitespace or =)
                    let type_end = type_part.find(|c: char| c.is_whitespace() || c == '=')
                        .unwrap_or(type_part.len());
                    let field_type = type_part[..type_end].trim();

                    // Extract bit width (before any whitespace or =)
                    let width_end = width_part.find(|c: char| c.is_whitespace() || c == '=')
                        .unwrap_or(width_part.len());
                    let width_str = width_part[..width_end].trim();

                    // Parse bit width
                    let bit_width = width_str.parse::<u8>().ok();

                    (field_type.to_string(), bit_width)
                } else {
                    // Regular field (no bit width)
                    // Find type end (before whitespace or =)
                    let type_end = rest_trimmed.find(|c: char| c.is_whitespace() || c == '=')
                        .unwrap_or(rest_trimmed.len());
                    let field_type = rest_trimmed[..type_end].trim().to_string();

                    (field_type, None)
                };

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

fn parse_method(input: &str) -> IResult<&str, MethodDef> {
    // Calculate the method signature's indentation
    let method_sig_indent = input.len() - input.trim_start().len();
    
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("fn")(input)?;
    let (input, _) = space1(input)?;
    let (input, name) = parse_identifier(input)?;

        // Parse parameters

        let (input, parameters) = opt(parse_method_params).parse(input)?;

        let parameters = parameters.unwrap_or_default();

    

        // Parse return type => type

        let (input, _) = multispace0(input)?;

        let (input, _) = tag("=>")(input)?;

        let (input, _) = multispace0(input)?;

        let (input, return_type) = parse_type_identifier(input)?;

    

        // Parse method body

        let (input, _) = multispace0(input)?;

        let (input, _) = tag(":")(input)?;

        let (input, _) = multispace0(input)?;

    

        let (input, body) = if input.contains('\n') {
            // Multi-line method body - collect until encountering a new top-level definition
            let lines: Vec<&str> = input.lines().collect();
            let mut body_lines = Vec::new();
            let mut consumed = 0;

            for (line_idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                let current_indent = line.len() - trimmed.len();

                // Check if this is a new top-level definition (method, class, enum)
                // These definitions must start at the same indentation as the method signature
                let is_new_definition = current_indent == method_sig_indent && (
                    trimmed.starts_with("fn ") ||
                    trimmed.starts_with("class ") ||
                    trimmed.starts_with("enum ") ||
                    trimmed.starts_with("main(")
                );

                // If we encounter a new definition and already have content, stop
                if is_new_definition && !body_lines.is_empty() {
                    break;
                }

                // Add non-empty lines to the method body
                if !trimmed.is_empty() {
                    body_lines.push(line.to_string());  // Keep original indentation
                }

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
            let body = body_lines.join("\n");
            (remaining, body)
        } else {
            // Single-line method body
            let (input, body) = take_until_line_end(input)?;
            (input, body.trim().to_string())
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

fn parse_method_params(input: &str) -> IResult<&str, Vec<MethodParameter>> {
    let (input, _) = tag("(")(input)?;
    let (input, params) = take_until_paren(input)?;
    let (input, _) = tag(")")(input)?;

    let mut parameters = Vec::new();
    for param in params.split(',') {
        let param = param.trim();
        if let Some(colon_pos) = param.find(':') {
            let name = param[..colon_pos].trim();
            let param_type = param[colon_pos + 1..].trim();
            if !name.is_empty() && !param_type.is_empty() {
                parameters.push(MethodParameter {
                    name: name.to_string(),
                    param_type: param_type.to_string(),
                });
            }
        }
    }

    Ok((input, parameters))
}

fn parse_enum_variant(input: &str) -> IResult<&str, EnumVariant> {
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_identifier(input)?;

    // Check if there are parentheses (for types or named parameters)
    // Only parse as variant with fields if '(' appears immediately after the identifier
    // This prevents mistakenly parsing "Red\n    Green\n    Blue\n    Rgb(...)" as a variant with fields
    if input.starts_with('(') {
        if let Some(closing_pos) = input[1..].find(')') {
            let closing_pos = closing_pos + 1; // Account for the '('
            let fields_str = &input[1..closing_pos];
            let remaining = &input[closing_pos + 1..];

            // Check if these are named parameters (field:type) or positional parameters (type)
            let mut fields = Vec::new();
            for field_part in fields_str.split(',') {
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

fn parse_inheritance(input: &str) -> IResult<&str, String> {
    let (input, _) = space1(input)?;
    let (input, _) = tag("of")(input)?;
    let (input, _) = space1(input)?;
    let (input, parent) = parse_identifier(input)?;
    Ok((input, parent.to_string()))
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
    let mut chars = input.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() || c == '_' || c == '(' || c == '<' => {}
        _ => return Err(nom::Err::Error(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Alpha,
        })),
    }

    let mut len = 1;
    let mut angle_depth = 0;
    let mut paren_depth = 0;

    for (_, c) in chars {
        match c {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            c if c.is_ascii_alphanumeric() || c == '_' || c == '(' || c == ')' || c == '+' || c == '-' || c == ',' || c == '<' || c == '>' => {}
            _ if c.is_whitespace() => {
                if angle_depth == 0 && paren_depth == 0 {
                    break;
                }
            }
            _ => break,
        }
        len += c.len_utf8();
    }

    Ok((&input[len..], &input[..len]))
}

fn take_until_paren(input: &str) -> IResult<&str, &str> {
    let mut depth = 0;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Ok((&input[i..], &input[..i]));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    Err(nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::TakeUntil,
    }))
}

fn take_until_angle_bracket(input: &str) -> IResult<&str, &str> {
    let mut depth = 0;
    for (i, c) in input.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                if depth == 0 {
                    return Ok((&input[i..], &input[..i]));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    Err(nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::TakeUntil,
    }))
}

fn take_until_line_end(input: &str) -> IResult<&str, &str> {
    if let Some(pos) = input.find('\n') {
        Ok((&input[pos..], &input[..pos]))
    } else {
        Ok(("", input))
    }
}
