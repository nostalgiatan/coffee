//! Lower collection `for x in xs` to range-for MIR (`for i in 0..len` + index).
//! When length cannot be named — not `[T; N]`, `[T]`, or a tuple, and not an
//! array/tuple literal (int/float/bool/str/unit/void/ref/fn/class/enum/variadic
//! — typecheck rejects these) — [`lower_collection_for`] returns `Err` so the
//! function is omitted from `hir_fns` (codegen error: missing MIR).
//! `compile_statement` For already errors if hit. Empty untyped array/tuple
//! literals and empty tuple types cannot infer an element type (`None` →
//! expand `Err`). Empty `[T; 0]` still lowers to `0..0`.
//! Homogeneous tuples lower to `ForRange` 0..N + [`HirExprKind::TupleField`].

use crate::parser::ForLoop;
use crate::types::Type;

use super::expr::{HirExpr, HirExprKind};
use super::stmt::HirStmt;

/// Element type, optional length, and whether to use tuple-field expansion.
pub fn collection_elem_type(coll: &HirExpr) -> Option<(Type, Option<usize>, bool)> {
    match (&coll.ty, &coll.kind) {
        (Type::Array { elem, size }, _) => Some((*elem.clone(), Some(*size), false)),
        (Type::Slice(elem), _) => Some((*elem.clone(), None, false)),
        (Type::Tuple(elems), _) => {
            let elem = elems.first()?.clone();
            Some((elem, Some(elems.len()), true))
        }
        (_, HirExprKind::ArrayLiteral { elements }) => {
            let elem = elements.first()?.ty.clone();
            Some((elem, Some(elements.len()), false))
        }
        (_, HirExprKind::TupleLiteral { elements }) => {
            let elem = elements.first()?.ty.clone();
            Some((elem, Some(elements.len()), true))
        }
        _ => None,
    }
}

/// Expand `for var in coll` into [`HirStmt::ForRange`] when `coll` is `[T; N]`
/// (any expression, bound to a temp if needed), an array literal, a slice
/// (`[T]`, including `for x in foo()`, bound to a temp then `Len`), or a
/// homogeneous [`Type::Tuple`] (bound to a temp if not a variable, then
/// `TupleField` per element — LLVM extractvalue needs a constant index).
pub fn lower_collection_for(
    f: &ForLoop,
    mut coll: HirExpr,
    body: Vec<HirStmt>,
    temp_id: usize,
) -> Result<HirStmt, String> {
    let Some((elem_ty, known_len, tuple)) = collection_elem_type(&coll) else {
        return Err(
            "cannot expand collection for (not array, slice, tuple, or array/tuple literal)"
                .into(),
        );
    };

    if matches!(coll.kind, HirExprKind::ArrayLiteral { .. }) {
        if let Some(size) = known_len {
            coll.ty = Type::Array {
                elem: Box::new(elem_ty.clone()),
                size,
            };
        }
    }

    let (bind, coll) = bind_collection(f, coll, temp_id);
    let ranged = if tuple {
        let n = known_len.expect("tuple for-in has a known length");
        for_range_tuple_fields(f, coll, body, elem_ty, n, temp_id)
    } else {
        let end = match known_len {
            Some(size) => HirExpr {
                ty: Type::int(),
                kind: HirExprKind::Literal(size.to_string()),
            },
            None => HirExpr {
                ty: Type::int(),
                kind: HirExprKind::Len {
                    collection: Box::new(coll.clone()),
                },
            },
        };
        for_range_indexed(f, coll, body, elem_ty, end, temp_id)
    };
    Ok(match bind {
        Some(let_stmt) => HirStmt::Scope {
            body: vec![let_stmt, ranged],
        },
        None => ranged,
    })
}

fn bind_collection(f: &ForLoop, coll: HirExpr, temp_id: usize) -> (Option<HirStmt>, HirExpr) {
    if matches!(coll.kind, HirExprKind::Variable(_)) {
        return (None, coll);
    }
    let name = format!("__forin_coll_{}_{}", f.variable, temp_id);
    let bound = HirExpr {
        ty: coll.ty.clone(),
        kind: HirExprKind::Variable(name.clone()),
    };
    let let_stmt = HirStmt::Let {
        name,
        ty: coll.ty.clone(),
        value: coll,
    };
    (Some(let_stmt), bound)
}

