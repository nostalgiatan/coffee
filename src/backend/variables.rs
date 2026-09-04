//! Variable and assignment compilation for Coffee compiler
//!
//! Handles variable declarations, assignments, and array operations.

use super::codegen::CodeGenerator;
use super::memory_ops::VariableState;
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::types::{BasicTypeEnum, BasicType};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile let statement
    pub fn compile_let_statement(&mut self, line: &str) -> Result<(), String> {
        // MEDIUM-7 FIX: Limit number of local variables to prevent DoS
        const MAX_LOCAL_VARS: usize = 1000;
        if self.variables.len() >= MAX_LOCAL_VARS {
            return Err(self.error("compile_let",
                format!("Too many local variables in function\n  = note: current variables: {}, maximum: {}\n  = help: reduce number of local variables or use heap allocation",
                    self.variables.len(), MAX_LOCAL_VARS)));
        }

        // Parse: let name: type = value
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(self.error("compile_let",
                format!("invalid let statement syntax '{}' - expected 'let name: type = value'", line)));
        }

        let decl_part = parts[0].trim().trim_start_matches("let ").trim();
        let value_part = parts[1].trim();

        let (name, type_str) = if decl_part.contains(':') {
            let parts: Vec<&str> = decl_part.splitn(2, ':').collect();
            (parts[0].trim(), parts[1].trim())
        } else {
            (decl_part, "int") // Default to int
        };

        // Check if this is an array type
        if type_str.starts_with('[') && type_str.ends_with(']') {
            return self.compile_array_declaration(name, type_str, value_part);
        }

        let llvm_type = self.coffee_type_to_llvm(type_str)
            .map_err(|e| self.error("compile_let", format!("failed to resolve type '{}' for variable '{}': {}", type_str, name, e)))?;

        // CRITICAL-5 FIX: Check stack size before allocation to prevent stack overflow
        const MAX_STACK_SIZE: usize = 10 * 1024 * 1024; // 10 MB max stack per function
        const MAX_VAR_SIZE: usize = 1024 * 1024; // 1 MB max per variable

        let _type_size = llvm_type.size_of().ok_or_else(|| self.error("compile_let",
            "cannot get type size - type is unsized or too large".to_string()))?;

        // Check if this is a constant value (compile-time known size)
        // For simplicity, use conservative estimates for each type
        let alloc_size = match llvm_type {
            BasicTypeEnum::IntType(t) => (t.get_bit_width() / 8) as usize,
            BasicTypeEnum::FloatType(t) => {
                if t.get_bit_width() == 64 { 8 } else { 4 }
            }
            BasicTypeEnum::PointerType(_) => 8, // Assume 64-bit pointers
            BasicTypeEnum::ArrayType(_t) => {
                // Arrays are handled separately in compile_array_declaration
                return Err(self.error("compile_let",
                    "arrays should use compile_array_declaration".to_string()));
            }
            BasicTypeEnum::StructType(t) => {
                // For structs, be conservative - use reasonable limit
                (t.count_fields() * 8) as usize
            }
            BasicTypeEnum::VectorType(t) => {
                (t.get_size() * 8) as usize
            }
            BasicTypeEnum::ScalableVectorType(_) => {
                // Scalable vectors - be conservative
                1024 // 1 KB default
            }
        };

        if alloc_size > MAX_VAR_SIZE {
            return Err(self.error("compile_let",
                format!("Variable size exceeds maximum\n  = note: variable size: {} bytes, maximum: {} bytes\n  = help: use smaller types or heap allocation",
                    alloc_size, MAX_VAR_SIZE)));
        }

        if self.current_stack_size + alloc_size > MAX_STACK_SIZE {
            return Err(self.error("compile_let",
                format!("Stack allocation would exceed maximum stack size\n  = note: current stack usage: {} bytes, allocation: {} bytes, maximum: {} bytes\n  = help: reduce number or size of local variables to prevent stack overflow",
                    self.current_stack_size, alloc_size, MAX_STACK_SIZE)));
        }

        let alloca = self.backend.builder.build_alloca(llvm_type, name)
            .map_err(|e| self.error("compile_let", format!("failed to allocate variable '{}': {}", name, e)))?;

        // HIGH-12 FIX: Zero-initialize immediately after allocation to prevent use of uninitialized memory
        // This defense-in-depth approach ensures variables are never uninitialized, even if errors occur
        match llvm_type {
            BasicTypeEnum::IntType(t) => {
                let zero = t.const_zero();
                self.backend.builder.build_store(alloca, zero)
                    .map_err(|e| self.error("compile_let", format!("failed to zero-initialize variable '{}': {}", name, e)))?;
            }
            BasicTypeEnum::FloatType(t) => {
                let zero = t.const_zero();
                self.backend.builder.build_store(alloca, zero)
                    .map_err(|e| self.error("compile_let", format!("failed to zero-initialize variable '{}': {}", name, e)))?;
            }
            BasicTypeEnum::PointerType(t) => {
                let zero = t.const_zero();
                self.backend.builder.build_store(alloca, zero)
                    .map_err(|e| self.error("compile_let", format!("failed to zero-initialize variable '{}': {}", name, e)))?;
            }
            _ => {
                // For other types (arrays, structs), try to store a zero value
                // Since we can't directly create zeros for all types, we'll use a simpler approach:
                // Just initialize with the first value that will be stored
                // The actual initialization will happen when the value is compiled and stored
                // For arrays and structs, the existing initialization logic handles this
            }
        }

        // Update stack size tracking
        self.current_stack_size += alloc_size;

        let value = self.compile_expression_str(value_part)
            .map_err(|e| self.error("compile_let", format!("failed to compile initial value '{}' for variable '{}': {}", value_part, name, e)))?;

        self.backend.builder.build_store(alloca, value)
            .map_err(|e| self.error("compile_let", format!("failed to store initial value in variable '{}': {}", name, e)))?;

        self.variables.insert(name.to_string(), (alloca, llvm_type));
        
        // Store the class name if this is a class type
        // This is needed for method calls to find the correct method
        eprintln!("DEBUG: compile_let_statement: name='{}', type_str='{}'", name, type_str);
        if type_str.chars().all(|c| c.is_alphanumeric() || c == '_') {
            // Check if this is a class type (not a primitive type like int, float, bool, string)
            if !matches!(type_str, "int" | "float" | "bool" | "string" | "void" | "()") {
                eprintln!("DEBUG: compile_let_statement: inserting class_name '{}' for variable '{}'", type_str, name);
                self.variable_types.insert(name.to_string(), type_str.to_string());
            }
        }

        Ok(())
    }

    /// Compile array declaration with initialization
    pub fn compile_array_declaration(&mut self, name: &str, type_str: &str, value_part: &str) -> Result<(), String> {
        // Parse array type: [elem_type; size] or [elem_type]
        let (elem_type_str, array_size) = if let Some(semi_pos) = type_str.find(';') {
            // [elem_type; size]
            let elem = &type_str[1..semi_pos].trim();
            let size_str = &type_str[semi_pos+1..type_str.len()-1].trim();
            let size: usize = size_str.parse()
                .map_err(|_| self.error("compile_array_declaration",
                    format!("invalid array size: '{}'", size_str)))?;

            // HIGH-5 FIX: Limit maximum array size to prevent DoS via memory exhaustion
            const MAX_ARRAY_SIZE: usize = 1_000_000; // 1 million elements max
            if size > MAX_ARRAY_SIZE {
                return Err(self.error("compile_array_declaration",
                    format!("Array size exceeds maximum\n  = note: requested size: {} elements, maximum: {} elements\n  = help: use smaller arrays or heap allocation",
                        size, MAX_ARRAY_SIZE)));
            }

            (elem.to_string(), Some(size))
        } else {
            // [elem_type] - slice type, size determined from initializer
            let elem = &type_str[1..type_str.len()-1].trim();
            (elem.to_string(), None)
        };

        // Get element LLVM type
        let elem_llvm_type = self.coffee_type_to_llvm(&elem_type_str)
            .map_err(|e| self.error("compile_array_declaration",
                format!("failed to resolve element type '{}': {}", elem_type_str, e)))?;

        // Check if value is an array literal: [1, 2, 3]
        let (elements, actual_size) = if value_part.starts_with('[') && value_part.ends_with(']') {
            self.parse_array_literal(value_part, 0)? // Start at depth 0
        } else {
            // Single value or expression
            let value = self.compile_expression_str(value_part)?;
            (vec![value], array_size.unwrap_or(1))
        };

        // Security: Check size consistency
        let final_size = if let Some(declared_size) = array_size {
            if actual_size != declared_size {
                return Err(self.error("compile_array_declaration",
                    format!("Array size mismatch: declared size is {} but initializer has {} elements\n  = note: Either remove the size declaration or provide exactly {} elements",
                        declared_size, actual_size, declared_size)));
            }
            declared_size
        } else {
            actual_size
        };

        // Allocate array using proper LLVM array type
        let array_type = self.backend.context.i64_type().array_type(final_size as u32);

        // CRITICAL-5 FIX: Check stack size before array allocation
        const MAX_STACK_SIZE: usize = 10 * 1024 * 1024; // 10 MB max stack per function
        let array_alloc_size = final_size * 8; // i64 = 8 bytes per element

        if self.current_stack_size + array_alloc_size > MAX_STACK_SIZE {
            return Err(self.error("compile_array_declaration",
                format!("Array allocation would exceed maximum stack size\n  = note: current stack usage: {} bytes, array: {} bytes ({} elements × 8), maximum: {} bytes\n  = help: use smaller arrays or allocate on heap",
                    self.current_stack_size, array_alloc_size, final_size, MAX_STACK_SIZE)));
        }

        // Allocate the array in entry block (must dominate all uses)
        // Create allocas BEFORE any code execution (including initialization)
        let array_alloca = self.create_entry_alloca_preserving_terminator(array_type, name)
            .map_err(|e| self.error("compile_array_declaration",
                format!("failed to allocate array '{}': {}", name, e)))?;

        // Allocate length storage in entry block
        let len_type = self.backend.context.i64_type();
        let len_alloca = self.create_entry_alloca_preserving_terminator(len_type, &format!("{}_len", name))
            .map_err(|e| self.error("compile_array_declaration",
                format!("failed to allocate length storage: {}", e)))?;

        // Update stack size tracking
        self.current_stack_size += array_alloc_size;

        // CRITICAL-6 FIX: Zero-initialize entire array immediately after allocation
        // This prevents use of uninitialized memory even if initialization logic fails
        let i8_ptr_type = self.backend.context.ptr_type(inkwell::AddressSpace::default());
        let i32_type = self.backend.context.i32_type();

        // Declare memset function
        let memset_type = i32_type.fn_type(&[i8_ptr_type.into(), i32_type.into(), len_type.into()], false);
        let memset_func = self.backend.module.get_function("memset")
            .unwrap_or_else(|| self.backend.module.add_function("memset", memset_type, None));

        // Cast array pointer to i8* for memset
        let array_i8_ptr = self.backend.builder.build_bit_cast(
            array_alloca,
            i8_ptr_type,
            &format!("{}_i8ptr", name)
        ).map_err(|e| self.error("compile_array_declaration",
            format!("failed to cast array pointer for memset: {}", e)))?;

        // Calculate array size in bytes
        let element_size = self.backend.context.i64_type().const_int(8, false); // i64 = 8 bytes
        let array_size_bytes = self.backend.builder.build_int_mul(
            element_size,
            len_type.const_int(final_size as u64, false),
            "array_size_bytes"
        ).map_err(|e| self.error("compile_array_declaration",
            format!("failed to calculate array size: {}", e)))?;

        // Zero value for memset
        let zero = i32_type.const_int(0, false);

        // Call memset(ptr, 0, size)
        self.backend.builder.build_call(
            memset_func,
            &[array_i8_ptr.into(), zero.into(), array_size_bytes.into()],
            "memset_call"
        ).map_err(|e| self.error("compile_array_declaration",
            format!("failed to build memset call: {}", e)))?;

        // Store elements using GEP with proper array indexing
        // IMPORTANT: This happens in the current block (entry or continue), NOT in entry block
        for (i, elem_value) in elements.iter().enumerate() {
            let zero = self.backend.context.i32_type().const_int(0, false);
            let index = self.backend.context.i32_type().const_int(i as u64, false);

            // GEP for array: getelementptr [N x i64], ptr %array, i32 0, i32 %i
            // SAFETY: This unsafe block is safe because:
            // 1. Index is compile-time constant (i from enumerate)
            // 2. Loop range is 0..elements.len() which is <= final_size (array size)
            // 3. Size consistency check ensures elements.len() == final_size
            // 4. `in_bounds` GEP flag ensures LLVM assumes pointer is valid
            // 5. array_type matches the actual allocation type
            let elem_ptr = unsafe {
                self.backend.builder.build_in_bounds_gep(
                    array_type,
                    array_alloca,
                    &[zero, index],
                    &format!("{}_elem_{}", name, i)
                )
            }.map_err(|e| self.error("compile_array_declaration",
                format!("failed to get element pointer: {}", e)))?;

            self.backend.builder.build_store(elem_ptr, *elem_value)
                .map_err(|e| self.error("compile_array_declaration",
                    format!("failed to store element {}: {}", i, e)))?;
        }

        // Security: Zero-initialize any remaining elements
        // This prevents use of uninitialized memory
        for i in elements.len()..final_size {
            let zero = self.backend.context.i32_type().const_int(0, false);
            let index = self.backend.context.i32_type().const_int(i as u64, false);
            let zero_value = self.backend.context.i64_type().const_int(0, false);

            // SAFETY: This unsafe block is safe because:
            // 1. Index is compile-time constant (i from loop)
            // 2. Loop range is elements.len()..final_size (valid padding area)
            // 3. `in_bounds` GEP flag ensures LLVM assumes pointer is valid
            let elem_ptr = unsafe {
                self.backend.builder.build_in_bounds_gep(
                    array_type,
                    array_alloca,
                    &[zero, index],
                    &format!("{}_elem_{}_zero", name, i)
                )
            }.map_err(|e| self.error("compile_array_declaration",
                format!("failed to get element pointer for zero init: {}", e)))?;

            self.backend.builder.build_store(elem_ptr, zero_value)
                .map_err(|e| self.error("compile_array_declaration",
                    format!("failed to store zero in element {}: {}", i, e)))?;
        }

        // Store array length
        let len_value = len_type.const_int(final_size as u64, false);
        self.backend.builder.build_store(len_alloca, len_value)
            .map_err(|e| self.error("compile_array_declaration",
                format!("failed to store array length: {}", e)))?;

        // Store array info in dedicated HashMaps, NOT in self.variables
        // This prevents compile_expression_str from trying to load the array as i64
        self.array_allocas.insert(name.to_string(), array_alloca);
        self.array_sizes.insert(name.to_string(), final_size as u32);
        self.array_lengths.insert(name.to_string(), len_alloca);
        self.array_element_types.insert(name.to_string(), elem_llvm_type);

        Ok(())
    }

    /// Parse array literal: [1, 2, 3] or []
    /// Security: depth parameter prevents stack overflow from deeply nested arrays
    fn parse_array_literal(&mut self, literal: &str, depth: usize) -> Result<(Vec<BasicValueEnum<'ctx>>, usize), String> {
        const MAX_NESTING_DEPTH: usize = 64;

        // Check nesting depth to prevent stack overflow attacks
        if depth > MAX_NESTING_DEPTH {
            return Err(self.error("parse_array_literal",
                format!("Array nesting depth exceeds maximum of {}\n  = note: Deeply nested arrays can cause stack overflow", MAX_NESTING_DEPTH)));
        }

        let inner = &literal[1..literal.len()-1].trim();

        if inner.is_empty() {
            return Ok((Vec::new(), 0));
        }

        let mut elements = Vec::new();
        for elem_str in inner.split(',') {
            let elem_str = elem_str.trim();

            // Check for nested array
            if elem_str.starts_with('[') && elem_str.ends_with(']') {
                // Multi-dimensional arrays are not supported yet
                return Err(self.error("parse_array_literal",
                    "Multi-dimensional arrays are not supported yet"));
            }

            let elem_value = self.compile_expression_str(elem_str)
                .map_err(|e| self.error("parse_array_literal",
                    format!("failed to compile element '{}': {}", elem_str, e)))?;
            elements.push(elem_value);
        }

        let len = elements.len();
        Ok((elements, len))
    }

    /// Compile assignment
    pub fn compile_assignment(&mut self, line: &str) -> Result<(), String> {
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(self.error("compile_assignment",
                format!("invalid assignment syntax '{}' - expected 'variable = value'", line)));
        }

        let name = parts[0].trim();
        let value_str = parts[1].trim();

        let value = self.compile_expression_str(value_str)
            .map_err(|e| self.error("compile_assignment",
                format!("failed to compile value expression '{}' for variable '{}': {}", value_str, name, e)))?;

        // HIGH-10 FIX: Mark assigned variable as used (both read and write)
        self.used_variables.insert(name.to_string());

        if let Some(&(ptr, _)) = self.variables.get(name) {
            self.backend.builder.build_store(ptr, value)
                .map_err(|e| self.error("compile_assignment",
                    format!("failed to store value in variable '{}': {}", name, e)))?;
        } else {
            let available = if self.variables.is_empty() {
                String::from("(no variables in scope)")
            } else {
                format!("available variables: {}",
                    self.variables.keys().map(|k| format!("'{}'", k)).collect::<Vec<_>>().join(", "))
            };
            return Err(self.error("compile_assignment",
                format!("undefined variable '{}' - {}", name, available)));
        }

        Ok(())
    }

    /// Compile assignment from AST (for parsed assignment statements)
    /// This is used when assignment statements are parsed from the AST
    /// Supports both simple variable assignment (x = value) and member access assignment (self.x = value)
    pub fn compile_assignment_from_ast(&mut self, var_name: &str, value_expr: &str) -> Result<(), String> {
        // Check if this is a member access assignment (e.g., self.x = value)
        if var_name.contains('.') {
            let parts: Vec<&str> = var_name.splitn(2, '.').collect();
            if parts.len() == 2 {
                let object_str = parts[0].trim();
                let field_name = parts[1].trim();
                
                // Compile field access pointer
                let field_ptr = self.compile_field_access_ptr(object_str, field_name)
                    .map_err(|e| self.error("compile_assignment_from_ast",
                        format!("failed to get field pointer for '{}.{}': {}", object_str, field_name, e)))?;
                
                // Compile the value expression
                let value = self.compile_expression_str(value_expr)
                    .map_err(|e| self.error("compile_assignment_from_ast",
                        format!("failed to compile value expression '{}' for field '{}.{}': {}", value_expr, object_str, field_name, e)))?;
                
                // Mark object as used
                self.used_variables.insert(object_str.to_string());
                
                // Store the value (LLVM will handle type checking)
                self.backend.builder.build_store(field_ptr, value)
                    .map_err(|e| self.error("compile_assignment_from_ast",
                        format!("failed to store value to field '{}.{}': {}", object_str, field_name, e)))?;
                
                return Ok(());
            }
        }
        
        // Simple variable assignment (x = value)
        // Memory safety check: ensure variable is in a valid state for assignment
        if let Some(info) = self.memory_ctx.lifetimes.get(var_name) {
            match info.state {
                VariableState::Moved => {
                    return Err(self.error("compile_assignment_from_ast",
                        format!("cannot assign to variable '{}' - variable has been moved\n  = note: moved variables cannot be reassigned\n  = help: use 'clone' before move to preserve the original value", var_name)));
                }
                VariableState::Dropped => {
                    return Err(self.error("compile_assignment_from_ast",
                        format!("cannot assign to variable '{}' - variable has been removed/freed\n  = note: this operation would cause a use-after-free vulnerability", var_name)));
                }
                VariableState::Uninitialized => {
                    // This is okay for the first assignment
                }
                VariableState::Initialized => {
                    // This is okay for reassignment
                }
            }
        }

        let value = self.compile_expression_str(value_expr)
            .map_err(|e| self.error("compile_assignment_from_ast",
                format!("failed to compile value expression '{}' for variable '{}': {}", value_expr, var_name, e)))?;

        // Mark assigned variable as used (both read and write)
        self.used_variables.insert(var_name.to_string());

        if let Some(&(ptr, var_type)) = self.variables.get(var_name) {
            // Type checking: ensure value type matches variable type
            let value_type = value.get_type();
            if !self.are_types_compatible(value_type, var_type) {
                let value_type_str = self.type_to_string(value_type);
                let var_type_str = self.type_to_string(var_type);
                return Err(self.error("compile_assignment_from_ast",
                    format!("type mismatch in assignment to variable '{}': cannot assign type '{}' to variable of type '{}'\n  = note: Coffee does not support implicit type conversion between incompatible types\n  = help: ensure the value type matches the variable type or use explicit type conversion", 
                        var_name, value_type_str, var_type_str)));
            }

            // Convert value to the correct type if needed
            let converted_value = self.convert_value_to_type(value, var_type, var_name)?;

            self.backend.builder.build_store(ptr, converted_value)
                .map_err(|e| self.error("compile_assignment_from_ast",
                    format!("failed to store value in variable '{}': {}", var_name, e)))?;
        } else {
            let available = if self.variables.is_empty() {
                String::from("(no variables in scope)")
            } else {
                format!("available variables: {}",
                    self.variables.keys().map(|k| format!("'{}'", k)).collect::<Vec<_>>().join(", "))
            };
            return Err(self.error("compile_assignment_from_ast",
                format!("undefined variable '{}' - {}", var_name, available)));
        }

        Ok(())
    }

    /// Compile array index access with bounds checking
    /// Generates safe array access: array[index]
    pub fn compile_array_index(&mut self, array_str: &str, index_str: &str) -> Result<BasicValueEnum<'ctx>, String> {
        // Compile the index expression
        let index_value = self.compile_expression_str(index_str)
            .map_err(|e| self.error("compile_array_index",
                format!("failed to compile index expression '{}': {}", index_str, e)))?;

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
                    format!("array index must be an integer, got: {}", index_str)));
            }
        };

        // Look up the array variable to get its pointer and length
        let array_name = array_str;

        // Check if we have the array alloca stored (for proper GEP indexing)
        if let Some(&array_alloca) = self.array_allocas.get(array_name) {
            // CRITICAL-1 FIX: Use compile-time constant for bounds check instead of loading from memory
            // This prevents TOCTOU (time-of-check-time-of-use) vulnerabilities where the length
            // could theoretically be modified between the check and the array access.
            if let Some(&array_size) = self.array_sizes.get(array_name) {
                // Use compile-time constant for bounds checking
                let array_len = self.backend.context.i64_type().const_int(array_size as u64, false);

                // Perform bounds checking: 0 <= index < length
                self.check_array_bounds(index_i64, array_len, array_name, index_str)?;

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

                        // Load the element value
                        let elem_value = self.backend.builder.build_load(
                            elem_type,
                            elem_ptr,
                            "elem_value"
                        ).map_err(|e| self.error("compile_array_index",
                            format!("failed to load array element: {}", e)))?;

                        return Ok(elem_value);
                    }
                }
            }
        }

        // Fallback: treat as simple variable access
        Err(self.error("compile_array_index",
            format!("cannot find array '{}' or array length information not available\n  = note: arrays must be declared with their length\n  = help: use array literal syntax like [1, 2, 3] or declare array with known size", array_name)))
    }

    /// Check array bounds and panic if out of bounds
    fn check_array_bounds(
        &self,
        index: inkwell::values::IntValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
        array_name: &str,
        _index_str: &str,
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

        // Use puts to print the error message (puts is already declared in runtime)
        let puts_func = self.functions.get("puts").copied().unwrap_or_else(|| {
            let i8_ptr_type = self.backend.context.ptr_type(inkwell::AddressSpace::default());
            let i32_type = self.backend.context.i32_type();
            let fn_type = i32_type.fn_type(&[i8_ptr_type.into()], false);
            self.backend.module.add_function("puts", fn_type, None)
        });

        builder.build_call(puts_func, &[panic_msg.as_pointer_value().into()], "puts_call")
            .map_err(|e| format!("failed to build puts call: {}", e))?;

        // Call exit(1) to terminate the program
        // exit function is pre-declared in runtime functions
        let i32_type = self.backend.context.i32_type();
        let exit_func = self.functions.get("exit").copied().unwrap();
        let exit_code = i32_type.const_int(1, false);
        builder.build_call(exit_func, &[exit_code.into()], "exit_call")
            .map_err(|e| format!("failed to build exit call: {}", e))?;

        builder.build_unreachable()
            .map_err(|e| format!("failed to build unreachable: {}", e))?;

        // Position builder at continue block
        builder.position_at_end(continue_block);

        Ok(())
    }

    /// Compile local variable declaration (within function body)
    pub fn compile_local_variable_decl(&mut self, var: &crate::parser::var::VariableDecl) -> Result<(), String> {
        // MEDIUM-7 FIX: Limit number of local variables to prevent DoS
        const MAX_LOCAL_VARS: usize = 1000;
        if self.variables.len() >= MAX_LOCAL_VARS {
            return Err(self.error("compile_local_variable_decl",
                format!("Too many local variables in function\n  = note: current variables: {}, maximum: {}\n  = help: reduce number of local variables or use heap allocation",
                    self.variables.len(), MAX_LOCAL_VARS)));
        }

        let name = &var.name;
        let type_str = var.var_type.trim(); // Trim any whitespace
        let value_part = &var.value;

        // Check if this is an array type
        if type_str.starts_with('[') && type_str.ends_with(']') {
            return self.compile_array_declaration(name, type_str, value_part);
        }

        let llvm_type = self.coffee_type_to_llvm(type_str)
            .map_err(|e| self.error("compile_local_variable_decl",
                format!("failed to resolve type '{}' for variable '{}': {}", type_str, name, e)))?;

        // CRITICAL-5 FIX: Check stack size before allocation to prevent stack overflow
        const MAX_STACK_SIZE: usize = 10 * 1024 * 1024; // 10 MB max stack per function
        const MAX_VAR_SIZE: usize = 1024 * 1024; // 1 MB max per variable

        let _type_size = llvm_type.size_of().ok_or_else(|| self.error("compile_local_variable_decl",
            "cannot get type size - type is unsized or too large".to_string()))?;

        // Check if this is a constant value (compile-time known size)
        // For simplicity, use conservative estimates for each type
        let alloc_size = match llvm_type {
            BasicTypeEnum::IntType(t) => (t.get_bit_width() / 8) as usize,
            BasicTypeEnum::FloatType(t) => {
                if t.get_bit_width() == 64 { 8 } else { 4 }
            }
            BasicTypeEnum::PointerType(_) => 8, // Assume 64-bit pointers
            BasicTypeEnum::ArrayType(_t) => {
                // Arrays are handled separately in compile_array_declaration
                return Err(self.error("compile_local_variable_decl",
                    "arrays should use compile_array_declaration".to_string()));
            }
            BasicTypeEnum::StructType(t) => {
                // For structs, be conservative - use reasonable limit
                (t.count_fields() * 8) as usize
            }
            BasicTypeEnum::VectorType(t) => {
                (t.get_size() * 8) as usize
            }
            BasicTypeEnum::ScalableVectorType(_) => {
                // Scalable vectors - be conservative
                1024 // 1 KB default
            }
        };

        if alloc_size > MAX_VAR_SIZE {
            return Err(self.error("compile_local_variable_decl",
                format!("Variable size exceeds maximum\n  = note: variable size: {} bytes, maximum: {} bytes\n  = help: use smaller types or heap allocation",
                    alloc_size, MAX_VAR_SIZE)));
        }

        if self.current_stack_size + alloc_size > MAX_STACK_SIZE {
            return Err(self.error("compile_local_variable_decl",
                format!("Stack allocation would exceed maximum stack size\n  = note: current stack usage: {} bytes, allocation: {} bytes, maximum: {} bytes\n  = help: reduce number or size of local variables to prevent stack overflow",
                    self.current_stack_size, alloc_size, MAX_STACK_SIZE)));
        }

        let alloca = self.backend.builder.build_alloca(llvm_type, name)
            .map_err(|e| self.error("compile_local_variable_decl",
                format!("failed to allocate variable '{}': {}", name, e)))?;

        // HIGH-12 FIX: Zero-initialize immediately after allocation to prevent use of uninitialized memory
        match llvm_type {
            BasicTypeEnum::IntType(t) => {
                let zero = t.const_zero();
                self.backend.builder.build_store(alloca, zero)
                    .map_err(|e| self.error("compile_local_variable_decl",
                        format!("failed to zero-initialize variable '{}': {}", name, e)))?;
            }
            BasicTypeEnum::FloatType(t) => {
                let zero = t.const_zero();
                self.backend.builder.build_store(alloca, zero)
                    .map_err(|e| self.error("compile_local_variable_decl",
                        format!("failed to zero-initialize variable '{}': {}", name, e)))?;
            }
            BasicTypeEnum::PointerType(t) => {
                let zero = t.const_zero();
                self.backend.builder.build_store(alloca, zero)
                    .map_err(|e| self.error("compile_local_variable_decl",
                        format!("failed to zero-initialize variable '{}': {}", name, e)))?;
            }
            _ => {
                // For other types, initialization will happen when value is stored
            }
        }

        // Update stack size tracking
        self.current_stack_size += alloc_size;

        // 检查是否是内存操作表达式（move x, clone x, copy x）
        let trimmed_value = value_part.trim();
        let (source_var, memory_op_type) = if trimmed_value.starts_with("move ") {
            (trimmed_value[5..].trim(), "move")
        } else if trimmed_value.starts_with("clone ") {
            (trimmed_value[6..].trim(), "clone")
        } else if trimmed_value.starts_with("copy ") {
            (trimmed_value[5..].trim(), "copy")
        } else {
            // 普通表达式 - 需要进行类型转换
            let value = self.compile_expression_str(value_part)
                .map_err(|e| self.error("compile_local_variable_decl",
                    format!("failed to compile initial value '{}' for variable '{}': {}", value_part, name, e)))?;
            
            // 类型转换：将值转换为目标变量类型
            let converted_value = self.convert_value_to_type(value, llvm_type, name)
                .map_err(|e| self.error("compile_local_variable_decl",
                    format!("failed to convert value to type '{}' for variable '{}': {}", type_str, name, e)))?;
            
            self.backend.builder.build_store(alloca, converted_value)
                .map_err(|e| self.error("compile_local_variable_decl",
                    format!("failed to store initial value in variable '{}': {}", name, e)))?;

            self.variables.insert(name.to_string(), (alloca, llvm_type));
            
            // Store the class name if this is a class type
            // This is needed for method calls to find the correct method
            eprintln!("DEBUG: compile_local_variable_decl: name='{}', type_str='{}'", name, type_str);
            if type_str.chars().all(|c| c.is_alphanumeric() || c == '_') {
                // Check if this is a class type (not a primitive type like int, float, bool, string)
                if !matches!(type_str, "int" | "float" | "bool" | "string" | "void" | "()") {
                    eprintln!("DEBUG: compile_local_variable_decl: inserting class_name '{}' for variable '{}'", type_str, name);
                    self.variable_types.insert(name.to_string(), type_str.to_string());
                }
            }

            // 生命周期跟踪：注册变量出生
            // Check if this is a heap-allocated object (constructor call)
            if value_part.trim().ends_with("_new()") {
                self.memory_ctx.mark_heap_allocated(name);
                // Extract class name from constructor call (e.g., "Point_new()" -> "Point")
                if let Some(class_name) = value_part.trim().strip_suffix("_new()") {
                    self.memory_ctx.set_variable_type(name.to_string(), class_name.to_string());
                }
            }
            self.memory_ctx.register_birth(name.clone());

            // 生命周期跟踪：标记变量已初始化
            self.memory_ctx.mark_initialized(name);

            return Ok(());
        };

        // 处理内存操作
        if let Some(_) = self.variables.get(source_var) {
            use crate::backend::memory_ops;

            match memory_op_type {
                "move" => {
                    // 执行 move 操作
                    memory_ops::compile_move(
                        &self.backend.context,
                        &self.backend.builder,
                        &mut self.variables,
                        &mut self.memory_ctx,
                        source_var,
                        name
                    ).map_err(|e| self.error("compile_local_variable_decl", e))?;
                }
                "clone" => {
                    // 执行 clone 操作
                    memory_ops::compile_clone(
                        &self.backend.context,
                        &self.backend.builder,
                        &mut self.variables,
                        source_var,
                        name
                    ).map_err(|e| self.error("compile_local_variable_decl", e))?;
                }
                "copy" => {
                    // 执行 copy 操作
                    memory_ops::compile_copy(
                        &self.backend.context,
                        &self.backend.builder,
                        &mut self.variables,
                        source_var,
                        name
                    ).map_err(|e| self.error("compile_local_variable_decl", e))?;
                }
                _ => {
                                            return Err(self.error("compile_local_variable_decl",
                                                format!("unknown memory operation: {}", memory_op_type)));                }
            }
        } else {
            return Err(self.error("compile_local_variable_decl",
                format!("source variable '{}' not found for {} operation", source_var, memory_op_type)));
        }

        // 生命周期跟踪：注册变量出生
        self.memory_ctx.register_birth(name.clone());

        // 生命周期跟踪：标记变量已初始化
        self.memory_ctx.mark_initialized(name);

        Ok(())
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

        // Set initializer based on type
        match &var.var_type[..] {
            "int" | "i64" => {
                global.set_initializer(&self.backend.context.i64_type().const_zero());
            }
            "i32" => {
                global.set_initializer(&self.backend.context.i32_type().const_zero());
            }
            "i16" => {
                global.set_initializer(&self.backend.context.i16_type().const_zero());
            }
            "i8" | "byte" => {
                global.set_initializer(&self.backend.context.i8_type().const_zero());
            }
            "float" | "f64" => {
                global.set_initializer(&self.backend.context.f64_type().const_zero());
            }
            "f32" => {
                global.set_initializer(&self.backend.context.f32_type().const_zero());
            }
            "bool" => {
                global.set_initializer(&self.backend.context.bool_type().const_zero());
            }
            "string" | "str" => {
                global.set_initializer(&self.backend.context.ptr_type(inkwell::AddressSpace::default()).const_null());
            }
            _ => {
                // Default to i64 for unknown types
                global.set_initializer(&self.backend.context.i64_type().const_zero());
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
        if let Some(_terminator) = entry_block.get_terminator() {
            // Entry has terminator - create alloca in current block instead
            // This is necessary when overflow/bounds checking has created new blocks
            // LLVM allows allocas in non-entry blocks (though not ideal)
            let alloca = self.backend.builder.build_alloca(type_, name)
                .map_err(|e| self.error("create_entry_alloca_preserving_terminator",
                    format!("failed to build alloca '{}': {}", name, e)))?;
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
