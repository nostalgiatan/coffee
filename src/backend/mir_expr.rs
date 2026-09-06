//! Typed MIR expression codegen. Production never converts expressions with
//! `hir_expr_to_ast`. Computed callees use `compile_hir_computed_call_typed`.

use super::codegen::CodeGenerator;
use crate::hir::{HirExpr, HirExprKind};
use crate::types::Type;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType, IntType};
use inkwell::values::{AnyValue, AnyValueEnum, BasicValueEnum, FunctionValue, PointerValue};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_hir_expr_typed(
        &mut self,
        expr: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match &expr.kind {
            HirExprKind::Literal(lit) => self.compile_hir_literal_typed(&expr.ty, lit),
            HirExprKind::Variable(name) => self.compile_variable_ref(name),
            HirExprKind::Binary { left, op, right } => {
                if op == "&&" || op == "||" {
                    let l = self.compile_hir_expr_typed(left)?;
                    return self.compile_short_circuit_logic(op, l, |this| {
                        this.compile_hir_expr_typed(right)
                    });
                }
                let l = self.compile_hir_expr_typed(left)?;
                let r = self.compile_hir_expr_typed(right)?;
                self.build_binary_op(op, l, r)
            }
            HirExprKind::Unary { op, operand } => {
                if op == "&" || op == "&mut" {
                    return self.compile_hir_addr_of(operand, expr);
                }
                if op == "*" {
                    return self.compile_hir_deref(operand, expr);
                }
                let operand_val = self.compile_hir_expr_typed(operand)?;
                self.compile_unary_op(op, operand_val)
            }
            HirExprKind::FString {
                template,
                placeholders,
            } => self.compile_fstring_from_ast(template, placeholders),
            HirExprKind::ArrayLiteral { elements } if elements.is_empty() => {
                self.emit_empty_array_literal(&expr.ty)
            }
            HirExprKind::ArrayLiteral { elements } => {
                let compiled: Vec<BasicValueEnum<'ctx>> = elements
                    .iter()
                    .map(|e| self.compile_hir_expr_typed(e))
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit_array_literal(compiled)
            }
            HirExprKind::TupleLiteral { elements } => {
                let compiled: Vec<BasicValueEnum<'ctx>> = elements
                    .iter()
                    .map(|e| self.compile_hir_expr_typed(e))
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit_tuple_literal(compiled)
            }
            HirExprKind::Index { array, index } => self.compile_hir_index_typed(array, index, expr),
            HirExprKind::TupleField { tuple, index } => {
                self.compile_hir_tuple_field_typed(tuple, *index, expr)
            }
            HirExprKind::EnumTag { value, enum_name } => {
                self.compile_hir_enum_tag(value, enum_name)
            }
            HirExprKind::EnumPayload {
                value,
                enum_name,
                variant,
                index,
            } => self.compile_hir_enum_payload(value, enum_name, variant, *index),
            HirExprKind::Len { collection } => self.compile_hir_len_typed(collection),
            HirExprKind::Member {
                object,
                field,
                args,
            } if args.is_empty() => self.compile_hir_member_field_typed(object, field, expr),
            HirExprKind::Member {
                object,
                field,
                args,
            } => self.compile_hir_member_method_typed(object, field, args, expr),
            HirExprKind::Call { function, args } => {
                self.compile_hir_call_typed(function, args, expr)
            }
            HirExprKind::ConstructorCall { class_name, args } => {
                self.compile_hir_constructor_typed(class_name, args)
            }
            HirExprKind::StructLiteral { struct_name, fields } => {
                self.compile_hir_struct_literal_typed(struct_name, fields)
            }
            HirExprKind::TypeCast { target_type, value } => {
                let compiled = self.compile_hir_expr_typed(value)?;
                self.emit_type_cast(target_type, compiled)
            }
            HirExprKind::Assign {
                object,
                field_name,
                value,
            } => self.compile_hir_assign_typed(object, field_name, value),
            HirExprKind::AnonymousFunction { func } => {
                self.compile_hir_anonymous_function(func)
            }
        }
    }

    fn compile_hir_anonymous_function(
        &mut self,
        func: &crate::parser::function::Function,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let parent_block = self.backend.builder.get_insert_block();
        let result = self.compile_nested_function(func);
        if let Some(bb) = parent_block {
            self.backend.builder.position_at_end(bb);
        }
        result?;
        let fv = self.functions.get(&func.name).copied().ok_or_else(|| {
            self.error(
                "compile_expression",
                format!("anonymous function '{}' was not declared", func.name),
            )
        })?;
        Ok(fv.as_global_value().as_pointer_value().into())
    }

    /// First MIR bind that is not int/float/bool (those use a zero
    /// `compile_local_variable_decl` so `i = i + 1` can load the alloca).
    pub(crate) fn alloc_mir_local_slot(&mut self, name: &str, ty: &Type) -> Result<(), String> {
        let type_str = ty.to_string();
        let llvm_type = self.coffee_type_to_llvm(&type_str).map_err(|e| {
            self.error(
                "compile_local_variable_decl",
                format!(
                    "failed to resolve type '{}' for variable '{}': {}",
                    type_str, name, e
                ),
            )
        })?;
        let alloca = self
            .create_entry_alloca_preserving_terminator(llvm_type, name)
            .map_err(|e| {
                self.error(
                    "compile_local_variable_decl",
                    format!("failed to allocate variable '{}': {}", name, e),
                )
            })?;
        match llvm_type {
            inkwell::types::BasicTypeEnum::IntType(t) => {
                self.backend
                    .builder
                    .build_store(alloca, t.const_zero())
                    .map_err(|e| {
                        self.error(
                            "compile_local_variable_decl",
                            format!("failed to zero-initialize variable '{}': {}", name, e),
                        )
                    })?;
            }
            inkwell::types::BasicTypeEnum::FloatType(t) => {
                self.backend
                    .builder
                    .build_store(alloca, t.const_zero())
                    .map_err(|e| {
                        self.error(
                            "compile_local_variable_decl",
                            format!("failed to zero-initialize variable '{}': {}", name, e),
                        )
                    })?;
            }
            inkwell::types::BasicTypeEnum::PointerType(t) => {
                self.backend
                    .builder
                    .build_store(alloca, t.const_zero())
                    .map_err(|e| {
                        self.error(
                            "compile_local_variable_decl",
                            format!("failed to zero-initialize variable '{}': {}", name, e),
                        )
                    })?;
            }
            _ => {}
        }
        self.variables.insert(name.to_string(), (alloca, llvm_type));
        if let Some(inner) = type_str.strip_prefix("&mut ") {
            if let Ok(pointee) = self.coffee_type_to_llvm(inner.trim()) {
                self.ref_pointee_types.insert(name.to_string(), pointee);
            }
        } else if let Some(inner) = type_str.strip_prefix('&') {
            if let Ok(pointee) = self.coffee_type_to_llvm(inner.trim()) {
                self.ref_pointee_types.insert(name.to_string(), pointee);
            }
        }
        if !matches!(
            type_str.as_str(),
            "int" | "float" | "bool" | "string" | "void" | "()"
        ) {
            self.memory_ctx.set_variable_type(name.to_string(), type_str.clone());
        }
        if type_str.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !matches!(
                type_str.as_str(),
                "int" | "float" | "bool" | "string" | "void" | "()"
            )
        {
            self.variable_types.insert(name.to_string(), type_str.clone());
        }
        self.memory_ctx.register_birth(name.to_string());
        self.memory_ctx.mark_initialized(name);
        Ok(())
    }

    /// Typed RHS then the same store path as `compile_assignment_from_ast`.
    pub(crate) fn store_hir_assign(&mut self, var_name: &str, value: &HirExpr) -> Result<(), String> {
        use crate::backend::memory_ops::VariableState;

        if let Some(info) = self.memory_ctx.lifetimes.get(var_name) {
            match info.state {
                VariableState::Moved => {
                    return Err(self.error(
                        "compile_assignment_from_ast",
                        format!("cannot assign to variable '{}' - variable has been moved\n  = note: moved variables cannot be reassigned\n  = help: use 'clone' before move to preserve the original value", var_name),
                    ));
                }
                VariableState::Dropped => {
                    return Err(self.error(
                        "compile_assignment_from_ast",
                        format!("cannot assign to variable '{}' - variable has been removed/freed\n  = note: this operation would cause a use-after-free vulnerability", var_name),
                    ));
                }
                VariableState::Uninitialized | VariableState::Initialized => {}
            }
        }

        let value = self.compile_hir_expr_typed(value).map_err(|e| {
            self.error(
                "compile_assignment_from_ast",
                format!(
                    "failed to compile value expression for variable '{}': {}",
                    var_name, e
                ),
            )
        })?;

        self.used_variables.insert(var_name.to_string());

        if let Some(&(ptr, var_type)) = self.variables.get(var_name) {
            let converted_value = self.convert_value_to_type(value, var_type, var_name)?;
            self.backend
                .builder
                .build_store(ptr, converted_value)
                .map_err(|e| {
                    self.error(
                        "compile_assignment_from_ast",
                        format!("failed to store value in variable '{}': {}", var_name, e),
                    )
                })?;
            Ok(())
        } else {
            let available = if self.variables.is_empty() {
                String::from("(no variables in scope)")
            } else {
                format!(
                    "available variables: {}",
                    self.variables
                        .keys()
                        .map(|k| format!("'{}'", k))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            Err(self.error(
                "compile_assignment_from_ast",
                format!("undefined variable '{}' - {}", var_name, available),
            ))
        }
    }

    /// Array index: children compile typed when they are leaves; existing
    /// `compile_array_index` still needs AST, so wrap Variable/Literal only.
    fn compile_hir_len_typed(
        &mut self,
        collection: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let Some(name) = hir_variable_name(collection) else {
            return Err(self.error(
                "compile_hir_expr",
                "collection length requires a variable",
            ));
        };
        let i64_type = self.backend.context.i64_type();
        if let Some(&len_ptr) = self.array_lengths.get(name) {
            return self
                .backend
                .builder
                .build_load(i64_type, len_ptr, "arr_len")
                .map_err(|e| {
                    self.error(
                        "compile_hir_expr",
                        format!("failed to load length of collection '{}': {}", name, e),
                    )
                });
        }
        if let Some(&(ptr, var_type)) = self.variables.get(name) {
            if let BasicTypeEnum::StructType(st) = var_type {
                if st.count_fields() == 2 {
                    let loaded = self
                        .backend
                        .builder
                        .build_load(var_type, ptr, "slice_fat")
                        .map_err(|e| {
                            self.error(
                                "compile_hir_expr",
                                format!("failed to load slice '{}': {}", name, e),
                            )
                        })?;
                    return self
                        .backend
                        .builder
                        .build_extract_value(loaded.into_struct_value(), 1, "arr_len")
                        .map_err(|e| {
                            self.error(
                                "compile_hir_expr",
                                format!("failed to extract length of '{}': {}", name, e),
                            )
                        });
                }
            }
        }
        Err(self.error(
            "compile_for",
            format!(
                "cannot iterate over collection '{}' - length information not available\n  = note: collection length must be tracked when the collection is created\n  = help: use explicit range instead: for i in 0..length",
                name
            ),
        ))
    }

    fn enum_is_unit_only(&self, enum_name: &str) -> bool {
        self.enums.get(enum_name).map(|e| {
            e.variants.iter().all(|v| v.fields.is_empty())
        }).unwrap_or(true)
    }

    fn compile_hir_enum_tag(
        &mut self,
        value: &HirExpr,
        enum_name: &str,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let loaded = self.compile_hir_expr_typed(value)?;
        let i64_type = self.backend.context.i64_type();
        match loaded {
            BasicValueEnum::IntValue(i) => {
                if i.get_type() == i64_type {
                    Ok(i.into())
                } else {
                    self.backend
                        .builder
                        .build_int_s_extend(i, i64_type, "enum_tag")
                        .map(Into::into)
                        .map_err(|e| self.error("compile_hir_expr", e.to_string()))
                }
            }
            BasicValueEnum::PointerValue(p) => {
                if self.enum_is_unit_only(enum_name) {
                    self.backend
                        .builder
                        .build_ptr_to_int(p, i64_type, "enum_tag")
                        .map(Into::into)
                        .map_err(|e| self.error("compile_hir_expr", e.to_string()))
                } else {
                    self.backend
                        .builder
                        .build_load(i64_type, p, "enum_tag")
                        .map_err(|e| {
                            self.error(
                                "compile_hir_expr",
                                format!("failed to load enum tag: {}", e),
                            )
                        })
                }
            }
            other => Err(self.error(
                "compile_hir_expr",
                format!("enum tag extract on non-enum value {:?}", other),
            )),
        }
    }

    pub(crate) fn enum_variant_llvm_struct(
        &self,
        enum_name: &str,
        variant: &str,
    ) -> Result<inkwell::types::StructType<'ctx>, String> {
        use crate::parser::class::VariantField;
        let def = self.enums.get(enum_name).ok_or_else(|| {
            self.error(
                "compile_hir_expr",
                format!("unknown enum '{}'", enum_name),
            )
        })?;
        let v = def
            .variants
            .iter()
            .find(|v| v.name == variant)
            .ok_or_else(|| {
                self.error(
                    "compile_hir_expr",
                    format!("unknown variant '{}::{}'", enum_name, variant),
                )
            })?;
        let mut field_tys = vec![self.backend.context.i64_type().into()];
        for f in &v.fields {
            let ty_str = match f {
                VariantField::Type(t) => t.as_str(),
                VariantField::Named { field_type, .. } => field_type.as_str(),
            };
            field_tys.push(self.coffee_type_to_llvm(ty_str).map_err(|e| {
                self.error(
                    "compile_hir_expr",
                    format!("enum payload type '{}': {}", ty_str, e),
                )
            })?);
        }
        Ok(self.backend.context.struct_type(&field_tys, false))
    }

    /// Load `{ tag, payload... }` from an enum pointer (or already-loaded struct)
    /// and extract payload field `index`.
    pub(crate) fn extract_enum_payload_value(
        &mut self,
        loaded: BasicValueEnum<'ctx>,
        enum_name: &str,
        variant: &str,
        index: usize,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let struct_val = match loaded {
            BasicValueEnum::StructValue(sv) => sv,
            BasicValueEnum::PointerValue(ptr) => {
                let st = self.enum_variant_llvm_struct(enum_name, variant)?;
                self.backend
                    .builder
                    .build_load(st, ptr, "enum_val")
                    .map_err(|e| {
                        self.error(
                            "compile_hir_expr",
                            format!("failed to load enum '{}::{}': {}", enum_name, variant, e),
                        )
                    })?
                    .into_struct_value()
            }
            other => {
                return Err(self.error(
                    "compile_hir_expr",
                    format!(
                        "enum payload extract requires a pointer scrutinee ({}::{}); got {:?}",
                        enum_name, variant, other
                    ),
                ));
            }
        };
        self.backend
            .builder
            .build_extract_value(struct_val, (index + 1) as u32, "enum_payload")
            .map_err(|e| {
                self.error(
                    "compile_hir_expr",
                    format!(
                        "failed to extract payload {} of '{}::{}': {}",
                        index, enum_name, variant, e
                    ),
                )
            })
    }

    fn compile_hir_enum_payload(
        &mut self,
        value: &HirExpr,
        enum_name: &str,
        variant: &str,
        index: usize,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let loaded = self.compile_hir_expr_typed(value)?;
        self.extract_enum_payload_value(loaded, enum_name, variant, index)
    }

    fn compile_hir_tuple_field_typed(
        &mut self,
        tuple: &HirExpr,
        index: usize,
        _full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let loaded = self.compile_hir_expr_typed(tuple)?;
        let struct_val = match loaded {
            BasicValueEnum::StructValue(s) => s,
            BasicValueEnum::PointerValue(ptr) => {
                let st = self.llvm_tuple_struct_type(tuple)?;
                self.backend
                    .builder
                    .build_load(st, ptr, "tup_val")
                    .map_err(|e| {
                        self.error(
                            "compile_hir_expr",
                            format!("failed to load tuple for field {}: {}", index, e),
                        )
                    })?
                    .into_struct_value()
            }
            _ => {
                return Err(self.error(
                    "compile_hir_expr",
                    format!("tuple field extract on non-tuple value (index {})", index),
                ));
            }
        };
        self.backend
            .builder
            .build_extract_value(struct_val, index as u32, &format!("tup_{}", index))
            .map_err(|e| {
                self.error(
                    "compile_hir_expr",
                    format!("failed to extract tuple field {}: {}", index, e),
                )
            })
    }

    fn llvm_tuple_struct_type(
        &self,
        tuple: &HirExpr,
    ) -> Result<inkwell::types::StructType<'ctx>, String> {
        let llvm_ty = self
            .coffee_type_to_llvm(&tuple.ty.to_string())
            .map_err(|e| {
                self.error(
                    "compile_hir_expr",
                    format!("tuple field type '{}': {}", tuple.ty, e),
                )
            })?;
        if let inkwell::types::BasicTypeEnum::StructType(st) = llvm_ty {
            return Ok(st);
        }
        if let Some(name) = hir_variable_name(tuple) {
            if let Some(&(_, inkwell::types::BasicTypeEnum::StructType(st))) = self.variables.get(name)
            {
                return Ok(st);
            }
        }
        Err(self.error(
            "compile_hir_expr",
            format!("tuple field extract needs a struct type, got '{}'", tuple.ty),
        ))
    }

    fn compile_hir_index_typed(
        &mut self,
        array: &HirExpr,
        index: &HirExpr,
        _full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let array_name = self.hir_index_array_name(array)?;
        let index_val = self.compile_hir_expr_typed(index)?;
        self.compile_array_index_from_value(&array_name, index_val)
    }

    fn compile_hir_addr_of(
        &mut self,
        operand: &HirExpr,
        _full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match &operand.kind {
            HirExprKind::Variable(name) => {
                let (ptr, _) = *self.variables.get(name).ok_or_else(|| {
                    self.error(
                        "compile_expression",
                        format!("cannot borrow undefined variable '{}'", name),
                    )
                })?;
                self.used_variables.insert(name.clone());
                Ok(ptr.into())
            }
            HirExprKind::Member {
                object,
                field,
                args,
            } if args.is_empty() => {
                let name = self.hir_place_object_name(object)?;
                let ptr = self.compile_field_access_ptr(&name, field)?;
                Ok(ptr.into())
            }
            HirExprKind::Index { array, index } => {
                let array_name = self.hir_index_array_name(array)?;
                let index_val = self.compile_hir_expr_typed(index)?;
                let ptr = self.compile_array_element_ptr_from_value(&array_name, index_val)?;
                Ok(ptr.into())
            }
            _ => Err(self.error(
                "compile_expression",
                "can only borrow a local variable, field, or array element",
            )),
        }
    }

    fn compile_hir_deref(
        &mut self,
        operand: &HirExpr,
        _full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match &operand.kind {
            HirExprKind::Unary { op, operand: inner } if op == "&" || op == "&mut" => {
                self.compile_hir_expr_typed(inner)
            }
            HirExprKind::Variable(name) => {
                let (alloca, var_type) = *self.variables.get(name).ok_or_else(|| {
                    self.error(
                        "compile_expression",
                        format!("cannot dereference undefined variable '{}'", name),
                    )
                })?;
                self.used_variables.insert(name.clone());
                let ptr_val = self
                    .backend
                    .builder
                    .build_load(var_type, alloca, name)
                    .map_err(|e| {
                        self.error(
                            "compile_expression",
                            format!("failed to load reference '{}': {}", name, e),
                        )
                    })?;
                let BasicValueEnum::PointerValue(ptr) = ptr_val else {
                    return Err(self.error(
                        "compile_expression",
                        format!("'{}' is not a pointer", name),
                    ));
                };
                let pointee = *self.ref_pointee_types.get(name).ok_or_else(|| {
                    self.error(
                        "compile_expression",
                        format!("missing pointee type for '{}'", name),
                    )
                })?;
                self.backend
                    .builder
                    .build_load(pointee, ptr, "deref")
                    .map_err(|e| {
                        self.error(
                            "compile_expression",
                            format!("failed to dereference '{}': {}", name, e),
                        )
                    })
            }
            _ => {
                let loaded = self.compile_hir_expr_typed(operand)?;
                let BasicValueEnum::PointerValue(ptr) = loaded else {
                    return Err(self.error(
                        "compile_expression",
                        "can only dereference a pointer value",
                    ));
                };
                let pointee_str = match &operand.ty {
                    Type::Ref { elem, .. } => elem.to_string(),
                    other => other.to_string(),
                };
                let pointee = self.coffee_type_to_llvm(&pointee_str).map_err(|e| {
                    self.error(
                        "compile_expression",
                        format!("deref pointee type '{}': {}", pointee_str, e),
                    )
                })?;
                self.backend
                    .builder
                    .build_load(pointee, ptr, "deref")
                    .map_err(|e| {
                        self.error(
                            "compile_expression",
                            format!("failed to dereference: {}", e),
                        )
                    })
            }
        }
    }

    fn compile_hir_assign_typed(
        &mut self,
        object: &HirExpr,
        field_name: &str,
        value: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let val = self.compile_hir_expr_typed(value)?;
        let name = self.hir_place_object_name(object)?;
        self.emit_field_store(&name, field_name, val)
    }

    fn hir_index_array_name(&mut self, array: &HirExpr) -> Result<String, String> {
        if let Some(name) = hir_variable_name(array) {
            return Ok(name.to_string());
        }
        self.bind_hir_array_temp(array)
    }

    fn hir_place_object_name(&mut self, object: &HirExpr) -> Result<String, String> {
        if let Some(name) = hir_variable_name(object) {
            return Ok(name.to_string());
        }
        self.bind_hir_place_temp(object)
    }

    fn bind_hir_array_temp(&mut self, array: &HirExpr) -> Result<String, String> {
        let compiled = self.compile_hir_expr_typed(array)?;
        if let Type::Slice(elem) = &array.ty {
            let elem_llvm = self
                .coffee_type_to_llvm(&elem.to_string())
                .map_err(|e| {
                    self.error(
                        "compile_hir_expr",
                        format!("array element type '{}': {}", elem, e),
                    )
                })?;
            let name = format!("__hir_arr_{}", self.array_allocas.len());
            match (&array.kind, compiled) {
                (HirExprKind::ArrayLiteral { elements }, BasicValueEnum::PointerValue(p)) => {
                    let n = self.backend.context.i64_type().const_int(elements.len() as u64, false);
                    self.pack_slice_from_buffer(&name, p, n, elem_llvm)?;
                }
                (_, fat) => {
                    self.bind_slice_fat(&name, fat, elem_llvm, false)?;
                }
            }
            self.memory_ctx
                .set_variable_type(name.clone(), array.ty.to_string());
            return Ok(name);
        }
        let (size, elem_ty) = hir_array_meta(&array.ty).ok_or_else(|| {
            self.error(
                "compile_hir_expr",
                format!(
                    "cannot index expression of type '{}' (need a fixed-size array)",
                    array.ty
                ),
            )
        })?;
        let elem_llvm = self
            .coffee_type_to_llvm(&elem_ty.to_string())
            .map_err(|e| {
                self.error(
                    "compile_hir_expr",
                    format!("array element type '{}': {}", elem_ty, e),
                )
            })?;
        let name = format!("__hir_arr_{}", self.array_allocas.len());
        let ptr = match compiled {
            BasicValueEnum::PointerValue(p) => p,
            other => {
                let array_llvm = self
                    .coffee_type_to_llvm(&array.ty.to_string())
                    .map_err(|e| {
                        self.error(
                            "compile_hir_expr",
                            format!("array type '{}': {}", array.ty, e),
                        )
                    })?;
                let alloca = self
                    .create_entry_alloca_preserving_terminator(array_llvm, &name)
                    .map_err(|e| {
                        self.error(
                            "compile_hir_expr",
                            format!("failed to allocate temp array '{}': {}", name, e),
                        )
                    })?;
                self.backend.builder.build_store(alloca, other).map_err(|e| {
                    self.error(
                        "compile_hir_expr",
                        format!("failed to store temp array '{}': {}", name, e),
                    )
                })?;
                alloca
            }
        };
        self.bind_array_ptr(&name, ptr, size, elem_llvm)?;
        Ok(name)
    }

    fn bind_hir_place_temp(&mut self, object: &HirExpr) -> Result<String, String> {
        let compiled = self.compile_hir_expr_typed(object)?;
        let llvm_type = compiled.get_type();
        let name = format!("__hir_obj_{}", self.variables.len());
        let alloca = self
            .create_entry_alloca_preserving_terminator(llvm_type, &name)
            .map_err(|e| {
                self.error(
                    "compile_hir_expr",
                    format!("failed to allocate temp object '{}': {}", name, e),
                )
            })?;
        self.backend.builder.build_store(alloca, compiled).map_err(|e| {
            self.error(
                "compile_hir_expr",
                format!("failed to store temp object '{}': {}", name, e),
            )
        })?;
        self.variables.insert(name.clone(), (alloca, llvm_type));
        if let Some(class) = hir_named_type(&object.ty) {
            self.variable_types.insert(name.clone(), class.to_string());
        }
        Ok(name)
    }

    /// Empty-arg member: field GEP, or zero-arg method if GEP fails.
    fn compile_hir_member_field_typed(
        &mut self,
        object: &HirExpr,
        field: &str,
        _full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if let Some(name) = hir_variable_name(object) {
            if self.enums.contains_key(name) {
                return self.compile_enum_simple_variant(name, field);
            }
            if self.variables.contains_key(name) {
                match self.compile_field_access_ptr(name, field) {
                    Ok(field_ptr) => return self.load_hir_field_ptr(name, field, field_ptr),
                    Err(_) => {
                        let object_value = self.compile_variable_ref(name)?;
                        let class_name = self
                            .variable_types
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| name.to_string());
                        return self.emit_method_call(name, &class_name, field, object_value, vec![]);
                    }
                }
            }
        }
        let object_value = self.compile_hir_expr_typed(object)?;
        let object_str = hir_variable_name(object).unwrap_or(field);
        self.emit_field_load(
            object_str,
            field,
            object_value,
            hir_named_type(&object.ty),
        )
    }

    fn load_hir_field_ptr(
        &mut self,
        object_str: &str,
        field_name: &str,
        field_ptr: inkwell::values::PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let class_name = self.variable_types.get(object_str).cloned().ok_or_else(|| {
            self.error(
                "compile_hir_expr",
                format!("cannot get class name for variable '{}'", object_str),
            )
        })?;
        let struct_type = *self.type_mapper.struct_types.get(&class_name).ok_or_else(|| {
            self.error(
                "compile_hir_expr",
                format!("class '{}' not found in struct_types cache", class_name),
            )
        })?;
        let field_index = self.get_field_index(&struct_type, field_name)?;
        let field_ty = struct_type
            .get_field_type_at_index(field_index as u32)
            .ok_or_else(|| {
                self.error(
                    "compile_hir_expr",
                    format!("missing LLVM type for field '{}'", field_name),
                )
            })?;
        let loaded = self
            .backend
            .builder
            .build_load(field_ty, field_ptr, &format!("{}_{}", object_str, field_name))
            .map_err(|e| {
                self.error(
                    "compile_hir_expr",
                    format!("failed to load field '{}': {}", field_name, e),
                )
            })?;
        if let Some(info) = self.packed_bitfield_info(object_str, field_name) {
            if info.bit_width > 0 {
                return self.extract_bitfield_value(loaded, info, field_name);
            }
        }
        Ok(loaded)
    }

    fn compile_hir_member_method_typed(
        &mut self,
        object: &HirExpr,
        field: &str,
        args: &[HirExpr],
        _full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if let Some(name) = hir_variable_name(object) {
            if self.enums.contains_key(name) {
                    let compiled_args = self.compile_hir_call_args(args)?;
                    return self.emit_enum_variant_call(name, field, compiled_args);
            }
        }
        let object_value = self.compile_hir_expr_typed(object)?;
        let object_str = hir_variable_name(object).unwrap_or(field);
        let class_name = hir_variable_name(object)
            .and_then(|n| self.variable_types.get(n).cloned())
            .or_else(|| hir_named_type(&object.ty).map(str::to_string))
            .unwrap_or_else(|| object_str.to_string());
        let compiled_args = self.compile_hir_call_args(args)?;
        self.emit_method_call(object_str, &class_name, field, object_value, compiled_args)
    }

    fn compile_hir_constructor_typed(
        &mut self,
        class_name: &str,
        args: &[HirExpr],
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let compiled = self.compile_hir_call_args(args)?;
        self.emit_constructor_call(class_name, compiled)
    }

    fn compile_hir_struct_literal_typed(
        &mut self,
        struct_name: &str,
        fields: &[(String, HirExpr)],
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let mut compiled = Vec::with_capacity(fields.len());
        for (field_name, field_expr) in fields {
            let field_value = if hir_struct_field_is_wildcard(field_name, field_expr) {
                self.backend.context.i64_type().const_int(0, false).into()
            } else {
                self.compile_hir_expr_typed(field_expr)?
            };
            compiled.push((field_name.clone(), field_value));
        }
        self.emit_struct_literal(struct_name, compiled)
    }

    /// Named and `obj.method` calls stay typed. Computed callees (`(f)(x)`,
    /// `arr[i](x)`) try FunctionValue / fn-pointer `build_call` first.
    fn compile_hir_call_typed(
        &mut self,
        function: &HirExpr,
        args: &[HirExpr],
        full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match &function.kind {
            HirExprKind::Member {
                object,
                field,
                args: member_args,
            } if member_args.is_empty() => {
                return self.compile_hir_member_method_typed(object, field, args, full);
            }
            HirExprKind::Variable(func_name) if self.variables.contains_key(func_name) => {
                self.compile_hir_computed_call_typed(function, args)
            }
            HirExprKind::Variable(func_name) => {
                self.compile_hir_named_call_typed(func_name, args, full)
            }
            _ => self.compile_hir_computed_call_typed(function, args),
        }
    }

    /// `(f)(x)`, `arr[i](x)`, and other callees that are not a bare name.
    fn compile_hir_computed_call_typed(
        &mut self,
        function: &HirExpr,
        args: &[HirExpr],
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let callee = self.compile_hir_expr_typed(function)?;
        if let AnyValueEnum::FunctionValue(fv) = callee.as_any_value_enum() {
            let compiled_args = self.compile_hir_call_args(args)?;
            return self.finish_direct_call(fv, compiled_args, "call");
        }
        if let BasicValueEnum::PointerValue(ptr) = callee {
            if let Some(fv) = self.function_value_for_ptr(ptr) {
                let compiled_args = self.compile_hir_call_args(args)?;
                return self.finish_direct_call(fv, compiled_args, "call");
            }
            if let Some((params, ret)) = hir_fn_sig(&function.ty) {
                let (fn_ty, param_llvm) = self.llvm_fn_type_from_coffee(params, ret)?;
                let compiled_args = self.compile_hir_call_args(args)?;
                return self.finish_indirect_call(fn_ty, ptr, param_llvm, compiled_args);
            }
        }
        Err(self.error(
            "function_call",
            format!(
                "type '{}' is not callable\n  = note: only functions and function pointers can be called",
                function.ty
            ),
        ))
    }

    fn compile_hir_call_args(
        &mut self,
        args: &[HirExpr],
    ) -> Result<Vec<BasicValueEnum<'ctx>>, String> {
        let compiled = args
            .iter()
            .map(|a| self.compile_hir_expr_typed(a))
            .collect::<Result<Vec<_>, _>>()?;
        self.mark_moved_owned_resource_args(args);
        Ok(compiled)
    }

    fn mark_moved_owned_resource_args(&mut self, args: &[HirExpr]) {
        for arg in args {
            if self.ty_is_owned_resource(&arg.ty) {
                if let HirExprKind::Variable(n) = &arg.kind {
                    let left = self.mir_name_uses_left.get(n).copied().unwrap_or(0);
                    if left == 0 {
                        self.memory_ctx.mark_moved(n.clone());
                    }
                }
            }
        }
    }

    fn function_value_for_ptr(&self, ptr: PointerValue<'ctx>) -> Option<FunctionValue<'ctx>> {
        self.functions
            .values()
            .copied()
            .find(|f| f.as_global_value().as_pointer_value() == ptr)
    }

    fn llvm_fn_type_from_coffee(
        &self,
        params: &[Type],
        ret: &Type,
    ) -> Result<(FunctionType<'ctx>, Vec<inkwell::types::BasicTypeEnum<'ctx>>), String> {
        let variadic = matches!(params.last(), Some(Type::Variadic));
        let mut param_llvm = Vec::new();
        for p in params {
            if matches!(p, Type::Variadic) {
                continue;
            }
            param_llvm.push(self.coffee_type_to_llvm(&p.to_string()).map_err(|e| {
                self.error(
                    "function_call",
                    format!("cannot map function-pointer parameter type '{}': {}", p, e),
                )
            })?);
        }
        let param_meta: Vec<BasicMetadataTypeEnum<'ctx>> =
            param_llvm.iter().copied().map(Into::into).collect();
        let fn_ty = match ret {
            Type::Void | Type::Unit => self
                .backend
                .context
                .void_type()
                .fn_type(&param_meta, variadic),
            _ => {
                let ret_llvm = self.coffee_type_to_llvm(&ret.to_string()).map_err(|e| {
                    self.error(
                        "function_call",
                        format!("cannot map function-pointer return type '{}': {}", ret, e),
                    )
                })?;
                ret_llvm.fn_type(&param_meta, variadic)
            }
        };
        Ok((fn_ty, param_llvm))
    }

    fn compile_hir_named_call_typed(
        &mut self,
        func_name: &str,
        args: &[HirExpr],
        _full: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if let Some((scope, variant)) = func_name.split_once("::") {
            if self.enums.contains_key(scope) {
                let compiled_args = self.compile_hir_call_args(args)?;
                return self.emit_enum_variant_call(scope, variant, compiled_args);
            }
        }
        if !func_name.contains("::") {
            if let Some((object_str, method_name)) = func_name.rsplit_once('.') {
                if self.variables.contains_key(object_str) {
                    let object_value = self.compile_variable_ref(object_str)?;
                    let class_name = self
                        .variable_types
                        .get(object_str)
                        .cloned()
                        .unwrap_or_else(|| object_str.to_string());
                    let compiled_args = self.compile_hir_call_args(args)?;
                    return self.emit_method_call(
                        object_str,
                        &class_name,
                        method_name,
                        object_value,
                        compiled_args,
                    );
                }
            }
        }
        let compiled_args = self.compile_hir_call_args(args)?;
        self.emit_named_function_call(func_name, compiled_args)
    }

    fn emit_named_function_call(
        &mut self,
        func_name: &str,
        compiled_args: Vec<BasicValueEnum<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if self.is_c_library_function(func_name) {
            let function = self
                .backend
                .module
                .get_function(func_name)
                .filter(|f| f.get_name().to_str().ok() == Some(func_name))
                .map(Ok)
                .unwrap_or_else(|| self.declare_external_function(func_name))?;
            let function = self
                .backend
                .module
                .get_function(func_name)
                .filter(|f| f.get_name().to_str().ok() == Some(func_name))
                .unwrap_or(function);
            return self.finish_direct_call(function, compiled_args, func_name);
        }
        let mangled = func_name.replace("::", "_");
        let function = match self.functions.get(func_name).copied().or_else(|| {
            if mangled != func_name {
                self.functions.get(&mangled).copied()
            } else {
                None
            }
        }) {
            Some(f) => f,
            None => {
                let qualified_name = if let Some(current_fn) = self.current_function {
                    let current_name = current_fn.get_name().to_str().unwrap_or("");
                    if current_name.contains('.') {
                        let parts: Vec<&str> = current_name.rsplitn(2, '.').collect();
                        if parts.len() == 2 {
                            let module_path = parts[1];
                            let qualified = format!("{}.{}", module_path, func_name);
                            if self.functions.contains_key(&qualified) {
                                Some(qualified)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(qual_name) = qualified_name {
                    *self.functions.get(&qual_name).unwrap()
                } else if func_name.contains('.') && !func_name.contains("::") {
                    self.declare_external_function(func_name)?
                } else if self.is_c_library_function(func_name) {
                    self.declare_external_function(func_name)?
                } else {
                    let mut help = String::new();
                    let candidates: Vec<String> = self
                        .functions
                        .keys()
                        .filter(|k| !crate::backend::functions::omit_from_unknown_fn_help(k))
                        .cloned()
                        .collect();
                    let similar =
                        crate::diagnostics::find_similar_names(func_name, &candidates, 2, 3);
                    if !similar.is_empty() {
                        help.push_str(&format!("\n  = help: did you mean {}?", similar.join(" or ")));
                    }
                    return Err(self.error(
                        "function_call",
                        format!(
                            "cannot find function '{}' in this scope{}",
                            func_name, help
                        ),
                    ));
                }
            }
        };

        self.finish_direct_call(function, compiled_args, func_name)
    }

    fn finish_direct_call(
        &mut self,
        function: FunctionValue<'ctx>,
        compiled_args: Vec<BasicValueEnum<'ctx>>,
        func_name: &str,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let param_types: Vec<_> = (0..function.count_params())
            .map(|i| {
                function.get_nth_param(i).map(|p| p.get_type()).ok_or_else(|| {
                    self.error(
                        "function_call",
                        format!(
                            "failed to get type for parameter {} of function '{}'",
                            i, func_name
                        ),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let converted = self.convert_compiled_call_args(&compiled_args, &param_types, func_name)?;
        let args_ref: Vec<_> = converted.iter().map(|a| (*a).into()).collect();
        let call = self
            .backend
            .builder
            .build_call(function, &args_ref, "call")
            .map_err(|e| {
                self.error(
                    "function_call",
                    format!("failed to build call to '{}': {}", func_name, e),
                )
            })?;
        self.call_result_or_zero(call)
    }

    fn finish_indirect_call(
        &mut self,
        fn_ty: FunctionType<'ctx>,
        ptr: PointerValue<'ctx>,
        param_llvm: Vec<inkwell::types::BasicTypeEnum<'ctx>>,
        compiled_args: Vec<BasicValueEnum<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let converted = self.convert_compiled_call_args(&compiled_args, &param_llvm, "call")?;
        let args_ref: Vec<_> = converted.iter().map(|a| (*a).into()).collect();
        let call = self
            .backend
            .builder
            .build_indirect_call(fn_ty, ptr, &args_ref, "call")
            .map_err(|e| {
                self.error(
                    "function_call",
                    format!("failed to build indirect call: {}", e),
                )
            })?;
        self.call_result_or_zero(call)
    }

    fn convert_compiled_call_args(
        &self,
        compiled_args: &[BasicValueEnum<'ctx>],
        param_types: &[inkwell::types::BasicTypeEnum<'ctx>],
        func_name: &str,
    ) -> Result<Vec<BasicValueEnum<'ctx>>, String> {
        let fixed_param_count = param_types.len();
        if compiled_args.len() < fixed_param_count {
            return Err(self.error(
                "function_call",
                format!(
                    "wrong number of arguments for function '{}'\n  = note: expected at least {} arguments, got {} arguments",
                    func_name, fixed_param_count, compiled_args.len()
                ),
            ));
        }
        let mut converted_args = Vec::new();
        for (i, arg) in compiled_args.iter().enumerate() {
            if i >= fixed_param_count {
                converted_args.push(*arg);
                continue;
            }
            let param_type = param_types[i];
            let arg_type = arg.get_type();
            if !self.are_types_compatible(arg_type, param_type) {
                let arg_type_str = self.type_to_string(arg_type);
                let param_type_str = self.type_to_string(param_type);
                return Err(self.error(
                    "function_call",
                    format!(
                        "type mismatch in argument {} of function '{}'\n  = note: expected type '{}', found type '{}'",
                        i + 1,
                        func_name,
                        param_type_str,
                        arg_type_str
                    ),
                ));
            }
            converted_args.push(self.convert_value_to_type(
                *arg,
                param_type,
                &format!("arg_{}", i),
            )?);
        }
        Ok(converted_args)
    }

    fn call_result_or_zero(
        &self,
        call: inkwell::values::CallSiteValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => Ok(val),
            inkwell::values::ValueKind::Instruction(_) => {
                Ok(self.backend.context.i64_type().const_zero().into())
            }
        }
    }

    fn compile_hir_literal_typed(
        &mut self,
        ty: &Type,
        lit: &str,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let lit = lit.trim();
        match ty {
            Type::Bool => {
                let bit = match lit {
                    "true" => 1,
                    "false" => 0,
                    _ => {
                        return Err(self.error(
                            "compile_hir_expr",
                            format!("invalid bool literal '{}'", lit),
                        ))
                    }
                };
                Ok(self.backend.context.i8_type().const_int(bit, false).into())
            }
            Type::Int { bits, signed } => {
                let int_ty = self.llvm_int_type_from_bits(*bits);
                let n = parse_int_bits(lit, *bits, *signed).ok_or_else(|| {
                    self.error(
                        "compile_hir_expr",
                        format!("invalid int literal '{}' for {}", lit, ty),
                    )
                })?;
                Ok(int_ty.const_int(n, *signed).into())
            }
            Type::Float { bits } => {
                let f = lit.parse::<f64>().map_err(|_| {
                    self.error(
                        "compile_hir_expr",
                        format!("invalid float literal '{}' for {}", lit, ty),
                    )
                })?;
                let fv = match bits {
                    32 => self.backend.context.f32_type().const_float(f),
                    _ => self.backend.context.f64_type().const_float(f),
                };
                Ok(fv.into())
            }
            Type::String => {
                let content = strip_string_quotes(lit);
                self.build_string_constant(content)
            }
            _ => self.compile_literal_atom(lit),
        }
    }

    fn llvm_int_type_from_bits(&self, bits: u8) -> IntType<'ctx> {
        match bits {
            8 => self.backend.context.i8_type(),
            16 => self.backend.context.i16_type(),
            32 => self.backend.context.i32_type(),
            64 => self.backend.context.i64_type(),
            128 => self.backend.context.i128_type(),
            n => self.backend.context.custom_width_int_type(n as u32),
        }
    }
}

fn strip_string_quotes(lit: &str) -> &str {
    if lit.len() >= 2 && lit.starts_with('"') && lit.ends_with('"') {
        &lit[1..lit.len() - 1]
    } else {
        lit
    }
}

fn parse_int_bits(lit: &str, bits: u8, signed: bool) -> Option<u64> {
    if signed {
        let v = lit.parse::<i128>().ok()?;
        return Some(v as u64);
    }
    let v = if let Some(rest) = lit.strip_prefix('-') {
        0u128.wrapping_sub(rest.parse::<u128>().ok()?)
    } else {
        lit.parse::<u128>().ok()?
    };
    let mask = if bits >= 64 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    };
    Some((v & mask) as u64)
}

fn hir_variable_name(expr: &HirExpr) -> Option<&str> {
    match &expr.kind {
        HirExprKind::Variable(n) => Some(n.as_str()),
        _ => None,
    }
}

fn hir_array_meta(ty: &Type) -> Option<(u32, Type)> {
    match ty {
        Type::Array { elem, size } => Some((*size as u32, (**elem).clone())),
        Type::Ref { elem, .. } => hir_array_meta(elem),
        _ => None,
    }
}

fn hir_named_type(ty: &Type) -> Option<&str> {
    match ty {
        Type::NamedType { name } => Some(name.as_str()),
        Type::Ref { elem, .. } => hir_named_type(elem),
        _ => None,
    }
}

fn hir_fn_sig(ty: &Type) -> Option<(&[Type], &Type)> {
    match ty {
        Type::Function { params, return_type } => Some((params.as_slice(), return_type)),
        Type::Ref { elem, .. } => hir_fn_sig(elem),
        _ => None,
    }
}

fn hir_struct_field_is_wildcard(field_name: &str, field_expr: &HirExpr) -> bool {
    field_name == "_"
        || matches!(&field_expr.kind, HirExprKind::Variable(v) if v == "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    fn dummy_ty() -> Type {
        Type::Int {
            bits: 64,
            signed: true,
        }
    }

    fn hir(kind: HirExprKind) -> HirExpr {
        HirExpr { ty: dummy_ty(), kind }
    }

    #[test]
    fn nested_index_base_is_not_a_variable_name() {
        let array = hir(HirExprKind::Variable("xs".into()));
        assert_eq!(hir_variable_name(&array), Some("xs"));
        let call_array = hir(HirExprKind::Call {
            function: Box::new(hir(HirExprKind::Variable("foo".into()))),
            args: vec![],
        });
        assert!(hir_variable_name(&call_array).is_none());
        let nested_array = hir(HirExprKind::Index {
            array: Box::new(hir(HirExprKind::Variable("xs".into()))),
            index: Box::new(hir(HirExprKind::Literal("0".into()))),
        });
        assert!(hir_variable_name(&nested_array).is_none());
        let arr_ty = Type::Array {
            elem: Box::new(dummy_ty()),
            size: 3,
        };
        assert_eq!(hir_array_meta(&arr_ty).map(|(n, _)| n), Some(3));
        assert_eq!(
            hir_array_meta(&Type::Ref {
                elem: Box::new(arr_ty),
                mutable: false,
            })
            .map(|(n, _)| n),
            Some(3)
        );
        assert!(hir_array_meta(&dummy_ty()).is_none());
    }

    #[test]
    fn member_typed_path_accepts_non_variable_object() {
        let obj = hir(HirExprKind::Variable("p".into()));
        assert_eq!(hir_variable_name(&obj), Some("p"));
        let call_obj = hir(HirExprKind::Call {
            function: Box::new(hir(HirExprKind::Variable("make".into()))),
            args: vec![],
        });
        assert!(hir_variable_name(&call_obj).is_none());
        assert_eq!(
            hir_named_type(&Type::NamedType {
                name: "Point".into()
            }),
            Some("Point")
        );
    }

    #[test]
    fn call_name_splits_enum_and_method() {
        assert_eq!("Option::Some".split_once("::"), Some(("Option", "Some")));
        assert_eq!("obj.method".rsplit_once('.'), Some(("obj", "method")));
        assert_eq!("printf".split_once("::"), None);
    }

    #[test]
    fn struct_wildcard_field_skips_typed_compile() {
        let named_hole = hir(HirExprKind::Literal("0".into()));
        assert!(hir_struct_field_is_wildcard("_", &named_hole));
        let var_hole = hir(HirExprKind::Variable("_".into()));
        assert!(hir_struct_field_is_wildcard("x", &var_hole));
        let real = hir(HirExprKind::Literal("1".into()));
        assert!(!hir_struct_field_is_wildcard("x", &real));
    }

    #[test]
    fn addr_of_nested_field_or_index_has_no_variable_base() {
        let v = hir(HirExprKind::Variable("x".into()));
        assert_eq!(hir_variable_name(&v), Some("x"));
        let nested_field = hir(HirExprKind::Member {
            object: Box::new(hir(HirExprKind::Member {
                object: Box::new(hir(HirExprKind::Variable("p".into()))),
                field: "inner".into(),
                args: vec![],
            })),
            field: "x".into(),
            args: vec![],
        });
        match &nested_field.kind {
            HirExprKind::Member { object, args, .. } => {
                assert!(hir_variable_name(object).is_none());
                assert!(args.is_empty());
            }
            _ => panic!("expected member"),
        }
        let idx = hir(HirExprKind::Index {
            array: Box::new(hir(HirExprKind::Call {
                function: Box::new(hir(HirExprKind::Variable("foo".into()))),
                args: vec![],
            })),
            index: Box::new(hir(HirExprKind::Literal("0".into()))),
        });
        match &idx.kind {
            HirExprKind::Index { array, index } => {
                assert!(hir_variable_name(array).is_none());
                assert!(matches!(&index.kind, HirExprKind::Literal(_)));
            }
            _ => panic!("expected index"),
        }
    }

    #[test]
    fn method_typed_path_needs_variable_receiver() {
        let obj = hir(HirExprKind::Variable("p".into()));
        assert_eq!(hir_variable_name(&obj), Some("p"));
        let ctor = hir(HirExprKind::ConstructorCall {
            class_name: "Point".into(),
            args: vec![],
        });
        assert!(hir_variable_name(&ctor).is_none());
    }

    #[test]
    fn computed_callee_fn_type_is_callable() {
        let fn_ty = Type::Function {
            params: vec![dummy_ty()],
            return_type: Box::new(dummy_ty()),
        };
        assert!(hir_fn_sig(&fn_ty).is_some());
        assert!(hir_fn_sig(&Type::Ref {
            elem: Box::new(fn_ty.clone()),
            mutable: false,
        })
        .is_some());
        assert!(hir_fn_sig(&dummy_ty()).is_none());
        let idx = hir(HirExprKind::Index {
            array: Box::new(hir(HirExprKind::Variable("arr".into()))),
            index: Box::new(hir(HirExprKind::Literal("0".into()))),
        });
        assert!(hir_variable_name(&idx).is_none());
        // Opaque SSA callees have no Function type; codegen must error, not
        // `hir_expr_to_ast` + `compile_expr`.
        assert!(hir_fn_sig(&idx.ty).is_none());
        assert!(hir_fn_sig(&Type::Bool).is_none());
        assert!(hir_fn_sig(&Type::String).is_none());
    }

    #[test]
    fn production_must_not_call_compile_expr() {
        let backend = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
        let mut offenders = Vec::new();
        walk_rs(&backend, &mut |path, text| {
            let rel = path.strip_prefix(&backend).unwrap();
            let mut in_test = false;
            for (i, line) in text.lines().enumerate() {
                let t = line.trim();
                if t.starts_with("#[cfg(test)]") {
                    in_test = true;
                }
                if in_test {
                    continue;
                }
                let code = code_without_comments_or_strings(line);
                if code.contains("self.compile_expr") {
                    offenders.push(format!("{}:{}: {}", rel.display(), i + 1, line.trim()));
                }
            }
        });
        assert!(
            offenders.is_empty(),
            "AST compile_expr is a dead trap; production must use compile_hir_expr_typed:\n{}",
            offenders.join("\n")
        );
    }

    #[test]
    fn mir_expr_does_not_rebuild_parser_expression() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend/mir_expr.rs"),
        )
        .unwrap();
        let mut offenders = Vec::new();
        let mut in_test = false;
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("#[cfg(test)]") {
                in_test = true;
            }
            if in_test {
                continue;
            }
            let code = code_without_comments_or_strings(line);
            if code.contains("Expression::") {
                offenders.push(format!("{}: {}", i + 1, line.trim()));
            }
        }
        assert!(
            offenders.is_empty(),
            "mir_expr must not wrap parser Expression:\n{}",
            offenders.join("\n")
        );
    }

    /// `hir_expr_to_ast(` is the definition in `hir/expr.rs` plus unit tests in
    /// `hir/mod.rs`. No backend / pipeline site may call it.
    #[test]
    fn production_src_does_not_call_hir_expr_to_ast() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        walk_rs(&src_root, &mut |path, text| {
            let rel = path.strip_prefix(&src_root).unwrap();
            if rel == std::path::Path::new("hir/expr.rs")
                || rel == std::path::Path::new("hir/mod.rs")
            {
                return;
            }
            for (i, line) in text.lines().enumerate() {
                if contains_ident_call(
                    &code_without_comments_or_strings(line),
                    "hir_expr_to_ast",
                ) {
                    offenders.push(format!("{}:{}: {}", rel.display(), i + 1, line.trim()));
                }
            }
        });
        assert!(
            offenders.is_empty(),
            "production MIR must not call hir_expr_to_ast:\n{}",
            offenders.join("\n")
        );
    }

    /// Helper must not exist in non-test builds (`#[cfg(test)]` on the fn and
    /// any `pub use`, or only defined inside `hir/mod.rs` tests).
    #[test]
    fn hir_expr_to_ast_is_cfg_test_only() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/hir");
        let expr = std::fs::read_to_string(root.join("expr.rs")).unwrap();
        let mod_rs = std::fs::read_to_string(root.join("mod.rs")).unwrap();
        if expr.contains("fn hir_expr_to_ast") {
            assert!(
                item_has_cfg_test(&expr, "fn hir_expr_to_ast"),
                "hir_expr_to_ast in hir/expr.rs must be #[cfg(test)]"
            );
        } else {
            assert!(
                mod_rs.contains("fn hir_expr_to_ast"),
                "hir_expr_to_ast must live in hir/expr.rs (cfg test) or hir/mod.rs tests"
            );
        }
        assert!(
            !unguarded_pub_use_includes(&mod_rs, "hir_expr_to_ast"),
            "production pub use must not export hir_expr_to_ast"
        );
    }

    /// Complete MIR must not reach AST `compile_computed_call`.
    #[test]
    fn hir_codegen_does_not_call_ast_computed_call() {
        let backend = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
        let mut offenders = Vec::new();
        walk_rs(&backend, &mut |path, text| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "call.rs" || !name.contains("mir") {
                return;
            }
            for (i, line) in text.lines().enumerate() {
                if contains_ident_call(
                    &code_without_comments_or_strings(line),
                    "compile_computed_call",
                ) {
                    offenders.push(format!("{}:{}: {}", name, i + 1, line.trim()));
                }
            }
        });
        assert!(
            offenders.is_empty(),
            "compile_hir_* must not call compile_computed_call:\n{}",
            offenders.join("\n")
        );
    }

    fn item_has_cfg_test(src: &str, item: &str) -> bool {
        let mut last_attr = "";
        for line in src.lines() {
            let t = line.trim();
            if t.starts_with("#[") {
                last_attr = t;
                continue;
            }
            if t.contains(item) {
                return last_attr == "#[cfg(test)]";
            }
            if !t.is_empty() && !t.starts_with("//") && !t.starts_with("///") {
                last_attr = "";
            }
        }
        false
    }

    fn unguarded_pub_use_includes(src: &str, name: &str) -> bool {
        let mut last_attr = "";
        for line in src.lines() {
            let t = line.trim();
            if t.starts_with("#[") {
                last_attr = t;
                continue;
            }
            if t.starts_with("pub use") && t.contains(name) && last_attr != "#[cfg(test)]" {
                return true;
            }
            if !t.is_empty() && !t.starts_with("//") && !t.starts_with("///") {
                last_attr = "";
            }
        }
        false
    }

    fn contains_ident_call(code: &str, name: &str) -> bool {
        let needle = format!("{}(", name);
        let mut rest = code;
        while let Some(idx) = rest.find(&needle) {
            let ok_before = idx == 0
                || rest[..idx]
                    .chars()
                    .last()
                    .is_some_and(|c| !c.is_ascii_alphanumeric() && c != '_');
            if ok_before {
                return true;
            }
            rest = &rest[idx + 1..];
        }
        false
    }

    fn code_without_comments_or_strings(line: &str) -> String {
        let t = line.trim_start();
        if t.starts_with("//") {
            return String::new();
        }
        let mut out = String::new();
        let mut chars = line.chars().peekable();
        let mut in_str = false;
        while let Some(c) = chars.next() {
            if in_str {
                if c == '\\' {
                    chars.next();
                } else if c == '"' {
                    in_str = false;
                }
                continue;
            }
            if c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            if c == '"' {
                in_str = true;
                continue;
            }
            out.push(c);
        }
        out
    }

    fn walk_rs(dir: &std::path::Path, visit: &mut impl FnMut(&std::path::Path, &str)) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs(&path, visit);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    visit(&path, &text);
                }
            }
        }
    }
}
