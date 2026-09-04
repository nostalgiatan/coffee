//! Arithmetic operations for Coffee compiler code generation
//! 
//! This module handles binary and unary arithmetic operations with safety checks.
//! It provides secure implementations of arithmetic operations that include
//! overflow detection, division by zero checks, and other safety mechanisms
//! to prevent common arithmetic-related vulnerabilities. The module supports
//! both integer and floating-point operations with appropriate error handling.

use inkwell::values::{IntValue, FloatValue, BasicValueEnum};

/// Structure to hold arithmetic operation context
/// 
/// The ArithmeticContext manages settings and state for arithmetic operations
/// in the Coffee compiler. It provides configurable safety checks for various
/// arithmetic operations, including overflow detection, division by zero checks,
/// and modulo by zero checks. The context also maintains a reference to a
/// panic function for error handling when safety violations occur.
/// 
/// The context ensures that arithmetic operations are performed safely and
/// can optionally trigger panic behavior when unsafe conditions are detected.
pub struct ArithmeticContext<'ctx> {
    /// Check for integer overflow in operations
    pub check_overflow: bool,
    /// Check for division by zero
    pub check_div_by_zero: bool,
    /// Check for modulo by zero
    pub check_mod_by_zero: bool,
    _phantom: std::marker::PhantomData<&'ctx ()>,
}

