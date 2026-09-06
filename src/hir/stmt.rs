//! HIR statements for a function body. Exotic parser stmts become [`HirStmt::Unsupported`].

use crate::parser::MemoryOp;
use crate::types::Type;

use super::expr::HirExpr;
use super::mir::NestedDecl;

#[derive(Debug, Clone, PartialEq)]
pub enum HirStmt {
    Let {
        name: String,
        ty: Type,
        value: HirExpr,
    },
    Assign {
        name: String,
        value: HirExpr,
    },
    Return(Option<HirExpr>),
    If {
        cond: HirExpr,
        then_body: Vec<HirStmt>,
        elifs: Vec<(HirExpr, Vec<HirStmt>)>,
        else_body: Option<Vec<HirStmt>>,
    },
    While {
        cond: HirExpr,
        body: Vec<HirStmt>,
    },
    /// Range `for var in start..end`. Collection `for` expands here in
    /// `for_in_cfg` when the collection is `[T; N]`, `[T]`, a tuple, or an
    /// array/tuple literal; otherwise lowering returns `Err`. Codegen uses this
    /// MIR form (`ForRange`), not the parser AST.
    ForRange {
        var: String,
        start: HirExpr,
        end: HirExpr,
        body: Vec<HirStmt>,
    },
    /// `raise`. Codegen uses `compile_mir_raise`.
    Raise(HirExpr),
    Expr(HirExpr),
    Break,
    Continue,
    Scope {
        body: Vec<HirStmt>,
    },
    MemoryOp(MemoryOp),
    /// Nested `fn` / `class` / `enum` / `main`. Outer MIR stays complete;
    /// codegen uses `compile_mir_nested`.
    Nested(NestedDecl),
    Unsupported {
        kind: &'static str,
    },
}
