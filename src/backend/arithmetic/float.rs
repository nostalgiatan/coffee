use inkwell::values::{FloatValue, IntValue};
use super::ArithmeticContext;

impl<'ctx> ArithmeticContext<'ctx> {
    fn float_is_inf(
        builder: &inkwell::builder::Builder<'ctx>,
        val: FloatValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, String> {
        let ty = val.get_type();
        let is_pinf = builder
            .build_float_compare(
                inkwell::FloatPredicate::OEQ,
                val,
                ty.const_float(f64::INFINITY),
                &format!("{name}_pinf"),
            )
            .map_err(|e| format!("failed to build +inf check: {}", e))?;
        let is_ninf = builder
            .build_float_compare(
                inkwell::FloatPredicate::OEQ,
                val,
                ty.const_float(f64::NEG_INFINITY),
                &format!("{name}_ninf"),
            )
            .map_err(|e| format!("failed to build -inf check: {}", e))?;
        builder
            .build_or(is_pinf, is_ninf, name)
            .map_err(|e| format!("failed to build inf or: {}", e))
    }

    /// Trap with `unreachable` when a float op yields NaN, or Inf from finite inputs.
    /// Gated on `check_overflow` (default on), matching integer overflow.
    fn emit_float_inf_nan_trap(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
        result: FloatValue<'ctx>,
        overflow_label: &str,
    ) -> Result<FloatValue<'ctx>, String> {
        let llvm_ctx = result.get_type().get_context();
        let is_nan = builder
            .build_float_compare(
                inkwell::FloatPredicate::UNO,
                result,
                result,
                "fovf_nan",
            )
            .map_err(|e| format!("failed to build nan check: {}", e))?;
        let res_inf = Self::float_is_inf(builder, result, "fovf_res_inf")?;
        let left_inf = Self::float_is_inf(builder, left, "fovf_l_inf")?;
        let right_inf = Self::float_is_inf(builder, right, "fovf_r_inf")?;
        let either_inf = builder
            .build_or(left_inf, right_inf, "fovf_in_inf")
            .map_err(|e| format!("failed to build operand inf or: {}", e))?;
        let not_either_inf = builder
            .build_not(either_inf, "fovf_in_finite")
            .map_err(|e| format!("failed to build not inf: {}", e))?;
        let inf_from_finite = builder
            .build_and(res_inf, not_either_inf, "fovf_new_inf")
            .map_err(|e| format!("failed to build inf-from-finite: {}", e))?;
        let trap = builder
            .build_or(is_nan, inf_from_finite, "fovf_trap")
            .map_err(|e| format!("failed to build trap cond: {}", e))?;

        let insert_block = builder.get_insert_block().unwrap();
        let function = insert_block.get_parent().unwrap();
        let result_alloca =
            self.create_entry_block_alloca(builder, result.get_type(), "fovf_result")?;

        let overflow_block = llvm_ctx.append_basic_block(function, overflow_label);
        let normal_block =
            llvm_ctx.append_basic_block(function, &format!("{overflow_label}_ok"));
        let merge_block =
            llvm_ctx.append_basic_block(function, &format!("{overflow_label}_merge"));

        builder
            .build_conditional_branch(trap, overflow_block, normal_block)
            .map_err(|e| format!("failed to build overflow branch: {}", e))?;

        builder.position_at_end(overflow_block);
        builder
            .build_unreachable()
            .map_err(|e| format!("failed to build unreachable on float overflow: {}", e))?;

        builder.position_at_end(normal_block);
        builder
            .build_store(result_alloca, result)
            .map_err(|e| format!("failed to store float result: {}", e))?;
        builder
            .build_unconditional_branch(merge_block)
            .map_err(|e| format!("failed to branch to merge: {}", e))?;

        builder.position_at_end(merge_block);
        let final_result = builder
            .build_load(result.get_type(), result_alloca, "fovf_final")
            .map_err(|e| format!("failed to load float result: {}", e))?
            .into_float_value();
        Ok(final_result)
    }

    /// Build float addition, trapping on Inf/NaN from finite inputs when overflow checks are on.
    pub fn build_float_add(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        let result = builder
            .build_float_add(left, right, "fadd")
            .map_err(|e| format!("failed to build float addition: {}", e))?;
        if self.check_overflow {
            self.emit_float_inf_nan_trap(builder, left, right, result, "fadd_overflow")
        } else {
            Ok(result)
        }
    }

    /// Build float subtraction, trapping on Inf/NaN from finite inputs when overflow checks are on.
    pub fn build_float_sub(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        let result = builder
            .build_float_sub(left, right, "fsub")
            .map_err(|e| format!("failed to build float subtraction: {}", e))?;
        if self.check_overflow {
            self.emit_float_inf_nan_trap(builder, left, right, result, "fsub_overflow")
        } else {
            Ok(result)
        }
    }

    /// Build float multiplication, trapping on Inf/NaN from finite inputs when overflow checks are on.
    pub fn build_float_mul(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        let result = builder
            .build_float_mul(left, right, "fmul")
            .map_err(|e| format!("failed to build float multiplication: {}", e))?;
        if self.check_overflow {
            self.emit_float_inf_nan_trap(builder, left, right, result, "fmul_overflow")
        } else {
            Ok(result)
        }
    }

    /// Build float division. Division by zero traps when `check_div_by_zero` is on.
    /// Inf/NaN from finite inputs traps when `check_overflow` is on.
    pub fn build_float_div(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        if self.check_div_by_zero {
            let is_zero = builder
                .build_float_compare(
                    inkwell::FloatPredicate::OEQ,
                    right,
                    right.get_type().const_zero(),
                    "is_zero",
                )
                .map_err(|e| format!("failed to build zero check: {}", e))?;

            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            let normal_block = context.append_basic_block(function, "fdiv_normal");
            let panic_block = context.append_basic_block(function, "fdiv_panic");
            let merge_block = context.append_basic_block(function, "fdiv_merge");

            builder
                .build_conditional_branch(is_zero, panic_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            builder.position_at_end(panic_block);
            builder
                .build_unreachable()
                .map_err(|e| format!("failed to build unreachable: {}", e))?;

            builder.position_at_end(normal_block);
            let result = builder
                .build_float_div(left, right, "fdiv")
                .map_err(|e| format!("failed to build float division: {}", e))?;
            let result = if self.check_overflow {
                self.emit_float_inf_nan_trap(builder, left, right, result, "fdiv_overflow")?
            } else {
                result
            };
            let checked_block = builder.get_insert_block().unwrap();
            builder
                .build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to branch to merge: {}", e))?;

            builder.position_at_end(merge_block);
            let phi = builder
                .build_phi(result.get_type(), "fdiv_result")
                .map_err(|e| format!("failed to build phi: {}", e))?;
            phi.add_incoming(&[(&result, checked_block)]);

            Ok(phi.as_basic_value().into_float_value())
        } else {
            let result = builder
                .build_float_div(left, right, "fdiv")
                .map_err(|e| format!("failed to build float division: {}", e))?;
            if self.check_overflow {
                self.emit_float_inf_nan_trap(builder, left, right, result, "fdiv_overflow")
            } else {
                Ok(result)
            }
        }
    }

    /// Build float comparison (IEEE unordered predicates; does not trap on NaN operands).
    pub fn build_float_compare(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        pred: inkwell::FloatPredicate,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        builder
            .build_float_compare(pred, left, right, name)
            .map_err(|e| format!("failed to build float comparison: {}", e))
    }
}
