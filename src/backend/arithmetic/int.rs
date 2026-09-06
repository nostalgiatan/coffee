use inkwell::values::{IntValue, BasicValueEnum};
use inkwell::types::IntType;
use super::ArithmeticContext;

fn signed_int_min<'ctx>(ty: IntType<'ctx>) -> IntValue<'ctx> {
    let bits = ty.get_bit_width();
    if bits == 0 {
        return ty.const_zero();
    }
    if bits <= 64 {
        return ty.const_int(1u64 << (bits - 1), true);
    }
    let nwords = ((bits + 63) / 64) as usize;
    let mut words = vec![0u64; nwords];
    let shift = (bits - 1) % 64;
    words[nwords - 1] = 1u64 << shift;
    ty.const_int_arbitrary_precision(&words)
}

impl<'ctx> ArithmeticContext<'ctx> {
    fn is_signed_div_overflow(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let ty = left.get_type();
        let int_min = signed_int_min(ty);
        let neg_one = ty.const_all_ones();
        let is_int_min = builder
            .build_int_compare(inkwell::IntPredicate::EQ, left, int_min, "is_int_min")
            .map_err(|e| format!("failed to build INT_MIN check: {}", e))?;
        let is_neg_one = builder
            .build_int_compare(inkwell::IntPredicate::EQ, right, neg_one, "is_neg_one")
            .map_err(|e| format!("failed to build -1 check: {}", e))?;
        builder
            .build_and(is_int_min, is_neg_one, "signed_div_overflow")
            .map_err(|e| format!("failed to build and: {}", e))
    }

    fn emit_signed_div_overflow_trap(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        overflow_label: &str,
        continue_label: &str,
    ) -> Result<(), String> {
        let overflow = self.is_signed_div_overflow(builder, left, right)?;
        let insert_block = builder.get_insert_block().unwrap();
        let function = insert_block.get_parent().unwrap();
        let overflow_block = context.append_basic_block(function, overflow_label);
        let continue_block = context.append_basic_block(function, continue_label);
        builder
            .build_conditional_branch(overflow, overflow_block, continue_block)
            .map_err(|e| format!("failed to build branch: {}", e))?;
        builder.position_at_end(overflow_block);
        builder
            .build_unreachable()
            .map_err(|e| format!("failed to build unreachable on signed div overflow: {}", e))?;
        builder.position_at_end(continue_block);
        Ok(())
    }

