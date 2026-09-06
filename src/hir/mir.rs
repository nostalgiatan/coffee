//! Statement MIR: basic blocks of high-level stmts. Not SSA.

use std::collections::HashSet;

use crate::parser::function::unique_function_key;
use crate::parser::{MemoryOp, Statement};
use super::expr::HirExpr;

/// Nested `fn` / `class` / `enum` / `main` without a cloned parser `Statement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestedKind {
    Function,
    Class,
    Enum,
    Main,
}

/// Source name plus `hir_fns` / LLVM key (renamed nested `fn`s).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedDecl {
    pub kind: NestedKind,
    pub name: String,
    pub hir_key: String,
}

impl NestedDecl {
    /// Nested `fn` keys use [`unique_function_key`] and are recorded in `taken`.
    pub fn from_statement(
        stmt: &Statement,
        parent: Option<&str>,
        taken: &mut HashSet<String>,
    ) -> Self {
        match stmt {
            Statement::Function(f) => {
                let hir_key = unique_function_key(&f.name, parent, |n| taken.contains(n));
                taken.insert(hir_key.clone());
                NestedDecl {
                    kind: NestedKind::Function,
                    name: f.name.clone(),
                    hir_key,
                }
            }
            Statement::Class(c) => NestedDecl {
                kind: NestedKind::Class,
                name: c.name.clone(),
                hir_key: c.name.clone(),
            },
            Statement::Enum(e) => NestedDecl {
                kind: NestedKind::Enum,
                name: e.name.clone(),
                hir_key: e.name.clone(),
            },
            Statement::Main(m) => NestedDecl {
                kind: NestedKind::Main,
                name: m.entry_function.clone(),
                hir_key: m.entry_function.clone(),
            },
            _ => NestedDecl {
                kind: NestedKind::Function,
                name: String::new(),
                hir_key: String::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirFn {
    pub name: String,
    pub blocks: Vec<MirBlock>,
    /// False when lowering dropped a non-comment/import `Unsupported`.
    /// Incomplete MIR is omitted from `hir_fns` (`lower_function` Err);
    /// missing MIR is a hard error (`compile_function` →
    /// `missing MIR for function`). If an incomplete `MirFn` still
    /// appears, `compile_mir_for_function` returns `Ok(false)`.
    pub complete: bool,
    /// Source parameter names (`declare_function` without a nested AST).
    pub param_names: Vec<String>,
    /// Coffee type strings, same order as `param_names`.
    pub param_types: Vec<String>,
    /// Coffee return type string (`void` / `()` / `int` / …).
    pub return_type: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MirBlock {
    pub stmts: Vec<MirStmt>,
}

/// Statement-list MIR. Control flow uses [`MirStmt::Goto`] / [`MirStmt::Branch`], not φ-nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum MirStmt {
    Assign {
        name: String,
        value: HirExpr,
    },
    Expr(HirExpr),
    Goto(usize),
    Branch {
        cond: HirExpr,
        then_bb: usize,
        else_bb: usize,
    },
    Return(Option<HirExpr>),
    Raise(HirExpr),
    /// Nested `fn` / `class` / `enum` / `main`. Codegen uses `compile_mir_nested`.
    Nested(NestedDecl),
    ScopeEnter,
    ScopeExit,
    MemoryOp(MemoryOp),
}

impl MirBlock {
    pub fn terminator(&self) -> Option<&MirStmt> {
        match self.stmts.last() {
            Some(
                s @ (MirStmt::Goto(_)
                | MirStmt::Branch { .. }
                | MirStmt::Return(_)
                | MirStmt::Raise(_)),
            ) => Some(s),
            _ => None,
        }
    }

    pub fn has_terminator(&self) -> bool {
        self.terminator().is_some()
    }

    pub fn successors(&self) -> Vec<usize> {
        match self.terminator() {
            Some(MirStmt::Goto(t)) => vec![*t],
            Some(MirStmt::Branch {
                then_bb, else_bb, ..
            }) => vec![*then_bb, *else_bb],
            Some(MirStmt::Return(_) | MirStmt::Raise(_)) => vec![],
            _ => vec![],
        }
    }
}

impl MirFn {
    /// Codegen order: Kahn on the CFG with DFS back-edges ignored, so `if`/`while`
    /// joins compile after then/elif/body (index order would `rm`/`return` first).
    pub fn codegen_order(&self) -> Vec<usize> {
        let n = self.blocks.len();
        if n == 0 {
            return Vec::new();
        }
        let succs: Vec<Vec<usize>> = self.blocks.iter().map(MirBlock::successors).collect();
        let mut color = vec![0u8; n];
        let mut back = vec![Vec::new(); n];
        fn dfs(u: usize, succs: &[Vec<usize>], color: &mut [u8], back: &mut [Vec<usize>]) {
            color[u] = 1;
            for &v in &succs[u] {
                if v >= color.len() {
                    continue;
                }
                if color[v] == 1 {
                    back[u].push(v);
                } else if color[v] == 0 {
                    dfs(v, succs, color, back);
                }
            }
            color[u] = 2;
        }
        dfs(0, &succs, &mut color, &mut back);

        let mut pred_count = vec![0usize; n];
        for u in 0..n {
            for &v in &succs[u] {
                if v < n && !back[u].contains(&v) {
                    pred_count[v] += 1;
                }
            }
        }
        let mut ready: Vec<usize> = vec![0];
        let mut order = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        while let Some(u) = ready.pop() {
            if seen[u] {
                continue;
            }
            seen[u] = true;
            order.push(u);
            for &v in succs[u].iter().rev() {
                if v >= n || back[u].contains(&v) {
                    continue;
                }
                pred_count[v] = pred_count[v].saturating_sub(1);
                if pred_count[v] == 0 {
                    ready.push(v);
                }
            }
        }
        for i in 0..n {
            if !seen[i] {
                order.push(i);
            }
        }
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::{lower_function, HirLower};
    use crate::parser::expr::Expression;
    use crate::parser::function::{Function, FunctionBody, Parameter};
    use crate::parser::var::ReturnStmt;
    use crate::parser::Statement;
    use crate::types::Type;

    fn stub_infer(_expr: &Expression) -> Result<Type, String> {
        Ok(Type::int())
    }

    #[test]
    fn lower_function_fills_param_and_return_signature() {
        let func = Function {
            type_params: vec![],
            name: "add".into(),
            parameters: vec![
                Parameter {
                    name: "a".into(),
                    param_type: "int".into(),
                    is_variadic: false,
                },
                Parameter {
                    name: "b".into(),
                    param_type: "int".into(),
                    is_variadic: false,
                },
            ],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![Statement::Return(ReturnStmt {
                value: Some(Expression::var("a")),
            })]),
            is_c: false,
        };
        let mut lower = HirLower::new(stub_infer);
        let mir = lower_function(&func, &mut lower).expect("fn mir");
        assert_eq!(mir.param_names, vec!["a", "b"]);
        assert_eq!(mir.param_types, vec!["int", "int"]);
        assert_eq!(mir.return_type, "int");
    }
}
