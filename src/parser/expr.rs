// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License
//
// Expression types for the Coffee language

/// Expression in the Coffee language
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// Literal value
    Literal(String),
    /// Variable reference
    Variable(String),
    /// Format string (f"...") with placeholders
    FString {
        template: String,           // The original template
        placeholders: Vec<String>,   // Variable names in {placeholders}
    },
    /// Binary operation
    Binary {
        left: Box<Expression>,
        op: String,
        right: Box<Expression>,
    },
    /// Unary operation
    Unary {
        op: String,
        operand: Box<Expression>,
    },
    /// Function call
    Call {
        function: Box<Expression>,
        args: Vec<Expression>,
    },
    /// Constructor call: class::new(args)
    ConstructorCall {
        class_name: String,
        args: Vec<Expression>,
    },
    /// Member access
    Member {
        object: Box<Expression>,
        field: String,
        args: Vec<Expression>,  // Arguments for method call (empty for field access)
    },
    /// Array index access: array[index]
    Index {
        array: Box<Expression>,
        index: Box<Expression>,
    },
    /// Array literal: [1, 2, 3]
    ArrayLiteral {
        elements: Vec<Expression>,
    },
    /// Tuple literal: (1, "hello", 3.14)
    TupleLiteral {
        elements: Vec<Expression>,
    },
    /// Struct literal: Point { x: 10, y: 20 }
    StructLiteral {
        struct_name: String,
        fields: Vec<(String, Expression)>,  // (field_name, field_value)
    },
    /// Assign expression: self.assign("field_name", value)
    /// Built-in method for assigning values to class fields
    Assign {
        object: Box<Expression>,  // Should be self
        field_name: String,       // Field name as string literal
        value: Box<Expression>,   // Value to assign
    },
    /// Type cast: int(value), float(value), bool(value)
    TypeCast {
        target_type: String,      // Target type name (int, float, bool)
        value: Box<Expression>,   // Value to cast
    },
}

impl Expression {
    /// Create a literal expression
    pub fn literal(value: impl Into<String>) -> Self {
        Expression::Literal(value.into())
    }

