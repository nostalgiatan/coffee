use super::*;
use crate::parser;
use crate::types::definition::{EnumVariant, Type, TypeDef, VariantField};
use crate::types::errors::TypeSystemError;
use crate::types::registry::TypeRegistry;
use std::sync::{Arc, RwLock};

    #[test]
    fn test_simple_type_checker() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("123".to_string())).unwrap(),
            Type::int()
        );
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("3.14".to_string())).unwrap(),
            Type::float()
        );
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("true".to_string())).unwrap(),
            Type::bool()
        );
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("\"hello\"".to_string())).unwrap(),
            Type::string()
        );
    }

    #[test]
    fn test_comprehensive_type_checker() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);

        // Test ownership tracking
        let decl = parser::var::VariableDecl {
            name: "x".to_string(),
            var_type: "int".to_string(),
            value: parser::expr::Expression::Literal("42".to_string()),
        };

        checker.check_variable_decl(&decl).unwrap();
        assert!(checker.values.contains_key("x"));

        // Test memory operation
        checker.check_memory_operation("x", "move").unwrap();
        assert_eq!(checker.values.get("x").unwrap().state, ValueState::Moved);
    }

    #[test]
    fn test_check_expression_types_simple_literals_without_c_call_parse() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("123".to_string())).unwrap(),
            Type::int()
        );
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("3.14".to_string())).unwrap(),
            Type::float()
        );
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("true".to_string())).unwrap(),
            Type::bool()
        );
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("\"hello\"".to_string())).unwrap(),
            Type::string()
        );
        assert_eq!(
            checker.check_expression(&parser::expr::Expression::Literal("\"printf()\"".to_string())).unwrap(),
            Type::string()
        );
    }

    #[test]
    fn test_unary_not_and_neg() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let not_true = parser::expr::Expression::Unary {
            op: "!".to_string(),
            operand: Box::new(parser::expr::Expression::Literal("true".to_string())),
        };
        assert_eq!(checker.check_expression(&not_true).unwrap(), Type::bool());

        let neg = parser::expr::Expression::Unary {
            op: "-".to_string(),
            operand: Box::new(parser::expr::Expression::Literal("10".to_string())),
        };
        assert_eq!(checker.check_expression(&neg).unwrap(), Type::int());

        let bad = parser::expr::Expression::Unary {
            op: "!".to_string(),
            operand: Box::new(parser::expr::Expression::Literal("1".to_string())),
        };
        assert!(checker.check_expression(&bad).is_err());
    }

    #[test]
    fn bind_function_locals_fills_let_names_after_check_function_clears_them() {
        let func = parser::function::Function {
            type_params: vec![],
            name: "f".to_string(),
            parameters: vec![],
            return_type: "int".to_string(),
            error_handler: None,
            is_c: false,
            body: parser::function::FunctionBody::Block(vec![
                parser::Statement::VariableDecl(parser::var::VariableDecl {
                    name: "n".to_string(),
                    var_type: "int".to_string(),
                    value: parser::expr::Expression::Literal("1".to_string()),
                }),
                parser::Statement::Return(parser::var::ReturnStmt {
                    value: Some(parser::expr::Expression::Variable("n".to_string())),
                }),
            ]),
        };
        let stmt = parser::Statement::Function(func.clone());
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        checker.check_statement(&stmt).expect("typecheck");
        assert!(
            !checker.values.contains_key("n"),
            "check_function should restore values, leaving locals unbound"
        );
        checker.bind_function_locals(&func).expect("bind");
        assert!(
            checker.values.contains_key("n"),
            "bind_function_locals should insert function let names into values"
        );
    }

    fn mixed_int_and_wildcard() -> Vec<parser::r#match::MatchArm> {
        vec![
            parser::r#match::MatchArm {
                pattern: parser::pattern::Pattern::Literal("1".into()),
                guard: None,
                body: vec![],
            },
            parser::r#match::MatchArm {
                pattern: parser::pattern::Pattern::Wildcard,
                guard: None,
                body: vec![],
            },
        ]
    }

    fn catchall_only() -> Vec<parser::r#match::MatchArm> {
        vec![parser::r#match::MatchArm {
            pattern: parser::pattern::Pattern::Wildcard,
            guard: None,
            body: vec![],
        }]
    }

    #[test]
    fn test_match_cfg_rejects_function_void_variadic_mixed() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let mixed = mixed_int_and_wildcard();
        let fn_ty = Type::Function {
            params: vec![],
            return_type: Box::new(Type::int()),
        };
        assert!(checker.check_match_exhaustiveness(&fn_ty, &mixed).is_err());
        assert!(checker
            .check_match_exhaustiveness(&Type::void(), &mixed)
            .is_err());
        assert!(checker
            .check_match_exhaustiveness(&Type::Variadic, &mixed)
            .is_err());
    }

    #[test]
    fn test_match_cfg_allows_function_void_variadic_catchall() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let arms = catchall_only();
        let fn_ty = Type::Function {
            params: vec![],
            return_type: Box::new(Type::int()),
        };
        assert!(checker.check_match_exhaustiveness(&fn_ty, &arms).is_ok());
        assert!(checker
            .check_match_exhaustiveness(&Type::void(), &arms)
            .is_ok());
        assert!(checker
            .check_match_exhaustiveness(&Type::Variadic, &arms)
            .is_ok());
    }

    fn int_fn(
        name: &str,
        handler: Option<&str>,
        params: Vec<parser::function::Parameter>,
    ) -> parser::function::Function {
        parser::function::Function {
            type_params: vec![],
            name: name.to_string(),
            parameters: params,
            return_type: "int".to_string(),
            error_handler: handler.map(|s| s.to_string()),
            is_c: false,
            body: parser::function::FunctionBody::Block(vec![parser::Statement::Return(
                parser::var::ReturnStmt {
                    value: Some(parser::expr::Expression::Literal("0".to_string())),
                },
            )]),
        }
    }

    fn err_param() -> parser::function::Parameter {
        parser::function::Parameter {
            name: "err".to_string(),
            param_type: "Error".to_string(),
            is_variadic: false,
        }
    }

    #[test]
    fn test_error_listener_ok() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let on_err = int_fn("on_err", None, vec![err_param()]);
        let f = int_fn("f", Some("on_err"), vec![]);
        checker.declare_function_sig(&on_err).unwrap();
        checker.declare_function_sig(&f).unwrap();
        checker.check_statement(&parser::Statement::Function(on_err)).unwrap();
        checker.check_statement(&parser::Statement::Function(f)).unwrap();
        assert!(checker.errors().is_empty());
    }

    #[test]
    fn test_error_listener_missing_handler() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let f = int_fn("f", Some("on_err"), vec![]);
        checker.declare_function_sig(&f).unwrap();
        assert!(checker
            .check_statement(&parser::Statement::Function(f))
            .is_err());
        assert!(checker.errors().iter().any(|e| matches!(
            e,
            TypeSystemError::UndefinedFunction { name, .. } if name == "on_err"
        )));
    }

    #[test]
    fn test_error_listener_same_name() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let f = int_fn("on_err", Some("on_err"), vec![err_param()]);
        checker.declare_function_sig(&f).unwrap();
        assert!(checker
            .check_statement(&parser::Statement::Function(f))
            .is_err());
        assert!(checker.errors().iter().any(|e| matches!(
            e,
            TypeSystemError::ConstraintViolation { constraint, .. } if constraint == "error listener"
        )));
    }

    #[test]
    fn test_error_listener_wrong_arity() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let on_err = int_fn("on_err", None, vec![]);
        let f = int_fn("f", Some("on_err"), vec![]);
        checker.declare_function_sig(&on_err).unwrap();
        checker.declare_function_sig(&f).unwrap();
        assert!(checker
            .check_statement(&parser::Statement::Function(f))
            .is_err());
        assert!(checker
            .errors()
            .iter()
            .any(|e| matches!(e, TypeSystemError::ArityMismatch { expected: 1, found: 0, .. })));
    }

    #[test]
    fn test_error_listener_wrong_param_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let on_err = int_fn(
            "on_err",
            None,
            vec![parser::function::Parameter {
                name: "err".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
        );
        let f = int_fn("f", Some("on_err"), vec![]);
        checker.declare_function_sig(&on_err).unwrap();
        checker.declare_function_sig(&f).unwrap();
        assert!(checker
            .check_statement(&parser::Statement::Function(f))
            .is_err());
        assert!(checker
            .errors()
            .iter()
            .any(|e| matches!(e, TypeSystemError::TypeMismatch { .. })));
    }

    #[test]
    fn test_error_listener_wrong_return_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let on_err = parser::function::Function {
            type_params: vec![],
            name: "on_err".to_string(),
            parameters: vec![err_param()],
            return_type: "bool".to_string(),
            error_handler: None,
            is_c: false,
            body: parser::function::FunctionBody::Block(vec![parser::Statement::Return(
                parser::var::ReturnStmt {
                    value: Some(parser::expr::Expression::Literal("true".to_string())),
                },
            )]),
        };
        let f = int_fn("f", Some("on_err"), vec![]);
        checker.declare_function_sig(&on_err).unwrap();
        checker.declare_function_sig(&f).unwrap();
        assert!(checker
            .check_statement(&parser::Statement::Function(f))
            .is_err());
        assert!(checker
            .errors()
            .iter()
            .any(|e| matches!(e, TypeSystemError::TypeMismatch { .. })));
    }

    #[test]
    fn test_check_class_rejects_user_error() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let class = parser::class::ClassDef {
            type_params: vec![],
            name: "Error".to_string(),
            parent: None,
            fields: vec![],
            methods: vec![],
            packed: false,
            has_constructor: false,
        };
        assert!(checker.check_class(&class).is_err());
        assert!(!checker.errors().is_empty());
    }

    fn opt_enum(payload: &str) -> TypeDef {
        TypeDef::Enum {
            name: "Opt".to_string(),
            variants: vec![
                EnumVariant {
                    name: "None".to_string(),
                    fields: vec![],
                },
                EnumVariant {
                    name: "Some".to_string(),
                    fields: vec![VariantField::Positional(payload.to_string())],
                },
            ],
            generics: vec![],
        }
    }

    fn opt_some_ident(name: &str) -> parser::pattern::Pattern {
        parser::pattern::Pattern::EnumVariant {
            enum_name: "Opt".to_string(),
            variant: "Some".to_string(),
            args: vec![parser::pattern::Pattern::Ident(name.to_string())],
        }
    }

    #[test]
    fn variant_payload_unresolved_type_is_error_not_int() {
        let inner = TypeRegistry::root();
        inner.define_type("Opt", opt_enum("NoSuchType")).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::simple(Arc::clone(&registry));
        let expected = Type::NamedType {
            name: "Opt".to_string(),
        };
        let err = checker
            .bind_pattern_vars(&opt_some_ident("x"), &expected)
            .expect_err("unresolved payload must not succeed as int");
        assert!(
            matches!(err, TypeSystemError::UndefinedType { ref name, .. } if name == "NoSuchType"),
            "expected UndefinedType NoSuchType, got {err:?}"
        );
        assert!(
            checker.errors().iter().any(|e| matches!(
                e,
                TypeSystemError::UndefinedType { name, .. } if name == "NoSuchType"
            )),
            "error must be recorded via add_error, got {:?}",
            checker.errors()
        );
        assert!(
            !checker.values.contains_key("x"),
            "must not bind payload as invented int"
        );
    }

    #[test]
    fn variant_payload_missing_slot_is_error_not_int() {
        let inner = TypeRegistry::root();
        inner.define_type("Opt", opt_enum("int")).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::simple(Arc::clone(&registry));
        let expected = Type::NamedType {
            name: "Opt".to_string(),
        };
        let pattern = parser::pattern::Pattern::EnumVariant {
            enum_name: "Opt".to_string(),
            variant: "None".to_string(),
            args: vec![parser::pattern::Pattern::Ident("x".to_string())],
        };
        assert!(
            checker.bind_pattern_vars(&pattern, &expected).is_err(),
            "missing payload slot must not invent int"
        );
        assert!(!checker.errors().is_empty());
        assert!(
            !checker.values.contains_key("x"),
            "must not bind missing payload as invented int"
        );
    }

    #[test]
    fn variant_payload_int_resolves_to_int() {
        let inner = TypeRegistry::root();
        inner.define_type("Opt", opt_enum("int")).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::simple(Arc::clone(&registry));
        let expected = Type::NamedType {
            name: "Opt".to_string(),
        };
        checker
            .bind_pattern_vars(&opt_some_ident("x"), &expected)
            .expect("int payload should resolve");
        assert_eq!(checker.values.get("x").map(|v| &v.ty), Some(&Type::int()));
        assert!(checker.errors().is_empty());
    }

    fn assert_unresolved_not_unit(err: &TypeSystemError, type_name: &str) {
        match err {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                panic!(
                    "must not type-check against unit (or any invented type); got mismatch {expected:?} vs {found:?}"
                );
            }
            TypeSystemError::UndefinedType { name, .. } => {
                assert_eq!(name, type_name);
            }
            TypeSystemError::ParseError { type_str, reason } => {
                assert!(
                    type_str.contains(type_name)
                        || reason.contains(type_name)
                        || reason.to_lowercase().contains("resolve"),
                    "ParseError should mention {type_name}, got type_str={type_str:?} reason={reason:?}"
                );
            }
            other => panic!("expected UndefinedType or ParseError for {type_name}, got {other:?}"),
        }
    }

    #[test]
    fn enum_variant_call_unresolvable_payload_is_err_not_unit() {
        let inner = TypeRegistry::root();
        inner.define_type("Opt", opt_enum("NoSuchType")).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::simple(Arc::clone(&registry));
        let arg = parser::expr::Expression::Literal("1".to_string());
        let err = checker
            .check_function_call("Opt::Some", &[arg])
            .expect_err("unresolvable variant payload must Err, not check against unit");
        assert_unresolved_not_unit(&err, "NoSuchType");
    }

    #[test]
    fn enum_variant_call_unresolvable_named_payload_is_err_not_unit() {
        let inner = TypeRegistry::root();
        inner
            .define_type(
                "Box",
                TypeDef::Enum {
                    name: "Box".to_string(),
                    variants: vec![EnumVariant {
                        name: "Val".to_string(),
                        fields: vec![VariantField::Named {
                            name: "inner".to_string(),
                            ty: "NoSuchType".to_string(),
                        }],
                    }],
                    generics: vec![],
                },
            )
            .unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::simple(Arc::clone(&registry));
        let arg = parser::expr::Expression::Literal("1".to_string());
        let err = checker
            .check_function_call("Box::Val", &[arg])
            .expect_err("unresolvable named variant payload must Err");
        assert_unresolved_not_unit(&err, "NoSuchType");
    }

    fn checker_with_c_fn(name: &str, param_types: &[&str], return_type: &str) -> TypeChecker {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let analyzer = crate::semantic::SemanticAnalyzer::new(Arc::clone(&registry));
        let mut table = crate::c::CSymbolTable::new("testlib".to_string());
        table.add(crate::c::CSymbol::new(
            name.to_string(),
            param_types
                .iter()
                .enumerate()
                .map(|(i, t)| parser::function::Parameter {
                    name: format!("p{i}"),
                    param_type: (*t).to_string(),
                    is_variadic: false,
                })
                .collect(),
            return_type.to_string(),
            false,
        ));
        let mut symbols = std::collections::HashMap::new();
        symbols.insert("testlib".to_string(), table);
        analyzer.set_cfc_symbols(symbols);
        TypeChecker::with_analyzer(
            registry,
            CheckingMode::Comprehensive,
            Some(Arc::new(RwLock::new(analyzer))),
        )
    }

    #[test]
    fn c_call_unresolvable_return_type_is_err_not_unit() {
        let mut checker = checker_with_c_fn("c_bad_ret", &[], "NoSuchType");
        let err = checker
            .check_function_call("c_bad_ret", &[])
            .expect_err("unresolvable C return type must Err, not Type::unit()");
        assert_unresolved_not_unit(&err, "NoSuchType");
        assert!(
            checker.type_of_global_name("c_bad_ret").is_none(),
            "type_of_global_name must not invent a Function returning unit"
        );
    }

    #[test]
    fn c_call_unresolvable_param_type_is_err_not_skipped() {
        let mut checker = checker_with_c_fn("c_bad_param", &["NoSuchType"], "int");
        let err = checker
            .check_function_call("c_bad_param", &[])
            .expect_err(
                "unresolvable C param must Err, not skip the param and accept zero args",
            );
        assert_unresolved_not_unit(&err, "NoSuchType");
        match checker.type_of_global_name("c_bad_param") {
            None => {}
            Some(Type::Function { params, .. }) => {
                panic!(
                    "must not silently drop unresolvable C params (got {} params)",
                    params.len()
                );
            }
            other => panic!("unexpected type_of_global_name: {other:?}"),
        }
    }

    #[test]
    fn set_function_return_type_poisoned_registry_is_err() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let registry_for_panic = Arc::clone(&registry);
        let join = std::thread::spawn(move || {
            let _guard = registry_for_panic.write().unwrap();
            panic!("poison registry");
        })
        .join();
        assert!(join.is_err());
        let mut checker = TypeChecker::simple(Arc::clone(&registry));
        let err = checker
            .set_function_return_type("int")
            .expect_err("poisoned registry lock must return TypeSystemError");
        match err {
            TypeSystemError::Internal { reason } => {
                assert!(
                    reason.contains("poison") || reason.contains("registry"),
                    "unexpected internal reason: {reason}"
                );
            }
            TypeSystemError::ParseError { reason, .. } => {
                assert!(
                    reason.contains("registry") || reason.contains("access"),
                    "unexpected parse reason: {reason}"
                );
            }
            other => panic!("expected Internal or ParseError, got {other:?}"),
        }
    }

    #[test]
    fn check_function_call_poisoned_registry_is_err() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let registry_for_panic = Arc::clone(&registry);
        let join = std::thread::spawn(move || {
            let _guard = registry_for_panic.write().unwrap();
            panic!("poison registry");
        })
        .join();
        assert!(join.is_err());
        let mut checker = TypeChecker::simple(Arc::clone(&registry));
        let err = checker
            .check_function_call("printf", &[])
            .expect_err("poisoned registry lock must return TypeSystemError, not panic");
        match err {
            TypeSystemError::Internal { reason } => {
                assert!(
                    reason.contains("poison") || reason.contains("registry"),
                    "unexpected internal reason: {reason}"
                );
            }
            TypeSystemError::ParseError { reason, .. } => {
                assert!(
                    reason.contains("registry") || reason.contains("access"),
                    "unexpected parse reason: {reason}"
                );
            }
            other => panic!("expected Internal or ParseError, got {other:?}"),
        }
    }

    #[test]
    fn resolve_param_type_accepts_slice() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let ty = checker
            .resolve_param_type("[int]")
            .expect("Coffee fn slice parameters are allowed");
        assert!(
            matches!(ty, Type::Slice(_)),
            "expected [int] to be a slice, got {ty:?}"
        );
    }

    #[test]
    fn resolve_param_type_rejects_c_slice() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let err = checker
            .resolve_param_type_for("[int]", true)
            .expect_err("c fn slice parameters must be rejected");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("slice"),
            "error should mention slices cannot be C parameters, got: {msg}"
        );
    }

    fn assert_span_covers_name(err: &TypeSystemError, name: &str) {
        assert_eq!(
            err.span(),
            Some(TypeSystemError::span_for_name(name)),
            "expected span covering {name:?}, got {:?} for {err:?}",
            err.span()
        );
    }

    fn point_class() -> TypeDef {
        TypeDef::Class {
            name: "Point".to_string(),
            fields: vec![crate::types::definition::ClassField {
                name: "coord".to_string(),
                ty: "int".to_string(),
                visibility: crate::types::definition::Visibility::Public,
            }],
            methods: vec![],
            generics: vec![],
            parent: None,
        }
    }

    #[test]
    fn not_callable_span_covers_type_name() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let expr = parser::expr::Expression::Call {
            function: Box::new(parser::expr::Expression::Literal("1".to_string())),
            args: vec![],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "int");
    }

    #[test]
    fn index_non_int_span_covers_index_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let expr = parser::expr::Expression::Index {
            array: Box::new(parser::expr::Expression::ArrayLiteral {
                elements: vec![parser::expr::Expression::Literal("1".to_string())],
            }),
            index: Box::new(parser::expr::Expression::Literal("true".to_string())),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "bool");
    }

    #[test]
    fn array_literal_mismatch_span_covers_found_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let expr = parser::expr::Expression::ArrayLiteral {
            elements: vec![
                parser::expr::Expression::Literal("1".to_string()),
                parser::expr::Expression::Literal("true".to_string()),
            ],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "bool");
    }

    #[test]
    fn member_on_int_span_covers_field_operator() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let expr = parser::expr::Expression::Member {
            object: Box::new(parser::expr::Expression::Literal("1".to_string())),
            field: "x".to_string(),
            args: vec![],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, ".x");
    }

    #[test]
    fn type_cast_mismatch_span_covers_target_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let expr = parser::expr::Expression::TypeCast {
            target_type: "str".to_string(),
            value: Box::new(parser::expr::Expression::Literal("1".to_string())),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "str");
    }

    #[test]
    fn assign_arity_span_covers_assign() {
        let inner = TypeRegistry::root();
        inner.define_type("Point", point_class()).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::comprehensive(registry);
        checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "p".to_string(),
                var_type: "Point".to_string(),
                value: parser::expr::Expression::ConstructorCall {
                    class_name: "Point".to_string(),
                    args: vec![],
                },
            })
            .unwrap();
        let expr = parser::expr::Expression::Member {
            object: Box::new(parser::expr::Expression::Variable("p".to_string())),
            field: "assign".to_string(),
            args: vec![],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "assign");
    }

    #[test]
    fn field_assign_mismatch_span_covers_field_name() {
        let inner = TypeRegistry::root();
        inner.define_type("Point", point_class()).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::comprehensive(registry);
        checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "p".to_string(),
                var_type: "Point".to_string(),
                value: parser::expr::Expression::ConstructorCall {
                    class_name: "Point".to_string(),
                    args: vec![],
                },
            })
            .unwrap();
        let expr = parser::expr::Expression::Assign {
            object: Box::new(parser::expr::Expression::Variable("p".to_string())),
            field_name: "coord".to_string(),
            value: Box::new(parser::expr::Expression::Literal("true".to_string())),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "coord");
    }

    #[test]
    fn enum_variant_arity_span_covers_variant_name() {
        let inner = TypeRegistry::root();
        inner.define_type("Opt", opt_enum("int")).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::comprehensive(registry);
        let expr = parser::expr::Expression::Member {
            object: Box::new(parser::expr::Expression::Variable("Opt".to_string())),
            field: "Some".to_string(),
            args: vec![],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "Some");
    }

    #[test]
    fn constructor_extra_args_span_covers_class_name() {
        let inner = TypeRegistry::root();
        inner.define_type("Point", point_class()).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::comprehensive(registry);
        let expr = parser::expr::Expression::ConstructorCall {
            class_name: "Point".to_string(),
            args: vec![parser::expr::Expression::Literal("1".to_string())],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "Point");
    }

    #[test]
    fn method_not_found_span_covers_method_name() {
        let inner = TypeRegistry::root();
        inner.define_type("Point", point_class()).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::comprehensive(registry);
        checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "p".to_string(),
                var_type: "Point".to_string(),
                value: parser::expr::Expression::ConstructorCall {
                    class_name: "Point".to_string(),
                    args: vec![],
                },
            })
            .unwrap();
        let expr = parser::expr::Expression::Member {
            object: Box::new(parser::expr::Expression::Variable("p".to_string())),
            field: "missing".to_string(),
            args: vec![parser::expr::Expression::Literal("1".to_string())],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "missing");
    }

    #[test]
    fn function_arity_span_covers_function_name() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let f = int_fn("id", None, vec![]);
        checker.declare_function_sig(&f).unwrap();
        let expr = parser::expr::Expression::Call {
            function: Box::new(parser::expr::Expression::Variable("id".to_string())),
            args: vec![parser::expr::Expression::Literal("1".to_string())],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "id");
    }

    #[test]
    fn if_non_bool_span_covers_bool() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let err = checker
            .check_statement(&parser::Statement::If(parser::IfExpr {
                condition: parser::expr::Expression::Literal("1".to_string()),
                body: vec![],
                elifs: vec![],
                else_body: None,
            }))
            .unwrap_err();
        assert_span_covers_name(&err, "bool");
    }

    #[test]
    fn for_range_non_int_span_covers_int() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let err = checker
            .check_statement(&parser::Statement::For(parser::ForLoop {
                variable: "i".to_string(),
                iterator: parser::ForIterator::Range {
                    start: parser::expr::Expression::Literal("true".to_string()),
                    end: parser::expr::Expression::Literal("1".to_string()),
                },
                body: vec![],
            }))
            .unwrap_err();
        assert_span_covers_name(&err, "int");
    }

    #[test]
    fn for_in_non_collection_span_covers_found_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let err = checker
            .check_statement(&parser::Statement::For(parser::ForLoop {
                variable: "x".to_string(),
                iterator: parser::ForIterator::Collection(parser::expr::Expression::Literal(
                    "true".to_string(),
                )),
                body: vec![],
            }))
            .unwrap_err();
        assert_span_covers_name(&err, "bool");
        match err {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                assert!(
                    !matches!(
                        expected,
                        Type::Array { ref elem, size: 0 } if **elem == Type::int()
                    ),
                    "non-collection for-in must not invent expected [int; 0], got {expected:?}"
                );
                assert_eq!(found, Type::bool());
            }
            TypeSystemError::ParseError { reason, .. } => {
                let r = reason.to_lowercase();
                assert!(
                    r.contains("collection") || r.contains("not a collection"),
                    "ParseError should say not a collection, got {reason}"
                );
            }
            other => panic!(
                "expected TypeMismatch without dummy [int; 0] or ParseError, got {other:?}"
            ),
        }
    }

    #[test]
    fn for_in_empty_tuple_cannot_infer_elem() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let err = checker
            .check_statement(&parser::Statement::For(parser::ForLoop {
                variable: "x".to_string(),
                iterator: parser::ForIterator::Collection(
                    parser::expr::Expression::TupleLiteral { elements: vec![] },
                ),
                body: vec![],
            }))
            .expect_err("empty tuple must not invent int as loop element type");
        match err {
            TypeSystemError::ParseError { reason, .. } => {
                let r = reason.to_lowercase();
                assert!(
                    r.contains("infer") || r.contains("empty") || r.contains("element"),
                    "error should mention cannot infer element type, got {reason}"
                );
            }
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                assert_ne!(
                    expected,
                    Type::int(),
                    "empty tuple must not fake expected int"
                );
                assert_ne!(found, Type::int(), "empty tuple must not fake found int");
            }
            other => panic!(
                "expected ParseError or type error without invented int, got {other:?}"
            ),
        }
    }

    #[test]
    fn raise_non_error_span_covers_error_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let err = checker
            .check_statement(&parser::Statement::Raise(parser::RaiseStmt {
                error_expr: parser::expr::Expression::Literal("1".to_string()),
            }))
            .unwrap_err();
        assert_span_covers_name(&err, "Error");
    }

    #[test]
    fn match_nonexhaustive_int_span_covers_type_name() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let arms = vec![parser::r#match::MatchArm {
            pattern: parser::pattern::Pattern::Literal("1".into()),
            guard: None,
            body: vec![],
        }];
        let err = checker
            .check_match_exhaustiveness(&Type::int(), &arms)
            .unwrap_err();
        assert_span_covers_name(&err, "int");
    }

    #[test]
    fn tuple_pattern_mismatch_span_covers_expected_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let expected = Type::Tuple(vec![Type::int(), Type::int()]);
        let err = checker
            .bind_pattern_vars(
                &parser::pattern::Pattern::Tuple(vec![parser::pattern::Pattern::Ident(
                    "x".to_string(),
                )]),
                &expected,
            )
            .unwrap_err();
        assert_span_covers_name(&err, "(int, int)");
    }

    #[test]
    fn variant_payload_missing_slot_span_covers_variant() {
        let inner = TypeRegistry::root();
        inner.define_type("Opt", opt_enum("int")).unwrap();
        let registry = Arc::new(RwLock::new(inner));
        let mut checker = TypeChecker::comprehensive(registry);
        let expected = Type::NamedType {
            name: "Opt".to_string(),
        };
        let pattern = parser::pattern::Pattern::EnumVariant {
            enum_name: "Opt".to_string(),
            variant: "None".to_string(),
            args: vec![parser::pattern::Pattern::Ident("x".to_string())],
        };
        let err = checker.bind_pattern_vars(&pattern, &expected).unwrap_err();
        assert_span_covers_name(&err, "None");
    }

    fn checker_with_c_symbol(
        name: &str,
        parameters: Vec<parser::function::Parameter>,
        return_type: &str,
    ) -> TypeChecker {
        use crate::c::{CSymbol, CSymbolTable};
        use crate::semantic::SemanticAnalyzer;
        use std::collections::HashMap;

        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let analyzer = SemanticAnalyzer::new(Arc::clone(&registry));
        let mut table = CSymbolTable::new("testlib".to_string());
        table.add(CSymbol {
            name: name.to_string(),
            parameters,
            return_type: return_type.to_string(),
            is_variadic: false,
        });
        let mut symbols = HashMap::new();
        symbols.insert("testlib".to_string(), table);
        analyzer.set_cfc_symbols(symbols);
        TypeChecker::with_analyzer(
            registry,
            CheckingMode::Comprehensive,
            Some(Arc::new(RwLock::new(analyzer))),
        )
    }

    fn c_param(name: &str, param_type: &str) -> parser::function::Parameter {
        parser::function::Parameter {
            name: name.to_string(),
            param_type: param_type.to_string(),
            is_variadic: false,
        }
    }

    #[test]
    fn main_arity_span_covers_entry_function() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let f = int_fn("id", None, vec![]);
        checker.declare_function_sig(&f).unwrap();
        let err = checker
            .check_statement(&parser::Statement::Main(parser::MainEntry {
                entry_function: "id".to_string(),
                args: vec![parser::expr::Expression::Literal("1".to_string())],
            }))
            .unwrap_err();
        assert_span_covers_name(&err, "id");
    }

    #[test]
    fn c_main_unresolvable_param_type_is_err_not_dropped() {
        let mut checker = checker_with_c_symbol(
            "c_entry",
            vec![c_param("x", "NotAType")],
            "int",
        );
        let err = checker
            .check_statement(&parser::Statement::Main(parser::MainEntry {
                entry_function: "c_entry".to_string(),
                args: vec![],
            }))
            .expect_err("unresolvable C param type must Err, not drop the param");
        match err {
            TypeSystemError::UndefinedType { name, .. } => {
                assert_eq!(name, "NotAType");
            }
            TypeSystemError::ParseError { type_str, .. } => {
                assert!(
                    type_str.contains("NotAType"),
                    "unexpected parse type_str: {type_str}"
                );
            }
            other => panic!("expected UndefinedType or ParseError, got {other:?}"),
        }
    }

    #[test]
    fn c_main_unresolvable_return_type_is_err_not_unit() {
        let mut checker = checker_with_c_symbol("c_entry", vec![], "NotAType");
        let err = checker
            .check_statement(&parser::Statement::Main(parser::MainEntry {
                entry_function: "c_entry".to_string(),
                args: vec![],
            }))
            .expect_err("unresolvable C return type must Err, not invent unit");
        match err {
            TypeSystemError::UndefinedType { name, .. } => {
                assert_eq!(name, "NotAType");
            }
            TypeSystemError::ParseError { type_str, .. } => {
                assert!(
                    type_str.contains("NotAType"),
                    "unexpected parse type_str: {type_str}"
                );
            }
            other => panic!("expected UndefinedType or ParseError, got {other:?}"),
        }
    }

    #[test]
    fn addr_of_non_int_index_span_covers_index_type() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "xs".to_string(),
                var_type: "[int; 1]".to_string(),
                value: parser::expr::Expression::ArrayLiteral {
                    elements: vec![parser::expr::Expression::Literal("1".to_string())],
                },
            })
            .unwrap();
        let expr = parser::expr::Expression::Unary {
            op: "&".to_string(),
            operand: Box::new(parser::expr::Expression::Index {
                array: Box::new(parser::expr::Expression::Variable("xs".to_string())),
                index: Box::new(parser::expr::Expression::Literal("true".to_string())),
            }),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        assert_span_covers_name(&err, "bool");
    }

    fn file_hwnd_registry() -> TypeRegistry {
        let reg = TypeRegistry::root();
        reg.register_newtype("FILE".into(), Type::Variadic).unwrap();
        reg.register_newtype("HWND".into(), Type::Variadic).unwrap();
        reg
    }

    #[test]
    fn newtype_cast_allowed_one_hop_not_sibling() {
        let registry = Arc::new(RwLock::new(file_hwnd_registry()));
        let checker = TypeChecker::simple(registry);
        let file_ty = Type::NamedType {
            name: "FILE".into(),
        };
        let hwnd_ty = Type::NamedType {
            name: "HWND".into(),
        };
        assert!(checker.cast_allowed(&file_ty, &Type::Variadic));
        assert!(checker.cast_allowed(&Type::Variadic, &file_ty));
        assert!(!checker.cast_allowed(&file_ty, &hwnd_ty));
        assert!(!checker.cast_allowed(&file_ty, &Type::int()));
        assert!(!checker.cast_allowed(&file_ty, &Type::buf()));
    }

    #[test]
    fn newtype_implicit_assign_file_is_not_object() {
        let registry = Arc::new(RwLock::new(file_hwnd_registry()));
        let mut checker = TypeChecker::comprehensive(registry);
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
        let err = checker
            .check_variable_decl(&parser::var::VariableDecl {
                name: "x".into(),
                var_type: "object".into(),
                value: parser::expr::Expression::Variable("f".into()),
            })
            .expect_err("let x: object = FILE must err");
        match err {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                assert_eq!(expected, Type::Variadic);
                assert_eq!(
                    found,
                    Type::NamedType {
                        name: "FILE".into()
                    }
                );
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn newtype_type_cast_file_as_object_ok() {
        let registry = Arc::new(RwLock::new(file_hwnd_registry()));
        let mut checker = TypeChecker::comprehensive(registry);
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
        let ty = checker
            .check_expression(&parser::expr::Expression::TypeCast {
                target_type: "object".into(),
                value: Box::new(parser::expr::Expression::Variable("f".into())),
            })
            .unwrap();
        assert_eq!(ty, Type::Variadic);
    }