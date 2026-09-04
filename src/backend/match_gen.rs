//! Match expression code generation
//!
//! Compiles Coffee `match` statements to LLVM IR. Pattern matching helpers
//! live here so they stay off the codegen coordinator.

use crate::coffee_debug;
use crate::parser::expr::Expression;
use crate::parser::Pattern;
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

    fn store_pattern_binding(
        &mut self,
        name: &str,
        value: BasicValueEnum<'ctx>,
    ) -> Result<(), String> {
        let var_type = value.get_type();
        let var_alloca = self.backend.builder.build_alloca(var_type, name)
            .map_err(|e| format!("failed to allocate variable '{}': {}", name, e))?;
        self.backend.builder.build_store(var_alloca, value)
            .map_err(|e| format!("failed to store variable '{}': {}", name, e))?;
        self.variables.insert(name.to_string(), (var_alloca, var_type));
        coffee_debug!("DEBUG: compile_match: inserted variable '{}' into variables map", name);
        Ok(())
    }

    /// Pointee type for a pointer scrutinee: Pattern struct name, else `Expression::Variable`.
    fn aggregate_pointee_type(
        &self,
        scrutinee: &Expression,
        struct_name: Option<&str>,
    ) -> Result<inkwell::types::StructType<'ctx>, String> {
        if let Some(name) = struct_name {
            if let Some(st) = self.type_mapper.struct_types.get(name).copied() {
                return Ok(st);
            }
        }
        match scrutinee {
            Expression::Variable(name) => {
                if let Some((_, BasicTypeEnum::StructType(st))) = self.variables.get(name) {
                    Ok(*st)
                } else if let Some(ty_name) = struct_name {
                    Err(format!(
                        "variable '{}' is not a struct (no struct type found for '{}')",
                        name, ty_name
                    ))
                } else {
                    Err(format!(
                        "variable '{}' is not an aggregate type (cannot destructure in match)",
                        name
                    ))
                }
            }
            _ => match struct_name {
                Some(ty_name) => Err(format!(
                    "cannot load struct '{}' from a non-variable match scrutinee (need a variable, not a computed expression)",
                    ty_name
                )),
                None => Err(
                    "tuple/aggregate match requires a variable scrutinee; cannot look up the pointee type for a non-variable match value".to_string(),
                ),
            },
        }
    }

    fn load_aggregate_value(
        &mut self,
        match_val: BasicValueEnum<'ctx>,
        scrutinee: &Expression,
        struct_name: Option<&str>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if let BasicValueEnum::PointerValue(ptr_val) = match_val {
            let struct_type = self.aggregate_pointee_type(scrutinee, struct_name)?;
            self.backend.builder
                .build_load(BasicTypeEnum::StructType(struct_type), ptr_val, "agg_val")
                .map_err(|e| format!("failed to load aggregate value: {}", e))
        } else {
            Ok(match_val)
        }
    }

    fn bind_pattern_value(
        &mut self,
        pattern: &Pattern,
        value: BasicValueEnum<'ctx>,
        scrutinee: &Expression,
    ) -> Result<(), String> {
        match pattern {
            Pattern::Wildcard | Pattern::Literal(_) => Ok(()),
            Pattern::Ident(name) => self.store_pattern_binding(name, value),
            Pattern::Tuple(elems) => {
                let loaded = self.load_aggregate_value(value, scrutinee, None)?;
                let BasicValueEnum::StructValue(tuple_val) = loaded else {
                    coffee_debug!("DEBUG: compile_match: tuple value is not StructValue, {:?}", loaded);
                    return Ok(());
                };
                for (j, elem) in elems.iter().enumerate() {
                    if matches!(elem, Pattern::Wildcard) {
                        continue;
                    }
                    let field_val = self.backend.builder
                        .build_extract_value(tuple_val, j as u32, &format!("tup_{}", j))
                        .map_err(|e| format!("failed to extract tuple field: {}", e))?;
                    self.bind_pattern_value(elem, field_val, scrutinee)?;
                }
                Ok(())
            }
            Pattern::Struct { name, fields } => {
                let loaded = self.load_aggregate_value(value, scrutinee, Some(name))?;
                let BasicValueEnum::StructValue(struct_val) = loaded else {
                    return Err("match value is not a struct".to_string());
                };
                for (field_name, field_pat) in fields.iter() {
                    if matches!(field_pat, Pattern::Wildcard) {
                        continue;
                    }
                    if matches!(field_pat, Pattern::Literal(_)) {
                        continue;
                    }
                    let field_index = self.type_mapper
                        .get_field_index(name, field_name)
                        .ok_or_else(|| {
                            format!("field '{}' not found in struct '{}'", field_name, name)
                        })?;
                    let field_val = self.backend.builder
                        .build_extract_value(struct_val, field_index as u32, field_name)
                        .map_err(|e| format!("failed to extract field '{}': {}", field_name, e))?;
                    self.bind_pattern_value(field_pat, field_val, scrutinee)?;
                }
                Ok(())
            }
            Pattern::EnumVariant { args, .. } => {
                for arg in args {
                    self.bind_pattern_value(arg, value, scrutinee)?;
                }
                Ok(())
            }
            Pattern::Or(alts) => {
                if let Some(first) = alts.first() {
                    self.bind_pattern_value(first, value, scrutinee)?;
                }
                Ok(())
            }
        }
    }

    fn compare_match_values(
        &mut self,
        match_val: BasicValueEnum<'ctx>,
        pattern_val: BasicValueEnum<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        match (match_val, pattern_val) {
            (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) => {
                self.backend.builder
                    .build_int_compare(IntPredicate::EQ, a, b, "matchcmp")
                    .map_err(|e| e.to_string())
            }
            (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) => {
                self.arithmetic_ctx
                    .build_float_compare(&self.backend.builder, FloatPredicate::OEQ, a, b, "matchcmpf")
                    .map_err(|e| e.to_string())
            }
            _ => Ok(self.backend.context.bool_type().const_int(1, false)),
        }
    }

    fn or_conds(
        &mut self,
        acc: Option<inkwell::values::IntValue<'ctx>>,
        next: inkwell::values::IntValue<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        match acc {
            None => Ok(next),
            Some(prev) => self.backend.builder
                .build_or(prev, next, "matchor")
                .map_err(|e| e.to_string()),
        }
    }

    /// Returns `None` when the arm always matches (wildcard / bindings / destructure).
    fn pattern_condition(
        &mut self,
        pattern: &Pattern,
        match_val: BasicValueEnum<'ctx>,
        scrutinee: &Expression,
    ) -> Result<Option<inkwell::values::IntValue<'ctx>>, String> {
        coffee_debug!("DEBUG: compile_match: pattern={:?}", pattern);
        match pattern {
            Pattern::Wildcard => Ok(None),
            Pattern::Ident(name) => {
                self.store_pattern_binding(name, match_val)?;
                Ok(None)
            }
            Pattern::Tuple(_) | Pattern::Struct { .. } => {
                self.bind_pattern_value(pattern, match_val, scrutinee)?;
                Ok(None)
            }
            Pattern::Literal(_) | Pattern::EnumVariant { .. } => {
                let Some(expr) = pattern.to_compare_expr() else {
                    return Ok(Some(self.backend.context.bool_type().const_int(1, false)));
                };
                let pattern_val = self.compile_expr(&expr)?;
                Ok(Some(self.compare_match_values(match_val, pattern_val)?))
            }
            Pattern::Or(alts) => {
                let mut acc: Option<inkwell::values::IntValue<'ctx>> = None;
                for alt in alts {
                    match self.pattern_condition(alt, match_val, scrutinee)? {
                        None => return Ok(None),
                        Some(cond) => acc = Some(self.or_conds(acc, cond)?),
                    }
                }
                Ok(acc.or_else(|| Some(self.backend.context.bool_type().const_int(1, false))))
            }
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

        let match_val = self.compile_expr(&match_expr.value)?;
        let merge_block = self.backend.context.append_basic_block(function, "matchend");

        let mut current_block = self.backend.builder.get_insert_block().unwrap();

        for (i, arm) in match_expr.arms.iter().enumerate() {
            let arm_block = self.backend.context.append_basic_block(function, &format!("matcharm_{}", i));
            let next_block = self.backend.context.append_basic_block(function, &format!("matchnext_{}", i));

            self.backend.builder.position_at_end(current_block);

            if let Some(guard_expr) = &arm.guard {
                self.bind_pattern_value(&arm.pattern, match_val, &match_expr.value)?;
                let guard_val = self.compile_expr(guard_expr)
                    .map_err(|e| format!("match guard: {}", e))?;
                let guard_bool = super::control_flow::value_to_bool(guard_val, &self.backend.builder)?;
                self.backend.builder.build_conditional_branch(guard_bool, arm_block, next_block)
                    .map_err(|e| e.to_string())?;
            } else {
                match self.pattern_condition(&arm.pattern, match_val, &match_expr.value)? {
                    None => {
                        self.backend.builder.build_unconditional_branch(arm_block)
                            .map_err(|e| e.to_string())?;
                    }
                    Some(cond) => {
                        self.backend.builder.build_conditional_branch(cond, arm_block, next_block)
                            .map_err(|e| e.to_string())?;
                    }
                }
            }

            self.backend.builder.position_at_end(arm_block);
            for stmt in &arm.body {
                self.compile_statement(stmt)?;
            }
            self.branch_to_if_unterminated(merge_block)?;

            current_block = next_block;
        }

        self.backend.builder.position_at_end(current_block);
        self.branch_to_if_unterminated(merge_block)?;

        self.backend.builder.position_at_end(merge_block);

        coffee_debug!("DEBUG: compile_match: LLVM IR:\n{}", self.backend.module.print_to_string().to_string());

        Ok(())
    }
}
