//! Variable and assignment compilation for Coffee compiler
//!
//! Handles variable declarations, assignments, and array operations.

use super::codegen::CodeGenerator;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile array declaration with initialization
    pub fn compile_array_declaration(&mut self, _name: &str, _type_str: &str, _value_expr: &crate::parser::expr::Expression) -> Result<(), String> {
        Err(self.error(
            "compile_array_declaration",
            "array let must come from MIR, not the parser AST",
        ))
    }

    pub(crate) fn bind_array_ptr(
        &mut self,
        name: &str,
        ptr: PointerValue<'ctx>,
        size: u32,
        elem_llvm_type: BasicTypeEnum<'ctx>,
    ) -> Result<(), String> {
        let len_type = self.backend.context.i64_type();
        let len_alloca = self
            .create_entry_alloca_preserving_terminator(len_type, &format!("{}_len", name))
            .map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to allocate length storage: {}", e),
                )
            })?;
        self.backend
            .builder
            .build_store(len_alloca, len_type.const_int(size as u64, false))
            .map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to store array length: {}", e),
                )
            })?;
        self.array_allocas.insert(name.to_string(), ptr);
        self.array_sizes.insert(name.to_string(), size);
        self.array_lengths.insert(name.to_string(), len_alloca);
        self.array_element_types
            .insert(name.to_string(), elem_llvm_type);
        Ok(())
    }

    /// Bind Coffee `[T]` from a fat `{ ptr, i64 }` value.
    /// `reuse_alloca`: parameter already stored in `variables`.
    pub(crate) fn bind_slice_fat(
        &mut self,
        name: &str,
        fat: BasicValueEnum<'ctx>,
        elem_llvm_type: BasicTypeEnum<'ctx>,
        reuse_alloca: bool,
    ) -> Result<(), String> {
        let fat_ty = self.type_mapper.slice_fat_type();
        let struct_val = match fat {
            BasicValueEnum::StructValue(s) => s,
            BasicValueEnum::PointerValue(p) => self
                .backend
                .builder
                .build_load(fat_ty, p, &format!("{}_fat", name))
                .map_err(|e| {
                    self.error(
                        "compile_array_declaration",
                        format!("failed to load slice '{}': {}", name, e),
                    )
                })?
                .into_struct_value(),
            other => {
                return Err(self.error(
                    "compile_array_declaration",
                    format!("slice '{}' must be a fat pointer, got {:?}", name, other),
                ));
            }
        };
        let data = self
            .backend
            .builder
            .build_extract_value(struct_val, 0, &format!("{}_data", name))
            .map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to extract slice ptr '{}': {}", name, e),
                )
            })?
            .into_pointer_value();
        let len = self
            .backend
            .builder
            .build_extract_value(struct_val, 1, &format!("{}_lenv", name))
            .map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to extract slice len '{}': {}", name, e),
                )
            })?
            .into_int_value();
        if !reuse_alloca {
            let alloca = self
                .create_entry_alloca_preserving_terminator(fat_ty, name)
                .map_err(|e| {
                    self.error(
                        "compile_array_declaration",
                        format!("failed to allocate slice '{}': {}", name, e),
                    )
                })?;
            self.backend.builder.build_store(alloca, struct_val).map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to store slice '{}': {}", name, e),
                )
            })?;
            self.variables.insert(name.to_string(), (alloca, fat_ty.into()));
        }
        let len_alloca = self
            .create_entry_alloca_preserving_terminator(
                self.backend.context.i64_type(),
                &format!("{}_len", name),
            )
            .map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to allocate slice length '{}': {}", name, e),
                )
            })?;
        self.backend.builder.build_store(len_alloca, len).map_err(|e| {
            self.error(
                "compile_array_declaration",
                format!("failed to store slice length '{}': {}", name, e),
            )
        })?;
        self.array_allocas.insert(name.to_string(), data);
        self.array_lengths.insert(name.to_string(), len_alloca);
        self.array_element_types
            .insert(name.to_string(), elem_llvm_type);
        Ok(())
    }

    pub(crate) fn pack_slice_from_buffer(
        &mut self,
        name: &str,
        buf: PointerValue<'ctx>,
        len: inkwell::values::IntValue<'ctx>,
        elem_llvm_type: BasicTypeEnum<'ctx>,
    ) -> Result<(), String> {
        let fat_ty = self.type_mapper.slice_fat_type();
        let mut current = fat_ty.const_zero();
        let inserted = self
            .backend
            .builder
            .build_insert_value(current, buf, 0, &format!("{}_ins_ptr", name))
            .map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to insert slice ptr '{}': {}", name, e),
                )
            })?;
        current = match inserted {
            inkwell::values::AggregateValueEnum::StructValue(v) => v,
            _ => {
                return Err(self.error(
                    "compile_array_declaration",
                    "slice insert_value did not return a struct".to_string(),
                ));
            }
        };
        let inserted = self
            .backend
            .builder
            .build_insert_value(current, len, 1, &format!("{}_ins_len", name))
            .map_err(|e| {
                self.error(
                    "compile_array_declaration",
                    format!("failed to insert slice len '{}': {}", name, e),
                )
            })?;
        current = match inserted {
            inkwell::values::AggregateValueEnum::StructValue(v) => v,
            _ => {
                return Err(self.error(
                    "compile_array_declaration",
                    "slice insert_value did not return a struct".to_string(),
                ));
            }
        };
        self.bind_slice_fat(name, current.into(), elem_llvm_type, false)
    }

    pub(crate) fn load_slice_fat_from_parts(
        &mut self,
        name: &str,
        data: PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let len_ty = self.backend.context.i64_type();
        let len = if let Some(&len_ptr) = self.array_lengths.get(name) {
            self.backend
                .builder
                .build_load(len_ty, len_ptr, &format!("{}_len_load", name))
                .map_err(|e| {
                    self.error(
                        "compile_expression",
                        format!("failed to load slice length '{}': {}", name, e),
                    )
                })?
                .into_int_value()
        } else {
            len_ty.const_int(0, false)
        };
        let fat_ty = self.type_mapper.slice_fat_type();
        let mut current = fat_ty.const_zero();
        let inserted = self
            .backend
            .builder
            .build_insert_value(current, data, 0, &format!("{}_pack_ptr", name))
            .map_err(|e| {
                self.error(
                    "compile_expression",
                    format!("failed to pack slice ptr '{}': {}", name, e),
                )
            })?;
        current = match inserted {
            inkwell::values::AggregateValueEnum::StructValue(v) => v,
            _ => {
                return Err(self.error(
                    "compile_expression",
                    "slice pack did not return a struct".to_string(),
                ));
            }
        };
        let inserted = self
            .backend
            .builder
            .build_insert_value(current, len, 1, &format!("{}_pack_len", name))
            .map_err(|e| {
                self.error(
                    "compile_expression",
                    format!("failed to pack slice len '{}': {}", name, e),
                )
            })?;
        match inserted {
            inkwell::values::AggregateValueEnum::StructValue(v) => Ok(v.into()),
            _ => Err(self.error(
                "compile_expression",
                "slice pack did not return a struct".to_string(),
            )),
        }
    }

    pub(crate) fn compile_array_element_ptr_from_value(
        &mut self,
        array_str: &str,
        index_value: BasicValueEnum<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        // Convert index to i64 if needed
        let index_i64 = match index_value {
            BasicValueEnum::IntValue(i) => {
                // Sign extend or truncate to i64
                let bit_width = i.get_type().get_bit_width();
                if bit_width < 64 {
                    self.backend.builder.build_int_s_extend(i, self.backend.context.i64_type(), "index_sext")
                        .map_err(|e| self.error("compile_array_index",
                            format!("failed to sign extend index: {}", e)))?
                } else if bit_width > 64 {
                    self.backend.builder.build_int_truncate(i, self.backend.context.i64_type(), "index_trunc")
                        .map_err(|e| self.error("compile_array_index",
                            format!("failed to truncate index: {}", e)))?
                } else {
                    i
                }
            }
            _ => {
                return Err(self.error("compile_array_index",
                    "array index must be an integer".to_string()));
            }
        };

        // Look up the array variable to get its pointer and length
        let array_name = array_str;

        // Check if we have the array alloca stored (for proper GEP indexing)
        if let Some(&array_alloca) = self.array_allocas.get(array_name) {
            if !self.array_sizes.contains_key(array_name) {
                return self.compile_slice_element_ptr(array_name, array_alloca, index_i64);
            }
            // CRITICAL-1 FIX: Use compile-time constant for bounds check instead of loading from memory
            // This prevents TOCTOU (time-of-check-time-of-use) vulnerabilities where the length
            // could theoretically be modified between the check and the array access.
            if let Some(&array_size) = self.array_sizes.get(array_name) {
                // Use compile-time constant for bounds checking
                let array_len = self.backend.context.i64_type().const_int(array_size as u64, false);

                // `--enable-safety` inserts SafetyContext bounds checks (bounds_panic).
                // Existing check_array_bounds still runs so default array access stays guarded.
                if self.enable_safety {
                    self.safety_ctx.check_bounds(
                        &self.backend.builder,
                        index_i64,
                        array_len,
                        "array index",
                    )?;
                }

                // Perform bounds checking: 0 <= index < length
                self.check_array_bounds(index_i64, array_len, array_name)?;

                // After bounds checking, update entry_successor to current block
                // This is necessary because bounds checking creates new blocks
                if let Some(current) = self.backend.builder.get_insert_block() {
                    self.entry_successor = Some(current);
                }

                // Get the element type and array size
                if let Some(&elem_type) = self.array_element_types.get(array_name) {
                    if let Some(&array_size) = self.array_sizes.get(array_name) {
                        // Use proper 2-index GEP for array indexing
                        // GEP format: getelementptr [N x type], ptr %array, i32 0, i32 %index
                        let zero = self.backend.context.i32_type().const_int(0, false);

                        // Security: Truncate index to i32 for GEP
                        // Note: Safe because array size is limited to 1M (<< i32::MAX)
                        let index_i32 = self.backend.builder.build_int_truncate(
                            index_i64,
                            self.backend.context.i32_type(),
                            "index_i32"
                        ).map_err(|e| self.error("compile_array_index",
                            format!("failed to truncate index to i32: {}", e)))?;

                        // Build the correct array type
                        let array_type = match elem_type {
                            BasicTypeEnum::IntType(int_type) => int_type.array_type(array_size),
                            BasicTypeEnum::FloatType(float_type) => float_type.array_type(array_size),
                            BasicTypeEnum::PointerType(ptr_type) => ptr_type.array_type(array_size),
                            BasicTypeEnum::StructType(st) => st.array_type(array_size),
                            BasicTypeEnum::ArrayType(at) => at.array_type(array_size),
                            BasicTypeEnum::VectorType(vt) => vt.array_type(array_size),
                            _ => return Err(self.error("compile_array_index",
                                format!("unsupported array element type"))),
                        };

                        // Use GEP with 2 indices directly on the array alloca
                        // SAFETY: This unsafe block is safe because:
                        // 1. Bounds checking above verified: 0 <= index < array_size
                        // 2. Index truncation is safe (array_size <= 1M << i32::MAX)
                        // 3. `in_bounds` GEP flag ensures LLVM assumes pointer is valid
                        // 4. array_type matches the actual allocation type
                        // 5. The alloca dominates all uses (SSA requirement satisfied)
                        // 6. Zero index ensures we're accessing the first (and only) array element of the alloca
                        let elem_ptr = unsafe {
                            self.backend.builder.build_in_bounds_gep(
                                array_type,
                                array_alloca,
                                &[zero, index_i32],
                                "elem_ptr"
                            )
                        }.map_err(|e| self.error("compile_array_index",
                            format!("failed to build GEP for array element: {}", e)))?;

                        return Ok(elem_ptr);
                    }
                }
            }
        }

        // Fallback: treat as simple variable access
        Err(self.error("compile_array_index",
            format!("cannot find array '{}' or array length information not available\n  = note: arrays must be declared with their length\n  = help: use array literal syntax like [1, 2, 3] or declare array with known size", array_name)))
    }

    fn compile_slice_element_ptr(
        &mut self,
        array_name: &str,
        data: PointerValue<'ctx>,
        index_i64: inkwell::values::IntValue<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        let len_ty = self.backend.context.i64_type();
        let array_len = if let Some(&len_ptr) = self.array_lengths.get(array_name) {
            self.backend
                .builder
                .build_load(len_ty, len_ptr, "slice_len")
                .map_err(|e| {
                    self.error(
                        "compile_array_index",
                        format!("failed to load slice length '{}': {}", array_name, e),
                    )
                })?
                .into_int_value()
        } else {
            return Err(self.error(
                "compile_array_index",
                format!("slice '{}' is missing length", array_name),
            ));
        };
        if self.enable_safety {
            self.safety_ctx.check_bounds(
                &self.backend.builder,
                index_i64,
                array_len,
                "slice index",
            )?;
        }
        self.check_array_bounds(index_i64, array_len, array_name)?;
        if let Some(current) = self.backend.builder.get_insert_block() {
            self.entry_successor = Some(current);
        }
        let elem_type = *self.array_element_types.get(array_name).ok_or_else(|| {
            self.error(
                "compile_array_index",
                format!("missing element type for slice '{}'", array_name),
            )
        })?;
        let elem_ptr = unsafe {
            self.backend.builder.build_in_bounds_gep(
                elem_type,
                data,
                &[index_i64],
                "slice_elem_ptr",
            )
        }
        .map_err(|e| {
            self.error(
                "compile_array_index",
                format!("failed to GEP slice element: {}", e),
            )
        })?;
        Ok(elem_ptr)
    }

    pub(crate) fn compile_array_index_from_value(
        &mut self,
        array_str: &str,
        index_value: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let elem_ptr = self.compile_array_element_ptr_from_value(array_str, index_value)?;
        self.load_array_element(array_str, elem_ptr)
    }

    fn load_array_element(
        &mut self,
        array_name: &str,
        elem_ptr: PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let elem_type = *self.array_element_types.get(array_name).ok_or_else(|| {
            self.error(
                "compile_array_index",
                format!("missing element type for array '{}'", array_name),
            )
        })?;
        self.backend.builder.build_load(elem_type, elem_ptr, "elem_value")
            .map_err(|e| self.error("compile_array_index",
                format!("failed to load array element: {}", e)))
    }

    /// Check array bounds and panic if out of bounds
    fn check_array_bounds(
        &self,
        index: inkwell::values::IntValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
        array_name: &str,
    ) -> Result<(), String> {
        let builder = &self.backend.builder;

        // Get current function and block first
        let current_block = builder.get_insert_block().unwrap();
        let function = current_block.get_parent().unwrap();

        // Check lower bound: index >= 0
        let zero = self.backend.context.i64_type().const_int(0, true);
        let is_negative = builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            index,
            zero,
            "is_negative"
        ).map_err(|e| format!("failed to build negative check: {}", e))?;

        // Check upper bound: index < length
        let is_out_of_bounds = builder.build_int_compare(
            inkwell::IntPredicate::SGE,
            index,
            length,
            "is_out_of_bounds"
        ).map_err(|e| format!("failed to build bounds check: {}", e))?;

        // Combine checks: out of bounds if negative OR >= length
        let bounds_error = builder.build_or(
            is_negative,
            is_out_of_bounds,
            "bounds_error"
        ).map_err(|e| format!("failed to combine bounds checks: {}", e))?;

        // Create panic and continue blocks
        let panic_block = self.backend.context.append_basic_block(
            function,
            format!("panic_bounds_{}", array_name).as_str()
        );
        let continue_block = self.backend.context.append_basic_block(
            function,
            format!("continue_{}", array_name).as_str()
        );

        // If we're in entry block, track the successor for later alloca creation
        if current_block == function.get_first_basic_block().unwrap() {
            // We can't directly compare BasicBlock, so use name check or save reference
            // For now, just track that continue_block is the successor
            // Note: This is &self, not &mut self, so we can't modify entry_successor here
            // We'll handle this differently
        }

        // Conditional branch
        builder.build_conditional_branch(bounds_error, panic_block, continue_block)
            .map_err(|e| format!("failed to build conditional branch for bounds check: {}", e))?;

        // Build panic block
        builder.position_at_end(panic_block);

        // Security: Don't leak array name in error message
        let panic_msg = builder.build_global_string_ptr(
            "Array index out of bounds",
            "panic_msg"
        ).map_err(|e| format!("failed to build panic message: {}", e))?;

        let puts_func = self.functions.get("puts").copied().ok_or_else(|| {
            self.error(
                "check_array_bounds",
                "puts must be declared from the libc table (declare_runtime_functions)",
            )
        })?;

        builder.build_call(puts_func, &[panic_msg.as_pointer_value().into()], "puts_call")
            .map_err(|e| format!("failed to build puts call: {}", e))?;

        // Call exit(1) to terminate the program
        // exit function is pre-declared in runtime functions
        let exit_func = self.functions.get("exit").copied().unwrap();
        let exit_code = crate::backend::functions::const_exit_status_one(
            exit_func,
            self.backend.context,
        );
        builder.build_call(exit_func, &[exit_code.into()], "exit_call")
            .map_err(|e| format!("failed to build exit call: {}", e))?;

        builder.build_unreachable()
            .map_err(|e| format!("failed to build unreachable: {}", e))?;

        // Position builder at continue block
        builder.position_at_end(continue_block);

        Ok(())
    }

    pub fn compile_local_variable_decl(&mut self, _var: &crate::parser::var::VariableDecl) -> Result<(), String> {
        Err(self.error(
            "compile_local_variable_decl",
            "let must come from MIR, not the parser AST",
        ))
    }

    /// Compile variable declaration
    pub fn compile_variable_decl(&mut self, var: &crate::parser::var::VariableDecl) -> Result<(), String> {
        // Global variable
        let llvm_type = self.coffee_type_to_llvm(&var.var_type)?;

        let global = self.backend.module.add_global(
            llvm_type,
            Some(inkwell::AddressSpace::default()),
            &var.name
        );

        let lit = match var.value.kind() {
            crate::parser::expr::Expression::Literal(s) => Some(s.as_str()),
            _ => None,
        };
        match llvm_type {
            BasicTypeEnum::IntType(t) => {
                if let Some(s) = lit {
                    if let Ok(v) = s.parse::<i64>() {
                        global.set_initializer(&t.const_int(v as u64, true));
                    } else if s == "true" {
                        global.set_initializer(&t.const_int(1, false));
                    } else if s == "false" {
                        global.set_initializer(&t.const_int(0, false));
                    } else {
                        global.set_initializer(&t.const_zero());
                    }
                } else {
                    global.set_initializer(&t.const_zero());
                }
            }
            BasicTypeEnum::FloatType(t) => {
                if let Some(s) = lit.and_then(|s| s.parse::<f64>().ok()) {
                    global.set_initializer(&t.const_float(s));
                } else {
                    global.set_initializer(&t.const_zero());
                }
            }
            BasicTypeEnum::PointerType(t) => global.set_initializer(&t.const_null()),
            BasicTypeEnum::ArrayType(t) => global.set_initializer(&t.const_zero()),
            BasicTypeEnum::StructType(t) => global.set_initializer(&t.const_zero()),
            BasicTypeEnum::VectorType(_) | BasicTypeEnum::ScalableVectorType(_) => {
                return Err(self.error(
                    "compile_variable_decl",
                    format!("cannot initialize global '{}': unsupported LLVM type", var.name),
                ));
            }
        }

        Ok(())
    }

    /// Create an alloca in the function's entry block BEFORE any terminator
    /// This ensures the alloca dominates all uses, satisfying LLVM's SSA requirements
    pub fn create_entry_alloca_preserving_terminator<T>(
        &self,
        type_: T,
        name: &str,
    ) -> Result<PointerValue<'ctx>, String>
    where
        T: inkwell::types::BasicType<'ctx>,
    {
        let insert_block = self.backend.builder.get_insert_block().unwrap();
        let function = insert_block.get_parent().unwrap();
        let entry_block = function.get_first_basic_block().unwrap();

        // Check if entry block has a terminator
        if let Some(terminator) = entry_block.get_terminator() {
            // Entry already terminated (e.g. MIR `entry -> br mir0`). Insert the
            // alloca before that terminator so it dominates all uses, then restore
            // the builder to the original insert block.
            self.backend.builder.position_before(&terminator);

            let alloca = self.backend.builder.build_alloca(type_, name)
                .map_err(|e| self.error("create_entry_alloca_preserving_terminator",
                    format!("failed to build alloca '{}': {}", name, e)))?;

            self.backend.builder.position_at_end(insert_block);
            Ok(alloca)
        } else {
            // No terminator - create at end of entry block
            let current_pos = self.backend.builder.get_insert_block();
            self.backend.builder.position_at_end(entry_block);

            let alloca = self.backend.builder.build_alloca(type_, name)
                .map_err(|e| self.error("create_entry_alloca_preserving_terminator",
                    format!("failed to build alloca '{}': {}", name, e)))?;

            if let Some(pos) = current_pos {
                self.backend.builder.position_at_end(pos);
            }

            Ok(alloca)
        }
    }
}
