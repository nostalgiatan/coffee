use crate::hir::expr::{HirExpr, HirExprKind};
use crate::parser::expr::Expression;
use crate::types::Type;

use super::HirLower;

/// Lower one expression whose type is already known, using `rec` for nested nodes.
pub fn lower_expr(
    expr: &Expression,
    ty: Type,
    rec: impl Fn(&Expression) -> Result<HirExpr, String>,
) -> Result<HirExpr, String> {
    if let Expression::Spanned { inner, .. } = expr {
        return lower_expr(inner, ty, rec);
    }
    let kind = match expr {
        Expression::Literal(v) => HirExprKind::Literal(v.clone()),
        Expression::Variable(v) => HirExprKind::Variable(v.clone()),
        Expression::FString {
            template,
            placeholders,
        } => HirExprKind::FString {
            template: template.clone(),
            placeholders: placeholders.clone(),
        },
        Expression::Binary { left, op, right } => HirExprKind::Binary {
            left: Box::new(rec(left)?),
            op: op.clone(),
            right: Box::new(rec(right)?),
        },
        Expression::Unary { op, operand } => HirExprKind::Unary {
            op: op.clone(),
            operand: Box::new(rec(operand)?),
        },
        Expression::Call { function, args } => HirExprKind::Call {
            function: Box::new(rec(function)?),
            args: map_exprs(args, &rec)?,
        },
        Expression::ConstructorCall { class_name, args } => HirExprKind::ConstructorCall {
            class_name: class_name.clone(),
            args: map_exprs(args, &rec)?,
        },
        Expression::Member {
            object,
            field,
            args,
        } => HirExprKind::Member {
            object: Box::new(rec(object)?),
            field: field.clone(),
            args: map_exprs(args, &rec)?,
        },
        Expression::Index { array, index } => HirExprKind::Index {
            array: Box::new(rec(array)?),
            index: Box::new(rec(index)?),
        },
        Expression::ArrayLiteral { elements } => HirExprKind::ArrayLiteral {
            elements: map_exprs(elements, &rec)?,
        },
        Expression::TupleLiteral { elements } => HirExprKind::TupleLiteral {
            elements: map_exprs(elements, &rec)?,
        },
        Expression::StructLiteral {
            struct_name,
            fields,
        } => HirExprKind::StructLiteral {
            struct_name: struct_name.clone(),
            fields: fields
                .iter()
                .map(|(name, e)| Ok((name.clone(), rec(e)?)))
                .collect::<Result<Vec<_>, String>>()?,
        },
        Expression::Assign {
            object,
            field_name,
            value,
        } => HirExprKind::Assign {
            object: Box::new(rec(object)?),
            field_name: field_name.clone(),
            value: Box::new(rec(value)?),
        },
        Expression::TypeCast { target_type, value } => HirExprKind::TypeCast {
            target_type: target_type.clone(),
            value: Box::new(rec(value)?),
        },
        Expression::AnonymousFunction { func } => HirExprKind::AnonymousFunction {
            func: func.clone(),
        },
        Expression::Spanned { .. } => {
            unreachable!("Spanned peeled before match")
        }
    };
    Ok(HirExpr { ty, kind })
}

fn map_exprs(
    exprs: &[Expression],
    rec: &impl Fn(&Expression) -> Result<HirExpr, String>,
) -> Result<Vec<HirExpr>, String> {
    exprs.iter().map(rec).collect()
}

/// Never invent `int`/`void` via `Expression::infer_type()`.
fn recover_nonvar_type(_expr: &Expression, err: String) -> Result<Type, String> {
    Err(err)
}