    /// Create a format string expression with safety validation
    /// Returns Err if the template contains invalid placeholders
    pub fn fstring(template: &str) -> Result<Self, String> {
        let mut placeholders = Vec::new();
        let mut chars = template.chars().peekable();
        let mut current = String::new();

        while let Some(c) = chars.next() {
            if c == '{' {
                // Check for escaped brace
                if chars.peek() == Some(&'{') {
                    chars.next();
                    current.push('{');
                    continue;
                }

                // Parse placeholder
                let mut placeholder = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '}' {
                        chars.next();
                        break;
                    }
                    if next.is_alphanumeric() || next == '_' {
                        placeholder.push(chars.next().unwrap());
                    } else {
                        return Err(format!(
                            "Invalid character '{}' in placeholder. \
                            Placeholders must be valid identifiers (alphanumeric + underscore)",
                            next
                        ));
                    }
                }

                if placeholder.is_empty() {
                    return Err("Empty placeholder {{}}. Use {{{{ for literal brace".to_string());
                }

                // Validate placeholder is a valid identifier
                if !placeholder.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) {
                    return Err(format!(
                        "Placeholder '{{{}}}' must start with a letter or underscore",
                        placeholder
                    ));
                }

                placeholders.push(placeholder.clone());
                current.push_str(&format!("{{{}}}", placeholder));
            } else if c == '}' {
                // Check for escaped brace
                if chars.peek() == Some(&'}') {
                    chars.next();
                    current.push('}');
                    continue;
                }
                // Unmatched closing brace
                return Err("Unmatched '}}'. Use '}}}' for literal brace".to_string());
            } else {
                current.push(c);
            }
        }

        Ok(Expression::FString {
            template: template.to_string(),
            placeholders,
        })
    }

    /// Create a variable expression
    pub fn var(name: impl Into<String>) -> Self {
        Expression::Variable(name.into())
    }

    /// Create a binary operation
    pub fn binary(left: Expression, op: impl Into<String>, right: Expression) -> Self {
        Expression::Binary {
            left: Box::new(left),
            op: op.into(),
            right: Box::new(right),
        }
    }

    /// Parse an expression from a string (simplified recursive descent parser)
    pub fn parse(input: &str) -> Result<Expression, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Empty expression".to_string());
        }

        parse_expression(input)
    }

    /// Get the type of this expression (simplified inference)
    pub fn infer_type(&self) -> crate::types::Type {
        match self {
            Expression::Literal(value) => {
                // Try to parse as integer
                if let Ok(_) = value.parse::<i64>() {
                    return crate::types::Type::int();
                }
                // Try to parse as float
                if let Ok(_) = value.parse::<f64>() {
                    return crate::types::Type::float();
                }
                // Check for boolean
                if value == "true" || value == "false" {
                    return crate::types::Type::bool();
                }
                // Check for string
                if value.starts_with('"') && value.ends_with('"') {
                    return crate::types::Type::string();
                }
                crate::types::Type::int()
            }
            Expression::Variable(_) => crate::types::Type::int(), // Placeholder
            Expression::FString { .. } => crate::types::Type::string(), // Format strings produce strings
            Expression::StructLiteral { struct_name, .. } => {
                crate::types::Type::NamedType { name: struct_name.clone() }
            }
            Expression::Assign { .. } => crate::types::Type::void(), // Assign expressions don't produce a value
            Expression::Binary { op, .. } => {
                if matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||") {
                    crate::types::Type::bool()
                } else {
                    crate::types::Type::int()
                }
            }
            Expression::Unary { .. } => crate::types::Type::int(),
            Expression::Call { .. } => crate::types::Type::void(),
            Expression::ConstructorCall { class_name, .. } => {
                crate::types::Type::NamedType { name: class_name.clone() }
            }
            Expression::Member { .. } => crate::types::Type::void(), // Method call returns void
            Expression::Index { .. } => crate::types::Type::int(), // Array indexing returns element type
            Expression::ArrayLiteral { .. } => crate::types::Type::int(), // Array literal returns array type (placeholder)
            Expression::TupleLiteral { .. } => crate::types::Type::int(), // Tuple literal returns tuple type (placeholder)
            Expression::TypeCast { target_type, .. } => {
                match target_type.as_str() {
                    "int" => crate::types::Type::int(),
                    "float" => crate::types::Type::float(),
                    "bool" => crate::types::Type::bool(),
                    _ => crate::types::Type::int(), // Fallback
                }
            }
        }
    }
}

/// Split a comma-separated argument list, respecting nested parens and strings.
fn parse_arg_list(args_str: &str) -> Result<Vec<Expression>, String> {
    let mut args = Vec::new();
    if args_str.is_empty() {
        return Ok(args);
    }
    let mut current_arg = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for ch in args_str.chars() {
        if escape_next {
            current_arg.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' => {
                escape_next = true;
                current_arg.push(ch);
            }
            '"' if !in_string => {
                in_string = true;
                current_arg.push(ch);
            }
            '"' if in_string => {
                in_string = false;
                current_arg.push(ch);
            }
            '(' if !in_string => {
                depth += 1;
                current_arg.push(ch);
            }
            ')' if !in_string => {
                depth -= 1;
                current_arg.push(ch);
            }
            ',' if !in_string && depth == 0 => {
                let arg = current_arg.trim().to_string();
                if !arg.is_empty() {
                    args.push(parse_expression(&arg)?);
                }
                current_arg.clear();
            }
            _ => {
                current_arg.push(ch);
            }
        }
    }
    let arg = current_arg.trim().to_string();
    if !arg.is_empty() {
        args.push(parse_expression(&arg)?);
    }
    Ok(args)
}

fn parse_rhs_or_literal(s: &str) -> Expression {
    parse_expression(s).unwrap_or_else(|_| Expression::Literal(s.to_string()))
}

pub(crate) fn assignment_rhs(s: &str) -> Expression {
    parse_rhs_or_literal(s)
}

