//! Arithmetic operations for Coffee compiler code generation
//! 
//! This module handles binary and unary arithmetic operations with safety checks.
//! It provides secure implementations of arithmetic operations that include
//! overflow detection, division by zero checks, and other safety mechanisms
//! to prevent common arithmetic-related vulnerabilities. The module supports
//! both integer and floating-point operations with appropriate error handling.


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


    /// See `create_entry_block_alloca`.
    pub(super) fn create_entry_block_alloca<T>(
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
}

mod int;
mod float;

impl<'ctx> Default for ArithmeticContext<'ctx> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use inkwell::values::AnyValue;

    #[test]
    fn test_arithmetic_context_creation() {
        let ctx = ArithmeticContext::new();
        assert!(ctx.check_overflow); // Now enabled by default
        assert!(ctx.check_div_by_zero);
        assert!(ctx.check_mod_by_zero);
    }

    fn emit_checked_binop_ir(op: &str) -> String {
        let context = Context::create();
        let module = context.create_module("arith_ovf");
        let builder = context.create_builder();
        let i64t = context.i64_type();
        let fn_ty = i64t.fn_type(&[i64t.into(), i64t.into()], false);
        let function = module.add_function("checked", fn_ty, None);
        let entry = context.append_basic_block(function, "entry");
        builder.position_at_end(entry);
        let left = function.get_nth_param(0).unwrap().into_int_value();
        let right = function.get_nth_param(1).unwrap().into_int_value();
        let arith = ArithmeticContext::new();
        let result = match op {
            "add" => arith.build_int_add(&context, &builder, left, right, true).unwrap(),
            "sub" => arith.build_int_sub(&context, &builder, left, right, true).unwrap(),
            "mul" => arith.build_int_mul(&context, &builder, left, right, true).unwrap(),
            other => panic!("unknown op {other}"),
        };
        builder.build_return(Some(&result)).unwrap();
        function.print_to_string().to_string()
    }

    fn labeled_block_body(ir: &str, label: &str) -> String {
        let needle = format!("{}:", label);
        let start = ir
            .find(&needle)
            .unwrap_or_else(|| panic!("missing {label} in IR:\n{ir}"));
        let mut body = String::new();
        for line in ir[start..].lines().skip(1) {
            if line.starts_with(' ') || line.starts_with('\t') || line.is_empty() {
                body.push_str(line);
                body.push('\n');
            } else {
                break;
            }
        }
        body
    }

    fn assert_overflow_traps(ir: &str, label: &str) {
        let body = labeled_block_body(ir, label);
        assert!(
            body.contains("unreachable"),
            "{label} must trap with unreachable, got:\n{body}\nfull IR:\n{ir}"
        );
        assert!(
            !body.contains("store"),
            "{label} must not store 0 and continue, got:\n{body}"
        );
    }

    #[test]
    fn test_add_overflow_traps_with_unreachable() {
        let ir = emit_checked_binop_ir("add");
        assert_overflow_traps(&ir, "add_overflow");
    }

    #[test]
    fn test_sub_overflow_traps_with_unreachable() {
        let ir = emit_checked_binop_ir("sub");
        assert_overflow_traps(&ir, "sub_overflow");
    }

    #[test]
    fn test_mul_overflow_traps_with_unreachable() {
        let ir = emit_checked_binop_ir("mul");
        assert_overflow_traps(&ir, "mul_overflow");
    }

    fn emit_checked_float_binop_ir(op: &str) -> String {
        let context = Context::create();
        let module = context.create_module("arith_fovf");
        let builder = context.create_builder();
        let f64t = context.f64_type();
        let fn_ty = f64t.fn_type(&[f64t.into(), f64t.into()], false);
        let function = module.add_function("checked_f", fn_ty, None);
        let entry = context.append_basic_block(function, "entry");
        builder.position_at_end(entry);
        let left = function.get_nth_param(0).unwrap().into_float_value();
        let right = function.get_nth_param(1).unwrap().into_float_value();
        let arith = ArithmeticContext::new();
        let result = match op {
            "add" => arith.build_float_add(&builder, left, right).unwrap(),
            "sub" => arith.build_float_sub(&builder, left, right).unwrap(),
            "mul" => arith.build_float_mul(&builder, left, right).unwrap(),
            "div" => arith
                .build_float_div(&context, &builder, left, right)
                .unwrap(),
            other => panic!("unknown op {other}"),
        };
        builder.build_return(Some(&result)).unwrap();
        function.print_to_string().to_string()
    }

    #[test]
    fn test_float_add_inf_nan_traps_with_unreachable() {
        let ir = emit_checked_float_binop_ir("add");
        assert_overflow_traps(&ir, "fadd_overflow");
    }

    #[test]
    fn test_float_sub_inf_nan_traps_with_unreachable() {
        let ir = emit_checked_float_binop_ir("sub");
        assert_overflow_traps(&ir, "fsub_overflow");
    }

    #[test]
    fn test_float_mul_inf_nan_traps_with_unreachable() {
        let ir = emit_checked_float_binop_ir("mul");
        assert_overflow_traps(&ir, "fmul_overflow");
    }

    #[test]
    fn test_float_div_inf_nan_traps_with_unreachable() {
        let ir = emit_checked_float_binop_ir("div");
        assert_overflow_traps(&ir, "fdiv_overflow");
    }

    #[test]
    fn test_float_div_by_zero_still_traps_with_unreachable() {
        let ir = emit_checked_float_binop_ir("div");
        assert_overflow_traps(&ir, "fdiv_panic");
    }

    #[test]
    fn test_float_add_skips_inf_nan_trap_when_overflow_check_off() {
        let context = Context::create();
        let module = context.create_module("arith_fovf_off");
        let builder = context.create_builder();
        let f64t = context.f64_type();
        let fn_ty = f64t.fn_type(&[f64t.into(), f64t.into()], false);
        let function = module.add_function("unchecked_fadd", fn_ty, None);
        let entry = context.append_basic_block(function, "entry");
        builder.position_at_end(entry);
        let left = function.get_nth_param(0).unwrap().into_float_value();
        let right = function.get_nth_param(1).unwrap().into_float_value();
        let mut arith = ArithmeticContext::new();
        arith.check_overflow = false;
        let result = arith.build_float_add(&builder, left, right).unwrap();
        builder.build_return(Some(&result)).unwrap();
        let ir = function.print_to_string().to_string();
        assert!(
            !ir.contains("fadd_overflow"),
            "overflow check off must not emit fadd_overflow, got:\n{ir}"
        );
        assert!(
            !ir.contains("unreachable"),
            "overflow check off must not trap, got:\n{ir}"
        );
    }

    fn emit_checked_divrem_ir(op: &str) -> String {
        let context = Context::create();
        let module = context.create_module("arith_div");
        let builder = context.create_builder();
        let i64t = context.i64_type();
        let fn_ty = i64t.fn_type(&[i64t.into(), i64t.into()], false);
        let function = module.add_function("checked_div", fn_ty, None);
        let entry = context.append_basic_block(function, "entry");
        builder.position_at_end(entry);
        let left = function.get_nth_param(0).unwrap().into_int_value();
        let right = function.get_nth_param(1).unwrap().into_int_value();
        let arith = ArithmeticContext::new();
        let result = match op {
            "div" => arith.build_int_div(&context, &builder, left, right, true).unwrap(),
            "mod" => arith.build_int_mod(&context, &builder, left, right, true).unwrap(),
            other => panic!("unknown op {other}"),
        };
        builder.build_return(Some(&result.into_int_value())).unwrap();
        function.print_to_string().to_string()
    }

    #[test]
    fn test_sdiv_int_min_neg_one_traps_with_unreachable() {
        let ir = emit_checked_divrem_ir("div");
        assert!(
            ir.contains("sdiv"),
            "signed division must still emit sdiv, got:\n{ir}"
        );
        assert_overflow_traps(&ir, "div_overflow");
    }

    #[test]
    fn test_srem_int_min_neg_one_traps_with_unreachable() {
        let ir = emit_checked_divrem_ir("mod");
        assert!(
            ir.contains("srem"),
            "signed remainder must still emit srem, got:\n{ir}"
        );
        assert_overflow_traps(&ir, "mod_overflow");
    }

    fn emit_unsigned_binop_ir(op: &str) -> String {
        let context = Context::create();
        let module = context.create_module("arith_unsigned");
        let builder = context.create_builder();
        let i64t = context.i64_type();
        let fn_ty = i64t.fn_type(&[i64t.into(), i64t.into()], false);
        let function = module.add_function("wrap", fn_ty, None);
        let entry = context.append_basic_block(function, "entry");
        builder.position_at_end(entry);
        let left = function.get_nth_param(0).unwrap().into_int_value();
        let right = function.get_nth_param(1).unwrap().into_int_value();
        let arith = ArithmeticContext::new();
        let result = match op {
            "add" => arith.build_int_add(&context, &builder, left, right, false).unwrap(),
            "sub" => arith.build_int_sub(&context, &builder, left, right, false).unwrap(),
            "mul" => arith.build_int_mul(&context, &builder, left, right, false).unwrap(),
            other => panic!("unknown op {other}"),
        };
        builder.build_return(Some(&result)).unwrap();
        function.print_to_string().to_string()
    }

    #[test]
    fn test_unsigned_add_wraps_without_signed_overflow_trap() {
        let ir = emit_unsigned_binop_ir("add");
        assert!(
            !ir.contains("add_overflow"),
            "unsigned add must not use signed overflow traps, got:\n{ir}"
        );
        assert!(
            !ir.contains("unreachable"),
            "unsigned wrapping add must not trap, got:\n{ir}"
        );
        assert!(
            ir.contains(" add "),
            "unsigned add must still emit add, got:\n{ir}"
        );
    }
}
