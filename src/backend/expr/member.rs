//! Member access, field pointers, assignment, and method calls.
//! Object values come from MIR (`compile_hir_expr_typed` / `compile_variable_ref`).

use crate::coffee_debug;
use crate::backend::codegen::CodeGenerator;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn emit_field_store(
        &mut self,
        object_str: &str,
        field_name: &str,
        val: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        if let Some(info) = self.packed_bitfield_info(object_str, field_name) {
            if info.bit_width > 0 {
                return self.store_packed_bitfield_value(object_str, field_name, val, info);
            }
        }
        let field_ptr = self.compile_field_access_ptr(object_str, field_name)?;
        self.backend.builder.build_store(field_ptr, val)
            .map_err(|e| self.error("compile_assign", format!("failed to store field '{}': {}", field_name, e)))?;
        Ok(val)
    }

    pub(crate) fn emit_field_load(
        &mut self,
        object_str: &str,
        field_name: &str,
        object_value: BasicValueEnum<'ctx>,
        class_name: Option<&str>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let object_type = object_value.get_type();
        let class_name = class_name
            .map(|s| s.to_string())
            .or_else(|| self.variable_types.get(object_str).cloned());
        let bitfield_key = class_name.clone().unwrap_or_else(|| object_str.to_string());

        if let Some(ref class_name) = class_name {
            if self.type_mapper.c_union_byte_sizes.contains_key(class_name) {
                let ptr = self.c_union_base_ptr(object_str, object_value)?;
                let field_ty = self.c_union_field_llvm(class_name, field_name)?;
                return self
                    .backend
                    .builder
                    .build_load(field_ty, ptr, &format!("{}_{}", object_str, field_name))
                    .map_err(|e| {
                        self.error(
                            "compile_field_access",
                            format!("failed to load union field '{}': {}", field_name, e),
                        )
                    });
            }
        }

        match object_type {
            BasicTypeEnum::PointerType(_) => {
                if let Some(ref class_name) = class_name {
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
                        if let Some(info) = self.packed_bitfield_info(&bitfield_key, field_name) {
                            if info.bit_width > 0 {
                                return self.extract_bitfield_value(field_value, info, field_name);
                            }
                        }
                        return Ok(field_value);
                    }
                }

                let i64_type = self.backend.context.i64_type();
                self.backend.builder.build_load(
                    i64_type,
                    object_value.into_pointer_value(),
                    &format!("{}_loaded", object_str)
                ).map_err(|e| self.error("compile_field_access",
                    format!("failed to load object: {}", e)))
            }
            BasicTypeEnum::StructType(struct_type) => {
                let field_index = self.get_field_index(&struct_type, field_name)?;
                let field_value = self.backend.builder.build_extract_value(
                    object_value.into_struct_value(),
                    field_index as u32,
                    &format!("{}_{}", object_str, field_name)
                ).map_err(|e| self.error("compile_field_access",
                    format!("failed to extract field '{}': {}", field_name, e)))?;
                if let Some(info) = self.packed_bitfield_info(&bitfield_key, field_name) {
                    if info.bit_width > 0 {
                        return self.extract_bitfield_value(field_value, info, field_name);
                    }
                }
                Ok(field_value)
            }
            _ => Err(self.error("compile_field_access",
                format!("field access on non-struct type: {}", self.type_to_string(object_type))))
        }
    }

    fn c_union_base_ptr(
        &self,
        object_str: &str,
        object_value: BasicValueEnum<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        if object_value.is_pointer_value() {
            return Ok(object_value.into_pointer_value());
        }
        if let Some((ptr, _)) = self.variables.get(object_str) {
            return Ok(*ptr);
        }
        Err(self.error(
            "compile_field_access",
            format!("union '{}' has no storage pointer", object_str),
        ))
    }

    fn c_union_field_llvm(
        &self,
        class_name: &str,
        field_name: &str,
    ) -> Result<BasicTypeEnum<'ctx>, String> {
        let field_ty = self
            .classes
            .get(class_name)
            .and_then(|c| c.fields.iter().find(|f| f.name == field_name))
            .map(|f| f.field_type.as_str())
            .ok_or_else(|| {
                self.error(
                    "compile_field_access",
                    format!("union '{}' has no field '{}'", class_name, field_name),
                )
            })?;
        self.coffee_type_to_llvm(field_ty)
    }

    pub fn compile_field_access_ptr(&mut self, object_str: &str, field_name: &str) -> Result<PointerValue<'ctx>, String> {
        if let Some(class_name) = self.variable_types.get(object_str).cloned() {
            if self.type_mapper.c_union_byte_sizes.contains_key(&class_name) {
                let object_value = self.compile_variable_ref(object_str)?;
                let _ = field_name;
                return self.c_union_base_ptr(object_str, object_value);
            }
        }
        let object_value = self.compile_variable_ref(object_str)?;
        let object_type = object_value.get_type();

        match object_type {
            BasicTypeEnum::PointerType(_) => {
                let object_ptr = object_value.into_pointer_value();

                let var_type = if let Some(&(_, var_type)) = self.variables.get(object_str) {
                    var_type
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("variable '{}' not found", object_str)));
                };

                let field_index = match var_type {
                    BasicTypeEnum::StructType(s) => self.get_field_index(&s, field_name)?,
                    BasicTypeEnum::PointerType(_) => {
                        if let Some(class_name) = self.variable_types.get(object_str) {
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
            BasicTypeEnum::StructType(_) => {
                Err(self.error("compile_field_access_ptr",
                    format!("field access on struct value not supported for assignment (use pointer)")))
            }
            _ => Err(self.error("compile_field_access_ptr",
                format!("field access on non-struct type: {}", self.type_to_string(object_type))))
        }
    }

    pub(crate) fn emit_method_call(
        &mut self,
        object_str: &str,
        class_name: &str,
        method_name: &str,
        object_value: BasicValueEnum<'ctx>,
        compiled_args: Vec<BasicValueEnum<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        coffee_debug!("DEBUG: compile_method_call: object_str='{}', method_name='{}', class_name='{}'",
            object_str, method_name, class_name);
        coffee_debug!("DEBUG: compile_method_call: functions keys: {:?}", self.functions.keys().collect::<Vec<_>>());

        let mut function = None;
        let mut current = Some(class_name.to_string());
        while let Some(name) = current {
            let full_method_name = format!("{}_{}", name, method_name);
            coffee_debug!("DEBUG: compile_method_call: try full_method_name='{}'", full_method_name);
            if let Some(f) = self.functions.get(&full_method_name).copied() {
                function = Some(f);
                break;
            }
            current = self.classes.get(&name).and_then(|c| c.parent.clone());
        }
        let function = function
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

    /// GEP index: parent fields first (declaration order), then this class's fields.
    /// With packed bitfields, index is the storage-unit member, not the source field index.
    pub(crate) fn get_field_index(&self, struct_type: &inkwell::types::StructType<'ctx>, field_name: &str) -> Result<usize, String> {
        let llvm_name = struct_type.get_name().map(|n| n.to_string_lossy().into_owned());

        if let Some(ref class_name) = llvm_name {
            if let Some(info) = self.packed_field_info.get(class_name).and_then(|m| m.get(field_name)) {
                return Ok(info.gep_index);
            }
            if let Some(idx) = self.type_mapper.get_field_index(class_name, field_name) {
                return Ok(idx);
            }
            if let Some(class_def) = self.classes.get(class_name) {
                if let Some(idx) = field_index_declaration_order(&self.classes, class_def, field_name) {
                    return Ok(idx);
                }
            }
        }

        Err(format!("field '{}' not found in struct", field_name))
    }

    pub(crate) fn packed_bitfield_info(&self, object_str: &str, field_name: &str) -> Option<crate::backend::class_layout::PackedFieldInfo> {
        let class_name = self.variable_types.get(object_str).cloned().unwrap_or_else(|| object_str.to_string());
        self.packed_field_info
            .get(&class_name)
            .and_then(|m| m.get(field_name))
            .copied()
    }

    pub(crate) fn extract_bitfield_value(
        &self,
        storage_value: BasicValueEnum<'ctx>,
        info: crate::backend::class_layout::PackedFieldInfo,
        field_name: &str,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let int_val = storage_value.into_int_value();
        let storage_ty = int_val.get_type();
        let shift = storage_ty.const_int(info.bit_offset as u64, false);
        let shifted = self.backend.builder.build_right_shift(int_val, shift, false, &format!("{}_lshr", field_name))
            .map_err(|e| self.error("compile_field_access", format!("bitfield shift: {}", e)))?;
        let mask = storage_ty.const_int((1u64 << info.bit_width) - 1, false);
        let masked = self.backend.builder.build_and(shifted, mask, &format!("{}_mask", field_name))
            .map_err(|e| self.error("compile_field_access", format!("bitfield mask: {}", e)))?;
        let dest_ty = self.backend.context.i64_type();
        if storage_ty.get_bit_width() < dest_ty.get_bit_width() {
            let ext = self.backend.builder.build_int_z_extend(masked, dest_ty, &format!("{}_zext", field_name))
                .map_err(|e| self.error("compile_field_access", format!("bitfield zext: {}", e)))?;
            Ok(ext.into())
        } else {
            Ok(masked.into())
        }
    }

    fn store_packed_bitfield_value(
        &mut self,
        object_str: &str,
        field_name: &str,
        val: BasicValueEnum<'ctx>,
        info: crate::backend::class_layout::PackedFieldInfo,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let field_ptr = self.compile_field_access_ptr(object_str, field_name)?;
        let storage_ty = self.bitfield_storage_int_type(info.storage_bits);
        let loaded = self.backend.builder.build_load(storage_ty, field_ptr, &format!("{}_storage", field_name))
            .map_err(|e| self.error("compile_assign", format!("bitfield load: {}", e)))?
            .into_int_value();
        let val_int = val.into_int_value();
        let val_trunc = if val_int.get_type().get_bit_width() > storage_ty.get_bit_width() {
            self.backend.builder.build_int_truncate(val_int, storage_ty, &format!("{}_trunc", field_name))
                .map_err(|e| self.error("compile_assign", format!("bitfield trunc: {}", e)))?
        } else if val_int.get_type().get_bit_width() < storage_ty.get_bit_width() {
            self.backend.builder.build_int_z_extend(val_int, storage_ty, &format!("{}_zext", field_name))
                .map_err(|e| self.error("compile_assign", format!("bitfield zext: {}", e)))?
        } else {
            val_int
        };
        let field_mask = storage_ty.const_int((1u64 << info.bit_width) - 1, false);
        let clipped = self.backend.builder.build_and(val_trunc, field_mask, &format!("{}_clip", field_name))
            .map_err(|e| self.error("compile_assign", format!("bitfield clip: {}", e)))?;
        let shift = storage_ty.const_int(info.bit_offset as u64, false);
        let placed = self.backend.builder.build_left_shift(clipped, shift, &format!("{}_shl", field_name))
            .map_err(|e| self.error("compile_assign", format!("bitfield shl: {}", e)))?;
        let clear_mask = storage_ty.const_int(!(((1u64 << info.bit_width) - 1) << info.bit_offset), false);
        let cleared = self.backend.builder.build_and(loaded, clear_mask, &format!("{}_clear", field_name))
            .map_err(|e| self.error("compile_assign", format!("bitfield clear: {}", e)))?;
        let merged = self.backend.builder.build_or(cleared, placed, &format!("{}_merge", field_name))
            .map_err(|e| self.error("compile_assign", format!("bitfield or: {}", e)))?;
        self.backend.builder.build_store(field_ptr, merged)
            .map_err(|e| self.error("compile_assign", format!("bitfield store: {}", e)))?;
        Ok(val)
    }
}

fn field_index_declaration_order(
    classes: &std::collections::HashMap<String, crate::parser::class::ClassDef>,
    class_def: &crate::parser::class::ClassDef,
    field_name: &str,
) -> Option<usize> {
    crate::backend::class_layout::flatten_class_fields(class_def, classes)
        .iter()
        .position(|f| f.name == field_name)
}
