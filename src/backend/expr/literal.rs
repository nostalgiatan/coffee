//! Literals, variables, arrays, tuples, structs, casts, and f-strings.

use crate::backend::codegen::CodeGenerator;
use crate::backend::type_inference;
use crate::backend::type_inference::TypeInferenceContext;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_literal_atom(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let expr = expr.trim();
        if let Ok(v) = type_inference::compile_literal_with_inference(
            self.backend.context,
            &self.backend.builder,
            expr,
            &TypeInferenceContext::new(),
        ) {
            return Ok(v);
        }
        if expr.contains("**") {
            return Err(self.error("compile_expression",
                "syntax error: '**' is not a valid operator in Coffee\n  = note: Coffee does not have a power operator".to_string()));
        }
        self.compile_variable_ref(expr)
    }

    pub(crate) fn compile_variable_ref(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
        if let Some(arg_num) = Self::parse_arg_number(expr) {
            if let (Some(argv), Some(argc)) = (self.main_argv, self.main_argc) {
                return self.get_command_line_arg(argv, argc, arg_num);
            }
            return Err(self.error("compile_expression",
                format!("command-line argument '{}' can only be used in the main() statement", expr)));
        }

        if let Some(&array_ptr) = self.array_allocas.get(expr) {
            self.used_variables.insert(expr.to_string());
            if self.array_sizes.contains_key(expr) {
                return Ok(array_ptr.into());
            }
            if let Some(&(ptr, var_type)) = self.variables.get(expr) {
                self.memory_ctx.record_use(expr);
                let value = self.backend.builder.build_load(var_type, ptr, expr)
                    .map_err(|e| self.error("compile_expression",
                        format!("failed to load slice '{}': {}", expr, e)))?;
                return Ok(value);
            }
            return self.load_slice_fat_from_parts(expr, array_ptr);
        }

        if let Some(&(ptr, var_type)) = self.variables.get(expr) {
            self.used_variables.insert(expr.to_string());
            self.memory_ctx.record_use(expr);
            self.note_mir_name_use(expr);
            if self.memory_ctx.is_moved(expr) {
                return Err(self.error("compile_expression",
                    format!("use of moved variable: '{}'", expr)));
            }
            if self.memory_ctx.is_dropped(expr) {
                return Err(self.error("compile_expression",
                    format!("use of dropped variable: '{}'", expr)));
            }
            if let BasicTypeEnum::StructType(_) | BasicTypeEnum::ArrayType(_) = var_type {
                return Ok(ptr.into());
            }
            let value = self.backend.builder.build_load(var_type, ptr, expr)
                .map_err(|e| self.error("compile_expression",
                    format!("failed to load variable '{}': {}", expr, e)))?;
            return Ok(value);
        }

        if let Some(gv) = self.backend.module.get_global(expr) {
            if let Some(init) = gv.get_initializer() {
                self.used_variables.insert(expr.to_string());
                let ty = init.get_type();
                let value = self.backend.builder.build_load(ty, gv.as_pointer_value(), expr)
                    .map_err(|e| self.error("compile_expression",
                        format!("failed to load global '{}': {}", expr, e)))?;
                return Ok(value);
            }
        }

        if expr.contains("::") {
            let parts: Vec<&str> = expr.split("::").collect();
            if parts.len() == 2 {
                return self.compile_enum_simple_variant(parts[0].trim(), parts[1].trim());
            }
        }

        let mut help_msg = String::new();
        if !self.variables.is_empty() {
            help_msg.push_str(&format!("\n  = note: available variables in current scope: {}",
                self.variables.keys().map(|k| format!("'{}'", k)).collect::<Vec<_>>().join(", ")));
        }
        Err(self.error("compile_expression",
            format!("cannot find value '{}' in this scope{}", expr, help_msg)))
    }

    pub(crate) fn compile_enum_simple_variant(&mut self, enum_name: &str, variant_name: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let global_name = format!("{}_{}", enum_name, variant_name);
        if let Some(global) = self.backend.module.get_global(&global_name) {
            let value = global.as_pointer_value();
            let loaded = self.backend.builder.build_load(
                self.backend.context.i64_type(),
                value,
                &global_name
            ).map_err(|e| self.error("compile_expression",
                format!("failed to load enum variant '{}': {}", global_name, e)))?;
            return Ok(loaded);
        }
        Err(self.error("compile_expression",
            format!("cannot find enum variant '{}::{}'", enum_name, variant_name)))
    }

    pub(crate) fn emit_empty_array_literal(
        &mut self,
        ty: &crate::types::Type,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let (elem_ty, size) = match ty {
            crate::types::Type::Array { elem, size } => (elem.as_ref().clone(), *size as u32),
            crate::types::Type::Slice(elem) => (elem.as_ref().clone(), 0),
            _ => (crate::types::Type::int(), 0),
        };
        let elem_llvm = self
            .coffee_type_to_llvm(&elem_ty.to_string())
            .map_err(|e| self.error("compile_expression", format!("empty array element type: {}", e)))?;
        let array_type = match elem_llvm {
            BasicTypeEnum::IntType(t) => t.array_type(size),
            BasicTypeEnum::FloatType(t) => t.array_type(size),
            BasicTypeEnum::PointerType(t) => t.array_type(size),
            BasicTypeEnum::StructType(t) => t.array_type(size),
            BasicTypeEnum::ArrayType(t) => t.array_type(size),
            BasicTypeEnum::VectorType(t) => t.array_type(size),
            other => {
                return Err(self.error(
                    "compile_expression",
                    format!("unsupported empty array element type {:?}", other),
                ))
            }
        };
        let alloca = self
            .backend
            .builder
            .build_alloca(array_type, "array_lit_empty")
            .map_err(|e| {
                self.error(
                    "compile_expression",
                    format!("failed to allocate empty array: {}", e),
                )
            })?;
        Ok(alloca.into())
    }

    pub(crate) fn emit_array_literal(
        &mut self,
        compiled: Vec<BasicValueEnum<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if compiled.is_empty() {
            return self.emit_empty_array_literal(&crate::types::Type::Array {
                elem: Box::new(crate::types::Type::int()),
                size: 0,
            });
        }
        let elem_type = compiled[0].get_type();
        let array_type = match elem_type {
            BasicTypeEnum::IntType(t) => t.array_type(compiled.len() as u32),
            BasicTypeEnum::FloatType(t) => t.array_type(compiled.len() as u32),
            BasicTypeEnum::PointerType(t) => t.array_type(compiled.len() as u32),
            BasicTypeEnum::StructType(t) => t.array_type(compiled.len() as u32),
            BasicTypeEnum::ArrayType(t) => t.array_type(compiled.len() as u32),
            BasicTypeEnum::VectorType(t) => t.array_type(compiled.len() as u32),
            other => {
                return Err(self.error(
                    "compile_expression",
                    format!("unsupported array element type {:?}", other),
                ))
            }
        };
        let alloca = self.backend.builder.build_alloca(array_type, "array_lit")
            .map_err(|e| self.error("compile_expression", format!("failed to allocate array literal: {}", e)))?;
        for (i, val) in compiled.iter().enumerate() {
            let idx = self.backend.context.i32_type().const_int(i as u64, false);
            let zero = self.backend.context.i32_type().const_int(0, false);
            let ptr = unsafe {
                self.backend.builder.build_in_bounds_gep(
                    array_type,
                    alloca,
                    &[zero, idx],
                    "elem_ptr",
                )
            }.map_err(|e| self.error("compile_expression", format!("GEP failed: {}", e)))?;
            self.backend.builder.build_store(ptr, *val)
                .map_err(|e| self.error("compile_expression", format!("store failed: {}", e)))?;
        }
        Ok(alloca.into())
    }

    pub(crate) fn emit_tuple_literal(
        &mut self,
        compiled_elements: Vec<BasicValueEnum<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let element_types: Vec<BasicTypeEnum> = compiled_elements.iter().map(|v| v.get_type()).collect();
        let tuple_type = self.backend.context.struct_type(&element_types, false);
        let tuple_ptr = self.backend.builder.build_alloca(tuple_type, "tuple")
            .map_err(|e| self.error("compile_expression", format!("failed to allocate space for tuple: {}", e)))?;
        let mut current_value = tuple_type.const_zero();
        for (i, elem) in compiled_elements.iter().enumerate() {
            let inserted = self.backend.builder.build_insert_value(
                current_value,
                *elem,
                i as u32,
                &format!("tuple_insert_{}", i)
            ).map_err(|e| self.error("compile_expression",
                format!("failed to insert tuple element {}: {}", i, e)))?;
            current_value = match inserted {
                inkwell::values::AggregateValueEnum::StructValue(v) => v,
                _ => return Err(self.error("compile_expression",
                    "insert_value did not return a StructValue".to_string())),
            };
        }
        self.backend.builder.build_store(tuple_ptr, current_value)
            .map_err(|e| self.error("compile_expression", format!("failed to store tuple value: {}", e)))?;
        self.backend.builder.build_load(tuple_type, tuple_ptr, "tuple_loaded")
            .map_err(|e| self.error("compile_expression", format!("failed to load tuple: {}", e)))
    }

    pub(crate) fn emit_struct_literal(
        &mut self,
        struct_name: &str,
        fields: Vec<(String, BasicValueEnum<'ctx>)>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let class_def = self.classes.get(struct_name)
            .ok_or_else(|| self.error("compile_struct_literal",
                format!("unknown class '{}'", struct_name)))?
            .clone();
        let struct_type = self.type_mapper.get_or_create_struct_type_from_class(struct_name, &class_def, &self.classes)
            .map_err(|e| self.error("compile_struct_literal", e))?;
        // `fn new` returns this pointer; a stack alloca would dangle after return.
        let alloca = if class_def.has_constructor {
            let malloc_fn = self.functions.get("malloc").copied().ok_or_else(|| {
                self.error("compile_struct_literal", "malloc function not found for heap allocation")
            })?;
            let size = self.llvm_target_data().get_store_size(&BasicTypeEnum::StructType(struct_type));
            let size_value = self.backend.context.i64_type().const_int(size, false);
            let heap_ptr = self.backend.builder.build_call(
                malloc_fn,
                &[size_value.into()],
                &format!("{}_malloc", struct_name),
            ).map_err(|e| self.error("compile_struct_literal",
                format!("failed to call malloc: {}", e)))?;
            match heap_ptr.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(BasicValueEnum::PointerValue(ptr)) => ptr,
                _ => {
                    return Err(self.error("compile_struct_literal", "malloc did not return a pointer"));
                }
            }
        } else {
            self.backend.builder.build_alloca(struct_type, &format!("{}_literal", struct_name))
                .map_err(|e| self.error("compile_struct_literal",
                    format!("failed to allocate struct '{}': {}", struct_name, e)))?
        };
        for (field_name, field_value) in fields {
            let field_index = self.type_mapper.get_field_index(struct_name, &field_name)
                .ok_or_else(|| self.error("compile_struct_literal",
                    format!("field '{}' not found in struct '{}'", field_name, struct_name)))?;
            let field_type = struct_type.get_field_type_at_index(field_index as u32)
                .ok_or_else(|| self.error("compile_struct_literal",
                    format!("field '{}' not found in struct '{}'", field_name, struct_name)))?;
            let field_ptr = unsafe {
                self.backend.builder.build_in_bounds_gep(
                    struct_type,
                    alloca,
                    &[
                        self.backend.context.i32_type().const_int(0, false),
                        self.backend.context.i32_type().const_int(field_index as u64, false),
                    ],
                    &format!("{}_{}_ptr", struct_name, field_name)
                ).map_err(|e| self.error("compile_struct_literal",
                    format!("failed to get field pointer: {}", e)))?
            };
            let converted_value = self.convert_value_to_type(field_value, field_type, &field_name)?;
            self.backend.builder.build_store(field_ptr, converted_value)
                .map_err(|e| self.error("compile_struct_literal",
                    format!("failed to store field '{}': {}", field_name, e)))?;
        }
        Ok(alloca.into())
    }

    pub(crate) fn emit_type_cast(
        &mut self,
        target_type: &str,
        value: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match target_type {
            "int" => match value {
                BasicValueEnum::FloatValue(f) => {
                    let result = self.backend.builder.build_float_to_signed_int(
                        f, self.backend.context.i64_type(), "fptosi"
                    ).map_err(|e| self.error("compile_expression",
                        format!("failed to convert float to int: {}", e)))?;
                    Ok(result.into())
                }
                BasicValueEnum::IntValue(i) => Ok(i.into()),
                _ => Err(self.error("compile_expression", "cannot convert type to int: unsupported type".to_string())),
            },
            "float" => match value {
                BasicValueEnum::IntValue(i) => {
                    let result = self.backend.builder.build_signed_int_to_float(
                        i, self.backend.context.f64_type(), "sitofp"
                    ).map_err(|e| self.error("compile_expression",
                        format!("failed to convert int to float: {}", e)))?;
                    Ok(result.into())
                }
                BasicValueEnum::FloatValue(f) => Ok(f.into()),
                _ => Err(self.error("compile_expression", "cannot convert type to float: unsupported type".to_string())),
            },
            "bool" => match value {
                BasicValueEnum::IntValue(i) => {
                    let result = self.backend.builder.build_int_truncate_or_bit_cast(
                        i, self.backend.context.i8_type(), "trunc"
                    ).map_err(|e| self.error("compile_expression",
                        format!("failed to convert to bool: {}", e)))?;
                    Ok(result.into())
                }
                BasicValueEnum::FloatValue(f) => {
                    let i64_val = self.backend.builder.build_float_to_signed_int(
                        f, self.backend.context.i64_type(), "fptosi"
                    ).map_err(|e| self.error("compile_expression",
                        format!("failed to convert float to bool: {}", e)))?;
                    let result = self.backend.builder.build_int_truncate_or_bit_cast(
                        i64_val, self.backend.context.i8_type(), "trunc"
                    ).map_err(|e| self.error("compile_expression",
                        format!("failed to convert to bool: {}", e)))?;
                    Ok(result.into())
                }
                _ => Err(self.error("compile_expression", "cannot convert type to bool: unsupported type".to_string())),
            },
            other => {
                let llvm_ty = self.coffee_type_to_llvm(other).map_err(|e| {
                    self.error(
                        "compile_expression",
                        format!("unknown type conversion function: {other}: {e}"),
                    )
                })?;
                self.convert_value_to_type(value, llvm_ty, "as_cast")
            }
        }
    }


    /// Compile an f-string from AST (template and placeholders already parsed).
    ///
    /// Two-pass snprintf: `snprintf(null, 0, ...)` for size, `malloc(n+1)`, then
    /// write. Negative return or truncation (`n >= size`) is `unreachable`.
    pub fn compile_fstring_from_ast(&mut self, template: &str, placeholders: &[String]) -> Result<BasicValueEnum<'ctx>, String> {
        if placeholders.is_empty() {
            return self.build_string_constant(template);
        }

        let snprintf_fn = self.ensure_fstring_builtin("snprintf")?;
        let malloc_fn = self.ensure_fstring_builtin("malloc")?;

        let mut spec_map: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        let mut loaded_args: Vec<inkwell::values::BasicMetadataValueEnum<'_>> = Vec::new();

        for placeholder in placeholders {
            self.used_variables.insert(placeholder.clone());
            self.memory_ctx.set_line(self.memory_ctx.current_line + 1);
            self.memory_ctx.record_use(placeholder);

            let (ptr, var_type) = *self.variables.get(placeholder)
                .ok_or_else(|| self.error("fstring",
                    format!("variable '{}' not found in scope", placeholder)))?;

            let coffee_ty = self.variable_types.get(placeholder).cloned();
            let spec = fstring_printf_spec(coffee_ty.as_deref(), fstring_llvm_kind(var_type))
                .map_err(|e| self.error("fstring", format!("{}: {}", placeholder, e)))?;
            spec_map.insert(placeholder.as_str(), spec);

            let mut value = self.backend.builder.build_load(
                var_type,
                ptr,
                placeholder
            ).map_err(|e| self.error("fstring",
                    format!("failed to load variable '{}': {}", placeholder, e)))?;

            value = self.promote_fstring_snprintf_arg(value, spec, coffee_ty.as_deref())?;
            loaded_args.push(value.into());
        }

        let format_cooked = rewrite_fstring_format(template, &spec_map);
        let format_str = self.build_string_constant(&format_cooked)?;

        let ptr_ty = self.backend.context.ptr_type(inkwell::AddressSpace::default());
        let null_ptr = ptr_ty.const_null();
        let i64_ty = self.backend.context.i64_type();
        let i32_ty = self.backend.context.i32_type();
        let zero_size = i64_ty.const_int(0, false);

        let mut size_args: Vec<inkwell::values::BasicMetadataValueEnum<'_>> = vec![
            null_ptr.into(),
            zero_size.into(),
            format_str.into(),
        ];
        size_args.extend(loaded_args.iter().copied());

        let needed = self.call_snprintf_i32(snprintf_fn, &size_args, "fstring_snprintf_size")?;
        let needed_neg = self.backend.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            needed,
            i32_ty.const_zero(),
            "fstring_size_neg",
        ).map_err(|e| self.error("fstring", format!("failed to compare snprintf size: {}", e)))?;
        self.fstring_trap_if(needed_neg, "fstring_size_fail", "fstring_size_ok")?;

        let needed_i64 = self.backend.builder.build_int_s_extend(needed, i64_ty, "fstring_n64")
            .map_err(|e| self.error("fstring", format!("failed to extend snprintf length: {}", e)))?;
        let buf_size = self.backend.builder.build_int_add(
            needed_i64,
            i64_ty.const_int(1, false),
            "fstring_buf_size",
        ).map_err(|e| self.error("fstring", format!("failed to add snprintf NUL: {}", e)))?;

        let malloc_call = self.backend.builder.build_call(
            malloc_fn,
            &[buf_size.into()],
            "fstring_malloc",
        ).map_err(|e| self.error("fstring", format!("failed to call malloc: {}", e)))?;
        let heap_ptr = match malloc_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(BasicValueEnum::PointerValue(p)) => p,
            _ => return Err(self.error("fstring", "malloc did not return a pointer")),
        };
        let heap_int = self.backend.builder.build_ptr_to_int(heap_ptr, i64_ty, "fstring_heap_int")
            .map_err(|e| self.error("fstring", format!("failed to ptrtoint malloc: {}", e)))?;
        let malloc_null = self.backend.builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            heap_int,
            i64_ty.const_zero(),
            "fstring_malloc_null",
        ).map_err(|e| self.error("fstring", format!("failed to compare malloc: {}", e)))?;
        self.fstring_trap_if(malloc_null, "fstring_malloc_fail", "fstring_malloc_ok")?;

        let mut write_args: Vec<inkwell::values::BasicMetadataValueEnum<'_>> = vec![
            heap_ptr.into(),
            buf_size.into(),
            format_str.into(),
        ];
        write_args.extend(loaded_args.iter().copied());

        let written = self.call_snprintf_i32(snprintf_fn, &write_args, "fstring_snprintf")?;
        let written_neg = self.backend.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            written,
            i32_ty.const_zero(),
            "fstring_write_neg",
        ).map_err(|e| self.error("fstring", format!("failed to compare snprintf write: {}", e)))?;
        let written_i64 = self.backend.builder.build_int_s_extend(written, i64_ty, "fstring_written64")
            .map_err(|e| self.error("fstring", format!("failed to extend snprintf write: {}", e)))?;
        let truncated = self.backend.builder.build_int_compare(
            inkwell::IntPredicate::SGE,
            written_i64,
            buf_size,
            "fstring_truncated",
        ).map_err(|e| self.error("fstring", format!("failed to compare truncation: {}", e)))?;
        let write_bad = self.backend.builder.build_or(written_neg, truncated, "fstring_write_bad")
            .map_err(|e| self.error("fstring", format!("failed to or snprintf checks: {}", e)))?;
        self.fstring_trap_if(write_bad, "fstring_write_fail", "fstring_write_ok")?;

        Ok(heap_ptr.into())
    }

    fn ensure_fstring_builtin(
        &mut self,
        name: &str,
    ) -> Result<inkwell::values::FunctionValue<'ctx>, String> {
        if !self.functions.contains_key(name) {
            self.used_c_functions.insert(name.to_string());
            self.declare_external_function(name)?;
        }
        self.functions
            .get(name)
            .copied()
            .ok_or_else(|| self.error("fstring", format!("{} function not declared", name)))
    }

    fn call_snprintf_i32(
        &mut self,
        snprintf_fn: inkwell::values::FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        let call = self.backend.builder.build_call(snprintf_fn, args, name)
            .map_err(|e| self.error("fstring", format!("failed to build snprintf call: {}", e)))?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(BasicValueEnum::IntValue(v)) => Ok(v),
            inkwell::values::ValueKind::Instruction(_) => {
                Err(self.error("fstring", "snprintf returned void"))
            }
            _ => Err(self.error("fstring", "snprintf returned unexpected type")),
        }
    }

    fn fstring_trap_if(
        &mut self,
        cond: inkwell::values::IntValue<'ctx>,
        trap_name: &str,
        cont_name: &str,
    ) -> Result<(), String> {
        let function = self
            .backend
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or_else(|| self.error("fstring", "no parent function for trap"))?;
        let trap = self.backend.context.append_basic_block(function, trap_name);
        let cont = self.backend.context.append_basic_block(function, cont_name);
        self.backend.builder.build_conditional_branch(cond, trap, cont)
            .map_err(|e| self.error("fstring", format!("failed to build trap branch: {}", e)))?;
        self.backend.builder.position_at_end(trap);
        self.backend.builder.build_unreachable()
            .map_err(|e| self.error("fstring", format!("failed to build unreachable: {}", e)))?;
        self.backend.builder.position_at_end(cont);
        Ok(())
    }

    fn promote_fstring_snprintf_arg(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &str,
        coffee_type: Option<&str>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match value {
            BasicValueEnum::IntValue(i) => {
                let bits = i.get_type().get_bit_width();
                let unsigned = coffee_type_is_unsigned(coffee_type)
                    || coffee_type == Some("bool");
                if matches!(spec, "%lld" | "%llu") && bits < 64 {
                    let i64t = self.backend.context.i64_type();
                    let ext = if unsigned {
                        self.backend.builder.build_int_z_extend(i, i64t, "fstring_zext64")
                    } else {
                        self.backend.builder.build_int_s_extend(i, i64t, "fstring_sext64")
                    }.map_err(|e| self.error("fstring", format!("failed to extend int for snprintf: {}", e)))?;
                    return Ok(ext.into());
                }
                if bits < 32 {
                    let i32t = self.backend.context.i32_type();
                    let ext = if unsigned {
                        self.backend.builder.build_int_z_extend(i, i32t, "fstring_zext32")
                    } else {
                        self.backend.builder.build_int_s_extend(i, i32t, "fstring_sext32")
                    }.map_err(|e| self.error("fstring", format!("failed to promote int for snprintf: {}", e)))?;
                    return Ok(ext.into());
                }
                Ok(value)
            }
            BasicValueEnum::FloatValue(f) if spec == "%g" && f.get_type().get_bit_width() < 64 => {
                let f64t = self.backend.context.f64_type();
                let ext = self.backend.builder.build_float_ext(f, f64t, "fstring_fpext")
                    .map_err(|e| self.error("fstring", format!("failed to promote float for snprintf: {}", e)))?;
                Ok(ext.into())
            }
            _ => Ok(value),
        }
    }

    /// Process escape sequences in a string literal
    pub(crate) fn process_escape_sequences(s: &str) -> Vec<u8> {
        let mut result = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    match next {
                        'n' => result.push(b'\n'),
                        't' => result.push(b'\t'),
                        'r' => result.push(b'\r'),
                        '\\' => result.push(b'\\'),
                        '"' => result.push(b'"'),
                        '\'' => result.push(b'\''),
                        '0' => result.push(b'\0'),
                        'x' => {
                            // Hex escape: \xHH
                            let mut hex_str = String::new();
                            for _ in 0..2 {
                                if let Some(&h) = chars.peek() {
                                    if h.is_ascii_hexdigit() {
                                        hex_str.push(chars.next().unwrap());
                                    }
                                }
                            }
                            if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                                result.push(byte);
                            }
                        }
                        _ => {
                            // Unknown escape, keep as-is
                            result.push(b'\\');
                            result.push(next as u8);
                        }
                    }
                } else {
                    result.push(b'\\');
                }
            } else {
                result.push(c as u8);
            }
        }

        result
    }

    /// Build string constant
    /// Build string concatenation: str1 + str2
    pub(crate) fn build_string_concat(&mut self, left: PointerValue<'ctx>, right: PointerValue<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        // Get or declare strlen function
        if !self.functions.contains_key("strlen") {
            self.used_c_functions.insert("strlen".to_string());
        }
        let strlen_fn = if self.functions.contains_key("strlen") {
            *self.functions.get("strlen").unwrap()
        } else {
            // Declare it
            self.declare_external_function("strlen")?
        };

        // Get or declare strcpy function
        if !self.functions.contains_key("strcpy") {
            self.used_c_functions.insert("strcpy".to_string());
        }
        let strcpy_fn = if self.functions.contains_key("strcpy") {
            *self.functions.get("strcpy").unwrap()
        } else {
            // Declare it
            self.declare_external_function("strcpy")?
        };

        // Get or declare strcat function
        if !self.functions.contains_key("strcat") {
            self.used_c_functions.insert("strcat".to_string());
        }
        let strcat_fn = if self.functions.contains_key("strcat") {
            *self.functions.get("strcat").unwrap()
        } else {
            // Declare it
            self.declare_external_function("strcat")?
        };

        // Get or declare malloc function
        if !self.functions.contains_key("malloc") {
            self.used_c_functions.insert("malloc".to_string());
        }
        let malloc_fn = if self.functions.contains_key("malloc") {
            *self.functions.get("malloc").unwrap()
        } else {
            // Declare it
            self.declare_external_function("malloc")?
        };

        // Calculate lengths of both strings
        let left_len_call = self.backend.builder.build_call(strlen_fn, &[left.into()], "left_len")
            .map_err(|e| self.error("string_concat", format!("failed to call strlen: {}", e)))?;

        let left_len = match left_len_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("string_concat", "strlen returned void"));
            }
        };

        let right_len_call = self.backend.builder.build_call(strlen_fn, &[right.into()], "right_len")
            .map_err(|e| self.error("string_concat", format!("failed to call strlen: {}", e)))?;

        let right_len = match right_len_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("string_concat", "strlen returned void"));
            }
        };

        // Calculate total length (left_len + right_len + 1 for null terminator)
        let total_len = self.backend.builder.build_int_add(
            left_len.into_int_value(),
            right_len.into_int_value(),
            "total_len"
        ).map_err(|e| self.error("string_concat", format!("failed to build int add: {}", e)))?;
        let total_len = self.backend.builder.build_int_add(
            total_len,
            self.backend.context.i64_type().const_int(1, false),
            "total_len_with_null"
        ).map_err(|e| self.error("string_concat", format!("failed to build int add: {}", e)))?;

        // Allocate memory for concatenated string
        let result_ptr_call = self.backend.builder.build_call(malloc_fn, &[total_len.into()], "result_ptr")
            .map_err(|e| self.error("string_concat", format!("failed to call malloc: {}", e)))?;

        let result_ptr = match result_ptr_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("string_concat", "malloc returned void"));
            }
        };

        let result_ptr = result_ptr.into_pointer_value();

        // Copy left string to result
        self.backend.builder.build_call(strcpy_fn, &[result_ptr.into(), left.into()], "strcpy_left")
            .map_err(|e| self.error("string_concat", format!("failed to call strcpy: {}", e)))?;

        // Concatenate right string to result
        self.backend.builder.build_call(strcat_fn, &[result_ptr.into(), right.into()], "strcat_right")
            .map_err(|e| self.error("string_concat", format!("failed to call strcat: {}", e)))?;

        Ok(result_ptr.into())
    }

    pub(crate) fn build_string_constant(&mut self, s: &str) -> Result<BasicValueEnum<'ctx>, String> {
        if let Some(&ptr) = self.string_constants.get(s) {
            return Ok(ptr.into());
        }

        // HIGH-6 FIX: Limit string length to prevent DoS via huge strings
        const MAX_STRING_LENGTH: usize = 10 * 1024 * 1024; // 10 MB max string
        if s.len() > MAX_STRING_LENGTH {
            return Err(self.error("build_string_constant",
                format!("String constant exceeds maximum length\n  = note: string length: {} bytes, maximum: {} bytes\n  = help: use shorter strings",
                    s.len(), MAX_STRING_LENGTH)));
        }

        // LOW-2 FIX: Limit number of string constants to prevent DoS
        const MAX_STRING_CONSTANTS: usize = 10000;
        if self.string_constants.len() >= MAX_STRING_CONSTANTS {
            return Err(self.error("build_string_constant",
                format!("Too many string constants in program\n  = note: current string constants: {}, maximum: {}\n  = help: reduce number of string literals or reuse existing ones",
                    self.string_constants.len(), MAX_STRING_CONSTANTS)));
        }

        // Process escape sequences
        let bytes = Self::process_escape_sequences(s);
        let string_val = self.backend.context.const_string(&bytes, true);
        let global = self.backend.module.add_global(
            string_val.get_type(),
            Some(inkwell::AddressSpace::default()),
            "str"
        );
        global.set_initializer(&string_val);
        global.set_constant(true);

        let ptr = global.as_pointer_value();
        self.string_constants.insert(s.to_string(), ptr);

        Ok(ptr.into())
    }
}

