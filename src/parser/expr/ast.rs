// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License
//
// Expression types for the Coffee language

#[derive(Debug, Clone)]
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
    /// Anonymous function expression: `fn(x: int) => int:` + indented body.
    AnonymousFunction {
        func: Box<crate::parser::function::Function>,
    },
    /// Source range wrapping an inner expression. Peel with [`Expression::kind`].
    Spanned {
        #[cfg_attr(not(test), allow(dead_code))]
        span: crate::types::definition::Span,
        inner: Box<Expression>,
    },
}

impl Expression {
    /// Innermost non-`Spanned` node.
    pub fn kind(&self) -> &Expression {
        match self {
            Expression::Spanned { inner, .. } => inner.kind(),
            other => other,
        }
    }

    /// Outermost `Spanned` range, or `Span::new(0, 0)` for test constructors.
    #[cfg(test)]
    pub fn span(&self) -> crate::types::definition::Span {
        match self {
            Expression::Spanned { span, .. } => *span,
            _ => crate::types::definition::Span::new(0, 0),
        }
    }

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
    #[cfg(test)]
    pub fn binary(left: Expression, op: impl Into<String>, right: Expression) -> Self {
        Expression::Binary {
            left: Box::new(left),
            op: op.into(),
            right: Box::new(right),
        }
    }

    /// Parse an expression from a string (simplified recursive descent parser)
    #[cfg(test)]
    pub fn parse(input: &str) -> Result<Expression, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Empty expression".to_string());
        }

        super::parse_expression(input)
    }

    /// Test-only syntactic type from the expression shape. Production typing
    /// must use the type checker / infer callback — never this helper.
    /// Returns `None` when the type cannot be determined from syntax alone.
    #[cfg(test)]
    pub fn infer_type(&self) -> Option<crate::types::Type> {
        match self.kind() {
            Expression::Literal(value) => {
                if value.parse::<i64>().is_ok() {
                    Some(crate::types::Type::int())
                } else if value.parse::<f64>().is_ok() {
                    Some(crate::types::Type::float())
                } else if value == "true" || value == "false" {
                    Some(crate::types::Type::bool())
                } else if value.starts_with('"') && value.ends_with('"') {
                    Some(crate::types::Type::string())
                } else {
                    None
                }
            }
            Expression::Variable(_) => None,
            Expression::FString { .. } => Some(crate::types::Type::string()),
            Expression::StructLiteral { struct_name, .. } => {
                Some(crate::types::Type::NamedType {
                    name: struct_name.clone(),
                })
            }
            Expression::Assign { .. } => Some(crate::types::Type::void()),
            Expression::Binary { op, .. } => {
                if matches!(
                    op.as_str(),
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
                ) {
                    Some(crate::types::Type::bool())
                } else {
                    None
                }
            }
            Expression::Unary { operand, .. } => operand.infer_type(),
            Expression::Call { .. } => None,
            Expression::ConstructorCall { class_name, .. } => {
                Some(crate::types::Type::NamedType {
                    name: class_name.clone(),
                })
            }
            Expression::Member { .. } => None,
            Expression::Index { .. } => None,
            Expression::ArrayLiteral { .. } => None,
            Expression::TupleLiteral { .. } => None,
            Expression::TypeCast { target_type, .. } => Some(type_from_annotation(target_type)),
            Expression::AnonymousFunction { func } => {
                let params = func
                    .parameters
                    .iter()
                    .map(|p| type_from_annotation(&p.param_type))
                    .collect();
                let return_type = type_from_annotation(&func.return_type);
                Some(crate::types::Type::Function {
                    params,
                    return_type: Box::new(return_type),
                })
            }
            Expression::Spanned { .. } => unreachable!("kind() peels Spanned"),
        }
    }
}

#[cfg(test)]
fn type_from_annotation(s: &str) -> crate::types::Type {
    crate::types::Type::from_str(s).unwrap_or_else(|_| crate::types::Type::NamedType {
        name: s.to_string(),
    })
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
            Expression::AnonymousFunction { func } => {
                write!(f, "fn(")?;
                for (i, p) in func.parameters.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", p.name, p.param_type)?;
                }
                write!(f, ") => {}:", func.return_type)
            }
            Expression::Spanned { inner, .. } => write!(f, "{}", inner),
        }
    }
}

impl PartialEq for Expression {
    fn eq(&self, other: &Self) -> bool {
        match (self.kind(), other.kind()) {
            (Expression::Literal(a), Expression::Literal(b)) => a == b,
            (Expression::Variable(a), Expression::Variable(b)) => a == b,
            (
                Expression::FString {
                    template: ta,
                    placeholders: pa,
                },
                Expression::FString {
                    template: tb,
                    placeholders: pb,
                },
            ) => ta == tb && pa == pb,
            (
                Expression::Binary {
                    left: la,
                    op: oa,
                    right: ra,
                },
                Expression::Binary {
                    left: lb,
                    op: ob,
                    right: rb,
                },
            ) => la == lb && oa == ob && ra == rb,
            (
                Expression::Unary {
                    op: oa,
                    operand: a,
                },
                Expression::Unary {
                    op: ob,
                    operand: b,
                },
            ) => oa == ob && a == b,
            (
                Expression::Call {
                    function: fa,
                    args: aa,
                },
                Expression::Call {
                    function: fb,
                    args: ab,
                },
            ) => fa == fb && aa == ab,
            (
                Expression::ConstructorCall {
                    class_name: ca,
                    args: aa,
                },
                Expression::ConstructorCall {
                    class_name: cb,
                    args: ab,
                },
            ) => ca == cb && aa == ab,
            (
                Expression::Member {
                    object: oa,
                    field: fa,
                    args: aa,
                },
                Expression::Member {
                    object: ob,
                    field: fb,
                    args: ab,
                },
            ) => oa == ob && fa == fb && aa == ab,
            (
                Expression::Index {
                    array: aa,
                    index: ia,
                },
                Expression::Index {
                    array: ab,
                    index: ib,
                },
            ) => aa == ab && ia == ib,
            (
                Expression::ArrayLiteral { elements: a },
                Expression::ArrayLiteral { elements: b },
            ) => a == b,
            (
                Expression::TupleLiteral { elements: a },
                Expression::TupleLiteral { elements: b },
            ) => a == b,
            (
                Expression::StructLiteral {
                    struct_name: na,
                    fields: fa,
                },
                Expression::StructLiteral {
                    struct_name: nb,
                    fields: fb,
                },
            ) => na == nb && fa == fb,
            (
                Expression::Assign {
                    object: oa,
                    field_name: fa,
                    value: va,
                },
                Expression::Assign {
                    object: ob,
                    field_name: fb,
                    value: vb,
                },
            ) => oa == ob && fa == fb && va == vb,
            (
                Expression::TypeCast {
                    target_type: ta,
                    value: va,
                },
                Expression::TypeCast {
                    target_type: tb,
                    value: vb,
                },
            ) => ta == tb && va == vb,
            (
                Expression::AnonymousFunction { func: a },
                Expression::AnonymousFunction { func: b },
            ) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    use crate::parser::expr::parse_expression;

    #[test]
    fn display_spanned_equals_format_of_inner() {
        let expr = parse_expression("1").unwrap();
        let Expression::Spanned { inner, .. } = &expr else {
            panic!("parse_expression should wrap in Spanned, got {expr:?}");
        };
        assert_eq!(format!("{expr}"), format!("{inner}"));
        assert_eq!(format!("{expr}"), "1");
    }
}