impl<F> HirLower<F>
where
    F: FnMut(&Expression) -> Result<Type, String>,
{
    pub fn lower_expr(&mut self, expr: &Expression) -> Result<HirExpr, String> {
        let expr = expr.kind();
        if let Some(ctor) = self.try_lower_class_ctor_call(expr)? {
            return Ok(ctor);
        }
        let ty = if let Expression::Variable(name) = expr {
            // Do not use `infer_type` (always `int`): unknown locals must Err.
            if let Some(ty) = self.extra_types.get(name) {
                ty.clone()
            } else {
                match (self.infer)(expr) {
                    Ok(ty) => ty,
                    Err(err) => self
                        .registry_named_type(name)
                        .or_else(|| self.registry_static_callee(name))
                        .ok_or(err)?,
                }
            }
        } else {
            match (self.infer)(expr) {
                Ok(ty) => ty,
                Err(err) => {
                    // Pattern-bound args (`Option.Some(x) => as_int(x)`) are
                    // not in the checker; type the call from the callee.
                    // Do not treat a failed Call as a class ctor here: `as_int`
                    // is a function. Class `TestError("msg")` is rewritten by
                    // `try_lower_class_ctor_call` before `infer(Call)`.
                    if let Expression::Call { function, .. } = expr {
                        if let Some(ret) = self.callee_function_return(function) {
                            ret
                        } else {
                            recover_nonvar_type(expr, err)?
                        }
                    } else if let Some(hir) = self.try_lower_op_from_children(expr)? {
                        return Ok(hir);
                    } else {
                        recover_nonvar_type(expr, err)?
                    }
                }
            }
        };
        self.lower_expr_typed(expr, ty)
    }

    pub(super) fn lower_expr_typed(&mut self, expr: &Expression, ty: Type) -> Result<HirExpr, String> {
        if let Expression::Spanned { inner, .. } = expr {
            return self.lower_expr_typed(inner, ty);
        }
        // Collect children first, then assemble — each child calls `lower_expr`.
        match expr {
            Expression::Literal(_)
            | Expression::Variable(_)
            | Expression::FString { .. }
            | Expression::AnonymousFunction { .. } => {
                lower_expr(expr, ty, |_| {
                    Err("leaf expression has no children".to_string())
                })
            }
            Expression::Binary { left, op, right } => {
                let left = self.lower_expr(left)?;
                let right = self.lower_expr(right)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::Binary {
                        left: Box::new(left),
                        op: op.clone(),
                        right: Box::new(right),
                    },
                })
            }
            Expression::Unary { op, operand } => {
                let operand = self.lower_expr(operand)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::Unary {
                        op: op.clone(),
                        operand: Box::new(operand),
                    },
                })
            }
            Expression::Call { function, args } => {
                if let Some(ctor) = self.try_lower_class_ctor_call(expr)? {
                    let mut ctor = ctor;
                    if !matches!(ty, Type::Void | Type::Unit) {
                        ctor.ty = ty;
                    }
                    return Ok(ctor);
                }
                let function = self.lower_expr(function)?;
                let args = self.lower_expr_list(args)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::Call {
                        function: Box::new(function),
                        args,
                    },
                })
            }
            Expression::ConstructorCall { class_name, args } => {
                let args = self.lower_expr_list(args)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::ConstructorCall {
                        class_name: class_name.clone(),
                        args,
                    },
                })
            }
            Expression::Member {
                object,
                field,
                args,
            } => {
                let object = self.lower_expr(object)?;
                let args = self.lower_expr_list(args)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::Member {
                        object: Box::new(object),
                        field: field.clone(),
                        args,
                    },
                })
            }
            Expression::Index { array, index } => {
                let array = self.lower_expr(array)?;
                let index = self.lower_expr(index)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::Index {
                        array: Box::new(array),
                        index: Box::new(index),
                    },
                })
            }
            Expression::ArrayLiteral { elements } => {
                let elements = self.lower_expr_list(elements)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::ArrayLiteral { elements },
                })
            }
            Expression::TupleLiteral { elements } => {
                let elements = self.lower_expr_list(elements)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::TupleLiteral { elements },
                })
            }
            Expression::StructLiteral {
                struct_name,
                fields,
            } => {
                let mut out = Vec::with_capacity(fields.len());
                for (name, e) in fields {
                    out.push((name.clone(), self.lower_expr(e)?));
                }
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::StructLiteral {
                        struct_name: struct_name.clone(),
                        fields: out,
                    },
                })
            }
            Expression::Assign {
                object,
                field_name,
                value,
            } => {
                let object = self.lower_expr(object)?;
                let value = self.lower_expr(value)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::Assign {
                        object: Box::new(object),
                        field_name: field_name.clone(),
                        value: Box::new(value),
                    },
                })
            }
            Expression::TypeCast { target_type, value } => {
                let value = self.lower_expr(value)?;
                Ok(HirExpr {
                    ty,
                    kind: HirExprKind::TypeCast {
                        target_type: target_type.clone(),
                        value: Box::new(value),
                    },
                })
            }
            Expression::Spanned { .. } => {
                unreachable!("Spanned peeled before match")
            }
        }
    }

    fn lower_expr_list(&mut self, exprs: &[Expression]) -> Result<Vec<HirExpr>, String> {
        let mut out = Vec::with_capacity(exprs.len());
        for e in exprs {
            out.push(self.lower_expr(e)?);
        }
        Ok(out)
    }

    /// `raise Error("msg")` / `raise E("msg")` parse as [`Expression::Call`].
    /// Rewrite only exception types (`Error` and `of Error`) so ordinary
    /// `Point(...)` stays a Call to `new`. Skip `::` / `.` callees.
    fn try_lower_class_ctor_call(
        &mut self,
        expr: &Expression,
    ) -> Result<Option<HirExpr>, String> {
        let Expression::Call { function, args } = expr else {
            return Ok(None);
        };
        let Expression::Variable(name) = function.kind() else {
            return Ok(None);
        };
        if !self.callee_is_ctor_type(name) {
            return Ok(None);
        }
        if matches!((self.infer)(function), Ok(Type::Function { .. })) {
            return Ok(None);
        }
        let args = self.lower_expr_list(args)?;
        Ok(Some(HirExpr {
            ty: Type::NamedType { name: name.clone() },
            kind: HirExprKind::ConstructorCall {
                class_name: name.clone(),
                args,
            },
        }))
    }

    fn callee_is_ctor_type(&self, name: &str) -> bool {
        if name.contains("::") || name.contains('.') {
            return false;
        }
        let Some(reg) = &self.registry else {
            return false;
        };
        let Ok(reg) = reg.read() else {
            return false;
        };
        reg.is_exception_type(name)
    }

    /// Failed `Call`: callee `Function` return type (infer, extra_types, or registry).
    fn callee_function_return(&mut self, function: &Expression) -> Option<Type> {
        let ty = if let Expression::Variable(name) = function.kind() {
            if let Some(ty) = self.extra_types.get(name) {
                Some(ty.clone())
            } else {
                match (self.infer)(function) {
                    Ok(ty) => Some(ty),
                    Err(_) => self
                        .registry_named_type(name)
                        .or_else(|| self.registry_static_callee(name)),
                }
            }
        } else {
            match (self.infer)(function) {
                Ok(ty) => Some(ty),
                Err(_) => None,
            }
        }?;
        match ty {
            Type::Function { return_type, .. } => Some(*return_type),
            _ => None,
        }
    }

    /// When `infer(Binary/Unary)` fails (pattern binds only in `extra_types`),
    /// type the op from lowered children — not `Expression::infer_type()`.
    fn try_lower_op_from_children(&mut self, expr: &Expression) -> Result<Option<HirExpr>, String> {
        match expr {
            Expression::Binary { left, op, right } => {
                let left = self.lower_expr(left)?;
                let right = self.lower_expr(right)?;
                let ty = if matches!(
                    op.as_str(),
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
                ) {
                    Type::bool()
                } else {
                    left.ty.clone()
                };
                Ok(Some(HirExpr {
                    ty,
                    kind: HirExprKind::Binary {
                        left: Box::new(left),
                        op: op.clone(),
                        right: Box::new(right),
                    },
                }))
            }
            Expression::Unary { op, operand } => {
                let operand = self.lower_expr(operand)?;
                Ok(Some(HirExpr {
                    ty: operand.ty.clone(),
                    kind: HirExprKind::Unary {
                        op: op.clone(),
                        operand: Box::new(operand),
                    },
                }))
            }
            Expression::Index { array, index } => {
                let array = self.lower_expr(array)?;
                let index = self.lower_expr(index)?;
                let ty = match &array.ty {
                    Type::Array { elem, .. } | Type::Slice(elem) => (**elem).clone(),
                    Type::Tuple(elems) => match elems.first() {
                        Some(e) => e.clone(),
                        None => return Ok(None),
                    },
                    _ => return Ok(None),
                };
                Ok(Some(HirExpr {
                    ty,
                    kind: HirExprKind::Index {
                        array: Box::new(array),
                        index: Box::new(index),
                    },
                }))
            }
            // Loop-body `let p` is only in `extra_types` (bind snapshot
            // drops scoped locals). `p.x` must type from the object.
            Expression::Member {
                object,
                field,
                args,
            } if args.is_empty() => {
                let object = self.lower_expr(object)?;
                let Type::NamedType { name } = &object.ty else {
                    return Ok(None);
                };
                let Ok(ty) =
                    crate::hir::match_cfg::class_field_type(self.registry.as_ref(), name, field)
                else {
                    return Ok(None);
                };
                Ok(Some(HirExpr {
                    ty,
                    kind: HirExprKind::Member {
                        object: Box::new(object),
                        field: field.clone(),
                        args: Vec::new(),
                    },
                }))
            }
            _ => Ok(None),
        }
    }

    /// Type or enum name confirmed by the registry — not an unknown local.
    fn registry_named_type(&self, name: &str) -> Option<Type> {
        let reg = self.registry.as_ref()?.read().ok()?;
        if let Some(ty) = reg.get_alias(name) {
            return Some(ty);
        }
        if reg.get_type(name).is_some() {
            Some(Type::NamedType {
                name: name.to_string(),
            })
        } else {
            None
        }
    }

    /// `Class::method` / `Enum::Variant` callees are not locals.
    fn registry_static_callee(&self, name: &str) -> Option<Type> {
        let (class, method) = name.split_once("::")?;
        let class = class.trim();
        let method = method.trim();
        if class.is_empty() || method.is_empty() {
            return None;
        }
        let reg = self.registry.as_ref()?.read().ok()?;
        let mut current = Some(class.to_string());
        while let Some(cname) = current {
            match reg.get_type(&cname) {
                Some(crate::types::definition::TypeDef::Class {
                    methods, parent, ..
                }) => {
                    if let Some(m) = methods.iter().find(|m| m.name == method) {
                        return Some(Type::Function {
                            params: m.params.clone(),
                            return_type: Box::new(m.return_type.clone()),
                        });
                    }
                    current = parent;
                }
                Some(crate::types::definition::TypeDef::Enum { .. }) => {
                    return Some(Type::NamedType {
                        name: class.to_string(),
                    });
                }
                _ => return None,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::expr::parse_expression;
    use crate::types::TypeRegistry;
    use std::sync::{Arc, RwLock};

    #[test]
    fn lower_parsed_literal_peels_spanned() {
        let mut lower = HirLower::new(|_e| Ok(Type::int()));
        let hir = lower
            .lower_expr(&parse_expression("1").unwrap())
            .expect("lower parse_expression(\"1\")");
        assert!(
            matches!(hir.kind, HirExprKind::Literal(ref v) if v == "1"),
            "expected Literal, got {:?}",
            hir.kind
        );
    }

    #[test]
    fn extra_types_sees_parsed_variable() {
        let mut lower = HirLower::new(|_e| Err("not in checker".into()));
        lower.extra_types.insert("x".into(), Type::int());
        let hir = lower
            .lower_expr(&parse_expression("x").unwrap())
            .expect("Spanned Variable should use extra_types");
        assert!(matches!(hir.kind, HirExprKind::Variable(ref n) if n == "x"));
        assert_eq!(hir.ty, Type::int());
    }

    #[test]
    fn lower_parsed_error_call_becomes_constructor_call() {
        let mut lower = HirLower::new(|_e| Ok(Type::int()));
        lower.set_registry(Arc::new(RwLock::new(TypeRegistry::root())));
        let hir = lower
            .lower_expr(&parse_expression("Error(\"m\")").unwrap())
            .expect("Error(...) ctor");
        match hir.kind {
            HirExprKind::ConstructorCall { class_name, args } => {
                assert_eq!(class_name, "Error");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected ConstructorCall after peeling Spanned Call, got {:?}", other),
        }
    }
}
