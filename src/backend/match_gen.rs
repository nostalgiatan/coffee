//! Match expression code generation
//!
//! Compiles Coffee `match` statements to LLVM IR. Pattern matching helpers
//! live here so they stay off the codegen coordinator.

use crate::coffee_debug;
use super::codegen::CodeGenerator;

use inkwell::values::BasicValueEnum;
use inkwell::types::BasicTypeEnum;
use inkwell::IntPredicate;
use inkwell::FloatPredicate;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Branch to `dest` if the current insert block has no terminator.
    fn branch_to_if_unterminated(
        &self,
        dest: inkwell::basic_block::BasicBlock,
    ) -> Result<(), String> {
        let Some(block) = self.backend.builder.get_insert_block() else {
            return Ok(());
        };
        if block.get_terminator().is_none() {
            self.backend.builder
                .build_unconditional_branch(dest)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Parse a field pair from a string like "field_name: var_name"
    fn parse_field_pair(field_pair: &str) -> (String, String) {
        let field_pair = field_pair.trim();
        if let Some(colon_pos) = field_pair.find(':') {
            let field_name = field_pair[..colon_pos].trim().to_string();
            let var_name = field_pair[colon_pos + 1..].trim().to_string();
            (field_name, var_name)
        } else {
            (field_pair.to_string(), "_".to_string())
        }
    }
    
    /// * `match_expr` - The parsed MatchExpr to compile
    /// 
    /// # Returns
        /// 
                /// * `Ok(())` - If the match expression was compiled successfully
                /// * `Err(String)` - If there was an error during compilation
            pub fn compile_match(&mut self, match_expr: &crate::parser::MatchExpr) -> Result<(), String> {
                let function = self.current_function
                    .ok_or("match outside function")?;
        
                let match_value_str = match &match_expr.value {
                    crate::parser::expr::Expression::Variable(n) => n.clone(),
                    other => other.to_string(),
                };
                let match_val = self.compile_expr(&match_expr.value)?;
                let merge_block = self.backend.context.append_basic_block(function, "matchend");
        
                // For each arm, create a comparison and branch
                let mut current_block = self.backend.builder.get_insert_block().unwrap();
        
                for (i, arm) in match_expr.arms.iter().enumerate() {
                    let arm_block = self.backend.context.append_basic_block(function, &format!("matcharm_{}", i));
                    let next_block = self.backend.context.append_basic_block(function, &format!("matchnext_{}", i));
        
                    self.backend.builder.position_at_end(current_block);
                    let pattern = match &arm.pattern {
                    crate::parser::expr::Expression::Variable(n) => n.clone(),
                    crate::parser::expr::Expression::Literal(s) => s.clone(),
                    other => other.to_string(),
                };
        
                    // Check if pattern is a tuple pattern (contains variables)
                    let is_tuple_pattern = pattern.starts_with('(') && pattern.contains(',') && pattern.ends_with(')');
                    coffee_debug!("DEBUG: compile_match: pattern='{}', is_tuple_pattern={}", pattern, is_tuple_pattern);
            // Match guard: binding + optional guard expression.
            if let Some(guard_expr) = &arm.guard {
                let base = pattern.trim();
                let is_binding = !base.is_empty() && base != "_"
                    && base.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
                    && base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if is_binding {
                    let bind_ty = match_val.get_type();
                    let alloca = self.backend.builder.build_alloca(bind_ty, base)
                        .map_err(|e| format!("match guard: failed to alloca '{}': {}", base, e))?;
                    self.backend.builder.build_store(alloca, match_val)
                        .map_err(|e| format!("match guard: failed to store '{}': {}", base, e))?;
                    self.variables.insert(base.to_string(), (alloca, bind_ty));
                }
                let guard_val = self.compile_expr(guard_expr)
                    .map_err(|e| format!("match guard: {}", e))?;
                let guard_bool = super::control_flow::value_to_bool(guard_val, &self.backend.builder)?;
                self.backend.builder.build_conditional_branch(guard_bool, arm_block, next_block)
                    .map_err(|e| e.to_string())?;
            } else if pattern != "_" {
                if is_tuple_pattern {
                    // Tuple pattern: extract variables and bind them
                    let inner = &pattern[1..pattern.len()-1].trim();
                    
                    // Parse the tuple pattern correctly, handling nested tuples
                    let mut vars: Vec<&str> = Vec::new();
                    let mut depth = 0;
                    let mut start = 0;
                    for (i, c) in inner.chars().enumerate() {
                        match c {
                            '(' => depth += 1,
                            ')' => depth -= 1,
                            ',' if depth == 0 => {
                                vars.push(&inner[start..i].trim());
                                start = i + 1;
                            }
                            _ => {}
                        }
                    }
                    if start < inner.len() {
                        vars.push(&inner[start..].trim());
                    }
                    
                    coffee_debug!("DEBUG: compile_match: parsed pattern '{}' into vars: {:?}", pattern, vars);

                    // Check if match_val is a pointer (needs to load) or already a value
                    let tuple_val = if let BasicValueEnum::PointerValue(ptr_val) = match_val {
                        // Load the struct value from the pointer
                        coffee_debug!("DEBUG: compile_match: match_val is PointerValue, loading struct value");
                        
                        // Try to get the type from the variable
                        let var_type = if let Some((_, var_type)) = self.variables.get(&match_value_str) {
                            Some(var_type)
                        } else {
                            None
                        };
                        
                        coffee_debug!("DEBUG: compile_match: var_type = {:?}", var_type);
                        
                        // Load the struct value from the pointer using the correct struct type
                        if let Some(BasicTypeEnum::StructType(struct_type)) = var_type {
                            coffee_debug!("DEBUG: compile_match: using struct type {:?}", struct_type);
                            // Use struct_type.clone() to create an owned StructType
                            self.backend.builder.build_load(BasicTypeEnum::StructType(struct_type.clone()), ptr_val, "tuple_val")
                                .map_err(|e| format!("failed to load tuple value: {}", e))?
                        } else {
                            // Fallback: try to load as i64 (for simple tuples)
                            coffee_debug!("DEBUG: compile_match: var_type is not StructType, using i64_type");
                            let i64_type = self.backend.context.i64_type();
                            self.backend.builder.build_load(BasicTypeEnum::IntType(i64_type), ptr_val, "tuple_val")
                                .map_err(|e| format!("failed to load tuple value: {}", e))?
                        }
                    } else {
                        // Already a value
                        coffee_debug!("DEBUG: compile_match: match_val is already a value: {:?}", match_val);
                        match_val
                    };

                    coffee_debug!("DEBUG: compile_match: loaded value is {:?}", tuple_val);

                    if let BasicValueEnum::StructValue(tuple_val) = tuple_val {
                        coffee_debug!("DEBUG: compile_match: tuple_val is StructValue");
                        coffee_debug!("DEBUG: compile_match: vars = {:?}", vars);
                        for (j, var_name) in vars.iter().enumerate() {
                            coffee_debug!("DEBUG: compile_match: processing var '{}' (index {})", var_name, j);
                            if *var_name != "_" {
                                // Extract field from tuple using build_extract_value
                                let field_val = self.backend.builder.build_extract_value(
                                    tuple_val,
                                    j as u32,
                                    var_name
                                ).map_err(|e| format!("failed to extract tuple field: {}", e))?;
                                
                                coffee_debug!("DEBUG: compile_match: extracted field '{}' (index {}), type: {:?}", var_name, j, field_val.get_type());

                                // Check if the field is itself a tuple pattern
                                if var_name.starts_with('(') && var_name.contains(',') && var_name.ends_with(')') {
                                    // Recursively handle nested tuple pattern
                                    coffee_debug!("DEBUG: compile_match: field '{}' is a nested tuple pattern", var_name);
                                    
                                    // Parse the nested tuple pattern
                                    let inner = &var_name[1..var_name.len()-1].trim();
                                    let mut nested_vars: Vec<&str> = Vec::new();
                                    let mut depth = 0;
                                    let mut start = 0;
                                    for (i, c) in inner.chars().enumerate() {
                                        match c {
                                            '(' => depth += 1,
                                            ')' => depth -= 1,
                                            ',' if depth == 0 => {
                                                nested_vars.push(&inner[start..i].trim());
                                                start = i + 1;
                                            }
                                            _ => {}
                                        }
                                    }
                                    if start < inner.len() {
                                        nested_vars.push(&inner[start..].trim());
                                    }
                                    
                                    coffee_debug!("DEBUG: compile_match: nested vars: {:?}", nested_vars);
                                    
                                    // Extract nested tuple fields directly from the field value
                                    // Don't compile the pattern as an expression
                                    if let BasicValueEnum::StructValue(nested_tuple) = field_val {
                                        for (k, nested_var_name) in nested_vars.iter().enumerate() {
                                            if *nested_var_name != "_" {
                                                let nested_field_val = self.backend.builder.build_extract_value(
                                                    nested_tuple,
                                                    k as u32,
                                                    nested_var_name
                                                ).map_err(|e| format!("failed to extract nested tuple field: {}", e))?;

                                                coffee_debug!("DEBUG: compile_match: extracted nested field '{}' (index {}), type: {:?}", nested_var_name, k, nested_field_val.get_type());

                                                // Store the variable in the variables map
                                                let var_type = nested_field_val.get_type();
                                                let var_alloca = self.backend.builder.build_alloca(var_type, nested_var_name)
                                                    .map_err(|e| format!("failed to allocate variable '{}': {}", nested_var_name, e))?;
                                                self.backend.builder.build_store(var_alloca, nested_field_val)
                                                    .map_err(|e| format!("failed to store variable '{}': {}", nested_var_name, e))?;

                                                // Add to variables map
                                                self.variables.insert(nested_var_name.to_string(), (var_alloca, var_type));
                                                coffee_debug!("DEBUG: compile_match: inserted variable '{}' into variables map", nested_var_name);
                                            }
                                        }
                                    } else {
                                        coffee_debug!("DEBUG: compile_match: field '{}' is not a StructValue, it's {:?}", var_name, field_val);
                                    }
                                } else {
                                    // Store the variable in the variables map
                                    let var_type = field_val.get_type();
                                    let var_alloca = self.backend.builder.build_alloca(var_type, var_name)
                                        .map_err(|e| format!("failed to allocate variable '{}': {}", var_name, e))?;
                                    self.backend.builder.build_store(var_alloca, field_val)
                                        .map_err(|e| format!("failed to store variable '{}': {}", var_name, e))?;

                                    // Add to variables map
                                    self.variables.insert(var_name.to_string(), (var_alloca, var_type));
                                    coffee_debug!("DEBUG: compile_match: inserted variable '{}' into variables map", var_name);
                                }
                            }
                        }

                        // Always match tuple patterns (for simplicity)
                        self.backend.builder.build_unconditional_branch(arm_block)
                            .map_err(|e| e.to_string())?;
                    } else {
                        coffee_debug!("DEBUG: compile_match: loaded value is not StructValue, it's {:?}", tuple_val);
                        // Not a struct, compare as before
                        let pattern_val = self.compile_expr(&arm.pattern)?;
                        let cond = match (match_val, pattern_val) {
                            (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) => {
                                self.backend.builder.build_int_compare(IntPredicate::EQ, a, b, "matchcmp")
                                    .map_err(|e| e.to_string())?
                            }
                            (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) => {
                                self.arithmetic_ctx.build_float_compare(&self.backend.builder, FloatPredicate::OEQ, a, b, "matchcmpf")
                                    .map_err(|e| e.to_string())?
                            }
                            _ => self.backend.context.bool_type().const_int(1, false),
                        };
                        self.backend.builder.build_conditional_branch(cond, arm_block, next_block)
                            .map_err(|e| e.to_string())?;
                    }
                } else {
                    // Check if pattern is a struct pattern (contains { and })
                    let is_struct_pattern = pattern.contains('{') && pattern.contains('}');
                    coffee_debug!("DEBUG: compile_match: pattern='{}', is_struct_pattern={}", pattern, is_struct_pattern);

                    if is_struct_pattern {
                        // Struct pattern: extract fields and bind them
                        // Parse the struct pattern: ClassName { field1: var1, field2: var2, ... }
                        let struct_name_end = pattern.find('{').unwrap();
                        let struct_name = pattern[..struct_name_end].trim();
                        let fields_str = &pattern[struct_name_end + 1..pattern.len() - 1].trim();

                        coffee_debug!("DEBUG: compile_match: struct_name='{}', fields_str='{}'", struct_name, fields_str);

                        // Parse fields: field1: var1, field2: var2, ...
                        let mut fields: Vec<(String, String)> = Vec::new();
                        let mut current_field = String::new();
                        let mut depth = 0;
                        for c in fields_str.chars() {
                            match c {
                                '{' => depth += 1,
                                '}' => depth -= 1,
                                ',' if depth == 0 => {
                                    fields.push(Self::parse_field_pair(&current_field));
                                    current_field.clear();
                                }
                                _ => current_field.push(c),
                            }
                        }
                        if !current_field.trim().is_empty() {
                            fields.push(Self::parse_field_pair(&current_field));
                        }

                        coffee_debug!("DEBUG: compile_match: parsed fields: {:?}", fields);

                        // Load the struct value
                        let struct_val = if let BasicValueEnum::PointerValue(ptr_val) = match_val {
                            // The struct type may be stored on the variable directly
                            // (StructType) OR — for class instances stored as heap
                            // pointers — the variable's type is PointerType. In the
                            // latter case, fall back to the class name in the pattern
                            // (struct_name) to look up the named struct type.
                            let struct_type = match self.variables.get(&match_value_str) {
                                Some((_, BasicTypeEnum::StructType(st))) => Some(*st),
                                _ => self.type_mapper.struct_types.get(struct_name).copied(),
                            };

                            if let Some(struct_type) = struct_type {
                                self.backend.builder.build_load(BasicTypeEnum::StructType(struct_type.clone()), ptr_val, "struct_val")
                                    .map_err(|e| format!("failed to load struct value: {}", e))?
                            } else {
                                return Err(format!("variable '{}' is not a struct (no struct type found for '{}')", match_value_str, struct_name));
                            }
                        } else {
                            match_val
                        };

                        if let BasicValueEnum::StructValue(struct_val) = struct_val {
                            // Extract fields and bind them to variables
                            for (field_name, var_name) in fields.iter() {
                                if *var_name != "_" {
                                    // Get field index
                                    let field_index = self.type_mapper.get_field_index(struct_name, field_name)
                                        .ok_or_else(|| format!("field '{}' not found in struct '{}'", field_name, struct_name))?;

                                    // Extract field value
                                    let field_val = self.backend.builder.build_extract_value(
                                        struct_val,
                                        field_index as u32,
                                        var_name
                                    ).map_err(|e| format!("failed to extract field '{}': {}", field_name, e))?;

                                    coffee_debug!("DEBUG: compile_match: extracted field '{}' (index {}), type: {:?}", var_name, field_index, field_val.get_type());

                                    // Store the variable in the variables map
                                    let var_type = field_val.get_type();
                                    let var_alloca = self.backend.builder.build_alloca(var_type, var_name)
                                        .map_err(|e| format!("failed to allocate variable '{}': {}", var_name, e))?;
                                    self.backend.builder.build_store(var_alloca, field_val)
                                        .map_err(|e| format!("failed to store variable '{}': {}", var_name, e))?;

                                    // Add to variables map
                                    self.variables.insert(var_name.to_string(), (var_alloca, var_type));
                                    coffee_debug!("DEBUG: compile_match: inserted variable '{}' into variables map", var_name);
                                }
                            }
                        } else {
                            return Err(format!("match value is not a struct"));
                        }

                        // Always match struct patterns (for simplicity)
                        self.backend.builder.build_unconditional_branch(arm_block)
                            .map_err(|e| e.to_string())?;
                    } else {
                        // Non-tuple, non-struct pattern: compare as before
                        let pattern_val = self.compile_expr(&arm.pattern)?;
                        let cond = match (match_val, pattern_val) {
                            (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) => {
                                self.backend.builder.build_int_compare(IntPredicate::EQ, a, b, "matchcmp")
                                    .map_err(|e| e.to_string())?
                            }
                            (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) => {
                                self.arithmetic_ctx.build_float_compare(&self.backend.builder, FloatPredicate::OEQ, a, b, "matchcmpf")
                                    .map_err(|e| e.to_string())?
                            }
                            _ => self.backend.context.bool_type().const_int(1, false),
                        };
                        self.backend.builder.build_conditional_branch(cond, arm_block, next_block)
                            .map_err(|e| e.to_string())?;
                    }
                }
            } else {
                // Default case (wildcard _)
                self.backend.builder.build_unconditional_branch(arm_block)
                    .map_err(|e| e.to_string())?;
            }

            // Compile arm body as statements (not a newline-joined string)
            self.backend.builder.position_at_end(arm_block);
            for stmt in &arm.body {
                self.compile_statement(stmt)?;
            }
            // CRITICAL: check the builder's CURRENT block for a terminator, not
            // arm_block. Overflow-check sub-blocks (add_merge, mul_merge, …) are
            // the insert point after a nested arithmetic expression.
            self.branch_to_if_unterminated(merge_block)?;

            current_block = next_block;
        }

        // Fall-through next_block of the last arm (may have no predecessors).
        self.backend.builder.position_at_end(current_block);
        self.branch_to_if_unterminated(merge_block)?;

        self.backend.builder.position_at_end(merge_block);
        
        // Print LLVM IR for debugging
        coffee_debug!("DEBUG: compile_match: LLVM IR:\n{}", self.backend.module.print_to_string().to_string());
        
        Ok(())
    }
}
