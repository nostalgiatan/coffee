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
        }
        if args.is_empty() {
            return self.compile_field_access_expr(object, field);
        }
        self.compile_method_call_expr(object, field, args)
    }

    pub(crate) fn compile_assign_expr(
        &mut self,
        object: &Expression,
        field_name: &str,
        value: &Expression,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let field_ptr = self.compile_field_access_ptr_expr(object, field_name)?;
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
        self.compile_method_call_expr(&Expression::Variable(object_str.to_string()), method_name, args)
    }

    fn object_lookup_key(object: &Expression) -> String {
        match object {
            Expression::Variable(n) => n.clone(),
            other => other.to_string(),
        }
    }

    fn parse_arg_list_src(args_str: &str) -> Vec<String> {
        let mut args = Vec::new();
        if args_str.is_empty() {
            return args;
        }
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
        args
    }

    fn parse_source_expr(src: &str) -> Expression {
        match Expression::parse(src) {
            Ok(tree) => tree,
            Err(_) => Expression::Literal(src.to_string()),
        }
    }

    /// Compile field access: object.field
    pub fn compile_field_access(&mut self, object_str: &str, field_name: &str) -> Result<BasicValueEnum<'ctx>, String> {
        self.compile_field_access_expr(&Expression::Variable(object_str.to_string()), field_name)
    }

    fn compile_field_access_expr(&mut self, object: &Expression, field_name: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let object_str = Self::object_lookup_key(object);
        let object_value = self.compile_expr(object)?;

        let object_type = object_value.get_type();

        coffee_debug!("DEBUG: compile_field_access: object_str='{}', field_name='{}', object_type={:?}",
            object_str, field_name, object_type);

        match object_type {
            BasicTypeEnum::PointerType(_) => {
                if let Some(class_name) = self.variable_types.get(&object_str) {
                    if let Some(&struct_type) = self.type_mapper.struct_types.get(class_name) {
                        let field_index = self.get_field_index(&struct_type, field_name)?;

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

                        let field_value = self.backend.builder.build_load(
                            struct_type.get_field_type_at_index(field_index as u32).unwrap(),
                            field_ptr,
                            &format!("{}_{}", object_str, field_name)
                        ).map_err(|e| self.error("compile_field_access",
                            format!("failed to load field '{}': {}", field_name, e)))?;

                        return Ok(field_value);
                    }
                }

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
                let field_index = self.get_field_index(&struct_type, field_name)?;

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
        self.compile_field_access_ptr_expr(&Expression::Variable(object_str.to_string()), field_name)
    }

    fn compile_field_access_ptr_expr(&mut self, object: &Expression, field_name: &str) -> Result<PointerValue<'ctx>, String> {
        let object_str = Self::object_lookup_key(object);
        let object_value = self.compile_expr(object)?;

        let object_type = object_value.get_type();

        match object_type {
            BasicTypeEnum::PointerType(_) => {
                let object_ptr = object_value.into_pointer_value();

                let var_type = if let Some(&(_, var_type)) = self.variables.get(&object_str) {
                    var_type
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("variable '{}' not found", object_str)));
                };

                let field_index = match var_type {
                    BasicTypeEnum::StructType(s) => self.get_field_index(&s, field_name)?,
                    BasicTypeEnum::PointerType(_) => {
                        if let Some(class_name) = self.variable_types.get(&object_str) {
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

                let zero = self.backend.context.i64_type().const_int(0, false);
                let field_index_val = self.backend.context.i32_type().const_int(field_index as u64, false);

                let class_name = if let Some(name) = self.variable_types.get(&object_str) {
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
            BasicTypeEnum::StructType(_) => {
                Err(self.error("compile_field_access_ptr",
                    format!("field access on struct value not supported for assignment (use pointer)")))
            }
            _ => Err(self.error("compile_field_access_ptr",
                format!("field access on non-struct type: {}", self.type_to_string(object_type))))
        }
    }

    /// Compile method call: object.method(args)
    pub fn compile_method_call(&mut self, object_str: &str, method_name: &str, args_str: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let parsed_args: Vec<Expression> = Self::parse_arg_list_src(args_str)
            .into_iter()
            .map(|s| Self::parse_source_expr(&s))
            .collect();
        self.compile_method_call_expr(&Expression::Variable(object_str.to_string()), method_name, &parsed_args)
    }

    fn compile_method_call_expr(&mut self, object: &Expression, method_name: &str, args: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        let object_str = Self::object_lookup_key(object);
        let object_value = self.compile_expr(object)?;

        let class_name = if let Some(class_name) = self.variable_types.get(&object_str) {
            class_name.clone()
        } else {
            object_str.clone()
        };

        coffee_debug!("DEBUG: compile_method_call: variable_types keys: {:?}", self.variable_types.keys().collect::<Vec<_>>());
        coffee_debug!("DEBUG: compile_method_call: object_str='{}', variable_types.get(object_str)={:?}",
            object_str, self.variable_types.get(&object_str));

        let mut compiled_args = Vec::new();
        for arg in args {
            compiled_args.push(self.compile_expr(arg)?);
        }

        let full_method_name = format!("{}_{}", class_name, method_name);

        coffee_debug!("DEBUG: compile_method_call: object_str='{}', method_name='{}', class_name='{}', full_method_name='{}'",
            object_str, method_name, class_name, full_method_name);
        coffee_debug!("DEBUG: compile_method_call: functions keys: {:?}", self.functions.keys().collect::<Vec<_>>());

        let function = self.functions.get(&full_method_name)
            .copied()
            .or_else(|| self.functions.get(method_name).copied())
            .ok_or_else(|| self.error("compile_method_call",
                format!("method '{}' not found\n  = help: ensure the method is defined in the class", method_name)))?;

        coffee_debug!("DEBUG: compile_method_call: function='{}', param_count={}", function.get_name().to_str().unwrap_or("unknown"), function.get_params().len());
        for (i, param) in function.get_params().into_iter().enumerate() {
            coffee_debug!("DEBUG: compile_method_call: param {} type={:?}", i, param.get_type());
        }

        let mut call_args = vec![object_value];
        call_args.extend(compiled_args);

        let metadata_args: Vec<inkwell::values::BasicMetadataValueEnum> = call_args.iter()
            .map(|v| (*v).into())
            .collect();

        let call_result = self.backend.builder.build_call(
            function,
            &metadata_args,
            &format!("{}_call", method_name)
        ).map_err(|e| self.error("compile_method_call",
            format!("failed to call method '{}': {}", method_name, e)))?;

        match call_result.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => Ok(val),
            inkwell::values::ValueKind::Instruction(_) => Ok(inkwell::values::BasicValueEnum::IntValue(
                self.backend.context.i64_type().const_int(0, false)
            )),
        }
    }

    /// Get field index by name in a struct type
    pub(crate) fn get_field_index(&self, struct_type: &inkwell::types::StructType<'ctx>, field_name: &str) -> Result<usize, String> {
        for (class_name, class_def) in &self.classes {
            if struct_type.get_name().map_or(false, |n| n.to_string_lossy().contains(class_name)) {
                for (i, field) in class_def.fields.iter().enumerate() {
                    if field.name == field_name {
                        return Ok(i);
                    }
                }
            }
        }

        let count = struct_type.count_fields() as usize;
        for i in 0..count {
            if struct_type.get_field_type_at_index(i as u32).is_some() {
                return Ok(i);
            }
        }

        Err(format!("field '{}' not found in struct", field_name))
    }
}
