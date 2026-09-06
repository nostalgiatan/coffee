//! Binary and unary operators.

use crate::backend::codegen::CodeGenerator;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    fn coffee_int_is_signed(&self, value: inkwell::values::IntValue<'ctx>) -> bool {
        let name = value.get_name().to_string_lossy();
        if name.is_empty() {
            return true;
        }
        if let Some(ty) = self.lookup_clone_type(name.as_ref()) {
            let t = ty.trim();
            if t.starts_with("int(") && t.ends_with('-') {
                return false;
            }
        }
        true
    }

    pub(crate) fn compile_unary_op(&mut self, op: &str, operand: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        match op {
            "-" => match operand {
                BasicValueEnum::IntValue(i) => {
                    let signed = self.coffee_int_is_signed(i);
                    Ok(self.arithmetic_ctx.build_int_neg(
                        self.backend.context,
                        &self.backend.builder,
                        i,
                        signed,
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
            "~" => match operand {
                BasicValueEnum::IntValue(i) => {
                    let ones = i.get_type().const_all_ones();
                    Ok(self.backend.builder.build_xor(i, ones, "bitnot")
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to build bitwise NOT: {}", e)))?
                        .into())
                }
                _ => Err(self.error("compile_expression", "bitwise NOT is integer-only")),
            },
            "clone" => self.compile_unary_clone(operand),
            _ => Err(self.error("compile_expression", format!("unknown unary operator: {}", op))),
        }
    }

    fn lookup_clone_type(&self, name: &str) -> Option<String> {
        self.variable_types
            .get(name)
            .cloned()
            .or_else(|| self.memory_ctx.get_variable_type(name).cloned())
    }

    fn clone_type_hint_for_name(&self, load_name: &str) -> Option<String> {
        let base = load_name.split('.').next().unwrap_or(load_name);
        if let Some(t) = self.lookup_clone_type(base) {
            return Some(t);
        }
        let stripped = base.trim_end_matches(|c: char| c.is_ascii_digit());
        if stripped != base {
            if let Some(t) = self.lookup_clone_type(stripped) {
                return Some(t);
            }
        }
        let mut best: Option<(usize, String)> = None;
        for (name, ty) in &self.variable_types {
            if !name.is_empty() && base.starts_with(name.as_str()) {
                if best.as_ref().map(|(n, _)| name.len() > *n).unwrap_or(true) {
                    best = Some((name.len(), ty.clone()));
                }
            }
        }
        if let Some((_, t)) = best {
            return Some(t);
        }
        let mut best_ctx: Option<(usize, String)> = None;
        for (name, ty) in &self.memory_ctx.variable_types {
            if !name.is_empty() && base.starts_with(name.as_str()) {
                if best_ctx.as_ref().map(|(n, _)| name.len() > *n).unwrap_or(true) {
                    best_ctx = Some((name.len(), ty.clone()));
                }
            }
        }
        best_ctx.map(|(_, t)| t)
    }

    fn compile_unary_clone(
        &mut self,
        operand: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match operand {
            BasicValueEnum::IntValue(_) | BasicValueEnum::FloatValue(_) => Ok(operand),
            BasicValueEnum::PointerValue(ptr) => {
                use inkwell::types::BasicTypeEnum;
                let load_name = ptr.get_name().to_string_lossy().into_owned();
                let type_hint = self.clone_type_hint_for_name(&load_name);
                let n = self.variables.len();
                let src = format!("__clone_src_{}", n);
                let dst = format!("__clone_dst_{}", n);
                let pty = BasicTypeEnum::PointerType(ptr.get_type());
                let alloca = self
                    .backend
                    .builder
                    .build_alloca(pty, &src)
                    .map_err(|e| self.error("compile_expression", format!("clone temp: {}", e)))?;
                self.backend
                    .builder
                    .build_store(alloca, ptr)
                    .map_err(|e| {
                        self.error("compile_expression", format!("clone temp store: {}", e))
                    })?;
                self.variables.insert(src.clone(), (alloca, pty));
                if let Some(ref t) = type_hint {
                    self.memory_ctx.set_variable_type(src.clone(), t.clone());
                }
                crate::backend::memory_ops::compile_memory_op(
                    &crate::parser::MemoryOp::Clone {
                        source: src.clone(),
                        target: dst.clone(),
                    },
                    self.backend.context,
                    &self.backend.builder,
                    &mut self.variables,
                    &mut self.memory_ctx,
                    &self.functions,
                )
                .map_err(|e| self.error("compile_expression", e))?;
                let (dst_ptr, dst_ty) = *self.variables.get(&dst).ok_or_else(|| {
                    self.error("compile_expression", "clone did not bind result")
                })?;
                let value = self
                    .backend
                    .builder
                    .build_load(dst_ty, dst_ptr, "clone_result")
                    .map_err(|e| self.error("compile_expression", format!("clone load: {}", e)))?;
                self.variables.remove(&src);
                self.variables.remove(&dst);
                Ok(value)
            }
            _ => Err(self.error("compile_expression", "clone not supported for this type")),
        }
    }

    /// Build binary operation using arithmetic module
    pub(crate) fn build_binary_op(&mut self, op: &str, left: BasicValueEnum<'ctx>, right: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        use inkwell::IntPredicate;
        match (left, right) {
            (BasicValueEnum::PointerValue(ptr), BasicValueEnum::IntValue(off)) if op == "+" => {
                return self.build_pointer_byte_offset(ptr, off);
            }
            (BasicValueEnum::IntValue(off), BasicValueEnum::PointerValue(ptr)) if op == "+" => {
                return self.build_pointer_byte_offset(ptr, off);
            }
            (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => {
                let signed = self.coffee_int_is_signed(l) && self.coffee_int_is_signed(r);
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
                    "+" => self.arithmetic_ctx.build_int_add(self.backend.context, &self.backend.builder, l, r, signed)?.into(),
                    "-" => self.arithmetic_ctx.build_int_sub(self.backend.context, &self.backend.builder, l, r, signed)?.into(),
                    "*" => self.arithmetic_ctx.build_int_mul(self.backend.context, &self.backend.builder, l, r, signed)?.into(),
                    "/" => self.arithmetic_ctx.build_int_div(self.backend.context, &self.backend.builder, l, r, signed)?,
                    "%" => self.arithmetic_ctx.build_int_mod(self.backend.context, &self.backend.builder, l, r, signed)?,
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
                match op {
                    "+" => self.build_string_concat(left_ptr, right_ptr),
                    "==" | "!=" => self.build_string_compare(op, left_ptr, right_ptr),
                    _ => {
                        let left_type_str = self.type_to_string(left_ptr.get_type().into());
                        let right_type_str = self.type_to_string(right_ptr.get_type().into());
                        Err(self.error("binary_operation",
                            format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: supported string operators: +, ==, !=",
                                op, left_type_str, right_type_str)))
                    }
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

    fn build_pointer_byte_offset(
        &mut self,
        ptr: PointerValue<'ctx>,
        off: inkwell::values::IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let i8t = self.backend.context.i8_type();
        let gep = unsafe {
            self.backend
                .builder
                .build_in_bounds_gep(i8t, ptr, &[off], "ptr_off")
        }
        .map_err(|e| {
            self.error(
                "binary_operation",
                format!("failed to offset pointer: {}", e),
            )
        })?;
        Ok(gep.into())
    }

    /// `str` `==` / `!=` via libc `strcmp` (or an already-declared `strcmp`).
    fn build_string_compare(
        &mut self,
        op: &str,
        left: PointerValue<'ctx>,
        right: PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let strcmp_fn = if let Some(&f) = self.functions.get("strcmp") {
            f
        } else {
            // memcmp needs a known length; C `str` uses strcmp.
            self.declare_strcmp()?
        };

        let call = self.backend.builder.build_call(
            strcmp_fn,
            &[left.into(), right.into()],
            "strcmp",
        )
        .map_err(|e| self.error("binary_operation", format!("failed to call strcmp: {}", e)))?;

        let cmp = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("binary_operation", "strcmp returned void"));
            }
        };
        let zero = cmp.get_type().const_int(0, true);
        let pred = if op == "==" {
            inkwell::IntPredicate::EQ
        } else {
            inkwell::IntPredicate::NE
        };
        Ok(self.arithmetic_ctx
            .build_int_compare(&self.backend.builder, pred, cmp, zero, "str_cmp")?
            .into())
    }

    fn declare_strcmp(&mut self) -> Result<inkwell::values::FunctionValue<'ctx>, String> {
        self.used_c_functions.insert("strcmp".to_string());
        self.declare_external_function("strcmp")?;
        self.functions.get("strcmp").copied().ok_or_else(|| {
            self.error("binary_operation", "failed to declare strcmp")
        })
    }
}
