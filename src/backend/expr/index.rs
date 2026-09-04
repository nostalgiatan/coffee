//! Array/index expression compilation.

use crate::backend::codegen::CodeGenerator;
use crate::parser::expr::Expression;
use inkwell::values::BasicValueEnum;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_index_expr(&mut self, array: &Expression, index: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        let array_name = match array {
            Expression::Variable(n) => n.clone(),
            other => other.to_string(),
        };
        let index_src = match index {
            Expression::Literal(s) | Expression::Variable(s) => s.clone(),
            other => other.to_string(),
        };
        self.compile_array_index(&array_name, &index_src)
    }
}
