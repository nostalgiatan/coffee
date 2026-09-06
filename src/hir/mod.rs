//! Typed HIR and statement MIR for Coffee.
//!
//! Frontend lowers functions to [`MirFn`]; complete MIR drives codegen.
//! Functions omitted from `hir_fns` (`lower_function` Err) are a codegen
//! error (`missing MIR for function`), not an AST body.

#![allow(dead_code, unused_imports)]

pub mod expr;
pub mod for_in_cfg;
pub mod lower;
pub mod match_cfg;
pub mod mir;
pub mod ssa;
pub mod stmt;

pub use expr::{HirExpr, HirExprKind};
#[cfg(test)]
pub use expr::hir_expr_to_ast;
pub use lower::{hir_stmts_to_mir, lower_expr, lower_function, lower_function_keyed, HirLower};
pub use mir::{MirBlock, MirFn, MirStmt, NestedDecl, NestedKind};
pub use ssa::to_ssa;
pub use stmt::HirStmt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::expr::Expression;
    use crate::parser::function::{Function, FunctionBody, Parameter};
    use crate::parser::pattern::Pattern;
    use crate::parser::r#match::{MatchArm, MatchExpr};
    use crate::parser::var::{ReturnStmt, VariableDecl};
    use crate::parser::class::{ClassDef, ClassField, EnumDef, EnumVariant};
    use crate::parser::comment::SingleLineComment;
    use crate::parser::{ForIterator, ForLoop, IfExpr, Import, MemoryOp, Statement, WhileLoop};
    use crate::types::definition::{EnumVariant as DefEnumVariant, TypeDef, VariantField};
    use crate::types::{Type, TypeRegistry};
    use std::collections::HashSet;
    use std::sync::{Arc, RwLock};

    fn option_result_registry() -> Arc<RwLock<TypeRegistry>> {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Option",
                TypeDef::Enum {
                    name: "Option".into(),
                    variants: vec![
                        DefEnumVariant {
                            name: "Some".into(),
                            fields: vec![VariantField::Positional("int".into())],
                        },
                        DefEnumVariant {
                            name: "None".into(),
                            fields: vec![],
                        },
                    ],
                    generics: vec![],
                },
            )
            .expect("Option");
        registry
            .define_type(
                "Result",
                TypeDef::Enum {
                    name: "Result".into(),
                    variants: vec![
                        DefEnumVariant {
                            name: "Ok".into(),
                            fields: vec![VariantField::Positional("Option".into())],
                        },
                        DefEnumVariant {
                            name: "Success".into(),
                            fields: vec![VariantField::Positional("Option".into())],
                        },
                    ],
                    generics: vec![],
                },
            )
            .expect("Result");
        Arc::new(RwLock::new(registry))
    }

    fn stub_infer(expr: &Expression) -> Result<Type, String> {
        if let Expression::Variable(name) = expr {
            if name == "xs" {
                return Ok(Type::Array {
                    elem: Box::new(Type::int()),
                    size: 2,
                });
            }
            // Test locals: the infer callback, not Expression::infer_type.
            return Ok(Type::int());
        }
        if let Expression::ArrayLiteral { elements } = expr {
            let Some(first) = elements.first() else {
                return Err("empty array literal has no element type".into());
            };
            let elem = stub_infer(first)?;
            return Ok(Type::Array {
                elem: Box::new(elem),
                size: elements.len(),
            });
        }
        if let Expression::TupleLiteral { elements } = expr {
            if elements.is_empty() {
                return Err("empty tuple literal has no element type".into());
            }
            let tys = elements
                .iter()
                .map(stub_infer)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Type::Tuple(tys));
        }
        expr.infer_type()
            .ok_or_else(|| format!("no syntactic type for {expr}"))
    }

    #[test]
    fn lower_add_literals() {
        let expr = Expression::binary(Expression::literal("1"), "+", Expression::literal("2"));
        let mut lower = HirLower::new(stub_infer);
        let hir = lower.lower_expr(&expr).expect("lower 1+2");
        assert_eq!(hir.ty, Type::int());
        match hir.kind {
            HirExprKind::Binary { op, left, right } => {
                assert_eq!(op, "+");
                assert!(matches!(left.kind, HirExprKind::Literal(ref v) if v == "1"));
                assert!(matches!(right.kind, HirExprKind::Literal(ref v) if v == "2"));
            }
            other => panic!("expected binary, got {:?}", other),
        }
    }

    #[test]
    fn lower_variable_expr() {
        let expr = Expression::var("x");
        let mut lower = HirLower::new(stub_infer);
        let hir = lower.lower_expr(&expr).expect("lower var");
        assert!(matches!(hir.kind, HirExprKind::Variable(ref n) if n == "x"));
    }

    #[test]
    fn lower_error_call_becomes_constructor_call() {
        let expr = Expression::Call {
            function: Box::new(Expression::var("Error")),
            args: vec![Expression::literal("1")],
        };
        let mut lower = HirLower::new(stub_infer);
        lower.set_registry(Arc::new(RwLock::new(TypeRegistry::root())));
        let hir = lower.lower_expr(&expr).expect("Error(...) ctor");
        match hir.kind {
            HirExprKind::ConstructorCall { class_name, args } => {
                assert_eq!(class_name, "Error");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected ConstructorCall, got {:?}", other),
        }
        assert_eq!(
            hir.ty,
            Type::NamedType {
                name: "Error".into()
            }
        );
    }

    #[test]
    fn lower_ordinary_class_call_stays_call() {
        let expr = Expression::Call {
            function: Box::new(Expression::var("Point")),
            args: vec![Expression::literal("1"), Expression::literal("2")],
        };
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Point",
                crate::types::definition::TypeDef::Class {
                    name: "Point".into(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: None,
                },
            )
            .expect("Point");
        fn infer(expr: &Expression) -> Result<Type, String> {
            if matches!(expr, Expression::Call { .. }) {
                return Ok(Type::NamedType {
                    name: "Point".into(),
                });
            }
            stub_infer(expr)
        }
        let mut lower = HirLower::new(infer);
        lower.set_registry(Arc::new(RwLock::new(registry)));
        let hir = lower.lower_expr(&expr).expect("Point(...)");
        match hir.kind {
            HirExprKind::Call { function, args } => {
                assert!(matches!(function.kind, HirExprKind::Variable(ref n) if n == "Point"));
                assert_eq!(args.len(), 2);
            }
            other => panic!("Point(...) must stay Call, got {:?}", other),
        }
    }

    #[test]
    fn lower_as_int_call_stays_call_after_infer_fail() {
        fn infer(expr: &Expression) -> Result<Type, String> {
            match expr {
                Expression::Call { function, .. } => {
                    if let Expression::Variable(name) = function.as_ref() {
                        if name == "as_int" {
                            return Err("pattern bind x is not in checker".into());
                        }
                    }
                    stub_infer(expr)
                }
                Expression::Variable(name) if name == "as_int" => Ok(Type::Function {
                    params: vec![Type::int()],
                    return_type: Box::new(Type::int()),
                }),
                _ => stub_infer(expr),
            }
        }
        let expr = Expression::Call {
            function: Box::new(Expression::var("as_int")),
            args: vec![Expression::var("x")],
        };
        let mut lower = HirLower::new(infer);
        lower.set_registry(Arc::new(RwLock::new(TypeRegistry::root())));
        lower.extra_types.insert("x".into(), Type::int());
        let hir = lower.lower_expr(&expr).expect("as_int(x)");
        assert_eq!(hir.ty, Type::int());
        match hir.kind {
            HirExprKind::Call { function, args } => {
                assert!(matches!(function.kind, HirExprKind::Variable(ref n) if n == "as_int"));
                assert_eq!(args.len(), 1);
            }
            other => panic!("as_int must stay Call, got {:?}", other),
        }
    }

    #[test]
    fn lower_expr_uses_rec_callback() {
        let expr = Expression::binary(Expression::literal("1"), "+", Expression::literal("2"));
        let rec = |e: &Expression| {
            let ty = e
                .infer_type()
                .or_else(|| match e {
                    Expression::Binary { left, .. } => left.infer_type(),
                    _ => None,
                })
                .ok_or_else(|| format!("no syntactic type for {e}"))?;
            lower_expr(e, ty, |inner| {
                Ok(HirExpr {
                    ty: inner
                        .infer_type()
                        .ok_or_else(|| format!("no syntactic type for {inner}"))?,
                    kind: match inner {
                        Expression::Literal(v) => HirExprKind::Literal(v.clone()),
                        Expression::Variable(v) => HirExprKind::Variable(v.clone()),
                        _ => return Err("nested rec only handles leaves in this test".into()),
                    },
                })
            })
        };
        let hir = rec(&expr).expect("standalone lower_expr");
        assert!(matches!(hir.kind, HirExprKind::Binary { .. }));
    }

    #[test]
    fn mir_if_lowers_to_branch() {
        let mut lower = HirLower::new(stub_infer);
        let stmt = Statement::If(IfExpr {
            condition: Expression::var("c"),
            body: vec![Statement::Return(ReturnStmt {
                value: Some(Expression::literal("1")),
            })],
            elifs: vec![],
            else_body: Some(vec![Statement::Return(ReturnStmt {
                value: Some(Expression::literal("0")),
            })]),
        });
        let hir = lower.lower_stmt(&stmt).expect("if stmt");
        let mir = hir_stmts_to_mir("f", &[hir]);
        let has_branch = mir
            .blocks
            .iter()
            .any(|b| matches!(b.stmts.iter().last(), Some(MirStmt::Branch { .. })));
        assert!(has_branch, "if should become Branch, got {:?}", mir);
        assert!(mir.blocks.len() >= 3);
    }

    #[test]
    fn mir_while_lowers_to_header_branch() {
        let mut lower = HirLower::new(stub_infer);
        let stmt = Statement::While(WhileLoop {
            condition: Expression::var("c"),
            body: vec![Statement::Assignment(
                "x".into(),
                Expression::literal("1"),
            )],
        });
        let hir = lower.lower_stmt(&stmt).expect("while");
        let mir = hir_stmts_to_mir("loop_fn", &[hir]);
        let branches: Vec<_> = mir
            .blocks
            .iter()
            .filter_map(|b| match b.stmts.last() {
                Some(MirStmt::Branch { then_bb, else_bb, .. }) => Some((*then_bb, *else_bb)),
                _ => None,
            })
            .collect();
        assert_eq!(branches.len(), 1);
    }

    #[test]
    fn lower_function_block_to_mir() {
        let func = Function {
            type_params: vec![],
            name: "add".into(),
            parameters: vec![Parameter {
                name: "a".into(),
                param_type: "int".into(),
                is_variadic: false,
            }],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::VariableDecl(VariableDecl {
                    name: "x".into(),
                    var_type: "int".into(),
                    value: Expression::binary(
                        Expression::var("a"),
                        "+",
                        Expression::literal("2"),
                    ),
                }),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::var("x")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let mir = lower_function(&func, &mut lower).expect("fn mir");
        assert_eq!(mir.name, "add");
        let has_assign = mir.blocks.iter().any(|b| {
            b.stmts
                .iter()
                .any(|s| matches!(s, MirStmt::Assign { name, .. } if name == "x"))
        });
        let has_ret = mir
            .blocks
            .iter()
            .any(|b| b.stmts.iter().any(|s| matches!(s, MirStmt::Return(_))));
        assert!(has_assign && has_ret, "{:?}", mir);
        assert!(mir.complete, "simple block body should lower without Unsupported");
    }

    #[test]
    fn lower_rm_keeps_mir_complete() {
        let func = Function {
            type_params: vec![],
            name: "f".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::VariableDecl(VariableDecl {
                    name: "x".into(),
                    var_type: "int".into(),
                    value: Expression::literal("1"),
                }),
                Statement::MemoryOp(MemoryOp::Remove {
                    target: "x".into(),
                }),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let mir = lower_function(&func, &mut lower).expect("fn mir");
        assert!(mir.complete, "{:?}", mir);
        let has_rm = mir.blocks.iter().any(|b| {
            b.stmts
                .iter()
                .any(|s| matches!(s, MirStmt::MemoryOp(MemoryOp::Remove { target }) if target == "x"))
        });
        assert!(has_rm, "{:?}", mir);
    }

    #[test]
    fn lower_range_for_keeps_mir_complete() {
        let stmt = Statement::For(ForLoop {
            variable: "i".into(),
            iterator: ForIterator::Range {
                start: Expression::literal("0"),
                end: Expression::literal("3"),
            },
            body: vec![Statement::Assignment(
                "s".into(),
                Expression::binary(Expression::var("s"), "+", Expression::var("i")),
            )],
        });
        let mut lower = HirLower::new(stub_infer);
        let hir = lower.lower_stmt(&stmt).expect("range for");
        let mir = hir_stmts_to_mir("main", &[hir]);
        assert!(mir.complete, "{:?}", mir);
        let has_branch = mir
            .blocks
            .iter()
            .any(|b| matches!(b.stmts.last(), Some(MirStmt::Branch { .. })));
        assert!(has_branch, "{:?}", mir);
    }

    fn for_range_body(stmt: &HirStmt) -> &[HirStmt] {
        match stmt {
            HirStmt::Scope { body } => body
                .iter()
                .find_map(|s| match s {
                    HirStmt::ForRange { body, .. } => Some(body.as_slice()),
                    _ => None,
                })
                .expect("ForRange after bind"),
            HirStmt::ForRange { body, .. } => body,
            other => panic!("expected Scope/ForRange, got {:?}", other),
        }
    }

    #[test]
    fn collection_for_loop_var_has_element_type() {
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::ArrayLiteral {
                elements: vec![Expression::literal("1.5"), Expression::literal("2.5")],
            }),
            body: vec![Statement::Expr(Box::new(Expression::var("x")))],
        });
        let mut lower = HirLower::new(stub_infer);
        let hir = lower.lower_stmt(&stmt).expect("float array for-in");
        let x = for_range_body(&hir)
            .iter()
            .find_map(|s| match s {
                HirStmt::Expr(e)
                    if matches!(e.kind, HirExprKind::Variable(ref n) if n == "x") =>
                {
                    Some(e)
                }
                _ => None,
            })
            .expect("body expr x");
        assert_eq!(
            x.ty,
            Type::float(),
            "loop var must be element type, not infer_type int"
        );
    }

    #[test]
    fn range_for_body_types_loop_var_int_and_other_exprs_via_infer() {
        fn infer(expr: &Expression) -> Result<Type, String> {
            match expr {
                Expression::Variable(name) if name == "s" => Ok(Type::float()),
                Expression::Call { .. } => Ok(Type::float()),
                _ => stub_infer(expr),
            }
        }
        let stmt = Statement::For(ForLoop {
            variable: "i".into(),
            iterator: ForIterator::Range {
                start: Expression::literal("0"),
                end: Expression::literal("3"),
            },
            body: vec![
                Statement::Expr(Box::new(Expression::var("s"))),
                Statement::Expr(Box::new(Expression::var("i"))),
                Statement::Expr(Box::new(Expression::Call {
                    function: Box::new(Expression::var("foo")),
                    args: vec![],
                })),
            ],
        });
        let mut lower = HirLower::new(infer);
        let hir = lower.lower_stmt(&stmt).expect("range for");
        let body = for_range_body(&hir);
        match &body[0] {
            HirStmt::Expr(e) => assert_eq!(e.ty, Type::float(), "non-loop local must use infer, not int"),
            other => panic!("expected Expr(s), got {:?}", other),
        }
        match &body[1] {
            HirStmt::Expr(e) => {
                assert!(matches!(e.kind, HirExprKind::Variable(ref n) if n == "i"));
                assert_eq!(e.ty, Type::int());
            }
            other => panic!("expected Expr(i), got {:?}", other),
        }
        match &body[2] {
            HirStmt::Expr(e) => {
                assert!(matches!(e.kind, HirExprKind::Call { .. }));
                assert_eq!(e.ty, Type::float(), "Call must use infer, not syntactic void");
            }
            other => panic!("expected Call, got {:?}", other),
        }
    }

    #[test]
    fn lower_match_raise_collection_for_keep_mir_complete() {
        let mut lower = HirLower::new(stub_infer);
        let match_stmt = Statement::Match(crate::parser::MatchExpr {
            value: Expression::var("x"),
            arms: vec![crate::parser::r#match::MatchArm {
                pattern: crate::parser::Pattern::Wildcard,
                guard: None,
                body: vec![Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                })],
            }],
        });
        let match_hir = lower.lower_stmt(&match_stmt).expect("match");
        let match_mir = hir_stmts_to_mir("m", &[match_hir]);
        assert!(match_mir.complete, "{:?}", match_mir);

        let raise_stmt = Statement::Raise(crate::parser::RaiseStmt {
            error_expr: Expression::var("E"),
        });
        let raise_hir = lower.lower_stmt(&raise_stmt).expect("raise");
        let raise_mir = hir_stmts_to_mir("r", &[raise_hir]);
        assert!(raise_mir.complete, "{:?}", raise_mir);

        let for_stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::var("xs")),
            body: vec![],
        });
        let for_hir = lower.lower_stmt(&for_stmt).expect("collection for");
        assert!(
            matches!(for_hir, HirStmt::ForRange { .. }),
            "array for-in should expand to ForRange, got {:?}",
            for_hir
        );
        let for_mir = hir_stmts_to_mir("f", &[for_hir]);
        assert!(for_mir.complete, "{:?}", for_mir);
        let has_branch = for_mir
            .blocks
            .iter()
            .any(|b| matches!(b.stmts.last(), Some(MirStmt::Branch { .. })));
        assert!(has_branch, "{:?}", for_mir);
    }

    #[test]
    fn lower_array_literal_for_in_expands_to_for_range() {
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::ArrayLiteral {
                elements: vec![Expression::literal("1"), Expression::literal("2")],
            }),
            body: vec![],
        });
        let mut lower = HirLower::new(stub_infer);
        let hir = lower.lower_stmt(&stmt).expect("array literal for");
        let ranged = match hir {
            HirStmt::Scope { body } => body
                .into_iter()
                .find(|s| matches!(s, HirStmt::ForRange { .. }))
                .expect("ForRange after bind"),
            HirStmt::ForRange { .. } => hir,
            other => panic!("expected Scope/ForRange, got {:?}", other),
        };
        let mir = hir_stmts_to_mir("f", &[ranged]);
        assert!(mir.complete, "{:?}", mir);
    }

    #[test]
    fn lower_empty_array_literal_for_in_cannot_infer_elem() {
        fn infer(expr: &Expression) -> Result<Type, String> {
            if matches!(expr, Expression::ArrayLiteral { .. }) {
                // Placeholder collection type — not `[T; N]` — so elem comes from
                // the literal, which is empty and must not invent `int`.
                return Ok(Type::int());
            }
            stub_infer(expr)
        }
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::ArrayLiteral { elements: vec![] }),
            body: vec![],
        });
        let mut lower = HirLower::new(infer);
        let err = lower
            .lower_stmt(&stmt)
            .expect_err("empty untyped [] has no element type");
        assert!(
            err.contains("cannot expand"),
            "expected cannot-expand error, got {err}"
        );
    }

    #[test]
    fn lower_array_call_for_in_binds_then_for_range() {
        fn infer(expr: &Expression) -> Result<Type, String> {
            if let Expression::Call { function, .. } = expr {
                if let Expression::Variable(name) = function.as_ref() {
                    if name == "make_xs" {
                        return Ok(Type::Array {
                            elem: Box::new(Type::int()),
                            size: 2,
                        });
                    }
                }
            }
            stub_infer(expr)
        }
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::Call {
                function: Box::new(Expression::var("make_xs")),
                args: vec![],
            }),
            body: vec![],
        });
        let mut lower = HirLower::new(infer);
        let hir = lower.lower_stmt(&stmt).expect("call for-in");
        match hir {
            HirStmt::Scope { body } => {
                assert!(
                    matches!(body.first(), Some(HirStmt::Let { .. })),
                    "expected temp bind, got {:?}",
                    body
                );
                assert!(
                    body.iter().any(|s| matches!(s, HirStmt::ForRange { .. })),
                    "{:?}",
                    body
                );
            }
            other => panic!("expected Scope with bind+ForRange, got {:?}", other),
        }
    }

    #[test]
    fn lower_slice_call_for_in_binds_then_for_range_len() {
        fn infer(expr: &Expression) -> Result<Type, String> {
            if let Expression::Call { .. } = expr {
                return Ok(Type::Slice(Box::new(Type::int())));
            }
            stub_infer(expr)
        }
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::Call {
                function: Box::new(Expression::var("make_slice")),
                args: vec![],
            }),
            body: vec![],
        });
        let mut lower = HirLower::new(infer);
        let hir = lower.lower_stmt(&stmt).expect("slice call for-in");
        match hir {
            HirStmt::Scope { body } => {
                assert!(
                    matches!(body.first(), Some(HirStmt::Let { .. })),
                    "expected temp bind, got {:?}",
                    body
                );
                let ranged = body
                    .iter()
                    .find(|s| matches!(s, HirStmt::ForRange { .. }))
                    .expect("ForRange after bind");
                match ranged {
                    HirStmt::ForRange { end, .. } => {
                        assert!(
                            matches!(end.kind, HirExprKind::Len { .. }),
                            "slice for-in end should be Len, got {:?}",
                            end
                        );
                    }
                    _ => unreachable!(),
                }
            }
            other => panic!("expected Scope with bind+ForRange, got {:?}", other),
        }
    }

    #[test]
    fn lower_tuple_call_for_in_binds_then_for_range() {
        fn infer(expr: &Expression) -> Result<Type, String> {
            if let Expression::Call { .. } = expr {
                return Ok(Type::Tuple(vec![Type::int(), Type::int()]));
            }
            stub_infer(expr)
        }
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::Call {
                function: Box::new(Expression::var("make_t")),
                args: vec![],
            }),
            body: vec![],
        });
        let mut lower = HirLower::new(infer);
        let hir = lower.lower_stmt(&stmt).expect("tuple call for-in");
        match hir {
            HirStmt::Scope { body } => {
                assert!(
                    matches!(body.first(), Some(HirStmt::Let { .. })),
                    "expected temp bind, got {:?}",
                    body
                );
                assert!(
                    body.iter().any(|s| matches!(s, HirStmt::ForRange { .. })),
                    "{:?}",
                    body
                );
            }
            other => panic!("expected Scope with bind+ForRange, got {:?}", other),
        }
    }

    #[test]
    fn lower_non_slice_call_for_in_is_err() {
        fn infer(expr: &Expression) -> Result<Type, String> {
            if let Expression::Call { .. } = expr {
                return Ok(Type::int());
            }
            stub_infer(expr)
        }
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::Call {
                function: Box::new(Expression::var("make_int")),
                args: vec![],
            }),
            body: vec![],
        });
        let mut lower = HirLower::new(infer);
        let result = lower.lower_stmt(&stmt);
        assert!(
            result.is_err(),
            "length cannot be tracked for non-slice/non-array; must not leftover ForIn, got {:?}",
            result
        );
    }

    fn point_registry() -> Arc<RwLock<TypeRegistry>> {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Point",
                TypeDef::Class {
                    name: "Point".into(),
                    fields: vec![crate::types::definition::ClassField {
                        name: "x".into(),
                        ty: "int".into(),
                        visibility: crate::types::definition::Visibility::Public,
                    }],
                    methods: vec![],
                    generics: vec![],
                    parent: None,
                },
            )
            .expect("Point");
        Arc::new(RwLock::new(registry))
    }

    fn snapshot_misses_loop_locals(expr: &Expression) -> Result<Type, String> {
        match expr {
            Expression::Variable(name) if name == "xs" => Ok(Type::Array {
                elem: Box::new(Type::int()),
                size: 1,
            }),
            Expression::Variable(name) if name == "x" || name == "p" => {
                Err(format!("{name} is a loop-body local, not in bind snapshot"))
            }
            Expression::Member { .. } | Expression::Binary { .. } | Expression::StructLiteral { .. } => {
                Err("checker snapshot has no loop-body locals".into())
            }
            _ => stub_infer(expr),
        }
    }

    fn hir_has_member(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Let { value, .. } | HirStmt::Assign { value, .. } | HirStmt::Expr(value) => {
                expr_has_member(value)
            }
            HirStmt::Return(Some(value)) | HirStmt::Raise(value) => expr_has_member(value),
            HirStmt::If {
                cond,
                then_body,
                elifs,
                else_body,
            } => {
                expr_has_member(cond)
                    || then_body.iter().any(hir_has_member)
                    || elifs
                        .iter()
                        .any(|(c, b)| expr_has_member(c) || b.iter().any(hir_has_member))
                    || else_body
                        .as_ref()
                        .is_some_and(|b| b.iter().any(hir_has_member))
            }
            HirStmt::While { cond, body } => {
                expr_has_member(cond) || body.iter().any(hir_has_member)
            }
            HirStmt::ForRange {
                start, end, body, ..
            } => expr_has_member(start) || expr_has_member(end) || body.iter().any(hir_has_member),
            HirStmt::Scope { body } => body.iter().any(hir_has_member),
            _ => false,
        }
    }

    fn expr_has_member(e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Member { .. } => true,
            HirExprKind::Binary { left, right, .. } => {
                expr_has_member(left) || expr_has_member(right)
            }
            HirExprKind::Unary { operand, .. }
            | HirExprKind::Index { array: operand, .. }
            | HirExprKind::Len { collection: operand }
            | HirExprKind::TupleField { tuple: operand, .. } => expr_has_member(operand),
            HirExprKind::Call { function, args } => {
                expr_has_member(function) || args.iter().any(expr_has_member)
            }
            _ => false,
        }
    }

    #[test]
    fn for_body_let_enters_extra_types_so_member_lowers() {
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::var("xs")),
            body: vec![
                Statement::VariableDecl(VariableDecl {
                    name: "p".into(),
                    var_type: "Point".into(),
                    value: Expression::StructLiteral {
                        struct_name: "Point".into(),
                        fields: vec![("x".into(), Expression::var("x"))],
                    },
                }),
                Statement::If(IfExpr {
                    condition: Expression::binary(
                        Expression::Member {
                            object: Box::new(Expression::var("p")),
                            field: "x".into(),
                            args: vec![],
                        },
                        ">",
                        Expression::literal("0"),
                    ),
                    body: vec![],
                    elifs: vec![],
                    else_body: None,
                }),
            ],
        });
        let mut lower = HirLower::new(snapshot_misses_loop_locals);
        lower.set_registry(point_registry());
        let hir = lower
            .lower_stmt(&stmt)
            .expect("loop-body let p must enter extra_types so p.x lowers");
        assert!(
            hir_has_member(&hir),
            "expected Member p.x inside for body, got {:?}",
            hir
        );
        match hir {
            HirStmt::ForRange { .. } => {}
            other => panic!("array for-in must be ForRange, got {:?}", other),
        }
    }

    fn helper_fn(name: &str) -> Function {
        Function {
            type_params: vec![],
            name: name.into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![Statement::Return(ReturnStmt {
                value: Some(Expression::literal("1")),
            })]),
            is_c: false,
        }
    }

    #[test]
    fn nested_decl_from_statement_uses_unique_function_key() {
        let stmt = Statement::Function(helper_fn("helper"));
        let mut taken = HashSet::from(["helper".to_string()]);
        let decl = NestedDecl::from_statement(&stmt, Some("b"), &mut taken);
        assert_eq!(
            decl,
            NestedDecl {
                kind: NestedKind::Function,
                name: "helper".into(),
                hir_key: "b_helper".into(),
            }
        );
        assert!(taken.contains("b_helper"));
    }

    #[test]
    fn lower_nested_fn_hir_is_nested_decl() {
        let func = Function {
            type_params: vec![],
            name: "outer".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::Function(helper_fn("helper")),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let hir = lower.lower_function_body(&func).expect("hir");
        assert!(
            matches!(
                hir.first(),
                Some(HirStmt::Nested(NestedDecl {
                    kind: NestedKind::Function,
                    name,
                    hir_key,
                })) if name == "helper" && hir_key == "helper"
            ),
            "{:?}",
            hir
        );
    }

    #[test]
    fn lower_nested_fn_keeps_mir_complete() {
        let func = Function {
            type_params: vec![],
            name: "outer".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::Function(Function {
                    type_params: vec![],
                    name: "helper".into(),
                    parameters: vec![],
                    return_type: "int".into(),
                    error_handler: None,
                    body: FunctionBody::Block(vec![Statement::Return(ReturnStmt {
                        value: Some(Expression::literal("1")),
                    })]),
                    is_c: false,
                }),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let mir = lower_function(&func, &mut lower).expect("fn mir");
        assert!(mir.complete, "{:?}", mir);
        let has_nested_fn = mir.blocks.iter().any(|b| {
            b.stmts.iter().any(|s| {
                matches!(
                    s,
                    MirStmt::Nested(NestedDecl {
                        kind: NestedKind::Function,
                        name,
                        hir_key,
                    }) if name == "helper" && hir_key == "helper"
                )
            })
        });
        assert!(has_nested_fn, "{:?}", mir);
    }

    #[test]
    fn lower_nested_class_keeps_mir_complete() {
        let func = Function {
            type_params: vec![],
            name: "outer".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::Class(ClassDef {
                    type_params: vec![],
                    name: "Point".into(),
                    parent: None,
                    fields: vec![ClassField {
                        name: "x".into(),
                        field_type: "int".into(),
                        bit_width: None,
                    }],
                    methods: vec![],
                    packed: false,
                    has_constructor: false,
                }),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let mir = lower_function(&func, &mut lower).expect("fn mir");
        assert!(mir.complete, "{:?}", mir);
        let has_nested_class = mir.blocks.iter().any(|b| {
            b.stmts.iter().any(|s| {
                matches!(
                    s,
                    MirStmt::Nested(NestedDecl {
                        kind: NestedKind::Class,
                        name,
                        ..
                    }) if name == "Point"
                )
            })
        });
        assert!(has_nested_class, "{:?}", mir);
    }

    #[test]
    fn lower_nested_enum_keeps_mir_complete() {
        let func = Function {
            type_params: vec![],
            name: "outer".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::Enum(EnumDef {
                    name: "Color".into(),
                    variants: vec![EnumVariant {
                        name: "Red".into(),
                        fields: vec![],
                    }],
                }),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let mir = lower_function(&func, &mut lower).expect("fn mir");
        assert!(mir.complete, "{:?}", mir);
        let has_nested_enum = mir.blocks.iter().any(|b| {
            b.stmts.iter().any(|s| {
                matches!(
                    s,
                    MirStmt::Nested(NestedDecl {
                        kind: NestedKind::Enum,
                        name,
                        ..
                    }) if name == "Color"
                )
            })
        });
        assert!(has_nested_enum, "{:?}", mir);
    }

    #[test]
    fn lower_nested_fn_in_if_keeps_mir_complete() {
        let stmt = Statement::If(IfExpr {
            condition: Expression::literal("true"),
            body: vec![Statement::Function(Function {
                type_params: vec![],
                name: "helper".into(),
                parameters: vec![],
                return_type: "int".into(),
                error_handler: None,
                body: FunctionBody::Block(vec![Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("1")),
                })]),
                is_c: false,
            })],
            elifs: vec![],
            else_body: None,
        });
        let mut lower = HirLower::new(stub_infer);
        let hir = lower.lower_stmt(&stmt).expect("if");
        let mir = hir_stmts_to_mir("outer", &[hir]);
        assert!(mir.complete, "{:?}", mir);
        let has_nested_fn = mir.blocks.iter().any(|b| {
            b.stmts.iter().any(|s| {
                matches!(
                    s,
                    MirStmt::Nested(NestedDecl {
                        kind: NestedKind::Function,
                        name,
                        hir_key,
                    }) if name == "helper" && hir_key == "helper"
                )
            })
        });
        assert!(has_nested_fn, "{:?}", mir);
    }

    #[test]
    fn lower_comment_and_import_keep_mir_complete() {
        let func = Function {
            type_params: vec![],
            name: "outer".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::SingleLineComment(SingleLineComment {
                    content: "note".into(),
                }),
                Statement::Import(Import::Simple {
                    path: "mod".into(),
                }),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let mir = lower_function(&func, &mut lower).expect("fn mir");
        assert!(mir.complete, "{:?}", mir);
    }

    #[test]
    fn nested_fn_and_class_in_source_keep_main_complete() {
        let src = r#"
fn main() => int:
    class Point:
        x: int
    fn helper() => int:
        return 1
    return 0
"#;
        let result = crate::compiler::CompilationPipeline::new().compile(src, Some("nested.cf"));
        assert!(
            result.errors.is_empty(),
            "frontend errors: {:?}",
            result.errors
        );
        let mir = result
            .hir_fns
            .iter()
            .find(|f| f.name == "main")
            .expect("main MIR");
        assert!(mir.complete, "{:?}", mir);
        let kinds: Vec<_> = mir
            .blocks
            .iter()
            .flat_map(|b| b.stmts.iter())
            .filter_map(|s| match s {
                MirStmt::Nested(NestedDecl {
                    kind: NestedKind::Class,
                    name,
                    ..
                }) => Some(format!("class:{}", name)),
                MirStmt::Nested(NestedDecl {
                    kind: NestedKind::Function,
                    name,
                    ..
                }) => Some(format!("fn:{}", name)),
                _ => None,
            })
            .collect();
        assert!(
            kinds.iter().any(|k| k == "class:Point") && kinds.iter().any(|k| k == "fn:helper"),
            "{:?}",
            kinds
        );
        let helper = result
            .hir_fns
            .iter()
            .find(|f| f.name == "helper")
            .expect("nested helper MIR");
        assert!(helper.complete, "{:?}", helper);
    }

    #[test]
    fn nested_helpers_in_different_functions_use_distinct_hir_keys() {
        let src = r#"
fn a() => int:
    fn helper() => int:
        return 1
    return helper()

fn b() => int:
    fn helper() => int:
        return 2
    return helper()

fn main() => int:
    return a() + b()
"#;
        let result = crate::compiler::CompilationPipeline::new().compile(src, Some("helpers.cf"));
        assert!(
            result.errors.is_empty(),
            "frontend errors: {:?}",
            result.errors
        );
        let names: Vec<&str> = result.hir_fns.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"a") && names.contains(&"b") && names.contains(&"main"), "{:?}", names);
        let helper_keys: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| *n == "helper" || n.ends_with("_helper"))
            .collect();
        assert_eq!(helper_keys.len(), 2, "{:?}", names);
        assert!(helper_keys.contains(&"helper"), "{:?}", helper_keys);
        assert!(helper_keys.contains(&"b_helper"), "{:?}", helper_keys);
        let nested_keys = |fn_name: &str| -> Vec<String> {
            result
                .hir_fns
                .iter()
                .find(|f| f.name == fn_name)
                .expect(fn_name)
                .blocks
                .iter()
                .flat_map(|b| b.stmts.iter())
                .filter_map(|s| match s {
                    MirStmt::Nested(NestedDecl {
                        kind: NestedKind::Function,
                        name,
                        hir_key,
                    }) if name == "helper" => Some(hir_key.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(nested_keys("a"), vec!["helper".to_string()], "{:?}", result.hir_fns);
        assert_eq!(nested_keys("b"), vec!["b_helper".to_string()], "{:?}", result.hir_fns);
    }

    #[test]
    fn hir_expr_to_ast_preserves_binary() {
        let hir = HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Binary {
                left: Box::new(HirExpr {
                    ty: Type::int(),
                    kind: HirExprKind::Literal("1".into()),
                }),
                op: "+".into(),
                right: Box::new(HirExpr {
                    ty: Type::int(),
                    kind: HirExprKind::Literal("2".into()),
                }),
            },
        };
        let ast = hir_expr_to_ast(&hir);
        assert_eq!(
            ast,
            Expression::binary(Expression::literal("1"), "+", Expression::literal("2"))
        );
    }

    fn int_var_expr() -> HirExpr {
        HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Variable("x".into()),
        }
    }

    #[test]
    fn or_of_literals_is_simple_tuple_is_not() {
        let or_match = MatchExpr {
            value: Expression::var("x"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Or(vec![
                        Pattern::Literal("1".into()),
                        Pattern::Literal("2".into()),
                    ]),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        };
        assert!(match_cfg::match_is_simple(&or_match, &Type::int()));
        assert!(!match_cfg::match_is_simple(&or_match, &Type::bool()));

        let tuple_match = MatchExpr {
            value: Expression::var("p"),
            arms: vec![MatchArm {
                pattern: Pattern::Tuple(vec![
                    Pattern::Literal("0".into()),
                    Pattern::Wildcard,
                ]),
                guard: None,
                body: vec![],
            }],
        };
        assert!(!match_cfg::match_is_simple(&tuple_match, &Type::int()));
        let tup_ty = Type::Tuple(vec![Type::int(), Type::int()]);
        assert!(match_cfg::match_is_simple(&tuple_match, &tup_ty));
    }

    fn int_match_x() -> MatchExpr {
        MatchExpr {
            value: Expression::var("x"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal("1".into()),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        }
    }

    #[test]
    fn bool_match_is_simple_tuple_is_not() {
        let bool_match = MatchExpr {
            value: Expression::var("x"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal("true".into()),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Literal("false".into()),
                    guard: None,
                    body: vec![],
                },
            ],
        };
        assert!(match_cfg::match_is_simple(&bool_match, &Type::bool()));
        assert!(!match_cfg::match_is_simple(&bool_match, &Type::int()));
        assert!(match_cfg::match_is_simple(&int_match_x(), &Type::int()));
        assert!(!match_cfg::match_is_simple(&int_match_x(), &Type::bool()));
        // "1" parses as f64, so int-looking arms are simple on float too.
        assert!(match_cfg::match_is_simple(&int_match_x(), &Type::float()));
        assert!(!match_cfg::match_is_simple(&int_match_x(), &Type::string()));

        let tuple_match = MatchExpr {
            value: Expression::var("p"),
            arms: vec![MatchArm {
                pattern: Pattern::Tuple(vec![
                    Pattern::Literal("0".into()),
                    Pattern::Wildcard,
                ]),
                guard: None,
                body: vec![],
            }],
        };
        assert!(!match_cfg::match_is_simple(&tuple_match, &Type::int()));
        assert!(!match_cfg::match_is_simple(&tuple_match, &Type::bool()));
        let tup_ty = Type::Tuple(vec![Type::int(), Type::int()]);
        assert!(match_cfg::match_is_simple(&tuple_match, &tup_ty));
    }

    #[test]
    fn class_field_match_is_simple() {
        let field_match = MatchExpr {
            value: Expression::Member {
                object: Box::new(Expression::var("p")),
                field: "x".into(),
                args: vec![],
            },
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal("10".into()),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        };
        assert!(match_cfg::match_is_simple(&field_match, &Type::int()));
        let enum_match = MatchExpr {
            value: Expression::var("c"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::EnumVariant {
                        enum_name: "Color".into(),
                        variant: "Red".into(),
                        args: vec![],
                    },
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        };
        assert!(match_cfg::match_is_simple(
            &enum_match,
            &Type::NamedType {
                name: "Color".into()
            }
        ));
        let nested_enum = MatchExpr {
            value: Expression::var("res"),
            arms: vec![MatchArm {
                pattern: Pattern::EnumVariant {
                    enum_name: "Result".into(),
                    variant: "Success".into(),
                    args: vec![Pattern::EnumVariant {
                        enum_name: "Option".into(),
                        variant: "Some".into(),
                        args: vec![Pattern::Ident("x".into())],
                    }],
                },
                guard: None,
                body: vec![],
            }],
        };
        assert!(match_cfg::match_is_simple(
            &nested_enum,
            &Type::NamedType {
                name: "Result".into()
            }
        ));
    }

    #[test]
    fn ident_match_on_array_and_function_is_simple() {
        let ident_match = MatchExpr {
            value: Expression::var("xs"),
            arms: vec![MatchArm {
                pattern: Pattern::Ident("ys".into()),
                guard: None,
                body: vec![],
            }],
        };
        let arr = Type::Array {
            elem: Box::new(Type::int()),
            size: 2,
        };
        assert!(match_cfg::match_is_simple(&ident_match, &arr));
        let fn_ty = Type::Function {
            params: vec![],
            return_type: Box::new(Type::int()),
        };
        assert!(match_cfg::match_is_simple(&ident_match, &fn_ty));
        let mixed = MatchExpr {
            value: Expression::var("f"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal("1".into()),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        };
        assert!(!match_cfg::match_is_simple(&mixed, &fn_ty));
        assert!(!match_cfg::match_is_simple(&mixed, &Type::void()));
        assert!(!match_cfg::match_is_simple(&mixed, &Type::Variadic));
    }

    #[test]
    fn non_simple_match_is_lower_error_not_leftover_hir_match() {
        // Conflicting pattern hints (literal vs enum) so match_is_simple stays false.
        let mixed = Statement::Match(MatchExpr {
            value: Expression::var("f"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal("1".into()),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::EnumVariant {
                        enum_name: "Color".into(),
                        variant: "Red".into(),
                        args: vec![],
                    },
                    guard: None,
                    body: vec![],
                },
            ],
        });
        let mut lower = HirLower::new(|e| {
            if let Expression::Variable(n) = e {
                if n == "f" {
                    return Ok(Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::int()),
                    });
                }
            }
            Err("unknown expr".into())
        });
        let result = lower.lower_stmt(&mixed);
        assert!(
            result.is_err(),
            "non-simple match must Err, not leftover HirStmt::Match, got {:?}",
            result
        );
    }

    #[test]
    fn non_simple_match_lower_function_is_err() {
        let func = Function {
            type_params: vec![],
            name: "f".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![
                Statement::Match(MatchExpr {
                    value: Expression::var("g"),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Literal("1".into()),
                            guard: None,
                            body: vec![],
                        },
                        MatchArm {
                            pattern: Pattern::EnumVariant {
                                enum_name: "Color".into(),
                                variant: "Red".into(),
                                args: vec![],
                            },
                            guard: None,
                            body: vec![],
                        },
                    ],
                }),
                Statement::Return(ReturnStmt {
                    value: Some(Expression::literal("0")),
                }),
            ]),
            is_c: false,
        };
        let mut lower = HirLower::new(|e| {
            if let Expression::Variable(n) = e {
                if n == "g" {
                    return Ok(Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::int()),
                    });
                }
            }
            Err("unknown expr".into())
        });
        let result = lower_function(&func, &mut lower);
        assert!(
            result.is_err(),
            "non-simple match must omit MirFn (lower_function Err), got {:?}",
            result
        );
    }

    fn str_literal_match_s() -> MatchExpr {
        MatchExpr {
            value: Expression::var("s"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal("\"a\"".into()),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        }
    }

    #[test]
    fn str_literal_match_is_simple() {
        assert!(match_cfg::match_is_simple(
            &str_literal_match_s(),
            &Type::string()
        ));
        assert!(!match_cfg::match_is_simple(
            &str_literal_match_s(),
            &Type::int()
        ));
    }

    #[test]
    fn str_literal_match_lowers_without_leftover_match() {
        let m = Statement::Match(str_literal_match_s());
        let mut lower = HirLower::new(|e| {
            if let Expression::Variable(n) = e {
                if n == "s" {
                    return Ok(Type::string());
                }
            }
            Err("unknown expr".into())
        });
        let hir = lower.lower_stmt(&m).expect("str match");
        assert!(
            !hir_has_leftover_match(&hir),
            "leftover HirStmt::Match: {:?}",
            hir
        );
        assert!(
            matches!(if_in_scope(hir), HirStmt::If { .. }),
            "expected If"
        );
    }

    fn fn_var_expr() -> HirExpr {
        HirExpr {
            ty: Type::Function {
                params: vec![],
                return_type: Box::new(Type::int()),
            },
            kind: HirExprKind::Variable("f".into()),
        }
    }

    fn void_var_expr() -> HirExpr {
        HirExpr {
            ty: Type::void(),
            kind: HirExprKind::Variable("v".into()),
        }
    }

    fn assert_catchall_lowers_to_if(stmt: HirStmt) {
        match stmt {
            HirStmt::If { .. } => {}
            HirStmt::Scope { body } => {
                assert!(
                    body.iter().any(|s| matches!(s, HirStmt::If { .. })),
                    "expected If inside Scope, got {:?}",
                    body
                );
                assert!(
                    !body.iter().any(hir_has_leftover_match),
                    "leftover HirStmt::Match in Scope"
                );
            }
            other => panic!("expected If or Scope+If, got {:?}", other),
        }
    }

    #[test]
    fn wildcard_match_on_function_lowers_to_if() {
        let stmt = match_cfg::match_arms_to_if(
            fn_var_expr(),
            vec![(Pattern::Wildcard, None, vec![])],
        )
        .expect("wildcard fn lower");
        assert_catchall_lowers_to_if(stmt);
    }

    #[test]
    fn wildcard_match_on_void_lowers_to_if() {
        let stmt = match_cfg::match_arms_to_if(
            void_var_expr(),
            vec![(Pattern::Wildcard, None, vec![])],
        )
        .expect("wildcard void lower");
        assert_catchall_lowers_to_if(stmt);
    }

    fn color_var_expr() -> HirExpr {
        HirExpr {
            ty: Type::NamedType {
                name: "Color".into(),
            },
            kind: HirExprKind::Variable("c".into()),
        }
    }

    fn option_var_expr() -> HirExpr {
        HirExpr {
            ty: Type::NamedType {
                name: "Option".into(),
            },
            kind: HirExprKind::Variable("opt".into()),
        }
    }

    fn if_in_scope(stmt: HirStmt) -> HirStmt {
        match stmt {
            HirStmt::Scope { body } => body
                .into_iter()
                .find(|s| matches!(s, HirStmt::If { .. }))
                .expect("If inside Scope"),
            other => other,
        }
    }

    #[test]
    fn enum_unit_match_lowers_to_if_tag_eq() {
        let stmt = match_cfg::match_arms_to_if(
            color_var_expr(),
            vec![
                (
                    Pattern::EnumVariant {
                        enum_name: "Color".into(),
                        variant: "Red".into(),
                        args: vec![],
                    },
                    None,
                    vec![],
                ),
                (Pattern::Wildcard, None, vec![]),
            ],
        )
        .expect("unit enum lower");
        match if_in_scope(stmt) {
            HirStmt::If {
                cond, else_body, ..
            } => {
                match cond.kind {
                    HirExprKind::Binary { op, left, .. } => {
                        assert_eq!(op, "==");
                        assert!(
                            matches!(left.kind, HirExprKind::EnumTag { .. }),
                            "expected EnumTag, got {:?}",
                            left.kind
                        );
                    }
                    other => panic!("expected tag ==, got {:?}", other),
                }
                assert!(else_body.is_some());
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn enum_payload_match_binds_enum_payload() {
        let stmt = match_cfg::match_arms_to_if_with_registry(
            option_var_expr(),
            vec![
                (
                    Pattern::EnumVariant {
                        enum_name: "Option".into(),
                        variant: "Some".into(),
                        args: vec![Pattern::Ident("x".into())],
                    },
                    None,
                    vec![],
                ),
                (
                    Pattern::EnumVariant {
                        enum_name: "Option".into(),
                        variant: "None".into(),
                        args: vec![],
                    },
                    None,
                    vec![],
                ),
            ],
            Some(option_result_registry()),
        )
        .expect("payload enum lower");
        match if_in_scope(stmt) {
            HirStmt::If { then_body, elifs, .. } => {
                match then_body.first() {
                    Some(HirStmt::Let { name, value, .. }) => {
                        assert_eq!(name, "x");
                        assert!(
                            matches!(value.kind, HirExprKind::EnumPayload { index: 0, .. }),
                            "expected EnumPayload, got {:?}",
                            value.kind
                        );
                    }
                    other => panic!("expected Let x = payload, got {:?}", other),
                }
                assert_eq!(elifs.len(), 1);
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn nested_enum_lower_stmt_is_if_payload() {
        let nested = Statement::Match(MatchExpr {
            value: Expression::var("res"),
            arms: vec![MatchArm {
                pattern: Pattern::EnumVariant {
                    enum_name: "Result".into(),
                    variant: "Success".into(),
                    args: vec![Pattern::EnumVariant {
                        enum_name: "Option".into(),
                        variant: "Some".into(),
                        args: vec![Pattern::Ident("x".into())],
                    }],
                },
                guard: None,
                body: vec![],
            }],
        });
        let mut lower = HirLower::new(|_e| {
            Ok(Type::NamedType {
                name: "Result".into(),
            })
        });
        lower.set_registry(option_result_registry());
        let hir = lower.lower_stmt(&nested).expect("nested enum");
        assert!(
            !hir_has_leftover_match(&hir),
            "leftover HirStmt::Match: {:?}",
            hir
        );
        match if_in_scope(hir) {
            HirStmt::If { cond, then_body, .. } => {
                fn has_nested_enum_ops(e: &HirExpr) -> bool {
                    match &e.kind {
                        HirExprKind::EnumTag { value, .. } => {
                            matches!(value.kind, HirExprKind::EnumPayload { .. })
                                || has_nested_enum_ops(value)
                        }
                        HirExprKind::EnumPayload { value, .. } => has_nested_enum_ops(value),
                        HirExprKind::Binary { left, right, .. } => {
                            has_nested_enum_ops(left) || has_nested_enum_ops(right)
                        }
                        _ => false,
                    }
                }
                assert!(
                    has_nested_enum_ops(&cond),
                    "expected inner EnumTag of EnumPayload, got {:?}",
                    cond.kind
                );
                match then_body.first() {
                    Some(HirStmt::Let { name, value, .. }) => {
                        assert_eq!(name, "x");
                        match &value.kind {
                            HirExprKind::EnumPayload { value: inner, .. } => {
                                assert!(
                                    matches!(inner.kind, HirExprKind::EnumPayload { .. }),
                                    "expected nested EnumPayload bind, got {:?}",
                                    inner.kind
                                );
                            }
                            other => panic!("expected Let x = EnumPayload, got {:?}", other),
                        }
                    }
                    other => panic!("expected Let x = payload, got {:?}", other),
                }
            }
            other => panic!("nested enum should lower to If, got {:?}", other),
        }
    }

    fn result_ok_option_some_match() -> Statement {
        Statement::Match(MatchExpr {
            value: Expression::var("res"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::EnumVariant {
                        enum_name: "Result".into(),
                        variant: "Ok".into(),
                        args: vec![Pattern::EnumVariant {
                            enum_name: "Option".into(),
                            variant: "Some".into(),
                            args: vec![Pattern::Ident("x".into())],
                        }],
                    },
                    guard: None,
                    body: vec![Statement::Return(ReturnStmt {
                        value: Some(Expression::binary(
                            Expression::var("x"),
                            "*",
                            Expression::literal("2"),
                        )),
                    })],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![Statement::Return(ReturnStmt {
                        value: Some(Expression::literal("0")),
                    })],
                },
            ],
        })
    }

    fn hir_has_leftover_match(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::If {
                then_body,
                elifs,
                else_body,
                ..
            } => {
                then_body.iter().any(hir_has_leftover_match)
                    || elifs.iter().any(|(_, b)| b.iter().any(hir_has_leftover_match))
                    || else_body
                        .as_ref()
                        .map(|b| b.iter().any(hir_has_leftover_match))
                        .unwrap_or(false)
            }
            HirStmt::Scope { body }
            | HirStmt::While { body, .. }
            | HirStmt::ForRange { body, .. } => body.iter().any(hir_has_leftover_match),
            _ => false,
        }
    }

    fn mir_has_leftover_match(mir: &MirFn) -> bool {
        mir.blocks.iter().any(|b| {
            b.stmts
                .iter()
                .any(|s| format!("{s:?}").starts_with("Match("))
        })
    }

    #[test]
    fn nested_result_ok_option_some_lowers_without_leftover_match() {
        let mut lower = HirLower::new(|e| {
            if let Expression::Variable(n) = e {
                if n == "res" {
                    return Ok(Type::NamedType {
                        name: "Result".into(),
                    });
                }
            }
            if matches!(e, Expression::Literal(_)) {
                return e
                    .infer_type()
                    .ok_or_else(|| format!("no syntactic type for {e}"));
            }
            Err("unknown expr".into())
        });
        lower.set_registry(option_result_registry());
        let hir = lower
            .lower_stmt(&result_ok_option_some_match())
            .expect("Result.Ok(Option.Some(x))");
        assert!(
            !hir_has_leftover_match(&hir),
            "leftover HirStmt::Match when Result is known: {:?}",
            hir
        );
        assert!(
            matches!(if_in_scope(hir), HirStmt::If { .. }),
            "expected If"
        );
    }

    #[test]
    fn nested_result_ok_lowers_when_infer_callback_fails() {
        let mut lower = HirLower::new(|e| match e {
            Expression::Literal(_) => Ok(e.infer_type().expect("literal infers")),
            _ => Err("checker failed".into()),
        });
        lower.set_registry(option_result_registry());
        let hir = lower
            .lower_stmt(&result_ok_option_some_match())
            .expect("pattern NamedType Result");
        assert!(
            !hir_has_leftover_match(&hir),
            "infer_type int fallback would leave Match: {:?}",
            hir
        );
    }

    #[test]
    fn member_match_recovers_pattern_hint_when_infer_fails() {
        let stmt = Statement::Match(MatchExpr {
            value: Expression::Member {
                object: Box::new(Expression::var("p")),
                field: "x".into(),
                args: vec![],
            },
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal("10".into()),
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        });
        let mut lower = HirLower::new(|e| match e {
            Expression::Literal(_) => Ok(e.infer_type().expect("literal infers")),
            _ => Err("checker failed".into()),
        });
        lower.extra_types.insert(
            "p".into(),
            Type::NamedType {
                name: "Point".into(),
            },
        );
        let hir = lower.lower_stmt(&stmt).expect("member match");
        assert!(
            !hir_has_leftover_match(&hir),
            "Member infer_type void would leave Match: {:?}",
            hir
        );
        match hir {
            HirStmt::Scope { body } => {
                let bound = body.iter().find_map(|s| match s {
                    HirStmt::Let { name, value, .. } if name == "__match_s" => Some(value),
                    _ => None,
                });
                match bound {
                    Some(value) => {
                        assert_eq!(value.ty, Type::int(), "{:?}", value);
                        assert!(matches!(value.kind, HirExprKind::Member { .. }));
                    }
                    None => panic!("expected Let __match_s, got {:?}", body),
                }
                assert!(
                    body.iter().any(|s| matches!(s, HirStmt::If { .. })),
                    "expected If after bind, got {:?}",
                    body
                );
            }
            other => panic!("expected Scope, got {:?}", other),
        }
    }

    #[test]
    fn call_for_in_propagates_infer_error_instead_of_void() {
        let stmt = Statement::For(ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::Call {
                function: Box::new(Expression::var("make_xs")),
                args: vec![],
            }),
            body: vec![],
        });
        let mut lower = HirLower::new(|_e| Err("checker failed".into()));
        let result = lower.lower_stmt(&stmt);
        assert!(
            result.is_err(),
            "void placeholder would leftover ForIn, got {:?}",
            result
        );
    }

    #[test]
    fn nested_result_ok_pipeline_has_no_mir_match() {
        let src = r#"
enum Option:
    Some(int)
    None

enum Result:
    Ok(Option)
    Error(str)

fn main() => int:
    let res: Result = Result.Ok(Option.Some(42))
    match res:
        Result.Ok(Option.Some(x)) => x * 2
        Result.Ok(Option.None) => 0
        Result.Error(msg) => -1
    rm res
    return 0
"#;
        let result =
            crate::compiler::CompilationPipeline::new().compile(src, Some("nested_ok.cf"));
        assert!(
            result.errors.is_empty(),
            "frontend errors: {:?}",
            result.errors
        );
        let mir = result
            .hir_fns
            .iter()
            .find(|f| f.name == "main")
            .expect("main MIR must not be dropped");
        assert!(mir.complete, "{:?}", mir);
        assert!(!mir_has_leftover_match(mir), "leftover MirStmt::Match: {:?}", mir);
    }

    #[test]
    fn str_literal_match_pipeline_has_no_mir_match() {
        let src = r#"
fn main() => int:
    let s: str = "a"
    match s:
        "a" => 1
        "b" => 2
        _ => 0
    rm s
    return 0
"#;
        let result =
            crate::compiler::CompilationPipeline::new().compile(src, Some("str_match.cf"));
        assert!(
            result.errors.is_empty(),
            "frontend errors: {:?}",
            result.errors
        );
        let mir = result
            .hir_fns
            .iter()
            .find(|f| f.name == "main")
            .expect("main MIR must not be dropped");
        assert!(mir.complete, "{:?}", mir);
        assert!(!mir_has_leftover_match(mir), "leftover MirStmt::Match: {:?}", mir);
    }

    #[test]
    fn shallow_enum_lower_stmt_is_if() {
        let m = Statement::Match(MatchExpr {
            value: Expression::var("c"),
            arms: vec![
                MatchArm {
                    pattern: Pattern::EnumVariant {
                        enum_name: "Color".into(),
                        variant: "Red".into(),
                        args: vec![],
                    },
                    guard: None,
                    body: vec![],
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: vec![],
                },
            ],
        });
        let mut lower = HirLower::new(|_e| {
            Ok(Type::NamedType {
                name: "Color".into(),
            })
        });
        let hir = lower.lower_stmt(&m).expect("shallow enum");
        let if_stmt = if_in_scope(hir);
        assert!(
            matches!(if_stmt, HirStmt::If { .. }),
            "shallow enum should lower to If, got {:?}",
            if_stmt
        );
    }

    #[test]
    fn or_literals_lower_to_if_or_eq() {
        let stmt = match_cfg::match_arms_to_if(
            int_var_expr(),
            vec![
                (
                    Pattern::Or(vec![
                        Pattern::Literal("1".into()),
                        Pattern::Literal("2".into()),
                    ]),
                    None,
                    vec![],
                ),
                (Pattern::Wildcard, None, vec![]),
            ],
        )
        .expect("or lower");
        let if_stmt = match stmt {
            HirStmt::Scope { body } => body
                .into_iter()
                .find(|s| matches!(s, HirStmt::If { .. }))
                .expect("If inside Scope"),
            other => other,
        };
        match if_stmt {
            HirStmt::If { cond, else_body, .. } => {
                match cond.kind {
                    HirExprKind::Binary { op, .. } => assert_eq!(op, "||"),
                    other => panic!("expected || of equalities, got {:?}", other),
                }
                assert!(else_body.is_some());
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn unknown_local_infer_err_is_err_not_named_type() {
        let mut lower = HirLower::new(|_e| Err("unknown local".into()));
        let err = lower
            .lower_expr(&Expression::var("dropped"))
            .expect_err("unknown local must not become NamedType");
        assert!(
            err.contains("unknown local"),
            "expected checker error, got {err}"
        );
    }

    #[test]
    fn extra_types_types_variable_before_infer() {
        let mut lower = HirLower::new(|_e| Err("checker has no x".into()));
        lower.extra_types.insert("x".into(), Type::float());
        let hir = lower.lower_expr(&Expression::var("x")).expect("extra_types");
        assert_eq!(hir.ty, Type::float());
        assert!(matches!(hir.kind, HirExprKind::Variable(ref n) if n == "x"));
    }

    #[test]
    fn registry_type_name_is_named_type_when_infer_fails() {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Point",
                crate::types::definition::TypeDef::Class {
                    name: "Point".into(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: None,
                },
            )
            .expect("Point");
        let mut lower = HirLower::new(|_e| Err("not a local".into()));
        lower.set_registry(Arc::new(RwLock::new(registry)));
        let hir = lower.lower_expr(&Expression::var("Point")).expect("type name");
        assert_eq!(
            hir.ty,
            Type::NamedType {
                name: "Point".into()
            }
        );
    }

    #[test]
    fn registry_enum_name_is_named_type_when_infer_fails() {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Color",
                crate::types::definition::TypeDef::Enum {
                    name: "Color".into(),
                    variants: vec![],
                    generics: vec![],
                },
            )
            .expect("Color");
        let mut lower = HirLower::new(|_e| Err("not a local".into()));
        lower.set_registry(Arc::new(RwLock::new(registry)));
        let hir = lower.lower_expr(&Expression::var("Color")).expect("enum name");
        assert_eq!(
            hir.ty,
            Type::NamedType {
                name: "Color".into()
            }
        );
    }

    #[test]
    fn failed_call_does_not_become_void_because_extra_types_nonempty() {
        let expr = Expression::Call {
            function: Box::new(Expression::var("mystery")),
            args: vec![],
        };
        let mut lower = HirLower::new(|_e| Err("no type".into()));
        lower.extra_types.insert("x".into(), Type::int());
        let err = lower
            .lower_expr(&expr)
            .expect_err("must not Type::void() just because extra_types is nonempty");
        assert!(err.contains("no type"), "got {err}");
    }

    #[test]
    fn recover_nonvar_type_never_uses_infer_type() {
        let expr = Expression::binary(Expression::var("a"), "+", Expression::var("b"));
        let mut lower = HirLower::new(|_e| Err("checker failed".into()));
        let err = lower
            .lower_expr(&expr)
            .expect_err("Binary must not fall back to infer_type()");
        assert!(err.contains("checker failed"), "got {err}");
    }

    #[test]
    fn let_invalid_type_str_is_err_not_int() {
        let stmt = Statement::VariableDecl(VariableDecl {
            name: "xs".into(),
            var_type: "[int; nope]".into(),
            value: Expression::literal("1"),
        });
        let mut lower = HirLower::new(stub_infer);
        let err = lower
            .lower_stmt(&stmt)
            .expect_err("type_from_str failure must not become int");
        assert!(
            err.to_lowercase().contains("invalid array size"),
            "got {err}"
        );
    }

    #[test]
    fn dump_while_break_mir_layout() {
        let src = r#"
fn main() => int:
    let i: int = 0
    let count: int = 0
    while i < 100:
        i = i + 1
        if i == 10:
            break
        if i == 5:
            break
        count = count + 1
    rm i, count
    return 0
"#;
        let result = crate::compiler::CompilationPipeline::new().compile(src, Some("dump.cf"));
        assert!(
            result.errors.is_empty(),
            "frontend errors: {:?}",
            result.errors
        );
        let mir = result
            .hir_fns
            .iter()
            .find(|f| f.name == "main")
            .expect("main MIR");
        eprintln!(
            "complete={} order={:?}",
            mir.complete,
            mir.codegen_order()
        );
        for (i, b) in mir.blocks.iter().enumerate() {
            eprintln!("bb{i}: {:?}", b.stmts);
        }
        assert!(mir.complete, "expected complete MIR");
    }
}
