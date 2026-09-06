use super::super::super::definition::{MethodSignature, Type, TypeDef};
use crate::types::errors::TypeSystemError;

impl super::super::TypeChecker {
    pub(in super::super) fn lookup_field_type(&self, class_name: &str, field: &str) -> Result<Type, TypeSystemError> {
        let registry = self.registry.read()
            .map_err(|_| TypeSystemError::internal("type registry lock poisoned"))?;

        let mut current = Some(class_name.to_string());
        let mut field_type_str = None;
        while let Some(name) = current {
            match registry.get_type(&name) {
                Some(TypeDef::Class { fields, parent, .. }) => {
                    if let Some(field_def) = fields.iter().find(|f| f.name == field) {
                        field_type_str = Some(field_def.ty.clone());
                        break;
                    }
                    current = parent.clone();
                }
                Some(_) => {
                    return Err(TypeSystemError::FieldNotFound {
                        type_name: name,
                        field_name: field.to_string(),
                        span: TypeSystemError::span_for_name(field),
                        similar: Vec::new(),
                    });
                }
                None => {
                    return Err(TypeSystemError::undefined_type(
                        name.clone(),
                        TypeSystemError::span_for_name(&name),
                    ));
                }
            }
        }

        let field_type_str = match field_type_str {
            Some(s) => s,
            None => {
                return Err(TypeSystemError::FieldNotFound {
                    type_name: class_name.to_string(),
                    field_name: field.to_string(),
                    span: TypeSystemError::span_for_name(field),
                    similar: Vec::new(),
                });
            }
        };

        registry.resolve_type(&field_type_str)
    }

    pub(in super::super) fn lookup_method(&self, class_name: &str, method_name: &str) -> Option<MethodSignature> {
        let registry = self.registry.read().ok()?;
        let mut current = Some(class_name.to_string());
        while let Some(name) = current {
            match registry.get_type(&name) {
                Some(TypeDef::Class { methods, parent, .. }) => {
                    if let Some(mut method) = methods.into_iter().find(|m| m.name == method_name) {
                        if name != class_name {
                            if let Some(Type::NamedType { name: recv }) = method.params.first_mut() {
                                if recv == &name {
                                    *recv = class_name.to_string();
                                }
                            }
                        }
                        return Some(method);
                    }
                    current = parent;
                }
                _ => return None,
            }
        }
        None
    }

    pub(in super::super) fn enum_has_variant(&self, enum_name: &str, variant: &str) -> bool {
        let Ok(reg) = self.registry.read() else {
            return false;
        };
        match reg.get_type(enum_name) {
            Some(TypeDef::Enum { variants, .. }) => variants.iter().any(|v| v.name == variant),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::TypeChecker;
    use crate::parser::expr::Expression;
    use crate::types::definition::{EnumVariant, Type, TypeDef};
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    #[test]
    fn field_lookup_on_enum_uses_named_underscore_dummy_rhs() {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Color",
                TypeDef::Enum {
                    name: "Color".to_string(),
                    variants: vec![EnumVariant {
                        name: "Red".to_string(),
                        fields: vec![],
                    }],
                    generics: vec![],
                },
            )
            .unwrap();
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(registry)));
        let expr = Expression::Member {
            object: Box::new(Expression::Member {
                object: Box::new(Expression::Variable("Color".to_string())),
                field: "Red".to_string(),
                args: vec![],
            }),
            field: "nope".to_string(),
            args: vec![],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        fn is_dummy(ty: &Type) -> bool {
            matches!(ty, Type::NamedType { name } if name == "_" || name == "()")
                || matches!(ty, Type::Unit | Type::Void)
        }
        match err {
            TypeSystemError::FieldNotFound {
                type_name,
                field_name,
                ..
            } => {
                assert_eq!(type_name, "Color");
                assert_eq!(field_name, "nope");
            }
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                assert_eq!(op, "member access");
                assert_eq!(left, right, "operands must be the object type, not a dummy");
                assert!(!is_dummy(&left), "must not invent `_`/`()` dummy types");
                assert!(!is_dummy(&right), "must not invent `_`/`()` dummy types");
            }
            other => panic!("expected FieldNotFound or InvalidOperation with left==right, got {other:?}"),
        }
    }
}
