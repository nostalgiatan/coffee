//! Class and enum compilation for Coffee compiler
//!
//! Handles class definitions, enum definitions, and their associated operations.

use super::codegen::CodeGenerator;
use inkwell::types::BasicTypeEnum;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile class definition
    pub fn compile_class(&mut self, class: &crate::parser::class::ClassDef) -> Result<(), String> {
        eprintln!("DEBUG: compile_class: compiling class '{}', methods={:?}", class.name, class.methods.iter().map(|m| &m.name).collect::<Vec<_>>());
        // Store class definition for C header generation
        self.classes.insert(class.name.clone(), class.clone());

        use crate::backend::memory::{Layout, StructLayout};

        // Check if this class has bit fields and bit fields are enabled
        let has_bit_fields = self.enable_bitfields && class.fields.iter().any(|f| f.bit_width.is_some());

        // Calculate layout for each field
        let mut field_layouts: Vec<Layout> = Vec::new();
        let mut field_types: Vec<BasicTypeEnum> = Vec::new();

        // Handle inheritance: add parent fields first
        if let Some(ref parent_name) = class.parent {
            if let Some(parent_class) = self.classes.get(parent_name) {
                for field in &parent_class.fields {
                    let field_type = self.coffee_type_to_llvm(&field.field_type)?;
                    let layout = Layout::from_llvm_type(&field_type);
                    field_layouts.push(layout);
                    field_types.push(field_type);
                }
            }
        }

        // Process bit fields if enabled
        if has_bit_fields {
            use crate::backend::memory::bitfields::{BitFieldSpec, BitFieldLayout};
            
            let mut bit_field_specs: Vec<BitFieldSpec> = Vec::new();
            let mut current_bit_offset = 0u8;
            let mut storage_unit_size = 8u8; // Start with i8 (8 bits)

            for field in &class.fields {
                if let Some(bit_width) = field.bit_width {
                    bit_field_specs.push(BitFieldSpec {
                        name: field.name.clone(),
                        width: bit_width,
                    });

                    current_bit_offset += bit_width;

                    // Check if we need a larger storage unit
                    if current_bit_offset > storage_unit_size {
                        storage_unit_size = if current_bit_offset <= 8 {
                            8
                        } else if current_bit_offset <= 16 {
                            16
                        } else if current_bit_offset <= 32 {
                            32
                        } else {
                            64
                        };
                    }
                }
            }

            // Calculate bit field layout
            if !bit_field_specs.is_empty() {
                let bit_field_layout = BitFieldLayout::calculate(&bit_field_specs);

                // Print bit field layout report
                if self.enable_bitfields {
                    eprintln!("  = note: class '{}' uses bit fields ({} bytes storage, {:.1}% efficiency)",
                        class.name, bit_field_layout.storage_size(), bit_field_layout.efficiency());
                }

                // Store bit field layout information
                let mut bit_field_map = std::collections::HashMap::new();
                for bf in &bit_field_layout.fields {
                    bit_field_map.insert(bf.name.clone(), (bf.offset, bf.width, bit_field_layout.storage_type.bits()));
                }
                self.bit_field_layouts.insert(class.name.clone(), bit_field_map);
            }
        }

        for field in &class.fields {
            // For bit fields, use the storage unit type instead of the declared type
            if has_bit_fields && field.bit_width.is_some() {
                // Bit fields will share storage units, skip individual field type processing
                // The storage unit will be added separately
                continue;
            }

            let field_type = self.coffee_type_to_llvm(&field.field_type)?;
            let layout = Layout::from_llvm_type(&field_type);
            field_layouts.push(layout);
            field_types.push(field_type);
        }

        // Add storage unit for bit fields if present
        if has_bit_fields {
            // Determine the storage unit size based on total bit width
            let total_bits: u8 = class.fields.iter()
                .filter_map(|f| f.bit_width)
                .sum();

            let storage_type = if total_bits <= 8 {
                self.backend.context.i8_type()
            } else if total_bits <= 16 {
                self.backend.context.i16_type()
            } else if total_bits <= 32 {
                self.backend.context.i32_type()
            } else {
                self.backend.context.i64_type()
            };

            let storage_enum = BasicTypeEnum::IntType(storage_type);
            let layout = Layout::from_llvm_type(&storage_enum);
            field_layouts.push(layout);
            field_types.push(storage_enum);
        }

        // Calculate optimal struct layout with field reordering
        // Use packed layout if class is marked as packed
        let struct_layout = if class.packed {
            StructLayout::calculate_packed(&field_layouts)
        } else {
            StructLayout::calculate(&field_layouts)
        };

        // Collect layout information for reporting
        let field_names: Vec<String> = class.fields.iter()
            .map(|f| f.name.clone())
            .collect();
        self.layout_collector.add_struct(class.name.clone(), struct_layout.clone(), field_names);

        // Check memory efficiency and warn if low
        let (data_bytes, padding_bytes, efficiency) = struct_layout.efficiency_metrics();
        if !class.packed && efficiency < 75.0 {
            eprintln!("  = warning: struct '{}' has low memory efficiency ({:.1}%)\n    = note: {} bytes data, {} bytes padding\n    = help: consider using 'packed class' or reordering fields to reduce padding",
                class.name, efficiency, data_bytes, padding_bytes);
        }

        // Reorder field types to match optimized layout
        let mut reordered_field_types: Vec<BasicTypeEnum> = Vec::new();
        for field_layout in &struct_layout.fields {
            reordered_field_types.push(field_types[field_layout.original_index].clone());
        }

        // Store layout information for final report (collected during compilation)
        // This will be used to generate a complete memory layout diagram after compilation
        // Store in a module-level cache or return as part of compilation result

        // Get or create struct type from cache
        let struct_type = match self.type_mapper.get_or_create_struct_type_from_class(&class.name, class, &self.classes) {
            Some(st) => st,
            None => {
                return Err(self.error("compile_class",
                    format!("duplicate definition of class '{}'\n  = note: class is already defined", class.name)));
            }
        };

        // Set struct body with optimized field order
        // Use packed for LLVM if class is marked as packed
        struct_type.set_body(&reordered_field_types, class.packed);

        // Compile class methods (including constructor)
        for method in &class.methods {
            let method_name = format!("{}_{}", class.name, method.name);
            
            // Parse method body into statements
            let method_statements = if method.body.contains('\n') {
                // Multi-line method body - parse as statements
                match crate::parser::parse_program(&method.body) {
                    Ok(program) => program.statements,
                    Err(_) => {
                        // If parsing fails, try to parse line by line
                        let lines: Vec<&str> = method.body.lines().collect();
                        let mut statements = Vec::new();
                        for line in lines {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() && !trimmed.starts_with("/#") {
                                if let Some(stmt) = crate::parser::parse_single_line_statement(trimmed) {
                                    statements.push(stmt);
                                }
                            }
                        }
                        if statements.is_empty() {
                            // Last resort: treat as single expression wrapped in return
                            vec![crate::parser::Statement::Return(crate::parser::var::ReturnStmt {
                                value: Some(method.body.clone()),
                            })]
                        } else {
                            statements
                        }
                    }
                }
            } else {
                // Single-line method body
                let trimmed = method.body.trim();
                
                // Check if it's an assignment statement (e.g., self.x = value)
                if trimmed.contains(" = ") && !trimmed.starts_with("return ") {
                    // It's an assignment statement - parse it directly
                    if let Some(stmt) = crate::parser::parse_single_line_statement(trimmed) {
                        vec![stmt]
                    } else {
                        // Fallback: treat as expression wrapped in return
                        vec![crate::parser::Statement::Return(crate::parser::var::ReturnStmt {
                            value: Some(method.body.clone()),
                        })]
                    }
                } else if trimmed.starts_with("return ") {
                    // It's already a return statement - parse it as is
                    if let Some(stmt) = crate::parser::parse_single_line_statement(trimmed) {
                        vec![stmt]
                    } else {
                        // Fallback: treat as expression wrapped in return (without "return" keyword)
                        let expr = trimmed.strip_prefix("return ").unwrap_or(trimmed);
                        vec![crate::parser::Statement::Return(crate::parser::var::ReturnStmt {
                            value: Some(expr.to_string()),
                        })]
                    }
                } else {
                    // It's an expression - wrap in return
                    vec![crate::parser::Statement::Return(crate::parser::var::ReturnStmt {
                        value: Some(method.body.clone()),
                    })]
                }
            };
            
            // Convert method to Function
            // Add self parameter as the first parameter (except for constructors)
            let method_parameters = if method.name == "new" {
                // Constructor - don't add self parameter
                method.parameters.iter().map(|p| {
                    crate::parser::function::Parameter {
                        name: p.name.clone(),
                        param_type: p.param_type.clone(),
                        is_variadic: false,
                    }
                }).collect()
            } else {
                // Regular method - add self parameter as the first parameter
                let mut params = vec![
                    crate::parser::function::Parameter {
                        name: "self".to_string(),
                        param_type: class.name.clone(),
                        is_variadic: false,
                    }
                ];
                params.extend(method.parameters.iter().map(|p| {
                    crate::parser::function::Parameter {
                        name: p.name.clone(),
                        param_type: p.param_type.clone(),
                        is_variadic: false,
                    }
                }));
                params
            };
            
            let func = crate::parser::Function {
                name: method_name.clone(),
                parameters: method_parameters,
                return_type: method.return_type.clone(),
                error_handler: None,
                is_c: false,
                body: crate::parser::function::FunctionBody::Block(method_statements),
            };

            // Declare and compile the function
            crate::backend::functions::declare_function(
                &func,
                &self.backend.module,
                self.backend.context,
                &self.type_mapper,
                &mut self.functions,
            )?;

                                // Compile function body
                                match &func.body {
                                    crate::parser::function::FunctionBody::Expression(expr_str) => {
                                        // Get the function value
                                        let fn_value = self.functions.get(&func.name).copied()
                                            .ok_or_else(|| self.error("compile_class",
                                                format!("function '{}' not found after declaration", func.name)))?;
            
                                        // Create entry block
                                        let entry_block = self.backend.context.append_basic_block(fn_value, "entry");
                                        self.backend.builder.position_at_end(entry_block);
            
                                        // Set current function
                                        let old_current_function = self.current_function;
                                        self.current_function = Some(fn_value);
            
                                        // Set up function parameters
                                        for (i, param) in fn_value.get_params().into_iter().enumerate() {
                                            if let Some(param_name) = func.parameters.get(i).map(|p| p.name.clone()) {
                                                // Create alloca for parameter
                                                let param_type: inkwell::types::BasicTypeEnum = param.get_type().into();
                                                let alloca = self.backend.builder.build_alloca(param_type, &format!("{}_param", param_name))
                                                    .map_err(|e| self.error("compile_class", format!("failed to build alloca: {}", e)))?;                            
                            // Store parameter value
                            self.backend.builder.build_store(alloca, param)
                                .map_err(|e| self.error("compile_class", format!("failed to build store: {}", e)))?;
                            
                            // Store parameter in memory context
                            use crate::backend::memory_ops::LifetimeInfo;
                            self.memory_ctx.lifetimes.insert(
                                param_name.clone(),
                                LifetimeInfo::new(param_name.clone(), 0)
                            );
                            
                            // Store parameter in variables map for access
                            self.variables.insert(param_name.clone(), (alloca.into(), param_type));
                            
                            // Store class name in variable_types for method calls
                            if param_name == "self" {
                                self.variable_types.insert(param_name.clone(), class.name.clone());
                            }
                        }
                    }

                    // Parse and compile the expression
                    let result = self.compile_expression_str(expr_str)?;

                    // Build return instruction
                    let ret_type = fn_value.get_type().get_return_type();
                    if ret_type.is_none() {
                        // Void return type
                        self.backend.builder.build_return(None)
                            .map_err(|e| self.error("compile_class", format!("failed to build return: {}", e)))?;
                    } else if let Some(ret_type) = ret_type {
                        // Convert value to target type
                        let converted = self.convert_value_to_type(result, ret_type, "method_return")?;
                        self.backend.builder.build_return(Some(&converted))
                            .map_err(|e| self.error("compile_class", format!("failed to build return: {}", e)))?;
                    }

                    // Restore current function
                    self.current_function = old_current_function;
                }
                crate::parser::function::FunctionBody::Block(statements) => {
                    // Compile function body with statements
                    let fn_value = self.functions.get(&func.name).copied()
                        .ok_or_else(|| self.error("compile_class",
                            format!("function '{}' not found after declaration", func.name)))?;

                    // Create entry block
                    let entry_block = self.backend.context.append_basic_block(fn_value, "entry");
                    self.backend.builder.position_at_end(entry_block);

                    // Set current function
                    let old_current_function = self.current_function;
                    self.current_function = Some(fn_value);

                    // Set up function parameters
                    for (i, param) in fn_value.get_params().into_iter().enumerate() {
                        if let Some(param_name) = func.parameters.get(i).map(|p| p.name.clone()) {
                            // Create alloca for parameter
                            let param_type: inkwell::types::BasicTypeEnum = param.get_type().into();
                            let alloca = self.backend.builder.build_alloca(param_type, &format!("{}_param", param_name))
                                .map_err(|e| self.error("compile_class", format!("failed to build alloca: {}", e)))?;
                            
                            // Store parameter value
                            self.backend.builder.build_store(alloca, param)
                                .map_err(|e| self.error("compile_class", format!("failed to build store: {}", e)))?;
                            
                            // Store parameter in memory context
                            use crate::backend::memory_ops::LifetimeInfo;
                            self.memory_ctx.lifetimes.insert(
                                param_name.clone(),
                                LifetimeInfo::new(param_name.clone(), 0)
                            );
                            
                            // Store parameter in variables map for access
                            self.variables.insert(param_name.clone(), (alloca, param_type));
                            
                            // Store class name in variable_types for method calls
                            // Check if the parameter type is a class type
                            let param_type_str = &func.parameters[i].param_type;
                            if param_type_str == &class.name {
                                // This is the self parameter
                                self.variable_types.insert(param_name.clone(), class.name.clone());
                            } else {
                                // Check if this parameter is a class type by looking it up in the classes map
                                if self.classes.contains_key(param_type_str) {
                                    // This is a class type parameter
                                    self.variable_types.insert(param_name.clone(), param_type_str.clone());
                                }
                            }
                        }
                    }

                    // Compile each statement
                    for stmt in statements {
                        self.compile_statement(stmt)?;
                    }

                    // Add default return if needed (but not if function has raise statement)
                    let has_terminator = self.backend.builder.get_insert_block()
                        .map(|b| b.get_terminator().is_some())
                        .unwrap_or(false);
                    if !has_terminator {
                        // Check if return type is void
                        let ret_type = fn_value.get_type().get_return_type();
                        if ret_type.is_none() {
                            // Void return type
                            self.backend.builder.build_return(None)
                                .map_err(|e| self.error("compile_class", format!("failed to build return: {}", e)))?;
                        } else if let Some(ret_type) = ret_type {
                            // Non-void return type - use default value
                            let default_value: inkwell::values::BasicValueEnum = match ret_type {
                                inkwell::types::BasicTypeEnum::IntType(int_type) => {
                                    int_type.const_int(0, false).into()
                                }
                                inkwell::types::BasicTypeEnum::FloatType(float_type) => {
                                    float_type.const_float(0.0).into()
                                }
                                inkwell::types::BasicTypeEnum::PointerType(ptr_type) => {
                                    // For pointer types, use a null pointer
                                    ptr_type.const_null().into()
                                }
                                _ => {
                                    // For other types, use undef
                                    self.backend.context.i64_type().const_int(0, false).into()
                                }
                            };
                            self.backend.builder.build_return(Some(&default_value))
                                .map_err(|e| self.error("compile_class", format!("failed to build return: {}", e)))?;
                        }
                    }

                    // Restore current function
                    self.current_function = old_current_function;
                }
                crate::parser::function::FunctionBody::External => {
                    // External function, no body to compile
                }
            }
        }

        // Generate drop function for classes with constructor (full classes)
        if class.has_constructor {
            let drop_func_name = format!("{}__drop", class.name);
            
            // Create drop function signature: void ClassName__drop(struct ClassName* self)
            let drop_func = crate::parser::Function {
                name: drop_func_name.clone(),
                parameters: vec![
                    crate::parser::function::Parameter {
                        name: "self".to_string(),
                        param_type: class.name.clone(),
                        is_variadic: false,
                    }
                ],
                return_type: "void".to_string(),
                error_handler: None,
                is_c: false,
                body: crate::parser::function::FunctionBody::Expression(String::new()),
            };

            // Declare the drop function
            crate::backend::functions::declare_function(
                &drop_func,
                &self.backend.module,
                self.backend.context,
                &self.type_mapper,
                &mut self.functions,
            )?;

            // Compile drop function body (empty for now)
            let fn_value = self.functions.get(&drop_func_name).copied()
                .ok_or_else(|| self.error("compile_class",
                    format!("drop function '{}' not found after declaration", drop_func_name)))?;

            // Create entry block
            let entry_block = self.backend.context.append_basic_block(fn_value, "entry");
            self.backend.builder.position_at_end(entry_block);

            // Just return void (drop function is a no-op for now)
            self.backend.builder.build_return(None)
                .map_err(|e| self.error("compile_class", format!("failed to build return in drop function: {}", e)))?;
        }

        // Note: We no longer generate automatic constructors
        // - Pure data structures (no custom constructor): use struct literals
        // - Full classes (with custom constructor): user-defined fn new() is compiled separately
        Ok(())
    }

    /// Compile enum definition
    pub fn compile_enum(&mut self, enum_def: &crate::parser::class::EnumDef) -> Result<(), String> {
        // Enums are compiled as integer constants
        let i64_type = self.backend.context.i64_type();

        for (index, variant) in enum_def.variants.iter().enumerate() {
            let const_name = format!("{}_{}", enum_def.name, variant.name);
            let global = self.backend.module.add_global(
                i64_type,
                Some(inkwell::AddressSpace::default()),
                &const_name
            );
            global.set_initializer(&i64_type.const_int(index as u64, false));
            global.set_constant(true);
        }

        // Store enum definition for later use
        self.enums.insert(enum_def.name.clone(), enum_def.clone());

        Ok(())
    }
}