//! Class and enum compilation for Coffee compiler
//!
//! Handles class definitions, enum definitions, and their associated operations.

use crate::coffee_debug;
use super::codegen::CodeGenerator;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile class definition
    pub fn compile_class(&mut self, class: &crate::parser::class::ClassDef) -> Result<(), String> {
        if self.c_value_names.contains(&class.name) {
            self.classes.insert(class.name.clone(), class.clone());
            return Ok(());
        }
        coffee_debug!("DEBUG: compile_class: compiling class '{}', methods={:?}", class.name, class.methods.iter().map(|m| &m.name).collect::<Vec<_>>());
        // Store class definition for C header generation
        self.classes.insert(class.name.clone(), class.clone());

        use crate::backend::memory::{Layout, StructLayout};

        let flattened = super::class_layout::flatten_class_fields(class, &self.classes);
        let td = self.llvm_target_data();
        let pack_bits = (self.enable_bitfields || class.packed)
            && flattened.iter().any(|f| f.bit_width.is_some());
        let packed_members = if pack_bits {
            super::class_layout::packed_llvm_members(&flattened)
        } else {
            Vec::new()
        };

        let mut field_layouts: Vec<Layout> = Vec::new();
        let mut field_names: Vec<String> = Vec::new();
        if pack_bits {
            let info = super::class_layout::packed_field_info_map(&flattened);
            let mut bit_field_map = std::collections::HashMap::new();
            for (name, p) in &info {
                if p.bit_width > 0 {
                    bit_field_map.insert(name.clone(), (p.bit_offset, p.bit_width, p.storage_bits));
                }
            }
            if !bit_field_map.is_empty() {
                self.bit_field_layouts.insert(class.name.clone(), bit_field_map);
            }
            self.packed_field_info.insert(class.name.clone(), info);

            for member in &packed_members {
                match member {
                    super::class_layout::LlvmStructMember::Regular(field) => {
                        let field_type = self.coffee_type_to_llvm(&field.field_type)?;
                        field_layouts.push(Layout::from_target_data(&field_type, &td));
                        field_names.push(field.name.clone());
                    }
                    super::class_layout::LlvmStructMember::BitStorage { bits, fields } => {
                        let ty: inkwell::types::BasicTypeEnum =
                            self.bitfield_storage_int_type(*bits).into();
                        field_layouts.push(Layout::from_target_data(&ty, &td));
                        field_names.push(
                            fields
                                .iter()
                                .map(|(n, _, _)| n.as_str())
                                .collect::<Vec<_>>()
                                .join("+"),
                        );
                    }
                }
            }
        } else {
            for field in &flattened {
                let field_type = self.coffee_type_to_llvm(&field.field_type)?;
                field_layouts.push(Layout::from_target_data(&field_type, &td));
            }
            field_names = flattened.iter().map(|f| f.name.clone()).collect();
        }

        let struct_layout = if class.packed {
            StructLayout::calculate_packed(&field_layouts)
        } else {
            StructLayout::calculate_declaration_order(&field_layouts)
        };

        self.layout_collector.add_struct(class.name.clone(), struct_layout.clone(), field_names);

        let (data_bytes, padding_bytes, efficiency) = struct_layout.efficiency_metrics();
        if !class.packed && efficiency < 75.0 {
            eprintln!("  = warning: struct '{}' has low memory efficiency ({:.1}%)\n    = note: {} bytes data, {} bytes padding\n    = help: consider using 'packed class' or reordering fields to reduce padding",
                class.name, efficiency, data_bytes, padding_bytes);
        }

        // Opaque struct + body is created once here, in declaration order.
        self.type_mapper
            .get_or_create_struct_type_from_class(&class.name, class, &self.classes)
            .map_err(|e| self.error("compile_class", e))?;

        if pack_bits {
            if let Some(&struct_type) = self.type_mapper.struct_types.get(&class.name) {
                let mut llvm_fields: Vec<inkwell::types::BasicTypeEnum> = Vec::new();
                for member in &packed_members {
                    match member {
                        super::class_layout::LlvmStructMember::Regular(field) => {
                            llvm_fields.push(self.coffee_type_to_llvm(&field.field_type)?);
                        }
                        super::class_layout::LlvmStructMember::BitStorage { bits, .. } => {
                            llvm_fields.push(self.bitfield_storage_int_type(*bits).into());
                        }
                    }
                }
                struct_type.set_body(&llvm_fields, class.packed);
            }
        }

        // Declare every method before compiling so later methods (e.g. `new`)
        // are visible to earlier associated functions (`origin` calling `Point::new`).
        let method_fns: Vec<_> = class.methods.iter()
            .map(|method| method.to_standalone_function(&class.name))
            .collect();
        for func in &method_fns {
            self.declare_function(func)?;
        }
        for func in &method_fns {
            self.compile_function(func)?;
        }

        // Generate drop function for classes with constructor (full classes)
        if class.has_constructor {
            let drop_func_name = format!("{}__drop", class.name);
            
            // Create drop function signature: void ClassName__drop(struct ClassName* self)
            let drop_func = crate::parser::Function {
                type_params: vec![],
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
                body: crate::parser::function::FunctionBody::Expression(crate::parser::expr::Expression::Literal(String::new())),
            };

            // Declare the drop function
            crate::backend::functions::declare_function(
                &drop_func,
                &self.backend.module,
                self.backend.context,
                &self.type_mapper,
                &mut self.functions,
            )?;

            // Compile drop function body: reverse-order nested class `__drop` calls.
            let fn_value = self.functions.get(&drop_func_name).copied()
                .ok_or_else(|| self.error("compile_class",
                    format!("drop function '{}' not found after declaration", drop_func_name)))?;

            // Create entry block
            let entry_block = self.backend.context.append_basic_block(fn_value, "entry");
            self.backend.builder.position_at_end(entry_block);

            let self_ptr = fn_value
                .get_nth_param(0)
                .ok_or_else(|| {
                    self.error(
                        "compile_class",
                        format!("drop function '{}' is missing the self parameter", drop_func_name),
                    )
                })?
                .into_pointer_value();

            let struct_type = *self.type_mapper.struct_types.get(&class.name).ok_or_else(|| {
                self.error(
                    "compile_class",
                    format!("struct type for '{}' not found when compiling drop", class.name),
                )
            })?;
            let packed_info = self.packed_field_info.get(&class.name).cloned();

            for (idx, field) in flattened.iter().enumerate().rev() {
                if field.bit_width.is_some() {
                    continue;
                }
                let Ok(field_ty) = crate::types::type_from_str(&field.field_type) else {
                    continue;
                };
                let gep_index = packed_info
                    .as_ref()
                    .and_then(|m| m.get(&field.name).map(|p| p.gep_index))
                    .unwrap_or(idx);
                let zero = self.backend.context.i32_type().const_int(0, false);
                let field_index_val = self
                    .backend
                    .context
                    .i32_type()
                    .const_int(gep_index as u64, false);
                let field_ptr = unsafe {
                    self.backend.builder.build_in_bounds_gep(
                        struct_type,
                        self_ptr,
                        &[zero, field_index_val],
                        &format!("{}_{}_drop_ptr", class.name, field.name),
                    )
                }
                .map_err(|e| {
                    self.error(
                        "compile_class",
                        format!("failed to GEP drop field '{}': {}", field.name, e),
                    )
                })?;

                let field_llvm = struct_type
                    .get_field_type_at_index(gep_index as u32)
                    .ok_or_else(|| {
                        self.error(
                            "compile_class",
                            format!("drop: missing LLVM field '{}'", field.name),
                        )
                    })?;

                crate::backend::memory_ops::drop_coffee_place(
                    self.backend.context,
                    &self.backend.builder,
                    &self.functions,
                    &field_ty,
                    field_ptr,
                    field_llvm,
                )
                .map_err(|e| self.error("compile_class", e))?;
            }

            self.backend.builder.build_return(None)
                .map_err(|e| self.error("compile_class", format!("failed to build return in drop function: {}", e)))?;

            // Class__clone: construct-order field walk (drop order reversed).
            let clone_func_name = format!("{}__clone", class.name);
            let clone_func = crate::parser::Function {
                type_params: vec![],
                name: clone_func_name.clone(),
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
                body: crate::parser::function::FunctionBody::Expression(crate::parser::expr::Expression::Literal(String::new())),
            };
            crate::backend::functions::declare_function(
                &clone_func,
                &self.backend.module,
                self.backend.context,
                &self.type_mapper,
                &mut self.functions,
            )?;
            let clone_fn_value = self.functions.get(&clone_func_name).copied()
                .ok_or_else(|| self.error("compile_class",
                    format!("clone function '{}' not found after declaration", clone_func_name)))?;
            let clone_entry = self.backend.context.append_basic_block(clone_fn_value, "entry");
            self.backend.builder.position_at_end(clone_entry);
            let clone_self = clone_fn_value
                .get_nth_param(0)
                .ok_or_else(|| {
                    self.error(
                        "compile_class",
                        format!("clone function '{}' is missing the self parameter", clone_func_name),
                    )
                })?
                .into_pointer_value();
            for (idx, field) in flattened.iter().enumerate() {
                if field.bit_width.is_some() {
                    continue;
                }
                let Ok(field_ty) = crate::types::type_from_str(&field.field_type) else {
                    continue;
                };
                let gep_index = packed_info
                    .as_ref()
                    .and_then(|m| m.get(&field.name).map(|p| p.gep_index))
                    .unwrap_or(idx);
                let zero = self.backend.context.i32_type().const_int(0, false);
                let field_index_val = self
                    .backend
                    .context
                    .i32_type()
                    .const_int(gep_index as u64, false);
                let field_ptr = unsafe {
                    self.backend.builder.build_in_bounds_gep(
                        struct_type,
                        clone_self,
                        &[zero, field_index_val],
                        &format!("{}_{}_clone_ptr", class.name, field.name),
                    )
                }
                .map_err(|e| {
                    self.error(
                        "compile_class",
                        format!("failed to GEP clone field '{}': {}", field.name, e),
                    )
                })?;
                let field_llvm = struct_type
                    .get_field_type_at_index(gep_index as u32)
                    .ok_or_else(|| {
                        self.error(
                            "compile_class",
                            format!("clone: missing LLVM field '{}'", field.name),
                        )
                    })?;
                crate::backend::memory_ops::clone_coffee_place(
                    self.backend.context,
                    &self.backend.builder,
                    &self.functions,
                    &field_ty,
                    field_ptr,
                    field_llvm,
                )
                .map_err(|e| self.error("compile_class", e))?;
            }
            self.backend.builder.build_return(None)
                .map_err(|e| self.error("compile_class", format!("failed to build return in clone function: {}", e)))?;
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