/// Simple recursive descent expression parser
pub fn parse_expression(input: &str) -> Result<Expression, String> {
    // This is a simplified parser - for now, handle basic cases
    let input = input.trim();

    // Handle f-strings: f"..."
    if input.starts_with("f\"") && input.ends_with('"') {
        let template = &input[2..input.len()-1];
        return Expression::fstring(template);
    }

    // Handle unary operators
    if input.starts_with('!') || input.starts_with('-') {
        let op = &input[..1];
        let rest = &input[1..].trim();
        let operand = parse_expression(rest)?;
        return Ok(Expression::Unary {
            op: op.to_string(),
            operand: Box::new(operand),
        });
    }

    // Handle parentheses - could be tuple or just grouping
    if input.starts_with('(') && matching_close_paren(input, 0) == Some(input.len() - 1) {
        let inner = &input[1..input.len()-1].trim();

        // Check if it's a tuple (contains comma)
        if inner.contains(',') {
            // It's a tuple literal
            let mut elements = Vec::new();
            let mut current_element = String::new();
            let mut paren_depth = 0;
            let mut brace_depth = 0;
            let mut bracket_depth = 0;
            let mut in_string = false;

            for ch in inner.chars() {
                match ch {
                    '"' if !in_string => {
                        in_string = true;
                        current_element.push(ch);
                    },
                    '"' if in_string => {
                        in_string = false;
                        current_element.push(ch);
                    },
                    '(' if !in_string => {
                        paren_depth += 1;
                        current_element.push(ch);
                    },
                    ')' if !in_string => {
                        paren_depth -= 1;
                        current_element.push(ch);
                    },
                    '{' if !in_string => {
                        brace_depth += 1;
                        current_element.push(ch);
                    },
                    '}' if !in_string => {
                        brace_depth -= 1;
                        current_element.push(ch);
                    },
                    '[' if !in_string => {
                        bracket_depth += 1;
                        current_element.push(ch);
                    },
                    ']' if !in_string => {
                        bracket_depth -= 1;
                        current_element.push(ch);
                    },
                    ',' if !in_string && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                        if !current_element.trim().is_empty() {
                            elements.push(parse_expression(&current_element.trim())?);
                        }
                        current_element.clear();
                    },
                    _ => current_element.push(ch),
                }
            }

            if !current_element.trim().is_empty() {
                elements.push(parse_expression(&current_element.trim())?);
            }

            return Ok(Expression::TupleLiteral { elements });
        } else {
            // It's just grouping parentheses
            return parse_expression(inner);
        }
    }

    // Handle array literals: [1, 2, 3]
    if input.starts_with('[') && input.ends_with(']') {
        let elements_str = &input[1..input.len()-1].trim();
        let mut elements = Vec::new();

        if !elements_str.is_empty() {
            let mut current_element = String::new();
            let mut paren_depth = 0;
            let mut brace_depth = 0;
            let mut bracket_depth = 0;
            let mut in_string = false;

            for ch in elements_str.chars() {
                match ch {
                    '"' if !in_string => {
                        in_string = true;
                        current_element.push(ch);
                    },
                    '"' if in_string => {
                        in_string = false;
                        current_element.push(ch);
                    },
                    '(' if !in_string => {
                        paren_depth += 1;
                        current_element.push(ch);
                    },
                    ')' if !in_string => {
                        paren_depth -= 1;
                        current_element.push(ch);
                    },
                    '{' if !in_string => {
                        brace_depth += 1;
                        current_element.push(ch);
                    },
                    '}' if !in_string => {
                        brace_depth -= 1;
                        current_element.push(ch);
                    },
                    '[' if !in_string => {
                        bracket_depth += 1;
                        current_element.push(ch);
                    },
                    ']' if !in_string => {
                        bracket_depth -= 1;
                        current_element.push(ch);
                    },
                    ',' if !in_string && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                        if !current_element.trim().is_empty() {
                            elements.push(parse_expression(&current_element.trim())?);
                        }
                        current_element.clear();
                    },
                    _ => current_element.push(ch),
                }
            }

            if !current_element.trim().is_empty() {
                elements.push(parse_expression(&current_element.trim())?);
            }
        }

        return Ok(Expression::ArrayLiteral { elements });
    }

    // Handle array indexing: array[index]
    // Must come before binary operators to give [ higher precedence
    if let Some(pos) = input.find('[') {
        if input.ends_with(']') {
            let array_str = &input[..pos].trim();
            let index_str = &input[pos + 1..input.len() - 1].trim();

            // Recursively parse array and index
            let array_expr = parse_expression(array_str)?;
            let index_expr = parse_expression(index_str)?;

            return Ok(Expression::Index {
                array: Box::new(array_expr),
                index: Box::new(index_expr),
            });
        }
    }

    // Handle struct literals: Point { x: 10, y: 20 }
    if let Some(pos) = input.find('{') {
        if input.ends_with('}') {
            let struct_name = input[..pos].trim();
            let fields_str = &input[pos + 1..input.len() - 1].trim();
            eprintln!("DEBUG: parse_expression struct literal: struct_name='{}', fields_str='{}'", struct_name, fields_str);

            // Validate struct_name - must be a valid identifier (only alphanumeric and underscore)
            if !struct_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                eprintln!("DEBUG: parse_expression struct literal: struct_name is not a valid identifier, skipping");
            } else {
                // Parse fields: x: 10, y: 20
            // Use smart parsing to handle commas in expressions (like Point::new(x1, y1))
            let mut fields = Vec::new();
            if !fields_str.is_empty() {
                let mut current_field = String::new();
                let mut paren_depth = 0;  // Track parentheses depth
                let mut brace_depth = 0;  // Track braces depth
                let mut in_string = false;

                for ch in fields_str.chars() {
                    match ch {
                        '"' if !in_string => {
                            in_string = true;
                            current_field.push(ch);  // Add opening quote
                        },
                        '"' if in_string => {
                            in_string = false;
                            current_field.push(ch);  // Add closing quote
                        },
                        '(' if !in_string => {
                            paren_depth += 1;
                            current_field.push(ch);
                        },
                        ')' if !in_string => {
                            paren_depth -= 1;
                            current_field.push(ch);
                        },
                        '{' if !in_string => {
                            brace_depth += 1;
                            current_field.push(ch);
                        },
                        '}' if !in_string => {
                            brace_depth -= 1;
                            current_field.push(ch);
                        },
                        ',' if !in_string && paren_depth == 0 && brace_depth == 0 => {
                            let field_pair = current_field.trim();
                            if !field_pair.is_empty() {
                                // Find the first colon that is not part of :: (double colon)
                                // This handles cases like "field: Point::new(x, y)"
                                let mut colon_pos = None;
                                let mut chars = field_pair.chars().enumerate();
                                while let Some((i, ch)) = chars.next() {
                                    if ch == ':' {
                                        // Check if this is part of ::
                                        if let Some(next_ch) = field_pair.chars().nth(i + 1) {
                                            if next_ch == ':' {
                                                // This is ::, skip both characters
                                                chars.next(); // skip the second colon
                                            } else {
                                                // This is a single colon, this is the field separator
                                                colon_pos = Some(i);
                                                break;
                                            }
                                        } else {
                                            // Single colon at the end
                                            colon_pos = Some(i);
                                            break;
                                        }
                                    }
                                }

                                if let Some(pos) = colon_pos {
                                    let field_name = field_pair[..pos].trim();
                                    let field_value = field_pair[pos + 1..].trim();
                                    eprintln!("DEBUG: parse_expression struct field: field_pair='{}', field_name='{}', field_value='{}'", field_pair, field_name, field_value);
                                    fields.push((field_name.to_string(), parse_expression(field_value)?));
                                }
                            }
                            current_field.clear();
                        }
                        _ => current_field.push(ch),
                    }
                }
                
                // Don't forget the last field
                let field_pair = current_field.trim();
                if !field_pair.is_empty() {
                    // Find the first colon that is not part of :: (double colon)
                    let mut colon_pos = None;
                    let mut chars = field_pair.chars().enumerate();
                    while let Some((i, ch)) = chars.next() {
                        if ch == ':' {
                            // Check if this is part of ::
                            if let Some(next_ch) = field_pair.chars().nth(i + 1) {
                                if next_ch == ':' {
                                    // This is ::, skip both characters
                                    chars.next(); // skip the second colon
                                } else {
                                    // This is a single colon, this is the field separator
                                    colon_pos = Some(i);
                                    break;
                                }
                            } else {
                                // Single colon at the end
                                colon_pos = Some(i);
                                break;
                            }
                        }
                    }

                    if let Some(pos) = colon_pos {
                        let field_name = field_pair[..pos].trim();
                        let field_value = field_pair[pos + 1..].trim();
                        eprintln!("DEBUG: parse_expression struct field (last): field_name='{}', field_value='{}'", field_name, field_value);
                        fields.push((field_name.to_string(), parse_expression(field_value)?));
                    }
                }
            }

            return Ok(Expression::StructLiteral {
                struct_name: struct_name.to_string(),
                fields,
            });
            }
        }
    }

    // Handle function calls: name(args) or class::new(args) or Enum::Variant(args)
        // Must come before member access to avoid treating printf("...", p.x) as member access
        // But must skip if func_name contains '.' (member access like p.move(5, 5))
        // Or if func_name contains operators (like 'a /' in 'a / (b + c)')
        if let Some(pos) = input.find('(') {
            if matching_close_paren(input, pos) == Some(input.len() - 1) {
                let func_name = input[..pos].trim().to_string();
                let args_str = &input[pos + 1..input.len() - 1].trim();
                eprintln!("DEBUG: parse_expression: input='{}', pos={}, func_name='{}', args_str='{}'", input, pos, func_name, args_str);
    
                // Skip if func_name contains '.' (member access like p.move(5, 5))
                // This will be handled by member access logic below
                // Also skip if func_name contains operators (like 'a /' in 'a / (b + c)')
                if func_name.contains('.') || func_name.contains('+') || func_name.contains('-') || func_name.contains('*') || func_name.contains('/') || func_name.contains('%') || func_name.contains('&') || func_name.contains('|') || func_name.contains('^') || func_name.contains('<') || func_name.contains('>') || func_name.contains('=') || func_name.contains('!') {
                    eprintln!("DEBUG: parse_expression: func_name contains operator or '.', skipping function call parsing");
                } else {
                // Check if this is a type conversion function: int(value), float(value), bool(value)
                // Type conversion functions are basic type names followed by parentheses
                if matches!(func_name.as_str(), "int" | "float" | "bool") {
                    eprintln!("DEBUG: parse_expression: type conversion function '{}'", func_name);
                    // Parse the argument
                    let arg_expr = parse_expression(args_str)?;
                    return Ok(Expression::TypeCast {
                        target_type: func_name.clone(),
                        value: Box::new(arg_expr),
                    });
                }
                
                // Check if this is a constructor call: class::new(args)
                eprintln!("DEBUG: parse_expression: func_name.contains(\"::new\")={}", func_name.contains("::new"));
                if func_name.contains("::new") {
                    let parts: Vec<&str> = func_name.split("::").collect();
                    if parts.len() == 2 && parts[1] == "new" {
                        let class_name = parts[0].to_string();
                        let args = parse_arg_list(args_str)?;
    
                        return Ok(Expression::ConstructorCall {
                            class_name,
                            args,
                        });
                    }
                }
                
                // Check if this is an enum variant: Enum::Variant(args)
                if func_name.contains("::") {
                    let parts: Vec<&str> = func_name.split("::").collect();
                    if parts.len() == 2 {
                        let enum_name = parts[0].to_string();
                        let variant_name = parts[1].to_string();
                        let mut args: Vec<String> = Vec::new();
                        
                        // Parse arguments
                        if !args_str.is_empty() {
                            let mut current_arg = String::new();
                            let mut depth = 0;
                            let mut in_string = false;
                            let mut escape_next = false;
    
                            for ch in args_str.chars() {
                                if escape_next {
                                    current_arg.push(ch);
                                    escape_next = false;
                                    continue;
                                }
    
                                match ch {
                                    '\\' => {
                                        escape_next = true;
                                        current_arg.push(ch);
                                    }
                                    '"' if !in_string => {
                                        in_string = true;
                                        current_arg.push(ch);
                                    }
                                    '"' if in_string => {
                                        in_string = false;
                                        current_arg.push(ch);
                                    }
                                    '(' if !in_string => {
                                        depth += 1;
                                        current_arg.push(ch);
                                    }
                                    ')' if !in_string => {
                                        depth -= 1;
                                        current_arg.push(ch);
                                    }
                                    ',' if !in_string && depth == 0 => {
                                        let arg = current_arg.trim().to_string();
                                        if !arg.is_empty() {
                                            args.push(arg);
                                        }
                                        current_arg.clear();
                                    }
                                    _ => {
                                        current_arg.push(ch);
                                    }
                                }
                            }
                            if !current_arg.trim().is_empty() {
                                args.push(current_arg.trim().to_string());
                            }
                        }
                        
                        // Return as a function call with enum name and variant name
                        // This will be handled by the backend as an enum variant
                        return Ok(Expression::Call {
                            function: Box::new(Expression::var(&format!("{}::{}", enum_name, variant_name))),
                            args: args.iter().map(|arg| parse_expression(arg)).collect::<Result<Vec<_>, _>>()?,
                        });
                    }
                }
                
                // Regular function call
                let mut args: Vec<Expression> = Vec::new();
                if !args_str.is_empty() {
                    // Use the same logic as split_function_args to handle commas in strings
                    let mut current_arg = String::new();
                    let mut depth = 0;
                    let mut in_string = false;
                    let mut escape_next = false;
    
                    for ch in args_str.chars() {
                        if escape_next {
                            current_arg.push(ch);
                            escape_next = false;
                            continue;
                        }
    
                        match ch {
                            '\\' => {
                                escape_next = true;
                                current_arg.push(ch);
                            }
                            '"' if !in_string => {
                                in_string = true;
                                current_arg.push(ch);
                            }
                            '"' if in_string => {
                                in_string = false;
                                current_arg.push(ch);
                            }
                            '(' if !in_string => {
                                depth += 1;
                                current_arg.push(ch);
                            }
                            ')' if !in_string => {
                                depth -= 1;
                                current_arg.push(ch);
                            }
                            ',' if !in_string && depth == 0 => {
                                // Top-level comma - this separates arguments
                                let arg = current_arg.trim().to_string();
                                if !arg.is_empty() {
                                    let parsed_arg = parse_expression(&arg)?;
                                    args.push(parsed_arg);
                                }
                                current_arg = String::new();
                            }
                            _ => {
                                current_arg.push(ch);
                            }
                        }
                    }
    
                    // Don't forget the last argument
                    let arg = current_arg.trim().to_string();
                    if !arg.is_empty() {
                        let parsed_arg = parse_expression(&arg)?;
                        args.push(parsed_arg);
                    }
                }
    
                return Ok(Expression::Call {
                    function: Box::new(Expression::var(func_name)),
                    args,
                });
            }
        }    }

    // Handle member access: object.method(args) or object.field
    // Must come after function calls to avoid treating printf("...", p.x) as member access
    if !input.starts_with('"') && !input.ends_with('"') && input.contains('.') {
        if let Some(pos) = input.rfind('.') {
            let object_str = &input[..pos].trim();
            let rest = &input[pos + 1..].trim();
            let object_ok = !object_str.is_empty()
                && object_str.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
            if object_ok && !object_str.chars().all(|c| c.is_ascii_digit() || c == '.') {
                let is_float_literal = object_str.parse::<f64>().is_ok() && rest.parse::<u64>().is_ok();
                if !is_float_literal {
                    if rest.contains('(') && rest.ends_with(')') {
                        let method_name = &rest[..rest.find('(').unwrap()];
                        if is_valid_identifier(method_name) {
                            let args_str = &rest[method_name.len() + 1..rest.len() - 1].trim();
                            let args = parse_arg_list(args_str)?;
                            return Ok(Expression::Member {
                                object: Box::new(parse_expression(object_str)?),
                                field: method_name.to_string(),
                                args,
                            });
                        }
                    } else if is_valid_identifier(rest) {
                        return Ok(Expression::Member {
                            object: Box::new(parse_expression(object_str)?),
                            field: rest.to_string(),
                            args: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    // Handle binary operators (simplified - only handle first operator found)
    // Skip operators inside string literals
    let mut in_string = false;
    let mut string_start = 0;
    for (i, c) in input.char_indices() {
        if c == '"' && (i == 0 || input.chars().nth(i - 1) != Some('\\')) {
            in_string = !in_string;
            if in_string {
                string_start = i;
            }
        }
    }
    if !in_string {
        for op in ["&&", "||", "==", "!=", "<=", ">=", "<<", ">>", "<", ">", "^", "|", "&", "+", "-", "*", "/", "%"] {
            if let Some(pos) = find_bin_op_outside_parens(input, op) {
                if pos > 0 {
                    let left_str = &input[..pos].trim();
                    let right_str = &input[pos + op.len()..].trim();
                    if !left_str.is_empty() && !right_str.is_empty() {
                        let left = parse_expression(left_str)?;
                        let right = parse_expression(right_str)?;
                        return Ok(Expression::Binary {
                            left: Box::new(left),
                            op: op.to_string(),
                            right: Box::new(right),
                        });
                    }
                }
            }
        }
    }

    // Handle literals and variables
    // Check if it's a string literal
    if input.starts_with('"') && input.ends_with('"') {
        return Ok(Expression::literal(input));
    }

    // Check if it's a boolean literal
    if input == "true" || input == "false" {
        return Ok(Expression::literal(input));
    }

    // Check if it's a number
    if input.parse::<i64>().is_ok() || input.parse::<f64>().is_ok() {
        return Ok(Expression::literal(input));
    }

    // Otherwise, treat as variable - but validate it's a valid identifier
    if is_valid_identifier(input) {
        Ok(Expression::var(input))
    } else {
        Err(format!("Invalid identifier '{}': contains invalid characters", input))
    }
}

/// Check if a string is a valid identifier
/// Valid identifiers start with a letter or underscore, and contain only letters, digits, and underscores
pub fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();

    // First character must be letter or underscore
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {},
        _ => return false,
    }

    // Remaining characters must be alphanumeric or underscore
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn matching_close_paren(input: &str, open_pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    if open_pos >= bytes.len() || bytes[open_pos] != b'(' {
        return None;
    }
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    for i in open_pos..bytes.len() {
        let ch = bytes[i] as char;
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn find_bin_op_outside_parens(expr: &str, op: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let expr_bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    let mut i = 0;
    while i < expr_bytes.len() {
        let ch = expr_bytes[i] as char;
        if escape_next {
            escape_next = false;
            i += 1;
        } else if ch == '\\' {
            escape_next = true;
            i += 1;
        } else if ch == '"' && !in_string {
            in_string = true;
            i += 1;
        } else if ch == '"' && in_string {
            in_string = false;
            i += 1;
        } else if !in_string && ch == '(' {
            depth += 1;
            i += 1;
        } else if !in_string && ch == ')' {
            depth -= 1;
            i += 1;
        } else if !in_string && depth == 0 && i + op_bytes.len() <= expr_bytes.len()
            && &expr_bytes[i..i + op_bytes.len()] == op_bytes
        {
            // Do not match a short operator that is a prefix of a longer one.
            let after = i + op_bytes.len();
            let next = expr_bytes.get(after).copied().map(|b| b as char);
            let skip = match op {
                "<" | ">" => next == Some(op.chars().next().unwrap()) || next == Some('='),
                "&" | "|" => next == Some(op.chars().next().unwrap()),
                "=" => next == Some('='),
                _ => false,
            };
            if !skip {
                return Some(i);
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_expression() {
        let expr = Expression::literal("42");
        assert!(matches!(expr, Expression::Literal(_)));
    }

    #[test]
    fn test_variable_expression() {
        let expr = Expression::var("x");
        assert!(matches!(expr, Expression::Variable(_)));
    }

    #[test]
    fn test_binary_expression() {
        let expr = Expression::binary(
            Expression::var("x"),
            "+",
            Expression::literal("10"),
        );
        assert!(matches!(expr, Expression::Binary { .. }));
    }

    #[test]
    fn test_parse_simple_expr() {
        let result = Expression::parse("x + 10");
        assert!(result.is_ok());
        if let Ok(Expression::Binary { op, .. }) = result {
            assert_eq!(op, "+");
        } else {
            panic!("Expected Binary expression");
        }
    }

    #[test]
    fn test_parse_complex_expr() {
        let result = Expression::parse("x + y * z");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unary() {
        let result = Expression::parse("!true");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_function_call() {
        let result = Expression::parse("f(x, y)");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_array_index() {
        let result = Expression::parse("arr[0]");
        assert!(result.is_ok());
        if let Ok(Expression::Index { array, index }) = result {
            assert!(matches!(*array, Expression::Variable(_)));
            assert!(matches!(*index, Expression::Literal(_)));
        } else {
            panic!("Expected Index expression");
        }
    }

    #[test]
    fn test_parse_array_index_with_variable() {
        let result = Expression::parse("arr[i]");
        assert!(result.is_ok());
        if let Ok(Expression::Index { array, index }) = result {
            assert!(matches!(*array, Expression::Variable(_)));
            assert!(matches!(*index, Expression::Variable(_)));
        } else {
            panic!("Expected Index expression");
        }
    }

    #[test]
    fn test_parse_array_index_with_expression() {
        let result = Expression::parse("arr[i + 1]");
        assert!(result.is_ok());
        if let Ok(Expression::Index { index, .. }) = result {
            assert!(matches!(*index, Expression::Binary { .. }));
        } else {
            panic!("Expected Index expression with binary index");
        }
    }

    #[test]
    fn constructor_args_are_expressions() {
        let result = parse_expression("Point::new(1, 2)").unwrap();
        match result {
            Expression::ConstructorCall { args, .. } => {
                assert_eq!(args.len(), 2);
                assert!(matches!(args[0], Expression::Literal(_)));
                assert!(matches!(args[1], Expression::Literal(_)));
            }
            other => panic!("{:?}", other),
        }
    }
}

impl std::fmt::Display for Expression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expression::Literal(value) => write!(f, "{}", value),
            Expression::Variable(name) => write!(f, "{}", name),
            Expression::FString { template, .. } => write!(f, "\"{}\"", template),
            Expression::Binary { left, op, right } => write!(f, "({} {} {})", left, op, right),
            Expression::Unary { op, operand } => write!(f, "({}{})", op, operand),
            Expression::Call { function, args } => {
                write!(f, "{}(", function)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expression::Member { object, field, args } => {
                if args.is_empty() {
                    write!(f, "{}.{}", object, field)
                } else {
                    write!(f, "{}.{}(", object, field)?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", arg)?;
                    }
                    write!(f, ")")
                }
            }
            Expression::Index { array, index } => write!(f, "{}[{}]", array, index),
            Expression::ArrayLiteral { elements } => {
                write!(f, "[")?;
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", element)?;
                }
                write!(f, "]")
            }
            Expression::TupleLiteral { elements } => {
                write!(f, "(")?;
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", element)?;
                }
                write!(f, ")")
            }
            Expression::ConstructorCall { class_name, args } => {
                write!(f, "{}::new(", class_name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expression::StructLiteral { struct_name, fields } => {
                write!(f, "{} {{ ", struct_name)?;
                for (i, (field_name, field_value)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", field_name, field_value)?;
                }
                write!(f, " }}")
            }
            Expression::Assign { object, field_name, value } => {
                write!(f, "{}.assign(\"{}\", {})", object, field_name, value)
            },
            Expression::TypeCast { target_type, value } => {
                write!(f, "{}({})", target_type, value)
            },
        }
    }
}
