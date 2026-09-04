// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License
//
// Match-arm patterns. Parsed from the same surface syntax as expressions,
// then lowered into this dedicated AST so match codegen and type-checking
// do not string-match `Expression::Display`.

use super::expr::Expression;

/// A pattern on the left of `=>` in a `match` arm (not including `if` guards).
#[derive(Debug, PartialEq, Clone)]
pub enum Pattern {
    Wildcard,
    Literal(String),
    Ident(String),
    Tuple(Vec<Pattern>),
    Struct {
        name: String,
        fields: Vec<(String, Pattern)>,
    },
    EnumVariant {
        enum_name: String,
        variant: String,
        args: Vec<Pattern>,
    },
    Or(Vec<Pattern>),
}

impl Pattern {
    /// Lower a parsed expression used as a match pattern.
    pub fn from_expr(expr: Expression) -> Self {
        match expr {
            Expression::Variable(name) if name == "_" => Pattern::Wildcard,
            Expression::Variable(name) => Pattern::Ident(name),
            Expression::Literal(value) => Pattern::Literal(value),
            Expression::Unary { op, operand } if op == "-" => match *operand {
                Expression::Literal(value) => {
                    if value.starts_with('-') {
                        Pattern::Literal(value)
                    } else {
                        Pattern::Literal(format!("-{}", value))
                    }
                }
                other => Pattern::Literal(format!("-{}", other)),
            },
            Expression::TupleLiteral { elements } => {
                Pattern::Tuple(elements.into_iter().map(Pattern::from_expr).collect())
            }
            Expression::StructLiteral { struct_name, fields } => Pattern::Struct {
                name: struct_name,
                fields: fields
                    .into_iter()
                    .map(|(name, value)| (name, Pattern::from_expr(value)))
                    .collect(),
            },
            Expression::Member { object, field, args } => {
                let enum_name = match *object {
                    Expression::Variable(n) => n,
                    other => other.to_string(),
                };
                Pattern::EnumVariant {
                    enum_name,
                    variant: field,
                    args: args.into_iter().map(Pattern::from_expr).collect(),
                }
            }
            Expression::Call { function, args } => match *function {
                Expression::Variable(name) => {
                    if let Some((enum_name, variant)) = name.split_once("::") {
                        Pattern::EnumVariant {
                            enum_name: enum_name.to_string(),
                            variant: variant.to_string(),
                            args: args.into_iter().map(Pattern::from_expr).collect(),
                        }
                    } else if let Some((enum_name, variant)) = name.split_once('.') {
                        Pattern::EnumVariant {
                            enum_name: enum_name.to_string(),
                            variant: variant.to_string(),
                            args: args.into_iter().map(Pattern::from_expr).collect(),
                        }
                    } else {
                        Pattern::EnumVariant {
                            enum_name: String::new(),
                            variant: name,
                            args: args.into_iter().map(Pattern::from_expr).collect(),
                        }
                    }
                }
                Expression::Member {
                    object,
                    field,
                    args: member_args,
                } if member_args.is_empty() => {
                    let enum_name = match *object {
                        Expression::Variable(n) => n,
                        other => other.to_string(),
                    };
                    Pattern::EnumVariant {
                        enum_name,
                        variant: field,
                        args: args.into_iter().map(Pattern::from_expr).collect(),
                    }
                }
                other => Pattern::from_expr(other),
            },
            Expression::Binary { left, op, right } if op == "|" => {
                let mut alts = Vec::new();
                flatten_or(Pattern::from_expr(*left), &mut alts);
                flatten_or(Pattern::from_expr(*right), &mut alts);
                Pattern::Or(alts)
            }
            other => Pattern::Literal(other.to_string()),
        }
    }

    /// Reconstruct an expression for value comparison (literals and enum tags).
    pub fn to_compare_expr(&self) -> Option<Expression> {
        match self {
            Pattern::Literal(value) => Some(Expression::Literal(value.clone())),
            Pattern::Ident(name) => Some(Expression::Variable(name.clone())),
            Pattern::EnumVariant {
                enum_name,
                variant,
                args,
            } => {
                let arg_exprs: Option<Vec<Expression>> =
                    args.iter().map(|a| a.to_compare_expr()).collect();
                Some(Expression::Member {
                    object: Box::new(Expression::Variable(enum_name.clone())),
                    field: variant.clone(),
                    args: arg_exprs?,
                })
            }
            _ => None,
        }
    }
}

fn flatten_or(pattern: Pattern, out: &mut Vec<Pattern>) {
    match pattern {
        Pattern::Or(alts) => {
            for alt in alts {
                flatten_or(alt, out);
            }
        }
        other => out.push(other),
    }
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pattern::Wildcard => write!(f, "_"),
            Pattern::Literal(value) => write!(f, "{}", value),
            Pattern::Ident(name) => write!(f, "{}", name),
            Pattern::Tuple(elems) => {
                write!(f, "(")?;
                for (i, el) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", el)?;
                }
                write!(f, ")")
            }
            Pattern::Struct { name, fields } => {
                write!(f, "{} {{ ", name)?;
                for (i, (field, pat)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", field, pat)?;
                }
                write!(f, " }}")
            }
            Pattern::EnumVariant {
                enum_name,
                variant,
                args,
            } => {
                if enum_name.is_empty() {
                    write!(f, "{}", variant)?;
                } else {
                    write!(f, "{}.{}", enum_name, variant)?;
                }
                if !args.is_empty() {
                    write!(f, "(")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", arg)?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Pattern::Or(alts) => {
                for (i, alt) in alts.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", alt)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(src: &str) -> Pattern {
        Pattern::from_expr(crate::parser::expr::parse_expression(src).unwrap())
    }

    #[test]
    fn wildcard_and_ident() {
        assert_eq!(pat("_"), Pattern::Wildcard);
        assert_eq!(pat("n"), Pattern::Ident("n".into()));
    }

    #[test]
    fn literals_and_negatives() {
        assert_eq!(pat("1"), Pattern::Literal("1".into()));
        assert_eq!(pat("-5"), Pattern::Literal("-5".into()));
        assert_eq!(pat("true"), Pattern::Literal("true".into()));
    }

    #[test]
    fn tuple_struct_enum_or() {
        assert!(matches!(pat("(a, _)"), Pattern::Tuple(_)));
        assert!(matches!(pat("Point { x: a, y: b }"), Pattern::Struct { .. }));
        match pat("Color.Red") {
            Pattern::EnumVariant {
                enum_name, variant, args
            } => {
                assert_eq!(enum_name, "Color");
                assert_eq!(variant, "Red");
                assert!(args.is_empty());
            }
            other => panic!("{:?}", other),
        }
        match pat("Option.Some(x)") {
            Pattern::EnumVariant { args, .. } => {
                assert_eq!(args, vec![Pattern::Ident("x".into())]);
            }
            other => panic!("{:?}", other),
        }
        match pat("1 | 2 | 3") {
            Pattern::Or(alts) => assert_eq!(alts.len(), 3),
            other => panic!("{:?}", other),
        }
    }
}