/// LLVM type kind used only to pick an f-string printf specifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FstringLlvmKind {
    Int { bits: u32 },
    Float { bits: u32 },
    Pointer,
    Other,
}

fn coffee_int_printf_spec(t: &str) -> Option<&'static str> {
    let rest = t.strip_prefix("int(")?;
    let (n, signed) = if let Some(inner) = rest.strip_suffix(")+") {
        (inner, true)
    } else if let Some(inner) = rest.strip_suffix(")-") {
        (inner, false)
    } else {
        return None;
    };
    let bytes: u32 = n.parse().ok()?;
    Some(match (bytes, signed) {
        (1, true) => "%hhd",
        (1, false) => "%hhu",
        (2, true) => "%hd",
        (2, false) => "%hu",
        (4, true) => "%d",
        (4, false) => "%u",
        (_, true) => "%lld",
        (_, false) => "%llu",
    })
}

/// Choose a printf conversion specifier from the Coffee type when known,
/// otherwise from the loaded LLVM type.
pub(crate) fn fstring_printf_spec(
    coffee_type: Option<&str>,
    llvm: FstringLlvmKind,
) -> Result<&'static str, String> {
    if let Some(ty) = coffee_type {
        let t = ty.trim();
        if t == "bool" {
            return Ok("%d");
        }
        if t == "str" || t == "string" {
            return Ok("%s");
        }
        if t == "float" || t.starts_with("float(") {
            return Ok("%g");
        }
        if t == "int" {
            return Ok("%lld");
        }
        if let Some(spec) = coffee_int_printf_spec(t) {
            return Ok(spec);
        }
        if t == "ptr" || t == "object" || t.starts_with('&') {
            return Ok("%s");
        }
    }
    match llvm {
        FstringLlvmKind::Float { .. } => Ok("%g"),
        FstringLlvmKind::Pointer => Ok("%s"),
        FstringLlvmKind::Int { bits } => Ok(match bits {
            1 => "%d",
            8 => "%hhd",
            16 => "%hd",
            32 => "%d",
            _ => "%lld",
        }),
        FstringLlvmKind::Other => Err(
            "unsupported f-string placeholder type (need int, float, bool, str, or pointer)".to_string(),
        ),
    }
}

