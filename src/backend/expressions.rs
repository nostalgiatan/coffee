//! Expression compilation for Coffee compiler.
//!
//! Function-body values compile from `HirExpr` (`compile_hir_expr_typed`).
//! Parser `Expression` is not an LLVM path.

use super::codegen::CodeGenerator;
use inkwell::values::BasicValueEnum;
use crate::parser::expr::Expression;

#[path = "expr/mod.rs"]
mod expr;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Dead trap. Function bodies and nested expressions use [`Self::compile_hir_expr_typed`].
    pub fn compile_expr(&mut self, _expr: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        Err(self.error(
            "compile_expr",
            "compile_expr cannot compile parser Expression; use compile_hir_expr_typed",
        ))
    }
}
