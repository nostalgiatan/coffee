//! Literals, variables, arrays, tuples, structs, casts, and f-strings.

use crate::backend::codegen::CodeGenerator;
use crate::backend::type_inference;
use crate::backend::type_inference::TypeInferenceContext;
use crate::parser::expr::Expression;
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

        if let Some(&(ptr, var_type)) = self.variables.get(expr) {
            self.used_variables.insert(expr.to_string());
            self.memory_ctx.record_use(expr);
            if self.memory_ctx.is_moved(expr) {
                return Err(self.error("compile_expression",
                    format!("use of moved variable: '{}'", expr)));
            }
            if self.memory_ctx.is_dropped(expr) {
                return Err(self.error("compile_expression",
                    format!("use of dropped variable: '{}'", expr)));
            }
            if let BasicTypeEnum::StructType(_) = var_type {
                return Ok(ptr.into());
            }
            let value = self.backend.builder.build_load(var_type, ptr, expr)
                .map_err(|e| self.error("compile_expression",
                    format!("failed to load variable '{}': {}", expr, e)))?;
            return Ok(value);
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

    pub(crate) fn compile_array_literal_expr(&mut self, elements: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        let mut compiled = Vec::new();
        for elem in elements {
            compiled.push(self.compile_expr(elem)?);
        }
        if compiled.is_empty() {
            return Err(self.error("compile_expression", "empty array literal"));
        }
        let elem_type = compiled[0].get_type();
        let array_type = match elem_type {
            BasicTypeEnum::IntType(t) => t.array_type(compiled.len() as u32),
            _ => return Err(self.error("compile_expression", "unsupported array element type")),
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

    pub(crate) fn compile_tuple_literal_expr(&mut self, elements: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        let mut compiled_elements = Vec::new();
        for elem in elements {
            compiled_elements.push(self.compile_expr(elem)?);
        }
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

    pub(crate) fn compile_struct_literal_from_ast(
        &mut self,
        struct_name: &str,
        fields: &[(String, Expression)],
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let class_def = self.classes.get(struct_name)
            .ok_or_else(|| self.error("compile_struct_literal",
                format!("unknown class '{}'", struct_name)))?
            .clone();
        let struct_type = self.type_mapper.get_or_create_struct_type_from_class(struct_name, &class_def, &self.classes)
            .ok_or_else(|| self.error("compile_struct_literal",
                format!("failed to create struct type for '{}'", struct_name)))?;
        let alloca = self.backend.builder.build_alloca(struct_type, &format!("{}_literal", struct_name))
            .map_err(|e| self.error("compile_struct_literal",
                format!("failed to allocate struct '{}': {}", struct_name, e)))?;
        for (field_name, field_value_expr) in fields {
            let field_value = if field_name == "_" || matches!(field_value_expr, Expression::Variable(v) if v == "_") {
                self.backend.context.i64_type().const_int(0, false).into()
            } else {
                self.compile_expr(field_value_expr)?
            };
            let field_index = self.type_mapper.get_field_index(struct_name, field_name)
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
            let converted_value = self.convert_value_to_type(field_value, field_type, field_name)?;
            self.backend.builder.build_store(field_ptr, converted_value)
                .map_err(|e| self.error("compile_struct_literal",
                    format!("failed to store field '{}': {}", field_name, e)))?;
        }
        Ok(alloca.into())
    }

    pub(crate) fn compile_type_cast_expr(&mut self, target_type: &str, value: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        let value = self.compile_expr(value)?;
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
            other => Err(self.error("compile_expression", format!("unknown type conversion function: {}", other))),
        }
    }


    /// Compile an f-string from AST (template and placeholders already parsed)
    pub fn compile_fstring_from_ast(&mut self, template: &str, placeholders: &[String]) -> Result<BasicValueEnum<'ctx>, String> {
        // Build format string and load placeholder values
        if placeholders.is_empty() {
            // No placeholders, just return the string constant
            return self.build_string_constant(template);
        }

        // Get or declare snprintf function
        if !self.functions.contains_key("snprintf") {
            self.used_c_functions.insert("snprintf".to_string());
            crate::backend::functions::declare_builtin_c_function(
                "snprintf",
                self.backend.context,
                &self.backend.module,
                &mut self.functions,
            )?;
        }

        let snprintf_fn = *self.functions.get("snprintf")
            .ok_or_else(|| self.error("fstring", "snprintf function not declared"))?;

        // Calculate required buffer size
        // Estimate: template length + placeholder values max length
        // For safety, use a reasonable limit (4KB)
        let template_len = template.len();
        let estimated_size = template_len + 1024; // Extra space for placeholder values
        
        // Cap buffer size to prevent stack overflow
        const MAX_FSTRING_SIZE: usize = 4096; // 4KB max
        let buffer_size = std::cmp::min(estimated_size, MAX_FSTRING_SIZE);
        
        // Allocate stack buffer for the formatted string
        let buffer_size_const = self.backend.context.i64_type().const_int(buffer_size as u64, false);
        let buffer_type = self.backend.context.i8_type().array_type(buffer_size as u32);
        let buffer_alloca = self.backend.builder.build_alloca(buffer_type, "fstring_buffer")
            .map_err(|e| self.error("fstring",
                format!("failed to allocate fstring buffer: {}", e)))?;

        // Get pointer to the buffer using GEP
        let buffer_ptr = unsafe {
            self.backend.builder.build_in_bounds_gep(
                buffer_type,
                buffer_alloca,
                &[self.backend.context.i32_type().const_int(0, false), self.backend.context.i32_type().const_int(0, false)],
                "fstring_buffer_ptr"
            ).map_err(|e| self.error("fstring",
                format!("failed to get buffer pointer: {}", e)))?
        };

        // Build format string
        let format_str = self.build_string_constant(template)?;

        // Build snprintf arguments
        let mut snprintf_args: Vec<inkwell::values::BasicMetadataValueEnum<'_>> = vec![
            buffer_ptr.into(),
            buffer_size_const.into(),
            format_str.into()
        ];

        // Load each placeholder variable
        for placeholder in placeholders {
            // HIGH-10 FIX: Mark placeholder variable as used
            self.used_variables.insert(placeholder.clone());

            // Record variable use for lifetime tracking
            // Update current line to ensure it's not zero
            self.memory_ctx.set_line(self.memory_ctx.current_line + 1);
            self.memory_ctx.record_use(placeholder);

            let &(ptr, var_type) = self.variables.get(placeholder)
                .ok_or_else(|| self.error("fstring",
                    format!("variable '{}' not found in scope", placeholder)))?;

            let value = self.backend.builder.build_load(
                var_type,
                ptr,
                placeholder
            ).map_err(|e| self.error("fstring",
                    format!("failed to load variable '{}': {}", placeholder, e)))?;

            snprintf_args.push(value.into());
        }

        // Call snprintf to format the string
        let snprintf_call = self.backend.builder.build_call(
            snprintf_fn,
            &snprintf_args,
            "fstring_snprintf"
        ).map_err(|e| self.error("fstring",
            format!("failed to build snprintf call: {}", e)))?;

        // Get the return value (number of characters written, excluding null terminator)
        let bytes_written = match snprintf_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("fstring", "snprintf returned void"));
            }
        };

        let bytes_written = match bytes_written {
            inkwell::values::BasicValueEnum::IntValue(val) => val,
            _ => return Err(self.error("fstring", "snprintf returned unexpected type")),
        };

        // Create a constant 0 for comparison
        let zero = self.backend.context.i32_type().const_int(0, false);
        
        // Check if snprintf returned negative value (error)
        let is_negative = self.backend.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            bytes_written,
            zero,
            "is_negative"
        ).map_err(|e| self.error("fstring",
            format!("failed to build comparison: {}", e)))?;

        // Build error message for buffer overflow
        let error_msg = self.build_string_constant(
            "Error: f-string buffer overflow - formatted string too long\n"
        )?;

        // Get printf function for error reporting
        if !self.functions.contains_key("printf") {
            self.used_c_functions.insert("printf".to_string());
            crate::backend::functions::declare_builtin_c_function(
                "printf",
                self.backend.context,
                &self.backend.module,
                &mut self.functions,
            )?;
        }

        let printf_fn = *self.functions.get("printf")
            .ok_or_else(|| self.error("fstring", "printf function not declared"))?;

        // If snprintf failed (returned negative), print error and use empty string
        let error_block = self.backend.context.append_basic_block(
            self.backend.builder.get_insert_block().unwrap().get_parent().unwrap(),
            "fstring_error"
        );
        
        let success_block = self.backend.context.append_basic_block(
            self.backend.builder.get_insert_block().unwrap().get_parent().unwrap(),
            "fstring_success"
        );

        // Conditional branch: if negative, go to error block
        self.backend.builder.build_conditional_branch(
            is_negative,
            error_block,
            success_block
        ).map_err(|e| self.error("fstring",
            format!("failed to build conditional branch: {}", e)))?;

        // Error block: print error message and use empty string
        self.backend.builder.position_at_end(error_block);
        self.backend.builder.build_call(
            printf_fn,
            &[error_msg.into()],
            "print_error"
        ).map_err(|e| self.error("fstring",
            format!("failed to build printf call: {}", e)))?;
        
        // Set first character to null (empty string)
        let null_val = self.backend.context.i8_type().const_int(0, false);
        self.backend.builder.build_store(
            buffer_ptr,
            null_val
        ).map_err(|e| self.error("fstring",
            format!("failed to store null terminator: {}", e)))?;

        self.backend.builder.build_unconditional_branch(success_block)
            .map_err(|e| self.error("fstring",
                format!("failed to build unconditional branch: {}", e)))?;

        // Success block: continue with the formatted string
        self.backend.builder.position_at_end(success_block);

        // Return the buffer pointer (string type)
        Ok(buffer_ptr.into())
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