fn fstring_llvm_kind(ty: BasicTypeEnum<'_>) -> FstringLlvmKind {
    match ty {
        BasicTypeEnum::IntType(t) => FstringLlvmKind::Int {
            bits: t.get_bit_width(),
        },
        BasicTypeEnum::FloatType(t) => FstringLlvmKind::Float {
            bits: t.get_bit_width(),
        },
        BasicTypeEnum::PointerType(_) => FstringLlvmKind::Pointer,
        _ => FstringLlvmKind::Other,
    }
}

fn coffee_type_is_unsigned(coffee_type: Option<&str>) -> bool {
    coffee_type
        .map(|t| t.trim().ends_with('-') && t.trim().starts_with("int("))
        .unwrap_or(false)
}

/// Rewrite `{name}` (and `{{` / `}}`) into a snprintf format string.
/// Literal `%` in the template is doubled so snprintf does not treat it as a spec.
pub(crate) fn rewrite_fstring_format(
    template: &str,
    specs: &std::collections::HashMap<&str, &str>,
) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            out.push_str("%%");
            continue;
        }
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                out.push('{');
                continue;
            }
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next == '}' {
                    chars.next();
                    break;
                }
                name.push(chars.next().unwrap());
            }
            if let Some(spec) = specs.get(name.as_str()) {
                out.push_str(spec);
            } else {
                out.push('{');
                out.push_str(&name);
                out.push('}');
            }
            continue;
        }
        if c == '}' {
            if chars.peek() == Some(&'}') {
                chars.next();
            }
            out.push('}');
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod fstring_spec_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn int_default_is_lld() {
        assert_eq!(
            fstring_printf_spec(Some("int"), FstringLlvmKind::Int { bits: 64 }).unwrap(),
            "%lld"
        );
    }

    #[test]
    fn int_widths_match_printf() {
        assert_eq!(
            fstring_printf_spec(Some("int(1)+"), FstringLlvmKind::Int { bits: 8 }).unwrap(),
            "%hhd"
        );
        assert_eq!(
            fstring_printf_spec(Some("int(2)+"), FstringLlvmKind::Int { bits: 16 }).unwrap(),
            "%hd"
        );
        assert_eq!(
            fstring_printf_spec(Some("int(4)+"), FstringLlvmKind::Int { bits: 32 }).unwrap(),
            "%d"
        );
        assert_eq!(
            fstring_printf_spec(Some("int(4)-"), FstringLlvmKind::Int { bits: 32 }).unwrap(),
            "%u"
        );
        assert_eq!(
            fstring_printf_spec(Some("int(8)-"), FstringLlvmKind::Int { bits: 64 }).unwrap(),
            "%llu"
        );
    }

    #[test]
    fn float_bool_str_and_ptr() {
        assert_eq!(
            fstring_printf_spec(Some("float"), FstringLlvmKind::Float { bits: 64 }).unwrap(),
            "%g"
        );
        assert_eq!(
            fstring_printf_spec(Some("float(4)"), FstringLlvmKind::Float { bits: 32 }).unwrap(),
            "%g"
        );
        assert_eq!(
            fstring_printf_spec(Some("bool"), FstringLlvmKind::Int { bits: 8 }).unwrap(),
            "%d"
        );
        assert_eq!(
            fstring_printf_spec(Some("str"), FstringLlvmKind::Pointer).unwrap(),
            "%s"
        );
        assert_eq!(
            fstring_printf_spec(None, FstringLlvmKind::Pointer).unwrap(),
            "%s"
        );
    }

    #[test]
    fn llvm_fallback_without_coffee_type() {
        assert_eq!(
            fstring_printf_spec(None, FstringLlvmKind::Int { bits: 64 }).unwrap(),
            "%lld"
        );
        assert_eq!(
            fstring_printf_spec(None, FstringLlvmKind::Float { bits: 64 }).unwrap(),
            "%g"
        );
    }

    #[test]
    fn rewrite_placeholder_to_spec() {
        let mut specs = HashMap::new();
        specs.insert("name", "%s");
        assert_eq!(rewrite_fstring_format("Hello {name}", &specs), "Hello %s");
    }

    #[test]
    fn rewrite_int_and_no_placeholder_shape() {
        let mut specs = HashMap::new();
        specs.insert("n", "%lld");
        assert_eq!(rewrite_fstring_format("n={n}", &specs), "n=%lld");
        let empty: HashMap<&str, &str> = HashMap::new();
        assert_eq!(rewrite_fstring_format("plain", &empty), "plain");
    }
}
