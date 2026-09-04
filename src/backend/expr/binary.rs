//! Binary and unary operators.

use crate::backend::codegen::CodeGenerator;
use inkwell::values::BasicValueEnum;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_unary_op(&mut self, op: &str, operand: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        match op {
            "-" => match operand {
                BasicValueEnum::IntValue(i) => {
                    Ok(self.arithmetic_ctx.build_int_neg(
                        self.backend.context,
                        &self.backend.builder,
                        i
                    )?.into())
                }
                BasicValueEnum::FloatValue(f) => {
                    let negated = self.backend.builder.build_float_neg(f, "fneg")
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to build float negation: {}", e)))?;
                    Ok(negated.into())
                }
                _ => Err(self.error("compile_expression", "negation not supported for this type")),
            },
            "!" => match operand {
                BasicValueEnum::IntValue(i) => {
                    let one = i.get_type().const_int(1, false);
                    Ok(self.backend.builder.build_xor(i, one, "not")
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to build logical NOT: {}", e)))?
                        .into())
                }
                _ => Err(self.error("compile_expression", "logical NOT not supported for this type")),
            },
            _ => Err(self.error("compile_expression", format!("unknown unary operator: {}", op))),
        }
    }

    /// Build binary operation using arithmetic module
    pub(crate) fn build_binary_op(&mut self, op: &str, left: BasicValueEnum<'ctx>, right: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        use inkwell::IntPredicate;
        match (left, right) {
            (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => {
                // HIGH-11 FIX: Use safe shift operations that check for negative amounts and overflow
                let result: BasicValueEnum<'ctx> = match op {
                    "<<" => self.arithmetic_ctx.build_int_shl(self.backend.context, &self.backend.builder, l, r)?.into(),
                    ">>" => self.arithmetic_ctx.build_int_shr(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "&" => self.backend.builder.build_and(l, r, "bitwise_and")
                        .map_err(|e| self.error("binary_operation", format!("failed to build bitwise and operation: {}", e)))?.into(),
                    "|" => self.backend.builder.build_or(l, r, "bitwise_or")
                        .map_err(|e| self.error("binary_operation", format!("failed to build bitwise or operation: {}", e)))?.into(),
                    "^" => self.backend.builder.build_xor(l, r, "bitwise_xor")
                        .map_err(|e| self.error("binary_operation", format!("failed to build bitwise xor operation: {}", e)))?.into(),
                    "+" => self.arithmetic_ctx.build_int_add(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "-" => self.arithmetic_ctx.build_int_sub(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "*" => self.arithmetic_ctx.build_int_mul(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "/" => self.arithmetic_ctx.build_int_div(self.backend.context, &self.backend.builder, l, r)?,
                    "%" => self.arithmetic_ctx.build_int_mod(self.backend.context, &self.backend.builder, l, r)?,
                    "&&" => self.backend.builder.build_and(l, r, "and")
                        .map_err(|e| self.error("binary_operation", format!("failed to build and operation: {}", e)))?.into(),
                    "||" => self.backend.builder.build_or(l, r, "or")
                        .map_err(|e| self.error("binary_operation", format!("failed to build or operation: {}", e)))?.into(),
                    "==" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::EQ, l, r, "eq")?.into(),
                    "!=" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::NE, l, r, "ne")?.into(),
                    "<" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SLT, l, r, "lt")?.into(),
                    ">" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SGT, l, r, "gt")?.into(),
                    "<=" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SLE, l, r, "le")?.into(),
                    ">=" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SGE, l, r, "ge")?.into(),
                    _ => return Err(self.error("binary_operation",
                        format!("unknown integer operator '{}'\n  = note: supported operators: <<, >>, &, |, ^, +, -, *, /, %, ==, !=, <, >, <=, >=", op))),
                };
                Ok(result)
            }
            (BasicValueEnum::FloatValue(l), BasicValueEnum::FloatValue(r)) => {
                use inkwell::FloatPredicate;
                // Arithmetic operations return FloatValue
                if matches!(op, "+" | "-" | "*" | "/") {
                    let result = match op {
                        "+" => self.arithmetic_ctx.build_float_add(&self.backend.builder, l, r)?,
                        "-" => self.arithmetic_ctx.build_float_sub(&self.backend.builder, l, r)?,
                        "*" => self.arithmetic_ctx.build_float_mul(&self.backend.builder, l, r)?,
                        "/" => self.arithmetic_ctx.build_float_div(self.backend.context, &self.backend.builder, l, r)?,
                        _ => unreachable!(),
                    };
                    Ok(result.into())
                } else {
                    // Comparison operations return IntValue (i1)
                    let pred = match op {
                        "==" => FloatPredicate::OEQ,
                        "!=" => FloatPredicate::ONE,
                        "<" => FloatPredicate::OLT,
                        ">" => FloatPredicate::OGT,
                        "<=" => FloatPredicate::OLE,
                        ">=" => FloatPredicate::OGE,
                        _ => return Err(self.error("binary_operation",
                            format!("unknown float operator '{}'\n  = note: supported operators: +, -, *, /, ==, !=, <, >, <=, >=", op))),
                    };
                    let result = self.arithmetic_ctx.build_float_compare(&self.backend.builder, pred, l, r, "fcmp")
                        .map_err(|e| self.error("binary_operation",
                            format!("failed to build float comparison {}: {}", op, e)))?;
                    Ok(result.into())
                }
            }
            // Handle int and float mixed operations - no implicit conversion
            (BasicValueEnum::IntValue(int_val), BasicValueEnum::FloatValue(float_val)) => {
                let left_type_str = self.type_to_string(int_val.get_type().into());
                let right_type_str = self.type_to_string(float_val.get_type().into());

                Err(self.error("binary_operation",
                    format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: operator '{}' does not support implicit type conversion between int and float\n  = help: use explicit type conversion or ensure both operands are of the same type",
                        op, left_type_str, right_type_str, op)))
            }
            (BasicValueEnum::FloatValue(float_val), BasicValueEnum::IntValue(int_val)) => {
                let left_type_str = self.type_to_string(float_val.get_type().into());
                let right_type_str = self.type_to_string(int_val.get_type().into());

                Err(self.error("binary_operation",
                    format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: operator '{}' does not support implicit type conversion between float and int\n  = help: use explicit type conversion or ensure both operands are of the same type",
                        op, left_type_str, right_type_str, op)))
            }
            (BasicValueEnum::PointerValue(left_ptr), BasicValueEnum::PointerValue(right_ptr)) => {
                // Check if both are string pointers (i8*)
                if op == "+" {
                    // String concatenation
                    return self.build_string_concat(left_ptr, right_ptr);
                } else {
                    let left_type_str = self.type_to_string(left_ptr.get_type().into());
                    let right_type_str = self.type_to_string(right_ptr.get_type().into());

                    Err(self.error("binary_operation",
                        format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: only '+' operator is supported for string concatenation",
                            op, left_type_str, right_type_str)))
                }
            }
            (left_val, right_val) => {
                let left_type_str = self.type_to_string(left_val.get_type());
                let right_type_str = self.type_to_string(right_val.get_type());

                Err(self.error("binary_operation",
                    format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: operator '{}' requires compatible numeric types\n  = help: ensure both operands are of the same numeric type (int or float)",
                        op, left_type_str, right_type_str, op)))
            }
        }
    }
}
