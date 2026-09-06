//! Call, constructor, and named-function compilation.

use crate::backend::codegen::CodeGenerator;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn emit_enum_variant_call(
        &mut self,
        scope_str: &str,
        variant_name: &str,
        compiled_args: Vec<BasicValueEnum<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let tag_index = self.enums.get(scope_str)
            .and_then(|enum_def| enum_def.variants.iter().position(|v| v.name == variant_name))
            .unwrap_or(0) as u64;
        let tag_value = self.backend.context.i64_type().const_int(tag_index, false);
        let mut field_values: Vec<BasicValueEnum<'ctx>> = vec![BasicValueEnum::IntValue(tag_value)];
        field_values.extend(compiled_args);
        let field_types: Vec<BasicTypeEnum<'ctx>> = field_values.iter().map(|v| v.get_type()).collect();
        let variant_type = self.backend.context.struct_type(&field_types, false);
        // Runtime payloads (literals, nested variants) are not LLVM constants, so
        // build via alloca + insertvalue rather than const_named_struct.
        let variant_ptr = self.backend.builder.build_alloca(variant_type, &format!("{}_{}", scope_str, variant_name))
            .map_err(|e| self.error("compile_expression",
                format!("failed to allocate space for enum variant: {}", e)))?;
        let mut current_value = variant_type.const_zero();
        for (i, field) in field_values.iter().enumerate() {
            let inserted = self.backend.builder.build_insert_value(
                current_value,
                *field,
                i as u32,
                &format!("{}_{}_insert_{}", scope_str, variant_name, i)
            ).map_err(|e| self.error("compile_expression",
                format!("failed to insert enum variant field {}: {}", i, e)))?;
            current_value = match inserted {
                inkwell::values::AggregateValueEnum::StructValue(v) => v,
                _ => return Err(self.error("compile_expression",
                    "insert_value did not return a StructValue".to_string())),
            };
        }
        self.backend.builder.build_store(variant_ptr, current_value)
            .map_err(|e| self.error("compile_expression",
                format!("failed to store enum variant value: {}", e)))?;
        // Named enum types map to ptr; return the alloca rather than a loaded struct.
        Ok(variant_ptr.into())
    }

    pub(crate) fn emit_constructor_call(
        &mut self,
        class_name: &str,
        compiled: Vec<BasicValueEnum<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let ctor_name = format!("{}_new", class_name);
        let ctor_fn = *self.functions.get(&ctor_name)
            .ok_or_else(|| self.error("compile_constructor_call",
                format!("constructor not found: '{}'", ctor_name)))?;
        let call_result = self.backend.builder.build_call(
            ctor_fn,
            &compiled.iter().map(|a| (*a).into()).collect::<Vec<_>>(),
            &format!("{}_call", ctor_name)
        ).map_err(|e| self.error("compile_constructor_call",
            format!("failed to call constructor '{}': {}", ctor_name, e)))?;
        let return_value = match call_result.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("compile_constructor_call",
                    "constructor returned void (should return object)"));
            }
        };
        match return_value {
            BasicValueEnum::PointerValue(ptr) => Ok(ptr.into()),
            BasicValueEnum::IntValue(int_val) => {
                let ptr_type = self.backend.context.ptr_type(inkwell::AddressSpace::default());
                let ptr = self.backend.builder.build_int_to_ptr(int_val, ptr_type, &format!("{}_ptr", ctor_name))
                    .map_err(|e| self.error("compile_constructor_call",
                        format!("failed to convert int to ptr: {}", e)))?;
                Ok(ptr.into())
            }
            BasicValueEnum::StructValue(struct_val) => {
                let malloc_fn = self.functions.get("malloc")
                    .copied()
                    .ok_or_else(|| self.error("compile_constructor_call",
                        "malloc function not found for heap allocation"))?;
                let size = 64u64;
                let size_value = self.backend.context.i64_type().const_int(size, false);
                let heap_ptr = self.backend.builder.build_call(
                    malloc_fn,
                    &[size_value.into()],
                    &format!("{}_malloc", ctor_name)
                ).map_err(|e| self.error("compile_constructor_call",
                    format!("failed to call malloc: {}", e)))?;
                let heap_ptr_value = match heap_ptr.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(val) => val,
                    _ => return Err(self.error("compile_constructor_call",
                        "malloc returned unexpected value")),
                };
                let heap_ptr = match heap_ptr_value {
                    BasicValueEnum::PointerValue(ptr) => ptr,
                    _ => return Err(self.error("compile_constructor_call",
                        "malloc did not return a pointer")),
                };
                self.backend.builder.build_store(heap_ptr, struct_val)
                    .map_err(|e| self.error("compile_constructor_call",
                        format!("failed to store struct to heap: {}", e)))?;
                Ok(heap_ptr.into())
            }
            _ => Err(self.error("compile_constructor_call",
                "constructor did not return an object (should return struct or pointer)")),
        }
    }

}
