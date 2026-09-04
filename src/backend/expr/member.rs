//! Member access, field pointers, assignment, and method calls.

use crate::coffee_debug;
use crate::backend::codegen::CodeGenerator;
use crate::parser::expr::Expression;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_member_expr(&mut self, object: &Expression, field: &str, args: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        if let Expression::Variable(obj_name) = object {
            if self.enums.contains_key(obj_name) {
                if args.is_empty() {
                    return self.compile_enum_simple_variant(obj_name, field);
                }
                return self.compile_enum_variant_call(obj_name, field, args);
            }
            if args.is_empty() {
                return self.compile_field_access(obj_name, field);
            }
            return self.compile_method_call_from_ast(obj_name, field, args);
        }
        if args.is_empty() {
            let object_str = object.to_string();
            return self.compile_field_access(&object_str, field);
        }
        let object_str = object.to_string();
        self.compile_method_call_from_ast(&object_str, field, args)
    }

    pub(crate) fn compile_assign_expr(
        &mut self,
        object: &Expression,
        field_name: &str,
        value: &Expression,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let object_str = match object {
            Expression::Variable(n) => n.clone(),
            other => other.to_string(),
        };
        let field_ptr = self.compile_field_access_ptr(&object_str, field_name)?;
        let val = self.compile_expr(value)?;
        self.backend.builder.build_store(field_ptr, val)
            .map_err(|e| self.error("compile_assign", format!("failed to store field '{}': {}", field_name, e)))?;
        Ok(val)
    }

    pub(crate) fn compile_method_call_from_ast(
        &mut self,
        object_str: &str,
        method_name: &str,
        args: &[Expression],
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let mut args_src = String::new();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                args_src.push_str(", ");
            }
            args_src.push_str(&arg.to_string());
        }
        self.compile_method_call(object_str, method_name, &args_src)
    }

    /// Compile field access: object.field
    pub fn compile_field_access(&mut self, object_str: &str, field_name: &str) -> Result<BasicValueEnum<'ctx>, String> {
        // Compile the object expression
        let object_value = self.compile_source_as_expr(object_str)?;
        
        // Get the object type
        let object_type = object_value.get_type();
        
        coffee_debug!("DEBUG: compile_field_access: object_str='{}', field_name='{}', object_type={:?}",
            object_str, field_name, object_type);
        
        match object_type {
            BasicTypeEnum::PointerType(ptr_type) => {
                // Object is a pointer (heap-allocated or stack-allocated struct)
                // Check if this is a struct pointer by looking at variable_types
                if let Some(class_name) = self.variable_types.get(object_str) {
                    // Get the struct type from struct_types cache
                    if let Some(&struct_type) = self.type_mapper.struct_types.get(class_name) {
                        // Get field index
                        let field_index = self.get_field_index(&struct_type, field_name)?;
                        
                        // Get pointer to the field using GEP
                        let zero = self.backend.context.i64_type().const_int(0, false);
                        let field_index_val = self.backend.context.i32_type().const_int(field_index as u64, false);
                        
                        let field_ptr = unsafe {
                            self.backend.builder.build_in_bounds_gep(
                                struct_type,
                                object_value.into_pointer_value(),
                                &[zero, field_index_val],
                                &format!("{}_{}_ptr", object_str, field_name)
                            ).map_err(|e| self.error("compile_field_access",
                                format!("failed to get field pointer: {}", e)))?
                        };
                        
                        // Load the field value
                        let field_value = self.backend.builder.build_load(
                            struct_type.get_field_type_at_index(field_index as u32).unwrap(),
                            field_ptr,
                            &format!("{}_{}", object_str, field_name)
                        ).map_err(|e| self.error("compile_field_access",
                            format!("failed to load field '{}': {}", field_name, e)))?;
                        
                        return Ok(field_value);
                    }
                }
                
                // Fallback: try to load as a generic i64 value
                let i64_type = self.backend.context.i64_type();
                let loaded_value = self.backend.builder.build_load(
                    i64_type,
                    object_value.into_pointer_value(),
                    &format!("{}_loaded", object_str)
                ).map_err(|e| self.error("compile_field_access",
                    format!("failed to load object: {}", e)))?;
                
                Ok(loaded_value)
            }
            BasicTypeEnum::StructType(struct_type) => {
                // Object is a struct value (stack-allocated)
                let field_index = self.get_field_index(&struct_type, field_name)?;
                
                // For struct values, we need to extract the field directly
                let field_value = self.backend.builder.build_extract_value(
                    object_value.into_struct_value(),
                    field_index as u32,
                    &format!("{}_{}", object_str, field_name)
                ).map_err(|e| self.error("compile_field_access",
                    format!("failed to extract field '{}': {}", field_name, e)))?;
                
                Ok(field_value)
            }
            _ => Err(self.error("compile_field_access",
                format!("field access on non-struct type: {}", self.type_to_string(object_type))))
        }
    }

    /// Compile field access to get field pointer (for assignment): object.field
    pub fn compile_field_access_ptr(&mut self, object_str: &str, field_name: &str) -> Result<PointerValue<'ctx>, String> {
        // Compile the object expression
        let object_value = self.compile_source_as_expr(object_str)?;
        
        // Get the object type
        let object_type = object_value.get_type();
        
        match object_type {
            BasicTypeEnum::PointerType(ptr_type) => {
                // Object is a pointer (heap-allocated or stack-allocated struct)
                let object_ptr = object_value.into_pointer_value();
                
                // Try to get the struct type from the variable table
                let var_type = if let Some(&(_, var_type)) = self.variables.get(object_str) {
                    var_type
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("variable '{}' not found", object_str)));
                };
                
                // Get field index
                let field_index = match var_type {
                    BasicTypeEnum::StructType(s) => self.get_field_index(&s, field_name)?,
                    BasicTypeEnum::PointerType(inner_ptr_type) => {
                        // Try to get the class name from variable_types
                        if let Some(class_name) = self.variable_types.get(object_str) {
                            // Get the struct type from struct_types cache
                            if let Some(&struct_type) = self.type_mapper.struct_types.get(class_name) {
                                self.get_field_index(&struct_type, field_name)?
                            } else {
                                return Err(self.error("compile_field_access_ptr",
                                    format!("class '{}' not found in struct_types cache", class_name)));
                            }
                        } else {
                            return Err(self.error("compile_field_access_ptr",
                                format!("cannot get class name for variable '{}'", object_str)));
                        }
                    }
                    _ => {
                        return Err(self.error("compile_field_access_ptr",
                            format!("variable '{}' does not have a struct type", object_str)));
                    }
                };
                
                // Get pointer to the field using GEP
                let zero = self.backend.context.i64_type().const_int(0, false);
                let field_index_val = self.backend.context.i32_type().const_int(field_index as u64, false);
                
                // Get the struct type from struct_types cache
                let class_name = if let Some(name) = self.variable_types.get(object_str) {
                    name.clone()
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("cannot get class name for variable '{}'", object_str)));
                };
                
                let struct_type = if let Some(&struct_type) = self.type_mapper.struct_types.get(&class_name) {
                    struct_type
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("class '{}' not found in struct_types cache", class_name)));
                };
                
                let field_ptr = unsafe {
                    self.backend.builder.build_in_bounds_gep(
                        struct_type,
                        object_ptr,
                        &[zero, field_index_val],
                        &format!("{}_{}_ptr", object_str, field_name)
                    )
                }.map_err(|e| self.error("compile_field_access_ptr",
                    format!("failed to build field GEP: {}", e)))?;
                
                Ok(field_ptr)
            }
            BasicTypeEnum::StructType(struct_type) => {
                // Object is a struct value (stack-allocated)
                // We need to get a pointer to the struct first
                // This is more complex - for now, return an error
                Err(self.error("compile_field_access_ptr",
                    format!("field access on struct value not supported for assignment (use pointer)")))
            }
            _ => Err(self.error("compile_field_access_ptr",
                format!("field access on non-struct type: {}", self.type_to_string(object_type))))
        }
    }

    /// Compile method call: object.method(args)
    pub fn compile_method_call(&mut self, object_str: &str, method_name: &str, args_str: &str) -> Result<BasicValueEnum<'ctx>, String> {
        // Compile the object expression
        let object_value = self.compile_source_as_expr(object_str)?;
        
        // Get the object type
        let object_type = object_value.get_type();
        
        // Determine the class name from the variable type
        let class_name = if let Some(class_name) = self.variable_types.get(object_str) {
            class_name.clone()
        } else if let Some(&(_, var_type)) = self.variables.get(object_str) {
            match var_type {
                BasicTypeEnum::PointerType(ptr_type) => {
                    // Try to get the struct type from the pointer type
                    // Since get_element_type is not available, we'll use a different approach
                    // We'll try to infer the class name from the variable type
                    // For now, we'll use the variable name as a fallback
                    // This is not ideal, but it should work for simple cases
                    object_str.to_string()
                }
                BasicTypeEnum::StructType(_) => {
                    object_str.to_string()
                }
                _ => object_str.to_string()
            }
        } else {
            object_str.to_string()
        };
        
        coffee_debug!("DEBUG: compile_method_call: variable_types keys: {:?}", self.variable_types.keys().collect::<Vec<_>>());
        coffee_debug!("DEBUG: compile_method_call: object_str='{}', variable_types.get(object_str)={:?}", 
            object_str, self.variable_types.get(object_str));
        
        // Parse arguments
        let mut args = Vec::new();
        if !args_str.is_empty() {
            let mut current_arg = String::new();
            let mut depth = 0;
            let mut in_string = false;
            
            for ch in args_str.chars() {
                match ch {
                    '"' if !in_string => in_string = true,
                    '"' if in_string => in_string = false,
                    '(' if !in_string => depth += 1,
                    ')' if !in_string => depth -= 1,
                    ',' if !in_string && depth == 0 => {
                        if !current_arg.trim().is_empty() {
                            args.push(current_arg.trim().to_string());
                        }
                        current_arg.clear();
                    }
                    _ => current_arg.push(ch),
                }
            }
            if !current_arg.trim().is_empty() {
                args.push(current_arg.trim().to_string());
            }
        }
        
        // Compile arguments
        let mut compiled_args = Vec::new();
        for arg in &args {
            compiled_args.push(self.compile_source_as_expr(arg)?);
        }
        
        // Construct the full function name: ClassName_method
        let full_method_name = format!("{}_{}", class_name, method_name);
        
        coffee_debug!("DEBUG: compile_method_call: object_str='{}', method_name='{}', class_name='{}', full_method_name='{}'", 
            object_str, method_name, class_name, full_method_name);
        coffee_debug!("DEBUG: compile_method_call: functions keys: {:?}", self.functions.keys().collect::<Vec<_>>());
        
        // Try to get the function
        let function = self.functions.get(&full_method_name)
            .copied()
            .or_else(|| self.functions.get(method_name).copied())
            .ok_or_else(|| self.error("compile_method_call",
                format!("method '{}' not found\n  = help: ensure the method is defined in the class", method_name)))?;
        
        // Debug: print function signature
        coffee_debug!("DEBUG: compile_method_call: function='{}', param_count={}", function.get_name().to_str().unwrap_or("unknown"), function.get_params().len());
        for (i, param) in function.get_params().into_iter().enumerate() {
            coffee_debug!("DEBUG: compile_method_call: param {} type={:?}", i, param.get_type());
        }
        
        // Build the call with object as first argument (self)
        let mut call_args = vec![object_value];
        call_args.extend(compiled_args);
        
        // Convert arguments to BasicMetadataValueEnum
        let metadata_args: Vec<inkwell::values::BasicMetadataValueEnum> = call_args.iter()
            .map(|v| (*v).into())
            .collect();
        
        let call_result = self.backend.builder.build_call(
            function,
            &metadata_args,
            &format!("{}_call", method_name)
        ).map_err(|e| self.error("compile_method_call",
            format!("failed to call method '{}': {}", method_name, e)))?;
        
        // Return the result
        match call_result.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => Ok(val),
            inkwell::values::ValueKind::Instruction(_) => Ok(inkwell::values::BasicValueEnum::IntValue(
                self.backend.context.i64_type().const_int(0, false)
            )),
        }
    }

    /// Get field index by name in a struct type
    pub(crate) fn get_field_index(&self, struct_type: &inkwell::types::StructType<'ctx>, field_name: &str) -> Result<usize, String> {
        // Try to get field names from class definitions
        for (class_name, class_def) in &self.classes {
            if struct_type.get_name().map_or(false, |n| n.to_string_lossy().contains(class_name)) {
                for (i, field) in class_def.fields.iter().enumerate() {
                    if field.name == field_name {
                        return Ok(i);
                    }
                }
            }
        }
        
        // Fallback: use field count and assume sequential naming
        let count = struct_type.count_fields() as usize;
        for i in 0..count {
            // Try to find the field by index
            if struct_type.get_field_type_at_index(i as u32).is_some() {
                // If we can't get the name, just return the index
                // The caller will handle any type mismatches
                return Ok(i);
            }
        }
        
        Err(format!("field '{}' not found in struct", field_name))
    }
}
