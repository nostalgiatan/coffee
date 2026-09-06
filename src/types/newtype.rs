//! Nominal `type Name: object` brands. Not aliases (`TypeRegistry.aliases`).

use super::definition::Type;
use super::errors::TypeSystemError;
use super::registry::TypeRegistry;

impl TypeRegistry {
    /// Register `type name: source`. This round `source` must be `object`.
    pub fn register_newtype(&self, name: String, source: Type) -> Result<(), TypeSystemError> {
        if !matches!(source, Type::Variadic) {
            return Err(TypeSystemError::ConstraintViolation {
                constraint: "newtype source".to_string(),
                reason: format!("type '{name}' source must be object"),
                span: TypeSystemError::span_for_name(&name),
            });
        }
        if self.get_alias(&name).is_some()
            || self.get_type(&name).is_some()
            || self.is_newtype(&name)
        {
            let span = TypeSystemError::span_for_name(&name);
            return Err(TypeSystemError::Duplicate {
                name,
                existing: span,
                new: span,
            });
        }
        let mut map = self.newtypes.write().map_err(|_| TypeSystemError::Internal {
            reason: "newtype registry lock poisoned".to_string(),
        })?;
        map.insert(name, source);
        Ok(())
    }

    pub fn newtype_source(&self, name: &str) -> Option<Type> {
        self.newtypes
            .read()
            .ok()
            .and_then(|m| m.get(name).cloned())
    }

    pub fn is_newtype(&self, name: &str) -> bool {
        self.newtypes
            .read()
            .ok()
            .map(|m| m.contains_key(name))
            .unwrap_or(false)
    }

    pub fn type_is_newtype(&self, ty: &Type) -> bool {
        match ty {
            Type::NamedType { name } => self.is_newtype(name),
            _ => false,
        }
    }

    pub fn register_c_layout(&self, name: String, layout: super::registry::CLayout) {
        if let Ok(mut m) = self.c_layouts.write() {
            m.insert(name, layout);
        }
    }

    pub fn c_layout(&self, name: &str) -> Option<super::registry::CLayout> {
        self.c_layouts.read().ok().and_then(|m| m.get(name).copied())
    }

    pub fn is_c_value_type(&self, name: &str) -> bool {
        self.c_layout(name).is_some()
    }

    pub fn is_c_union(&self, name: &str) -> bool {
        matches!(
            self.c_layout(name).map(|l| l.kind),
            Some(super::registry::CLayoutKind::Union)
        )
    }

    pub fn is_c_enum(&self, name: &str) -> bool {
        matches!(
            self.c_layout(name).map(|l| l.kind),
            Some(super::registry::CLayoutKind::Enum)
        )
    }