fn for_range_indexed(
    f: &ForLoop,
    coll: HirExpr,
    body: Vec<HirStmt>,
    elem_ty: Type,
    end: HirExpr,
    temp_id: usize,
) -> HirStmt {
    let idx = format!("__forin_{}_{}", f.variable, temp_id);
    let start = HirExpr {
        ty: Type::int(),
        kind: HirExprKind::Literal("0".into()),
    };
    let elem = HirExpr {
        ty: elem_ty.clone(),
        kind: HirExprKind::Index {
            array: Box::new(coll),
            index: Box::new(HirExpr {
                ty: Type::int(),
                kind: HirExprKind::Variable(idx.clone()),
            }),
        },
    };
    let mut range_body = Vec::with_capacity(body.len() + 1);
    range_body.push(HirStmt::Let {
        name: f.variable.clone(),
        ty: elem_ty,
        value: elem,
    });
    range_body.extend(body);
    HirStmt::ForRange {
        var: idx,
        start,
        end,
        body: range_body,
    }
}

fn tuple_field(coll: &HirExpr, index: usize, ty: Type) -> HirExpr {
    HirExpr {
        ty,
        kind: HirExprKind::TupleField {
            tuple: Box::new(coll.clone()),
            index,
        },
    }
}

fn eq_idx(idx: &str, n: usize) -> HirExpr {
    HirExpr {
        ty: Type::bool(),
        kind: HirExprKind::Binary {
            left: Box::new(HirExpr {
                ty: Type::int(),
                kind: HirExprKind::Variable(idx.to_string()),
            }),
            op: "==".into(),
            right: Box::new(HirExpr {
                ty: Type::int(),
                kind: HirExprKind::Literal(n.to_string()),
            }),
        },
    }
}