    /// Build integer addition with optional overflow checking
    /// 
    /// This method generates LLVM IR for integer addition with optional overflow
    /// detection. When overflow checking is enabled, it performs signed integer
    /// overflow detection by checking if the signs of the operands are the same
    /// but the result has a different sign, which indicates an overflow occurred.
    /// 
    /// The method creates appropriate control flow to handle overflow conditions
    /// safely. When an overflow is detected, it generates an unreachable instruction
    /// to trap execution rather than returning an incorrect result.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand for the addition
    /// * `right` - The right operand for the addition
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The result of the addition
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_int_add(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        signed: bool,
    ) -> Result<IntValue<'ctx>, String> {
        if self.check_overflow && signed {
            // Perform addition and check for overflow
            // For signed integers, overflow occurs if:
            // - (left > 0 and right > 0 and result < 0) OR
            // - (left < 0 and right < 0 and result > 0)
            let result = builder.build_int_add(left, right, "add")
                .map_err(|e| format!("failed to build integer addition: {}", e))?;

            // Check if left and right have the same sign
            let left_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                left,
                left.get_type().const_zero(),
                "left_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let right_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                right,
                right.get_type().const_zero(),
                "right_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let result_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                result,
                result.get_type().const_zero(),
                "result_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Same sign: (left_neg == right_neg)
            let same_sign = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                left_neg,
                right_neg,
                "same_sign"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Overflow if signs are different but operands had same sign
            let sign_changed = builder.build_int_compare(
                inkwell::IntPredicate::NE,
                left_neg,
                result_neg,
                "sign_changed"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Overflow occurred: same_sign AND sign_changed
            let overflow = builder.build_and(same_sign, sign_changed, "overflow")
                .map_err(|e| format!("failed to build and: {}", e))?;

            // Create basic blocks for handling overflow
            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            // Create alloca in entry block for result (must dominate all uses)
            let result_alloca = self.create_entry_block_alloca(builder, left.get_type(), "add_result")?;

            let overflow_block = context.append_basic_block(function, "add_overflow");
            let normal_block = context.append_basic_block(function, "add_normal");
            let merge_block = context.append_basic_block(function, "add_merge");

            // Check if overflow occurred
            builder.build_conditional_branch(overflow, overflow_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Overflow block: panic instead of returning 0 (CRITICAL-2 fix)
            builder.position_at_end(overflow_block);

            // Security: Panic on overflow instead of returning unsafe value
            // Use LLVM's unreachable instruction to trap
            // This will trigger undefined behavior sanitizers at runtime
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable on overflow: {}", e))?;

            // Normal block: return the result
            builder.position_at_end(normal_block);
            builder.build_store(result_alloca, result)
                .map_err(|e| format!("failed to store: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Merge block: load the result
            builder.position_at_end(merge_block);
            let final_result = builder.build_load(left.get_type(), result_alloca, "add_final")
                .map_err(|e| format!("failed to load: {}", e))?
                .into_int_value();

            Ok(final_result)
        } else {
            builder.build_int_add(left, right, "add")
                .map_err(|e| format!("failed to build integer addition: {}", e))
        }
    }

    /// Build integer subtraction with optional overflow checking
    /// 
    /// This method generates LLVM IR for integer subtraction with optional overflow
    /// detection. When overflow checking is enabled, it detects signed integer
    /// overflow by checking if the operands have different signs and the result
    /// has an unexpected sign, which indicates an overflow occurred.
    /// 
    /// The method creates appropriate control flow to handle overflow conditions
    /// safely. When an overflow is detected, it generates an unreachable instruction
    /// to trap execution rather than returning an incorrect result.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand (minuend) for the subtraction
    /// * `right` - The right operand (subtrahend) for the subtraction
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The result of the subtraction
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_int_sub(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        signed: bool,
    ) -> Result<IntValue<'ctx>, String> {
        if self.check_overflow && signed {
            // Perform subtraction and check for overflow
            // For signed integers, overflow occurs if:
            // - (left >= 0 and right < 0 and result < 0) OR
            // - (left < 0 and right >= 0 and result >= 0)
            let result = builder.build_int_sub(left, right, "sub")
                .map_err(|e| format!("failed to build integer subtraction: {}", e))?;

            // Check signs
            let left_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                left,
                left.get_type().const_zero(),
                "left_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let right_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                right,
                right.get_type().const_zero(),
                "right_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let result_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                result,
                result.get_type().const_zero(),
                "result_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Different signs: (left_neg != right_neg)
            let diff_sign = builder.build_int_compare(
                inkwell::IntPredicate::NE,
                left_neg,
                right_neg,
                "diff_sign"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Overflow if signs are the same but operands had different signs
            let sign_same = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                left_neg,
                result_neg,
                "sign_same"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Overflow occurred: diff_sign AND sign_same
            let overflow = builder.build_and(diff_sign, sign_same, "overflow")
                .map_err(|e| format!("failed to build and: {}", e))?;

            // Create basic blocks for handling overflow
            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            // Create alloca in entry block for result
            let result_alloca = self.create_entry_block_alloca(builder, left.get_type(), "sub_result")?;

            let overflow_block = context.append_basic_block(function, "sub_overflow");
            let normal_block = context.append_basic_block(function, "sub_normal");
            let merge_block = context.append_basic_block(function, "sub_merge");

            // Check if overflow occurred
            builder.build_conditional_branch(overflow, overflow_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Overflow block: panic instead of returning 0
            builder.position_at_end(overflow_block);
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable on overflow: {}", e))?;

            // Normal block: return the result
            builder.position_at_end(normal_block);
            builder.build_store(result_alloca, result)
                .map_err(|e| format!("failed to store: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Merge block: load the result
            builder.position_at_end(merge_block);
            let final_result = builder.build_load(left.get_type(), result_alloca, "sub_final")
                .map_err(|e| format!("failed to load: {}", e))?
                .into_int_value();

            Ok(final_result)
        } else {
            builder.build_int_sub(left, right, "sub")
                .map_err(|e| format!("failed to build integer subtraction: {}", e))
        }
    }

    /// Build integer multiplication with optional overflow checking
    /// 
    /// This method generates LLVM IR for integer multiplication with optional overflow
    /// detection. When overflow checking is enabled, it detects signed integer
    /// overflow by checking if the sign of the result is inconsistent with the
    /// expected sign based on the signs of the operands (the result should be
    /// negative only if exactly one operand is negative).
    /// 
    /// The method handles special cases where either operand is zero, as these
    /// cannot cause overflow. When overflow is detected, it generates an unreachable
    /// instruction to trap execution rather than returning an incorrect result.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand (multiplicand) for the multiplication
    /// * `right` - The right operand (multiplier) for the multiplication
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The result of the multiplication
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_int_mul(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        signed: bool,
    ) -> Result<IntValue<'ctx>, String> {
        if self.check_overflow && signed {
            // For multiplication, check sign consistency
            // Overflow if: left != 0, right != 0, but result sign != (left sign XOR right sign)
            let result = builder.build_int_mul(left, right, "mul")
                .map_err(|e| format!("failed to build integer multiplication: {}", e))?;

            // Check signs
            let left_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                left,
                left.get_type().const_zero(),
                "left_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let right_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                right,
                right.get_type().const_zero(),
                "right_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let result_neg = builder.build_int_compare(
                inkwell::IntPredicate::SLT,
                result,
                result.get_type().const_zero(),
                "result_neg"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Expected result sign = left_neg XOR right_neg
            let expected_neg = builder.build_xor(left_neg, right_neg, "expected_neg")
                .map_err(|e| format!("failed to build xor: {}", e))?;

            // Overflow if expected sign != actual sign AND neither operand is zero
            let sign_mismatch = builder.build_int_compare(
                inkwell::IntPredicate::NE,
                expected_neg,
                result_neg,
                "sign_mismatch"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            // Check if either operand is zero (no overflow possible)
            let left_zero = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                left,
                left.get_type().const_zero(),
                "left_zero"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let right_zero = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                right,
                right.get_type().const_zero(),
                "right_zero"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let either_zero = builder.build_or(left_zero, right_zero, "either_zero")
                .map_err(|e| format!("failed to build or: {}", e))?;

            // Overflow when: sign_mismatch AND NOT either_zero
            let no_zero = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                either_zero,
                either_zero.get_type().const_zero(),
                "no_zero"
            ).map_err(|e| format!("failed to build compare: {}", e))?;

            let overflow = builder.build_and(sign_mismatch, no_zero, "overflow")
                .map_err(|e| format!("failed to build and: {}", e))?;

            // Create basic blocks for handling overflow
            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            // Create alloca in entry block for result
            let result_alloca = self.create_entry_block_alloca(builder, left.get_type(), "mul_result")?;

            let overflow_block = context.append_basic_block(function, "mul_overflow");
            let normal_block = context.append_basic_block(function, "mul_normal");
            let merge_block = context.append_basic_block(function, "mul_merge");

            // Check if overflow occurred
            builder.build_conditional_branch(overflow, overflow_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Overflow block: panic instead of returning 0
            builder.position_at_end(overflow_block);
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable on overflow: {}", e))?;

            // Normal block: return the result
            builder.position_at_end(normal_block);
            builder.build_store(result_alloca, result)
                .map_err(|e| format!("failed to store: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Merge block: load the result
            builder.position_at_end(merge_block);
            let final_result = builder.build_load(left.get_type(), result_alloca, "mul_final")
                .map_err(|e| format!("failed to load: {}", e))?
                .into_int_value();

            Ok(final_result)
        } else {
            builder.build_int_mul(left, right, "mul")
                .map_err(|e| format!("failed to build integer multiplication: {}", e))
        }
    }

    /// Build integer division with zero checking
    /// 
    /// This method generates LLVM IR for integer division with division-by-zero
    /// detection. When division-by-zero checking is enabled, it checks if the
    /// divisor (right operand) is zero before performing the division.
    /// 
    /// If division-by-zero is detected, the method calls the configured panic
    /// function with an appropriate error message and then generates an
    /// unreachable instruction to halt execution. Otherwise, it performs the
    /// signed integer division operation.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand (dividend) for the division
    /// * `right` - The right operand (divisor) for the division
    /// 
    /// # Returns
    /// 
    /// * `Ok(BasicValueEnum)` - The result of the division
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_int_div(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        signed: bool,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if self.check_div_by_zero {
            let zero = right.get_type().const_zero();
            let is_zero = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                right,
                zero,
                "is_zero"
            ).map_err(|e| format!("failed to build zero check: {}", e))?;

            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            let after_zero_block = context.append_basic_block(function, "div_after_zero");
            let panic_block = context.append_basic_block(function, "div_panic");
            let merge_block = context.append_basic_block(function, "div_merge");

            builder.build_conditional_branch(is_zero, panic_block, after_zero_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            builder.position_at_end(panic_block);
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable: {}", e))?;

            builder.position_at_end(after_zero_block);
            if signed {
                self.emit_signed_div_overflow_trap(
                    context,
                    builder,
                    left,
                    right,
                    "div_overflow",
                    "div_normal",
                )?;
            }
            let result_block = builder.get_insert_block().unwrap();
            let result = if signed {
                builder.build_int_signed_div(left, right, "div")
                    .map_err(|e| format!("failed to build integer division: {}", e))?
            } else {
                builder.build_int_unsigned_div(left, right, "div")
                    .map_err(|e| format!("failed to build integer division: {}", e))?
            };
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to branch to merge: {}", e))?;

            builder.position_at_end(merge_block);
            let phi = builder.build_phi(result.get_type(), "div_result")
                .map_err(|e| format!("failed to build phi: {}", e))?;
            phi.add_incoming(&[(&result, result_block)]);

            Ok(phi.as_basic_value())
        } else if signed {
            self.emit_signed_div_overflow_trap(
                context,
                builder,
                left,
                right,
                "div_overflow",
                "div_normal",
            )?;
            builder.build_int_signed_div(left, right, "div")
                .map_err(|e| format!("failed to build integer division: {}", e))
                .map(|v| v.into())
        } else {
            builder.build_int_unsigned_div(left, right, "div")
                .map_err(|e| format!("failed to build integer division: {}", e))
                .map(|v| v.into())
        }
    }

    /// Build integer modulo with zero checking
    /// 
    /// This method generates LLVM IR for integer modulo (remainder) operation
    /// with modulo-by-zero detection. When modulo-by-zero checking is enabled,
    /// it checks if the divisor (right operand) is zero before performing the operation.
    /// 
    /// If modulo-by-zero is detected, the method generates an unreachable
    /// instruction to halt execution, preventing undefined behavior. Otherwise,
    /// it performs the signed integer remainder operation.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand (dividend) for the modulo operation
    /// * `right` - The right operand (divisor) for the modulo operation
    /// 
    /// # Returns
    /// 
    /// * `Ok(BasicValueEnum)` - The result of the modulo operation
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_int_mod(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        signed: bool,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if self.check_mod_by_zero {
            let zero = right.get_type().const_zero();
            let is_zero = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                right,
                zero,
                "is_zero"
            ).map_err(|e| format!("failed to build zero check: {}", e))?;

            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            let after_zero_block = context.append_basic_block(function, "mod_after_zero");
            let panic_block = context.append_basic_block(function, "mod_panic");
            let merge_block = context.append_basic_block(function, "mod_merge");

            builder.build_conditional_branch(is_zero, panic_block, after_zero_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            builder.position_at_end(panic_block);
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable: {}", e))?;

            builder.position_at_end(after_zero_block);
            if signed {
                self.emit_signed_div_overflow_trap(
                    context,
                    builder,
                    left,
                    right,
                    "mod_overflow",
                    "mod_normal",
                )?;
            }
            let result_block = builder.get_insert_block().unwrap();
            let result = if signed {
                builder.build_int_signed_rem(left, right, "rem")
                    .map_err(|e| format!("failed to build integer modulo: {}", e))?
            } else {
                builder.build_int_unsigned_rem(left, right, "rem")
                    .map_err(|e| format!("failed to build integer modulo: {}", e))?
            };
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to branch to merge: {}", e))?;

            builder.position_at_end(merge_block);
            let phi = builder.build_phi(result.get_type(), "mod_result")
                .map_err(|e| format!("failed to build phi: {}", e))?;
            phi.add_incoming(&[(&result, result_block)]);

            Ok(phi.as_basic_value())
        } else if signed {
            self.emit_signed_div_overflow_trap(
                context,
                builder,
                left,
                right,
                "mod_overflow",
                "mod_normal",
            )?;
            Ok(builder.build_int_signed_rem(left, right, "rem")
                .map_err(|e| format!("failed to build integer modulo: {}", e))?.into())
        } else {
            Ok(builder.build_int_unsigned_rem(left, right, "rem")
                .map_err(|e| format!("failed to build integer modulo: {}", e))?.into())
        }
    }

    /// Build integer comparison
    /// 
    /// This method generates LLVM IR for integer comparison operations using
    /// the specified predicate. It performs a comparison between two integer
    /// values and returns a boolean result (i1 type) indicating whether the
    /// comparison is true or false.
    /// 
    /// The method supports all standard integer comparison predicates such as
    /// equality, inequality, less than, greater than, etc. It does not perform
    /// any additional safety checks beyond the basic comparison operation.
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `pred` - The integer predicate to use for comparison (e.g., EQ, NE, SLT, SGT)
    /// * `left` - The left operand for the comparison
    /// * `right` - The right operand for the comparison
    /// * `name` - The name for the resulting comparison instruction
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The boolean result of the comparison (i1 type)
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_int_compare(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        pred: inkwell::IntPredicate,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, String> {
        builder.build_int_compare(pred, left, right, name)
            .map_err(|e| format!("failed to build integer comparison: {}", e))
    }

    pub fn build_int_neg(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        value: IntValue<'ctx>,
        signed: bool,
    ) -> Result<IntValue<'ctx>, String> {
        // CRITICAL FIX: Check if current block already has terminator
        let insert_block = builder.get_insert_block()
            .ok_or_else(|| "no current insert block".to_string())?;
        if insert_block.get_terminator().is_some() {
            // Block already terminated, this is dead code
            // Return simple negation without overflow checks
            return builder.build_int_neg(value, "neg")
                .map_err(|e| format!("failed to build integer negation: {}", e));
        }

        if self.check_overflow && signed {
            // For signed integers, negating INT_MIN causes overflow
            // INT_MIN = -2^(n-1), -INT_MIN = 2^(n-1) which exceeds INT_MAX = 2^(n-1) - 1

            let int_min = signed_int_min(value.get_type());

            // Check if value == INT_MIN
            let is_int_min = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                value,
                int_min,
                "is_int_min"
            ).map_err(|e| format!("failed to build INT_MIN check: {}", e))?;

            // Create basic blocks
            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            // Create alloca in entry block for result (must dominate all uses)
            let result_alloca = self.create_entry_block_alloca(builder, value.get_type(), "neg_result")?;

            let overflow_block = context.append_basic_block(function, "neg_overflow");
            let normal_block = context.append_basic_block(function, "neg_normal");
            let merge_block = context.append_basic_block(function, "neg_merge");

            // Branch based on INT_MIN check
            builder.build_conditional_branch(is_int_min, overflow_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Overflow block: panic on INT_MIN negation
            builder.position_at_end(overflow_block);
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable on negation overflow: {}", e))?;

            // Normal block: perform negation and store
            builder.position_at_end(normal_block);
            let result = builder.build_int_neg(value, "neg")
                .map_err(|e| format!("failed to build integer negation: {}", e))?;
            builder.build_store(result_alloca, result)
                .map_err(|e| format!("failed to store: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Merge block: load the result
            builder.position_at_end(merge_block);
            let final_result = builder.build_load(value.get_type(), result_alloca, "neg_final")
                .map_err(|e| format!("failed to load: {}", e))?
                .into_int_value();

            Ok(final_result)
        } else {
            builder.build_int_neg(value, "neg")
                .map_err(|e| format!("failed to build integer negation: {}", e))
        }
    }

    /// Build left shift with bounds checking
    /// HIGH-11 FIX: Checks that shift amount is non-negative and less than bit width
    pub fn build_int_shl(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        value: IntValue<'ctx>,
        shift_amount: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        // CRITICAL FIX: Check if current block already has terminator
        let insert_block = builder.get_insert_block()
            .ok_or_else(|| "no current insert block".to_string())?;
        if insert_block.get_terminator().is_some() {
            // Block already terminated, return simple shift without checks
            return builder.build_left_shift(value, shift_amount, "shl")
                .map_err(|e| format!("failed to build left shift: {}", e));
        }

        let bit_width = value.get_type().get_bit_width();

        // Check if shift amount is negative
        let is_negative = builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            shift_amount,
            shift_amount.get_type().const_zero(),
            "shift_negative"
        ).map_err(|e| format!("failed to build compare: {}", e))?;

        // Check if shift amount is >= bit_width
        let bit_width_val = shift_amount.get_type().const_int(bit_width as u64, true);
        let is_overflow = builder.build_int_compare(
            inkwell::IntPredicate::UGE,
            shift_amount,
            bit_width_val,
            "shift_overflow"
        ).map_err(|e| format!("failed to build compare: {}", e))?;

        // Invalid if negative OR >= bit_width
        let invalid = builder.build_or(is_negative, is_overflow, "shift_invalid")
            .map_err(|e| format!("failed to build or: {}", e))?;

        // Create basic blocks for handling invalid shift
        let insert_block = builder.get_insert_block().unwrap();
        let function = insert_block.get_parent().unwrap();

        // Create alloca in entry block for result
        let result_alloca = self.create_entry_block_alloca(builder, value.get_type(), "shl_result")?;

        let invalid_block = context.append_basic_block(function, "shl_invalid");
        let valid_block = context.append_basic_block(function, "shl_valid");
        let merge_block = context.append_basic_block(function, "shl_merge");

        // Check if shift is valid
        builder.build_conditional_branch(invalid, invalid_block, valid_block)
            .map_err(|e| format!("failed to build branch: {}", e))?;

        // Invalid block: return 0 (undefined behavior protection)
        builder.position_at_end(invalid_block);
        let safe_value = value.get_type().const_zero();
        builder.build_store(result_alloca, safe_value)
            .map_err(|e| format!("failed to store: {}", e))?;
        builder.build_unconditional_branch(merge_block)
            .map_err(|e| format!("failed to build branch: {}", e))?;

        // Valid block: perform shift
        builder.position_at_end(valid_block);
        let shifted = builder.build_left_shift(value, shift_amount, "shl")
            .map_err(|e| format!("failed to build left shift: {}", e))?;
        builder.build_store(result_alloca, shifted)
            .map_err(|e| format!("failed to store: {}", e))?;
        builder.build_unconditional_branch(merge_block)
            .map_err(|e| format!("failed to build branch: {}", e))?;

        // Merge block: load the result
        builder.position_at_end(merge_block);
        let final_result = builder.build_load(value.get_type(), result_alloca, "shl_final")
            .map_err(|e| format!("failed to load: {}", e))?
            .into_int_value();

        Ok(final_result)
    }

    /// Build right shift with bounds checking
    /// HIGH-11 FIX: Checks that shift amount is non-negative and less than bit width
    pub fn build_int_shr(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        value: IntValue<'ctx>,
        shift_amount: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        // CRITICAL FIX: Check if current block already has terminator
        let insert_block = builder.get_insert_block()
            .ok_or_else(|| "no current insert block".to_string())?;
        if insert_block.get_terminator().is_some() {
            // Block already terminated, return simple shift without checks
            return builder.build_right_shift(value, shift_amount, true, "shr")
                .map_err(|e| format!("failed to build right shift: {}", e));
        }

        // Same checks as left shift
        let bit_width = value.get_type().get_bit_width();

        let is_negative = builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            shift_amount,
            shift_amount.get_type().const_zero(),
            "shift_negative"
        ).map_err(|e| format!("failed to build compare: {}", e))?;

        let bit_width_val = shift_amount.get_type().const_int(bit_width as u64, true);
        let is_overflow = builder.build_int_compare(
            inkwell::IntPredicate::UGE,
            shift_amount,
            bit_width_val,
            "shift_overflow"
        ).map_err(|e| format!("failed to build compare: {}", e))?;

        let invalid = builder.build_or(is_negative, is_overflow, "shift_invalid")
            .map_err(|e| format!("failed to build or: {}", e))?;

        let insert_block = builder.get_insert_block().unwrap();
        let function = insert_block.get_parent().unwrap();

        // Create alloca in entry block for result
        let result_alloca = self.create_entry_block_alloca(builder, value.get_type(), "shr_result")?;

        let invalid_block = context.append_basic_block(function, "shr_invalid");
        let valid_block = context.append_basic_block(function, "shr_valid");
        let merge_block = context.append_basic_block(function, "shr_merge");

        builder.build_conditional_branch(invalid, invalid_block, valid_block)
            .map_err(|e| format!("failed to build branch: {}", e))?;

        builder.position_at_end(invalid_block);
        let safe_value = value.get_type().const_zero();
        builder.build_store(result_alloca, safe_value)
            .map_err(|e| format!("failed to store: {}", e))?;
        builder.build_unconditional_branch(merge_block)
            .map_err(|e| format!("failed to build branch: {}", e))?;

        builder.position_at_end(valid_block);
        let shifted = builder.build_right_shift(value, shift_amount, true, "shr")
            .map_err(|e| format!("failed to build right shift: {}", e))?;
        builder.build_store(result_alloca, shifted)
            .map_err(|e| format!("failed to store: {}", e))?;
        builder.build_unconditional_branch(merge_block)
            .map_err(|e| format!("failed to build branch: {}", e))?;

        builder.position_at_end(merge_block);
        let final_result = builder.build_load(value.get_type(), result_alloca, "shr_final")
            .map_err(|e| format!("failed to load: {}", e))?
            .into_int_value();

        Ok(final_result)
    }
}