    /// Named C structs/unions/enums are memcpy values, not Coffee resources.
    pub fn type_is_c_value(&self, ty: &Type) -> bool {
        match ty {
            Type::NamedType { name } => self.is_c_value_type(name),
            Type::Array { elem, .. } => self.type_is_c_value(elem),
            Type::Tuple(elems) => elems.iter().all(|e| self.type_is_c_value(e) || !e.is_resource()),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser;
    use crate::types::checker::TypeChecker;
    use crate::types::definition::{Type, TypeDef};
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    fn file_hwnd_registry() -> TypeRegistry {
        let reg = TypeRegistry::root();
        reg.register_newtype("FILE".into(), Type::Variadic).unwrap();
        reg.register_newtype("HWND".into(), Type::Variadic).unwrap();
        reg
    }

    #[test]
    fn register_newtype_resolves_to_named_not_object() {
        let reg = TypeRegistry::root();
        reg.register_newtype("FILE".into(), Type::Variadic).unwrap();
        assert!(reg.is_newtype("FILE"));
        assert_eq!(reg.newtype_source("FILE"), Some(Type::Variadic));
        assert_eq!(
            reg.resolve_type("FILE").unwrap(),
            Type::NamedType {
                name: "FILE".into()
            }
        );
        assert!(reg.get_alias("FILE").is_none());
        assert!(!reg.is_newtype("HWND"));
        assert_eq!(reg.newtype_source("HWND"), None);
    }

    #[test]
    fn register_newtype_rejects_non_object_source() {
        let reg = TypeRegistry::root();
        let err = reg
            .register_newtype("FILE".into(), Type::int())
            .expect_err("source must be object this round");
        assert!(
            matches!(
                err,
                TypeSystemError::ConstraintViolation { .. } | TypeSystemError::ParseError { .. }
            ),
            "unexpected error: {err}"
        );
        assert!(!reg.is_newtype("FILE"));
    }

    #[test]
    fn register_newtype_rejects_taken_alias_class_or_newtype() {
        let reg = TypeRegistry::root();
        let err = reg
            .register_newtype("int".into(), Type::Variadic)
            .expect_err("int is an alias");
        assert!(matches!(err, TypeSystemError::Duplicate { .. }), "{err}");

        reg.define_type(
            "Point",
            TypeDef::Class {
                name: "Point".into(),
                fields: vec![],
                methods: vec![],
                generics: vec![],
                parent: None,
            },
        )
        .unwrap();
        let err = reg
            .register_newtype("Point".into(), Type::Variadic)
            .expect_err("Point is a class");
        assert!(matches!(err, TypeSystemError::Duplicate { .. }), "{err}");

        reg.register_newtype("FILE".into(), Type::Variadic).unwrap();
        let err = reg
            .register_newtype("FILE".into(), Type::Variadic)
            .expect_err("FILE already a newtype");
        assert!(matches!(err, TypeSystemError::Duplicate { .. }), "{err}");
    }

    #[test]
    fn clone_shares_newtype_map() {
        let reg = TypeRegistry::root();
        reg.register_newtype("FILE".into(), Type::Variadic).unwrap();
        let cloned = reg.clone();
        assert!(cloned.is_newtype("FILE"));
        cloned
            .register_newtype("HWND".into(), Type::Variadic)
            .unwrap();
        assert!(reg.is_newtype("HWND"));
    }

    fn checker_with_file_hwnd() -> TypeChecker {
        TypeChecker::comprehensive(Arc::new(RwLock::new(file_hwnd_registry())))
    }

    fn bind_object_then_file(checker: &mut TypeChecker) {
        checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "p".into(),
                var_type: "object".into(),
                value: parser::expr::Expression::Literal("0".into()),
            })
            .unwrap();
        checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "f".into(),
                var_type: "FILE".into(),
                value: parser::expr::Expression::TypeCast {
                    target_type: "FILE".into(),
                    value: Box::new(parser::expr::Expression::Variable("p".into())),
                },
            })
            .unwrap();
    }

    fn takes_obj_fn() -> parser::function::Function {
        parser::function::Function {
            type_params: vec![],
            name: "takes_obj".into(),
            parameters: vec![
                parser::function::Parameter {
                    name: "x".into(),
                    param_type: "object".into(),
                    is_variadic: false,
                },
                parser::function::Parameter {
                    name: "n".into(),
                    param_type: "int".into(),
                    is_variadic: false,
                },
            ],
            return_type: "int".into(),
            error_handler: None,
            is_c: false,
            body: parser::function::FunctionBody::Block(vec![parser::Statement::Return(
                parser::var::ReturnStmt {
                    value: Some(parser::expr::Expression::Literal("0".into())),
                },
            )]),
        }
    }

    #[test]
    fn cast_allowed_one_hop_file_object_not_hwnd() {
        let checker = checker_with_file_hwnd();
        let file_ty = Type::NamedType {
            name: "FILE".into(),
        };
        let hwnd_ty = Type::NamedType {
            name: "HWND".into(),
        };
        assert!(checker.cast_allowed_for_test(&file_ty, &Type::Variadic));
        assert!(checker.cast_allowed_for_test(&Type::Variadic, &file_ty));
        assert!(!checker.cast_allowed_for_test(&file_ty, &hwnd_ty));
        assert!(!checker.cast_allowed_for_test(&file_ty, &Type::int()));
        assert!(!checker.cast_allowed_for_test(&file_ty, &Type::buf()));
        assert!(checker.cast_allowed_for_test(&file_ty, &file_ty));
    }

    #[test]
    fn implicit_assign_does_not_mix_newtype_and_object() {
        let mut checker = checker_with_file_hwnd();
        bind_object_then_file(&mut checker);
        let err = checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "x".into(),
                var_type: "object".into(),
                value: parser::expr::Expression::Variable("f".into()),
            })
            .expect_err("FILE is not object");
        assert!(
            matches!(err, TypeSystemError::TypeMismatch { .. }),
            "expected TypeMismatch, got {err}"
        );

        let err = checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "g".into(),
                var_type: "FILE".into(),
                value: parser::expr::Expression::Variable("p".into()),
            })
            .expect_err("object is not FILE");
        assert!(
            matches!(err, TypeSystemError::TypeMismatch { .. }),
            "expected TypeMismatch, got {err}"
        );
    }

    #[test]
    fn implicit_arg_does_not_mix_newtype_and_object() {
        let mut checker = checker_with_file_hwnd();
        bind_object_then_file(&mut checker);
        let f = takes_obj_fn();
        checker.declare_function_sig(&f).unwrap();
        checker
            .check_statement(&parser::Statement::Function(f))
            .unwrap();
        let err = checker
            .check_expression(&parser::expr::Expression::Call {
                function: Box::new(parser::expr::Expression::Variable("takes_obj".into())),
                args: vec![
                    parser::expr::Expression::Variable("f".into()),
                    parser::expr::Expression::Literal("0".into()),
                ],
            })
            .expect_err("FILE arg is not object");
        assert!(
            matches!(err, TypeSystemError::TypeMismatch { .. }),
            "expected TypeMismatch, got {err}"
        );
    }

    #[test]
    fn type_cast_file_to_object_and_sibling_hwnd() {
        let mut checker = checker_with_file_hwnd();
        bind_object_then_file(&mut checker);
        let ty = checker
            .check_expression(&parser::expr::Expression::TypeCast {
                target_type: "object".into(),
                value: Box::new(parser::expr::Expression::Variable("f".into())),
            })
            .unwrap();
        assert_eq!(ty, Type::Variadic);

        let err = checker
            .check_expression(&parser::expr::Expression::TypeCast {
                target_type: "HWND".into(),
                value: Box::new(parser::expr::Expression::Variable("f".into())),
            })
            .expect_err("FILE as HWND is two hops");
        assert!(
            matches!(err, TypeSystemError::TypeMismatch { .. }),
            "expected TypeMismatch, got {err}"
        );
    }

    #[test]
    fn class_named_type_still_accepted_as_object_arg() {
        let reg = TypeRegistry::root();
        reg.define_type(
            "Point",
            TypeDef::Class {
                name: "Point".into(),
                fields: vec![],
                methods: vec![],
                generics: vec![],
                parent: None,
            },
        )
        .unwrap();
        let mut checker = TypeChecker::comprehensive(Arc::new(RwLock::new(reg)));
        checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "pt".into(),
                var_type: "Point".into(),
                value: parser::expr::Expression::ConstructorCall {
                    class_name: "Point".into(),
                    args: vec![],
                },
            })
            .unwrap();
        let f = takes_obj_fn();
        checker.declare_function_sig(&f).unwrap();
        checker
            .check_statement(&parser::Statement::Function(f))
            .unwrap();
        checker
            .check_expression(&parser::expr::Expression::Call {
                function: Box::new(parser::expr::Expression::Variable("takes_obj".into())),
                args: vec![
                    parser::expr::Expression::Variable("pt".into()),
                    parser::expr::Expression::Literal("0".into()),
                ],
            })
            .expect("class NamedType still coerces to object");
    }
}