impl<'ctx> ArithmeticContext<'ctx> {
    /// Create a new ArithmeticContext with default safety settings
    /// 
    /// Initializes an ArithmeticContext with default safety checks enabled:
    /// - Integer overflow checking is enabled for security
    /// - Division by zero checking is enabled
    /// - Modulo by zero checking is enabled
    /// 
    /// # Returns
    /// 
    /// A new ArithmeticContext instance with default safety settings
    pub fn new() -> Self {
        Self {
            check_overflow: true,  // Enable overflow checking for security
            check_div_by_zero: true,
            check_mod_by_zero: true,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set the panic function (mutable reference version)
    /// 
    /// Configures the panic function that will be called when arithmetic safety
    /// violations are detected during compilation. This function is used for
    /// error handling when overflow, division by zero, or other arithmetic errors
    /// occur during code execution.
    /// 
    /// The panic function is typically set up during the compilation process when
    /// the compiler establishes its error handling infrastructure. Once set, it
    /// Helper function to create an alloca in the function's entry block
    /// 
    /// This internal helper function creates an LLVM alloca instruction in the
    /// function's entry block to ensure it dominates all uses, satisfying LLVM's
    /// Static Single Assignment (SSA) requirements. This is important for safety
    /// checks that need to store intermediate results in memory.
    /// 
    /// The function handles the case where an overflow checking block may have
    /// already been created by checking if the entry block has a terminator.
    /// If the entry block has a terminator, it creates the alloca in the current
    /// block instead to avoid violating LLVM's structural requirements.
    /// 
    /// This ensures the alloca dominates all uses, satisfying LLVM's SSA requirements
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for creating the alloca
    /// * `type_` - The LLVM type for the value to be allocated
    /// * `name` - The name for the alloca instruction
    /// 
    /// # Returns
    /// 
    /// * `Ok(PointerValue)` - The pointer to the allocated memory
    /// * `Err(String)` - If there was an error creating the alloca
    fn create_entry_block_alloca<T>(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        type_: T,
        name: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String>
    where
        T: inkwell::types::BasicType<'ctx>,
    {
        let insert_block = builder.get_insert_block().unwrap();
        let function = insert_block.get_parent().unwrap();
        let entry_block = function.get_first_basic_block().unwrap();

        // Check if entry block has a terminator
        if entry_block.get_terminator().is_some() {
            // Entry has terminator - create alloca in current block instead
            // This happens when overflow checking blocks have been created
            let alloca = builder.build_alloca(type_, name)
                .map_err(|e| format!("failed to build alloca '{}': {}", name, e))?;
            Ok(alloca)
        } else {
            // No terminator - position at end of entry block
            let current_pos = builder.get_insert_block();
            builder.position_at_end(entry_block);

            let alloca = builder.build_alloca(type_, name)
                .map_err(|e| format!("failed to build alloca '{}': {}", name, e))?;

            if let Some(pos) = current_pos {
                builder.position_at_end(pos);
            }

            Ok(alloca)
        }
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
    ) -> Result<IntValue<'ctx>, String> {
        if self.check_overflow {
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
    /// safely. When an overflow is detected, it returns a safe default value (0)
    /// as the result of the operation.
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
    ) -> Result<IntValue<'ctx>, String> {
        if self.check_overflow {
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

            // Overflow block: return 0 as safe default
            builder.position_at_end(overflow_block);
            let safe_value = left.get_type().const_zero();
            builder.build_store(result_alloca, safe_value)
                .map_err(|e| format!("failed to store: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

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
    /// cannot cause overflow. When overflow is detected, it returns a safe default
    /// value (0) as the result of the operation.
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
    ) -> Result<IntValue<'ctx>, String> {
        if self.check_overflow {
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

            // Overflow block: return 0 as safe default
            builder.position_at_end(overflow_block);
            let safe_value = left.get_type().const_zero();
            builder.build_store(result_alloca, safe_value)
                .map_err(|e| format!("failed to store: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

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
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if self.check_div_by_zero {
            // Check for division by zero
            let zero = right.get_type().const_zero();
            let is_zero = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                right,
                zero,
                "is_zero"
            ).map_err(|e| format!("failed to build zero check: {}", e))?;

            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            let normal_block = context.append_basic_block(function, "div_normal");
            let panic_block = context.append_basic_block(function, "div_panic");
            let merge_block = context.append_basic_block(function, "div_merge");

            builder.build_conditional_branch(is_zero, panic_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Panic block: division by zero
            builder.position_at_end(panic_block);

            // Use LLVM's unreachable instruction to trap
            // This will trigger undefined behavior sanitizers at runtime
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable: {}", e))?;

            // Normal block: perform division
            builder.position_at_end(normal_block);
            let result = builder.build_int_signed_div(left, right, "div")
                .map_err(|e| format!("failed to build integer division: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to branch to merge: {}", e))?;

            // Merge block: use PHI node
            builder.position_at_end(merge_block);
            let phi = builder.build_phi(result.get_type(), "div_result")
                .map_err(|e| format!("failed to build phi: {}", e))?;
            phi.add_incoming(&[(&result, normal_block)]);

            Ok(phi.as_basic_value())
        } else {
            builder.build_int_signed_div(left, right, "div")
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
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if self.check_mod_by_zero {
            // Check for modulo by zero
            let zero = right.get_type().const_zero();
            let is_zero = builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                right,
                zero,
                "is_zero"
            ).map_err(|e| format!("failed to build zero check: {}", e))?;

            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            let normal_block = context.append_basic_block(function, "mod_normal");
            let panic_block = context.append_basic_block(function, "mod_panic");
            let merge_block = context.append_basic_block(function, "mod_merge");

            builder.build_conditional_branch(is_zero, panic_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Panic block: modulo by zero
            builder.position_at_end(panic_block);
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable: {}", e))?;

            // Normal block: perform modulo
            builder.position_at_end(normal_block);
            let result = builder.build_int_signed_rem(left, right, "rem")
                .map_err(|e| format!("failed to build integer modulo: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to branch to merge: {}", e))?;

            // Merge block: use PHI node
            builder.position_at_end(merge_block);
            let phi = builder.build_phi(result.get_type(), "mod_result")
                .map_err(|e| format!("failed to build phi: {}", e))?;
            phi.add_incoming(&[(&result, normal_block)]);

            Ok(phi.as_basic_value())
        } else {
            Ok(builder.build_int_signed_rem(left, right, "rem")
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

    /// Build float addition
    /// 
    /// This method generates LLVM IR for floating-point addition. It adds two
    /// floating-point values and returns the result. Unlike integer arithmetic,
    /// floating-point arithmetic in LLVM follows IEEE 754 standards and handles
    /// special cases like infinity and NaN according to those standards.
    /// 
    /// The method does not perform additional safety checks beyond the standard
    /// IEEE 754 floating-point behavior, which includes proper handling of
    /// overflow to infinity and underflow to zero.
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand for the floating-point addition
    /// * `right` - The right operand for the floating-point addition
    /// 
    /// # Returns
    /// 
    /// * `Ok(FloatValue)` - The result of the floating-point addition
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_float_add(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        builder.build_float_add(left, right, "fadd")
            .map_err(|e| format!("failed to build float addition: {}", e))
    }

    /// Build float subtraction
    /// 
    /// This method generates LLVM IR for floating-point subtraction. It subtracts
    /// the right floating-point value from the left floating-point value and
    /// returns the result. Like other floating-point operations, it follows
    /// IEEE 754 standards for handling special cases like infinity and NaN.
    /// 
    /// The method performs standard IEEE 754 floating-point subtraction without
    /// additional safety checks, relying on the standard to handle edge cases.
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand (minuend) for the floating-point subtraction
    /// * `right` - The right operand (subtrahend) for the floating-point subtraction
    /// 
    /// # Returns
    /// 
    /// * `Ok(FloatValue)` - The result of the floating-point subtraction
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_float_sub(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        builder.build_float_sub(left, right, "fsub")
            .map_err(|e| format!("failed to build float subtraction: {}", e))
    }

    /// Build float multiplication
    /// 
    /// This method generates LLVM IR for floating-point multiplication. It multiplies
    /// two floating-point values and returns the result. The operation follows
    /// IEEE 754 standards for floating-point arithmetic, properly handling
    /// special values like infinity, NaN, and subnormal numbers.
    /// 
    /// As with other floating-point operations, it relies on IEEE 754 behavior
    /// for edge cases rather than implementing additional safety checks.
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand (multiplicand) for the floating-point multiplication
    /// * `right` - The right operand (multiplier) for the floating-point multiplication
    /// 
    /// # Returns
    /// 
    /// * `Ok(FloatValue)` - The result of the floating-point multiplication
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_float_mul(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        builder.build_float_mul(left, right, "fmul")
            .map_err(|e| format!("failed to build float multiplication: {}", e))
    }

    /// Build float division
    /// 
    /// This method generates LLVM IR for floating-point division with division-by-zero
    /// detection. When division-by-zero checking is enabled, it checks if the
    /// divisor (right operand) is zero before performing the division.
    /// 
    /// If division-by-zero is detected, the method calls the configured panic
    /// function with an appropriate error message and then generates an
    /// unreachable instruction. Otherwise, it performs the floating-point
    /// division operation following IEEE 754 standards.
    /// 
    /// Note that IEEE 754 defines division by zero as infinity, but for safety
    /// reasons, this implementation detects and handles it as an error condition.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `left` - The left operand (dividend) for the floating-point division
    /// * `right` - The right operand (divisor) for the floating-point division
    /// 
    /// # Returns
    /// 
    /// * `Ok(FloatValue)` - The result of the floating-point division
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_float_div(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> Result<FloatValue<'ctx>, String> {
        if self.check_div_by_zero {
            // Check for division by zero
            let is_zero = builder.build_float_compare(
                inkwell::FloatPredicate::OEQ,
                right,
                right.get_type().const_zero(),
                "is_zero"
            ).map_err(|e| format!("failed to build zero check: {}", e))?;

            let insert_block = builder.get_insert_block().unwrap();
            let function = insert_block.get_parent().unwrap();

            let normal_block = context.append_basic_block(function, "fdiv_normal");
            let panic_block = context.append_basic_block(function, "fdiv_panic");
            let merge_block = context.append_basic_block(function, "fdiv_merge");

            builder.build_conditional_branch(is_zero, panic_block, normal_block)
                .map_err(|e| format!("failed to build branch: {}", e))?;

            // Panic block: division by zero
            builder.position_at_end(panic_block);

            // Use LLVM's unreachable instruction to trap
            // This will trigger undefined behavior sanitizers at runtime
            builder.build_unreachable()
                .map_err(|e| format!("failed to build unreachable: {}", e))?;

            // Normal block: perform division
            builder.position_at_end(normal_block);
            let result = builder.build_float_div(left, right, "fdiv")
                .map_err(|e| format!("failed to build float division: {}", e))?;
            builder.build_unconditional_branch(merge_block)
                .map_err(|e| format!("failed to branch to merge: {}", e))?;

            // Merge block: use PHI node
            builder.position_at_end(merge_block);
            let phi = builder.build_phi(result.get_type(), "fdiv_result")
                .map_err(|e| format!("failed to build phi: {}", e))?;
            phi.add_incoming(&[(&result, normal_block)]);

            Ok(phi.as_basic_value().into_float_value())
        } else {
            builder.build_float_div(left, right, "fdiv")
                .map_err(|e| format!("failed to build float division: {}", e))
        }
    }

    /// Build float comparison
    /// 
    /// This method generates LLVM IR for floating-point comparison operations using
    /// the specified predicate. It performs a comparison between two floating-point
    /// values and returns a boolean result (i1 type) indicating whether the
    /// comparison is true or false.
    /// 
    /// The method supports all standard floating-point comparison predicates such as
    /// equality, inequality, less than, greater than, etc. It follows IEEE 754
    /// standards for handling special values like NaN (Not a Number) in comparisons.
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `pred` - The floating-point predicate to use for comparison (e.g., OEQ, ONE, OLT, OGT)
    /// * `left` - The left operand for the comparison
    /// * `right` - The right operand for the comparison
    /// * `name` - The name for the resulting comparison instruction
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The boolean result of the comparison (i1 type)
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_float_compare(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        pred: inkwell::FloatPredicate,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        builder.build_float_compare(pred, left, right, name)
            .map_err(|e| format!("failed to build float comparison: {}", e))
    }

    /// Build integer negation with overflow checking
    /// 
    /// This method generates LLVM IR for integer negation with overflow detection.
    /// It handles the special case where negating the minimum integer value
    /// (INT_MIN) causes overflow, since the absolute value of INT_MIN is greater
    /// than INT_MAX in two's complement representation.
    /// 
    /// In two's complement arithmetic, the range is asymmetric: for n-bit integers,
    /// the range is -2^(n-1) to 2^(n-1)-1. So INT_MIN (-2^(n-1)) cannot be
    /// negated to a positive value since 2^(n-1) exceeds INT_MAX.
    /// 
    /// Checks for the special case: negating INT_MIN causes overflow
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context
    /// * `builder` - The LLVM builder to use for instruction generation
    /// * `value` - The integer value to negate
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The result of the negation
    /// * `Err(String)` - If there was an error during instruction generation
    pub fn build_int_neg(
        &self,
        context: &'ctx inkwell::context::Context,
        builder: &inkwell::builder::Builder<'ctx>,
        value: IntValue<'ctx>,
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

        if self.check_overflow {
            // Check for INT_MIN overflow case
            // For signed integers, negating INT_MIN causes overflow
            // INT_MIN = -2^(n-1), -INT_MIN = 2^(n-1) which exceeds INT_MAX = 2^(n-1) - 1

            let bit_width = value.get_type().get_bit_width();
            let int_min = value.get_type().const_int(
                1u64 << (bit_width - 1),  // INT_MIN = -2^(n-1) in two's complement
                true  // signed
            );

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

    /// Check if a float value is NaN (Not a Number)
    #[allow(dead_code)]
    pub fn build_float_is_nan(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        value: FloatValue<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        // NaN != NaN is always true in IEEE 754
        let is_nan = builder.build_float_compare(
            inkwell::FloatPredicate::ONE,
            value,
            value,
            "is_nan"
        ).map_err(|e| format!("failed to build NaN check: {}", e))?;

        Ok(is_nan)
    }

    /// Check if a float value is infinite (+Inf or -Inf)
    #[allow(dead_code)]
    pub fn build_float_is_infinite(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        value: FloatValue<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        // Check if value is positive infinity (greater than max finite float)
        let max_finite = value.get_type().const_float(std::f64::MAX);
        let is_pos_inf = builder.build_float_compare(
            inkwell::FloatPredicate::OGE,
            value,
            max_finite,
            "is_pos_inf"
        ).map_err(|e| format!("failed to build pos infinity check: {}", e))?;

        // Check if value is negative infinity (less than min finite float)
        let min_finite = value.get_type().const_float(std::f64::MIN);
        let is_neg_inf = builder.build_float_compare(
            inkwell::FloatPredicate::OLE,
            value,
            min_finite,
            "is_neg_inf"
        ).map_err(|e| format!("failed to build neg infinity check: {}", e))?;

        // is_infinite = is_pos_inf OR is_neg_inf
        let is_infinite = builder.build_or(is_pos_inf, is_neg_inf, "is_infinite")
            .map_err(|e| format!("failed to build OR for infinity check: {}", e))?;

        Ok(is_infinite)
    }
}

impl<'ctx> Default for ArithmeticContext<'ctx> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic_context_creation() {
        let ctx = ArithmeticContext::new();
        assert!(ctx.check_overflow); // Now enabled by default
        assert!(ctx.check_div_by_zero);
        assert!(ctx.check_mod_by_zero);
    }
}
