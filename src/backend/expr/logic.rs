//! Short-circuit `&&` / `||`: compile left, branch, compile right only on the
//! taken path, merge with a bool phi. Bitwise `&` / `|` stay in `build_binary_op`.

use crate::backend::codegen::CodeGenerator;
use crate::backend::control_flow::value_to_bool;
use inkwell::values::BasicValueEnum;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// CFG for logical `&&` / `||`. `compile_right` runs only in the rhs block.
    pub(crate) fn compile_short_circuit_logic<F>(
        &mut self,
        op: &str,
        left: BasicValueEnum<'ctx>,
        compile_right: F,
    ) -> Result<BasicValueEnum<'ctx>, String>
    where
        F: FnOnce(&mut Self) -> Result<BasicValueEnum<'ctx>, String>,
    {
        let is_and = match op {
            "&&" => true,
            "||" => false,
            _ => {
                return Err(self.error(
                    "compile_expression",
                    format!("internal: compile_short_circuit_logic got '{}'", op),
                ))
            }
        };

        let lhs_bool = value_to_bool(left, &self.backend.builder)
            .map_err(|e| self.error("compile_expression", e))?;
        let lhs_block = self.backend.builder.get_insert_block().ok_or_else(|| {
            self.error(
                "compile_expression",
                "short-circuit &&/|| with no insert block",
            )
        })?;
        let function = lhs_block.get_parent().ok_or_else(|| {
            self.error(
                "compile_expression",
                "short-circuit &&/|| outside a function",
            )
        })?;

        let rhs_block = self
            .backend
            .context
            .append_basic_block(function, "logic_rhs");
        let merge_block = self
            .backend
            .context
            .append_basic_block(function, "logic_merge");

        if is_and {
            self.backend
                .builder
                .build_conditional_branch(lhs_bool, rhs_block, merge_block)
        } else {
            self.backend
                .builder
                .build_conditional_branch(lhs_bool, merge_block, rhs_block)
        }
        .map_err(|e| {
            self.error(
                "compile_expression",
                format!("failed to branch for {}: {}", op, e),
            )
        })?;

        self.backend.builder.position_at_end(rhs_block);
        let right = compile_right(self)?;
        let rhs_bool = value_to_bool(right, &self.backend.builder)
            .map_err(|e| self.error("compile_expression", e))?;
        let rhs_end = self.backend.builder.get_insert_block().ok_or_else(|| {
            self.error(
                "compile_expression",
                "short-circuit right-hand side lost insert block",
            )
        })?;
        self.backend
            .builder
            .build_unconditional_branch(merge_block)
            .map_err(|e| {
                self.error(
                    "compile_expression",
                    format!("failed to branch to merge after {}: {}", op, e),
                )
            })?;

        self.backend.builder.position_at_end(merge_block);
        let i1 = self.backend.context.bool_type();
        let short_val = if is_and {
            i1.const_zero()
        } else {
            i1.const_int(1, false)
        };
        let phi = self
            .backend
            .builder
            .build_phi(i1, "logic")
            .map_err(|e| {
                self.error(
                    "compile_expression",
                    format!("failed to build {} phi: {}", op, e),
                )
            })?;
        phi.add_incoming(&[(&short_val, lhs_block), (&rhs_bool, rhs_end)]);

        let as_bool = self
            .backend
            .builder
            .build_int_z_extend(
                phi.as_basic_value().into_int_value(),
                self.backend.context.i8_type(),
                "logic_bool",
            )
            .map_err(|e| {
                self.error(
                    "compile_expression",
                    format!("failed to extend {} result: {}", op, e),
                )
            })?;
        Ok(as_bool.into())
    }
}
