use super::super::super::definition::{Type, TypeDef};
use crate::types::errors::TypeSystemError;

impl super::super::TypeChecker {
    pub(in super::super) fn check_struct_literal(
        &mut self,
        struct_name: &str,
        fields: &[(String, crate::parser::expr::Expression)],
    ) -> Result<Type, TypeSystemError> {
        let expected_names: Vec<String> = {
            let registry = self.registry.read().map_err(|_| {
                TypeSystemError::internal("type registry lock poisoned")
            })?;
            match registry.get_type(struct_name) {
                Some(TypeDef::Class { .. }) => {
                    registry.class_field_names_including_ancestors(struct_name)?
                }
                None => {
                    return Err(TypeSystemError::undefined_type(
                        struct_name.to_string(),
                        TypeSystemError::span_for_name(struct_name),
                    ));
                }
                Some(_) => {
                    return Err(TypeSystemError::invalid_operation(
                        "struct literal",
                        Type::NamedType {
                            name: struct_name.to_string(),
                        },
                        Type::NamedType {
                            name: struct_name.to_string(),
                        },
                        TypeSystemError::span_for_name(struct_name),
                    ));
                }
            }
        };

        for (field_name, value) in fields {
            let expected = self.lookup_field_type(struct_name, field_name)?;
            let found = self.check_expression_hint(value, Some(&expected))?;
            if !self.types_compatible(&found, &expected)? {
                let error = TypeSystemError::type_mismatch(
                    expected,
                    found,
                    TypeSystemError::span_for_name(field_name),
                );
                self.add_error(error.clone());
                return Err(error);
            }
        }

        for name in &expected_names {
            if !fields.iter().any(|(n, _)| n == name) {
                return Err(TypeSystemError::FieldNotFound {
                    type_name: struct_name.to_string(),
                    field_name: name.clone(),
                    span: TypeSystemError::span_for_name(name),
                    similar: Vec::new(),
                });
            }
        }

        Ok(Type::NamedType { name: struct_name.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::TypeChecker;
    use crate::parser::expr::Expression;
    use crate::types::definition::TypeDef;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    #[test]
    fn empty_literal_errors_when_ancestor_field_walk_fails() {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Broken",
                TypeDef::Class {
                    name: "Broken".to_string(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: Some("NoSuchParent".to_string()),
                },
            )
            .unwrap();
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(registry)));
        let expr = Expression::StructLiteral {
            struct_name: "Broken".to_string(),
            fields: vec![],
        };
        let err = checker
            .check_expression(&expr)
            .expect_err("failed ancestor walk must not type-check as zero required fields");
        match err {
            TypeSystemError::Internal { .. } | TypeSystemError::UndefinedType { .. } => {}
            other => panic!("expected Internal or UndefinedType, got {other:?}"),
        }
    }

    #[test]
    fn non_class_struct_literal_uses_named_type_on_both_sides() {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Color",
                TypeDef::Enum {
                    name: "Color".to_string(),
                    variants: vec![],
                    generics: vec![],
                },
            )
            .unwrap();
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(registry)));
        let expr = Expression::StructLiteral {
            struct_name: "Color".to_string(),
            fields: vec![],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        match err {
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                assert_eq!(op, "struct literal");
                assert_eq!(
                    left,
                    crate::types::definition::Type::NamedType {
                        name: "Color".into()
                    }
                );
                assert_eq!(
                    right,
                    crate::types::definition::Type::NamedType {
                        name: "Color".into()
                    }
                );
            }
            other => panic!("expected InvalidOperation, got {other:?}"),
        }
    }
}