/// `for i in 0..N` with `let x = t.i` via [`HirExprKind::TupleField`].
/// `extractvalue` needs a constant index, so field 0 is the default and later
/// iterations `Assign` the matching field when `i == k`.
fn for_range_tuple_fields(
    f: &ForLoop,
    coll: HirExpr,
    body: Vec<HirStmt>,
    elem_ty: Type,
    n: usize,
    temp_id: usize,
) -> HirStmt {
    let idx = format!("__forin_{}_{}", f.variable, temp_id);
    let start = HirExpr {
        ty: Type::int(),
        kind: HirExprKind::Literal("0".into()),
    };
    let end = HirExpr {
        ty: Type::int(),
        kind: HirExprKind::Literal(n.to_string()),
    };
    let mut range_body = Vec::new();
    if n > 0 {
        range_body.push(HirStmt::Let {
            name: f.variable.clone(),
            ty: elem_ty.clone(),
            value: tuple_field(&coll, 0, elem_ty.clone()),
        });
        if n > 1 {
            let then_body = vec![HirStmt::Assign {
                name: f.variable.clone(),
                value: tuple_field(&coll, 1, elem_ty.clone()),
            }];
            let elifs = (2..n)
                .map(|i| {
                    (
                        eq_idx(&idx, i),
                        vec![HirStmt::Assign {
                            name: f.variable.clone(),
                            value: tuple_field(&coll, i, elem_ty.clone()),
                        }],
                    )
                })
                .collect();
            range_body.push(HirStmt::If {
                cond: eq_idx(&idx, 1),
                then_body,
                elifs,
                else_body: None,
            });
        }
    }
    range_body.extend(body);
    HirStmt::ForRange {
        var: idx,
        start,
        end,
        body: range_body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::expr::Expression;
    use crate::parser::{ForIterator, ForLoop};

    fn dummy_for() -> ForLoop {
        ForLoop {
            variable: "x".into(),
            iterator: ForIterator::Collection(Expression::var("t")),
            body: vec![],
        }
    }

    fn int_lit(v: &str) -> HirExpr {
        HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Literal(v.into()),
        }
    }

    fn tuple_var(name: &str, n: usize) -> HirExpr {
        HirExpr {
            ty: Type::Tuple(vec![Type::int(); n]),
            kind: HirExprKind::Variable(name.into()),
        }
    }

    fn stmt_has_index(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Let { value, .. } | HirStmt::Assign { value, .. } | HirStmt::Expr(value) => {
                expr_has_index(value)
            }
            HirStmt::Return(Some(value)) | HirStmt::Raise(value) => expr_has_index(value),
            HirStmt::If {
                cond,
                then_body,
                elifs,
                else_body,
            } => {
                expr_has_index(cond)
                    || then_body.iter().any(stmt_has_index)
                    || elifs
                        .iter()
                        .any(|(c, b)| expr_has_index(c) || b.iter().any(stmt_has_index))
                    || else_body
                        .as_ref()
                        .is_some_and(|b| b.iter().any(stmt_has_index))
            }
            HirStmt::While { cond, body } => expr_has_index(cond) || body.iter().any(stmt_has_index),
            HirStmt::ForRange {
                start, end, body, ..
            } => expr_has_index(start) || expr_has_index(end) || body.iter().any(stmt_has_index),
            HirStmt::Scope { body } => body.iter().any(stmt_has_index),
            _ => false,
        }
    }

    fn expr_has_index(e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Index { .. } => true,
            HirExprKind::TupleField { tuple, .. } => expr_has_index(tuple),
            HirExprKind::Binary { left, right, .. } => expr_has_index(left) || expr_has_index(right),
            HirExprKind::Unary { operand, .. }
            | HirExprKind::Len { collection: operand }
            | HirExprKind::EnumTag { value: operand, .. }
            | HirExprKind::Member { object: operand, .. } => expr_has_index(operand),
            HirExprKind::Call { function, args } => {
                expr_has_index(function) || args.iter().any(expr_has_index)
            }
            HirExprKind::ArrayLiteral { elements } | HirExprKind::TupleLiteral { elements } => {
                elements.iter().any(expr_has_index)
            }
            _ => false,
        }
    }

    fn collect_tuple_fields(stmt: &HirStmt, out: &mut Vec<usize>) {
        match stmt {
            HirStmt::Let { value, .. } | HirStmt::Assign { value, .. } | HirStmt::Expr(value) => {
                collect_tuple_fields_expr(value, out);
            }
            HirStmt::Return(Some(value)) | HirStmt::Raise(value) => {
                collect_tuple_fields_expr(value, out);
            }
            HirStmt::If {
                cond,
                then_body,
                elifs,
                else_body,
            } => {
                collect_tuple_fields_expr(cond, out);
                for s in then_body {
                    collect_tuple_fields(s, out);
                }
                for (c, b) in elifs {
                    collect_tuple_fields_expr(c, out);
                    for s in b {
                        collect_tuple_fields(s, out);
                    }
                }
                if let Some(b) = else_body {
                    for s in b {
                        collect_tuple_fields(s, out);
                    }
                }
            }
            HirStmt::While { cond, body } => {
                collect_tuple_fields_expr(cond, out);
                for s in body {
                    collect_tuple_fields(s, out);
                }
            }
            HirStmt::ForRange {
                start, end, body, ..
            } => {
                collect_tuple_fields_expr(start, out);
                collect_tuple_fields_expr(end, out);
                for s in body {
                    collect_tuple_fields(s, out);
                }
            }
            HirStmt::Scope { body } => {
                for s in body {
                    collect_tuple_fields(s, out);
                }
            }
            _ => {}
        }
    }

    fn collect_tuple_fields_expr(e: &HirExpr, out: &mut Vec<usize>) {
        match &e.kind {
            HirExprKind::TupleField { tuple, index } => {
                out.push(*index);
                collect_tuple_fields_expr(tuple, out);
            }
            HirExprKind::Index { array, index } => {
                collect_tuple_fields_expr(array, out);
                collect_tuple_fields_expr(index, out);
            }
            HirExprKind::Binary { left, right, .. } => {
                collect_tuple_fields_expr(left, out);
                collect_tuple_fields_expr(right, out);
            }
            HirExprKind::Unary { operand, .. }
            | HirExprKind::Len { collection: operand }
            | HirExprKind::EnumTag { value: operand, .. }
            | HirExprKind::Member { object: operand, .. } => {
                collect_tuple_fields_expr(operand, out);
            }
            HirExprKind::Call { function, args } => {
                collect_tuple_fields_expr(function, out);
                for a in args {
                    collect_tuple_fields_expr(a, out);
                }
            }
            HirExprKind::ArrayLiteral { elements } | HirExprKind::TupleLiteral { elements } => {
                for a in elements {
                    collect_tuple_fields_expr(a, out);
                }
            }
            _ => {}
        }
    }

    fn as_for_range(stmt: HirStmt) -> HirStmt {
        match stmt {
            HirStmt::Scope { body } => body
                .into_iter()
                .find(|s| matches!(s, HirStmt::ForRange { .. }))
                .expect("ForRange after bind"),
            ranged @ HirStmt::ForRange { .. } => ranged,
            other => panic!("expected Scope/ForRange, got {:?}", other),
        }
    }

    #[test]
    fn tuple_var_for_in_expands_to_for_range_tuple_field() {
        let hir = lower_collection_for(&dummy_for(), tuple_var("t", 3), vec![], 0).unwrap();
        let ranged = as_for_range(hir);
        match &ranged {
            HirStmt::ForRange { end, .. } => {
                assert_eq!(end.kind, HirExprKind::Literal("3".into()), "{:?}", end);
            }
            other => panic!("expected ForRange, got {:?}", other),
        }
        assert!(!stmt_has_index(&ranged), "tuple for-in must not use Index");
        let mut fields = Vec::new();
        collect_tuple_fields(&ranged, &mut fields);
        fields.sort();
        fields.dedup();
        assert_eq!(fields, vec![0, 1, 2], "expected TupleField 0..3, got {:?}", fields);
    }

    #[test]
    fn tuple_literal_for_in_binds_temp_then_tuple_field() {
        let coll = HirExpr {
            ty: Type::Tuple(vec![Type::int(), Type::int()]),
            kind: HirExprKind::TupleLiteral {
                elements: vec![int_lit("1"), int_lit("2")],
            },
        };
        let hir = lower_collection_for(&dummy_for(), coll, vec![], 7).unwrap();
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
                assert!(!stmt_has_index(ranged), "{:?}", ranged);
                let mut fields = Vec::new();
                collect_tuple_fields(ranged, &mut fields);
                fields.sort();
                fields.dedup();
                assert_eq!(fields, vec![0, 1], "{:?}", fields);
            }
            other => panic!("expected Scope with bind+ForRange, got {:?}", other),
        }
    }

    fn expr_has_call(e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Call { .. } => true,
            HirExprKind::Index { array, index } => expr_has_call(array) || expr_has_call(index),
            HirExprKind::Len { collection } | HirExprKind::TupleField { tuple: collection, .. } => {
                expr_has_call(collection)
            }
            HirExprKind::Binary { left, right, .. } => expr_has_call(left) || expr_has_call(right),
            _ => false,
        }
    }

    fn stmt_has_call(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Let { value, .. } | HirStmt::Assign { value, .. } | HirStmt::Expr(value) => {
                expr_has_call(value)
            }
            HirStmt::ForRange {
                start, end, body, ..
            } => expr_has_call(start) || expr_has_call(end) || body.iter().any(stmt_has_call),
            HirStmt::Scope { body } | HirStmt::If { then_body: body, .. } => {
                body.iter().any(stmt_has_call)
            }
            _ => false,
        }
    }

    #[test]
    fn call_collection_binds_once_outside_for_range() {
        let coll = HirExpr {
            ty: Type::Array {
                elem: Box::new(Type::int()),
                size: 2,
            },
            kind: HirExprKind::Call {
                function: Box::new(HirExpr {
                    ty: Type::Function {
                        params: vec![],
                        return_type: Box::new(Type::Array {
                            elem: Box::new(Type::int()),
                            size: 2,
                        }),
                    },
                    kind: HirExprKind::Variable("make_xs".into()),
                }),
                args: vec![],
            },
        };
        let hir = lower_collection_for(&dummy_for(), coll, vec![], 3).unwrap();
        match hir {
            HirStmt::Scope { body } => {
                match body.first() {
                    Some(HirStmt::Let { value, .. }) => {
                        assert!(expr_has_call(value), "temp should hold the call, got {:?}", value);
                    }
                    other => panic!("expected Let temp, got {:?}", other),
                }
                let ranged = body
                    .iter()
                    .find(|s| matches!(s, HirStmt::ForRange { .. }))
                    .expect("ForRange after bind");
                assert!(
                    !stmt_has_call(ranged),
                    "collection Call must not re-run in the loop, got {:?}",
                    ranged
                );
            }
            other => panic!("expected Scope with bind+ForRange, got {:?}", other),
        }
    }

    #[test]
    fn non_tuple_non_array_stays_for_in() {
        let coll = HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Call {
                function: Box::new(HirExpr {
                    ty: Type::int(),
                    kind: HirExprKind::Variable("make_int".into()),
                }),
                args: vec![],
            },
        };
        let err = lower_collection_for(&dummy_for(), coll, vec![], 0)
            .expect_err("non-array/slice/tuple for-in must not emit leftover ForIn");
        assert!(
            !err.is_empty(),
            "expected a lowering error, got empty message"
        );
    }

    fn leftover_coll(ty: Type) -> HirExpr {
        HirExpr {
            ty,
            kind: HirExprKind::Variable("xs".into()),
        }
    }

    #[test]
    fn empty_untyped_array_literal_cannot_infer_elem() {
        let coll = HirExpr {
            ty: Type::int(),
            kind: HirExprKind::ArrayLiteral { elements: vec![] },
        };
        let err = lower_collection_for(&dummy_for(), coll, vec![], 0)
            .expect_err("empty untyped [] must not invent int and 0..0");
        assert!(
            err.contains("cannot expand"),
            "expected cannot-expand error, got {err}"
        );
    }

    #[test]
    fn empty_tuple_type_cannot_infer_elem() {
        let err = lower_collection_for(&dummy_for(), leftover_coll(Type::Tuple(vec![])), vec![], 0)
            .expect_err("empty tuple type must not invent int");
        assert!(
            err.contains("cannot expand"),
            "expected cannot-expand error, got {err}"
        );
    }

    #[test]
    fn empty_tuple_literal_cannot_infer_elem() {
        let coll = HirExpr {
            ty: Type::int(),
            kind: HirExprKind::TupleLiteral { elements: vec![] },
        };
        let err = lower_collection_for(&dummy_for(), coll, vec![], 0)
            .expect_err("empty () must not invent int");
        assert!(
            err.contains("cannot expand"),
            "expected cannot-expand error, got {err}"
        );
    }

    #[test]
    fn empty_typed_array_lowers_to_zero_trip_for_range() {
        let coll = HirExpr {
            ty: Type::Array {
                elem: Box::new(Type::float()),
                size: 0,
            },
            kind: HirExprKind::Variable("xs".into()),
        };
        let hir = lower_collection_for(&dummy_for(), coll, vec![], 0)
            .expect("[T; 0] has a known element type");
        let ranged = as_for_range(hir);
        match ranged {
            HirStmt::ForRange { end, body, .. } => {
                assert_eq!(end.kind, HirExprKind::Literal("0".into()), "{:?}", end);
                match body.first() {
                    Some(HirStmt::Let { ty, .. }) => {
                        assert_eq!(ty, &Type::float(), "must keep [T; 0] elem, not invent int");
                    }
                    other => panic!("expected loop-var let, got {:?}", other),
                }
            }
            other => panic!("expected ForRange, got {:?}", other),
        }
    }

    #[test]
    fn leftover_for_in_is_non_array_slice_tuple() {
        for ty in [
            Type::int(),
            Type::float(),
            Type::bool(),
            Type::string(),
            Type::unit(),
            Type::void(),
            Type::make_ref_mut(Type::int()),
            Type::NamedType { name: "Point".into() },
            Type::Variadic,
            Type::Function {
                params: vec![],
                return_type: Box::new(Type::int()),
            },
        ] {
            let result =
                lower_collection_for(&dummy_for(), leftover_coll(ty.clone()), vec![], 0);
            assert!(
                result.is_err(),
                "expected Err (not leftover ForIn) for {:?}, got {:?}",
                ty,
                result
            );
        }
    }
}
