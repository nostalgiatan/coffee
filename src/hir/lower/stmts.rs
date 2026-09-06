use crate::hir::expr::{HirExpr, HirExprKind};
use crate::hir::mir::NestedDecl;
use crate::hir::stmt::HirStmt;
use crate::parser::expr::Expression;
use crate::parser::function::{Function, FunctionBody};
use crate::parser::var::{ReturnStmt, VariableDecl};
use crate::parser::{ForIterator, ForLoop, IfExpr, MatchExpr, Statement, WhileLoop};
use crate::types::{type_from_str, Type};

use super::HirLower;

impl<F> HirLower<F>
where
    F: FnMut(&Expression) -> Result<Type, String>,
{
    pub fn lower_stmt(&mut self, stmt: &Statement) -> Result<HirStmt, String> {
        match stmt {
            Statement::VariableDecl(VariableDecl {
                name,
                var_type,
                value,
            }) => {
                let ty = type_from_str(var_type)?;
                // Annotation types the RHS so `p.x` does not need checker
                // `values` (loop/if lets are popped after bind).
                let mut value = self.lower_expr_typed(value, ty.clone())?;
                value.ty = ty.clone();
                self.extra_types.insert(name.clone(), ty.clone());
                Ok(HirStmt::Let {
                    name: name.clone(),
                    ty,
                    value,
                })
            }
            Statement::Assignment(name, value) => Ok(HirStmt::Assign {
                name: name.clone(),
                value: self.lower_expr(value)?,
            }),
            Statement::Return(ReturnStmt { value }) => {
                let value = match value {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                };
                Ok(HirStmt::Return(value))
            }
            Statement::If(if_expr) => self.lower_if(if_expr),
            Statement::While(w) => self.lower_while(w),
            Statement::Expr(e) => Ok(HirStmt::Expr(self.lower_expr(e)?)),
            Statement::Break(_) => Ok(HirStmt::Break),
            Statement::Continue(_) => Ok(HirStmt::Continue),
            Statement::Import(_) => Ok(HirStmt::Unsupported { kind: "import" }),
            Statement::Function(_)
            | Statement::Main(_)
            | Statement::Class(_)
            | Statement::Enum(_)
            | Statement::TypeDecl(_) => {
                let parent = if self.nested_parent.is_empty() {
                    None
                } else {
                    Some(self.nested_parent.as_str())
                };
                Ok(HirStmt::Nested(NestedDecl::from_statement(
                    stmt,
                    parent,
                    &mut self.nested_taken,
                )))
            }
            Statement::Match(m) => self.lower_match_expr(m),
            Statement::For(f) => self.lower_for(f),
            Statement::SingleLineComment(_) | Statement::MultiLineComment(_) => {
                Ok(HirStmt::Unsupported { kind: "comment" })
            }
            Statement::MemoryOp(op) => Ok(HirStmt::MemoryOp(op.clone())),
            Statement::Raise(r) => Ok(HirStmt::Raise(self.lower_expr(&r.error_expr)?)),
        }
    }

    fn lower_if(&mut self, if_expr: &IfExpr) -> Result<HirStmt, String> {
        let cond = self.lower_expr(&if_expr.condition)?;
        let then_body = self.lower_stmts(&if_expr.body)?;
        let mut elifs = Vec::with_capacity(if_expr.elifs.len());
        for branch in &if_expr.elifs {
            let c = self.lower_expr(&branch.condition)?;
            let body = self.lower_stmts(&branch.body)?;
            elifs.push((c, body));
        }
        let else_body = match &if_expr.else_body {
            Some(stmts) => Some(self.lower_stmts(stmts)?),
            None => None,
        };
        Ok(HirStmt::If {
            cond,
            then_body,
            elifs,
            else_body,
        })
    }

    fn lower_while(&mut self, w: &WhileLoop) -> Result<HirStmt, String> {
        Ok(HirStmt::While {
            cond: self.lower_expr(&w.condition)?,
            body: self.lower_stmts(&w.body)?,
        })
    }

    fn restore_extra_type(&mut self, name: String, prev: Option<Type>) {
        match prev {
            Some(ty) => {
                self.extra_types.insert(name, ty);
            }
            None => {
                self.extra_types.remove(&name);
            }
        }
    }

    fn with_extra_type<R>(
        &mut self,
        name: String,
        ty: Type,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let prev = self.extra_types.insert(name.clone(), ty);
        let out = f(self);
        self.restore_extra_type(name, prev);
        out
    }

    fn lower_for(&mut self, f: &ForLoop) -> Result<HirStmt, String> {
        match &f.iterator {
            ForIterator::Range { start, end } => {
                let start = self.lower_expr(start)?;
                let end = self.lower_expr(end)?;
                let body = self.with_extra_type(f.variable.clone(), Type::int(), |this| {
                    this.lower_stmts(&f.body)
                });
                Ok(HirStmt::ForRange {
                    var: f.variable.clone(),
                    start,
                    end,
                    body: body?,
                })
            }
            ForIterator::Collection(coll) => {
                let coll_e = self.lower_expr(coll)?;
                let elem_ty = match crate::hir::for_in_cfg::collection_elem_type(&coll_e) {
                    Some((elem_ty, _, _)) => elem_ty,
                    None => {
                        return Err(
                            "cannot expand collection for (not array, slice, tuple, or array/tuple literal)"
                                .into(),
                        );
                    }
                };
                let body = self.with_extra_type(f.variable.clone(), elem_ty, |this| {
                    this.lower_stmts(&f.body)
                })?;
                let temp_id = self.for_in_temps;
                self.for_in_temps += 1;
                // `lower_collection_for` binds non-variable collections (array
                // literals / `foo()`) after reading `ArrayLiteral` kind / array type.
                crate::hir::for_in_cfg::lower_collection_for(f, coll_e, body, temp_id)
            }
        }
    }

    fn lower_match_expr(&mut self, m: &MatchExpr) -> Result<HirStmt, String> {
        let hint = crate::hir::match_cfg::scrutinee_ty_from_patterns(m);
        let mut value = match self.lower_expr(&m.value) {
            Ok(v) => v,
            Err(e) => {
                let Some(ty) = hint.clone() else {
                    return Err(e);
                };
                self.lower_expr_typed(&m.value, ty)?
            }
        };
        if let Some(ty) = hint {
            if !crate::hir::match_cfg::match_is_simple(m, &value.ty)
                && crate::hir::match_cfg::match_is_simple(m, &ty)
            {
                value.ty = ty;
            }
        }
        if !crate::hir::match_cfg::match_is_simple(m, &value.ty) {
            return Err(
                "cannot lower match to If (scrutinee/patterns are not simple)".into(),
            );
        }
        let mut arms = Vec::with_capacity(m.arms.len());
        for arm in &m.arms {
            let binds = crate::hir::match_cfg::pattern_binding_types(
                &arm.pattern,
                &value.ty,
                self.registry.as_ref(),
            )?;
            let mut saved = Vec::with_capacity(binds.len());
            for (name, ty) in binds {
                saved.push((name.clone(), self.extra_types.insert(name, ty)));
            }
            let lowered = (|| {
                let guard = match &arm.guard {
                    Some(g) => Some(self.lower_expr(g)?),
                    None => None,
                };
                let body = self.lower_stmts(&arm.body)?;
                Ok::<_, String>((guard, body))
            })();
            for (name, prev) in saved {
                self.restore_extra_type(name, prev);
            }
            let (guard, body) = lowered?;
            arms.push((arm.pattern.clone(), guard, body));
        }
        crate::hir::match_cfg::match_arms_to_if_with_registry(
            value,
            arms,
            self.registry.clone(),
        )
    }

    pub fn lower_stmts(&mut self, stmts: &[Statement]) -> Result<Vec<HirStmt>, String> {
        // Bind this scope's `let`s for the whole list (and nested if/while/for
        // bodies), then restore after the scope so they do not leak.
        let mut saved = Vec::new();
        for s in stmts {
            if let Statement::VariableDecl(VariableDecl { name, var_type, .. }) = s {
                let ty = type_from_str(var_type)?;
                saved.push((name.clone(), self.extra_types.insert(name.clone(), ty)));
            }
        }
        let result: Result<Vec<HirStmt>, String> =
            stmts.iter().map(|s| self.lower_stmt(s)).collect();
        for (name, prev) in saved.into_iter().rev() {
            self.restore_extra_type(name, prev);
        }
        result
    }

    /// Lower a function body to HIR statements (expression bodies become `return`).
    pub fn lower_function_body(&mut self, func: &Function) -> Result<Vec<HirStmt>, String> {
        let mut saved = Vec::new();
        for p in &func.parameters {
            if p.name.is_empty() {
                continue;
            }
            let ty = type_from_str(&p.param_type)?;
            saved.push((p.name.clone(), self.extra_types.insert(p.name.clone(), ty)));
        }
        let result = match &func.body {
            FunctionBody::Block(stmts) => self.lower_stmts(stmts),
            FunctionBody::Expression(e) => Ok(vec![HirStmt::Return(Some(self.lower_expr(e)?))]),
            FunctionBody::External => Ok(vec![]),
        };
        for (name, prev) in saved.into_iter().rev() {
            self.restore_extra_type(name, prev);
        }
        result
    }
}
