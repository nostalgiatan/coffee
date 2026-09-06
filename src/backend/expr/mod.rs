//! Expression variant helpers for `CodeGenerator`.
//!
//! Loaded as a private submodule of `backend::expressions` so `backend/mod.rs`
//! does not need to register `expr` (S1 owns that file).

mod literal;
mod binary;
mod logic;
mod call;
mod member;

use crate::backend::codegen::CodeGenerator;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Convert a value to a target type
    /// Performs safe type conversions when possible
    pub fn convert_value_to_type(&self, value: BasicValueEnum<'ctx>, target_type: BasicTypeEnum<'ctx>, name: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let value_type = value.get_type();

        // If types match, no conversion needed
        if value_type == target_type {
            return Ok(value);
        }

        // Handle integer type conversions
        if let (BasicValueEnum::IntValue(int_val), BasicTypeEnum::IntType(target_int)) = (value, target_type) {
            let current_width = int_val.get_type().get_bit_width();
            let target_width = target_int.get_bit_width();

            if current_width == target_width {
                return Ok(value);
            }

            // Truncate to smaller type (safe if value fits)
            if target_width < current_width {
                let truncated = self.backend.builder.build_int_truncate(
                    int_val,
                    target_int,
                    &format!("{}_trunc", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to truncate integer: {}", e)))?;
                return Ok(truncated.into());
            }

            // Extend to larger type (sign extension for signed)
            if target_width > current_width {
                let extended = self.backend.builder.build_int_s_extend(
                    int_val,
                    target_int,
                    &format!("{}_sext", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to sign-extend integer: {}", e)))?;
                return Ok(extended.into());
            }
        }

        // Handle float type conversions
        if let (BasicValueEnum::FloatValue(float_val), BasicTypeEnum::FloatType(target_float)) = (value, target_type) {
            let current_width = float_val.get_type().get_bit_width();
            let target_width = target_float.get_bit_width();

            if current_width == target_width {
                return Ok(value);
            }

            // Truncate to f32
            if target_width < current_width {
                let truncated = self.backend.builder.build_float_trunc(
                    float_val,
                    target_float,
                    &format!("{}_trunc", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to truncate float: {}", e)))?;
                return Ok(truncated.into());
            }

            // Extend to f64
            if target_width > current_width {
                let extended = self.backend.builder.build_float_ext(
                    float_val,
                    target_float,
                    &format!("{}_ext", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to extend float: {}", e)))?;
                return Ok(extended.into());
            }
        }

        // Handle int to float conversions
        if let (BasicValueEnum::IntValue(int_val), BasicTypeEnum::FloatType(target_float)) = (value, target_type) {
            let converted = self.backend.builder.build_signed_int_to_float(
                int_val,
                target_float,
                &format!("{}_sitofp", name)
            ).map_err(|e| self.error("type_conversion",
                format!("failed to convert int to float: {}", e)))?;
            return Ok(converted.into());
        }

        // Handle float to int conversions
        if let (BasicValueEnum::FloatValue(float_val), BasicTypeEnum::IntType(target_int)) = (value, target_type) {
            let converted = self.backend.builder.build_float_to_signed_int(
                float_val,
                target_int,
                &format!("{}_fptosi", name)
            ).map_err(|e| self.error("type_conversion",
                format!("failed to convert float to int: {}", e)))?;
            return Ok(converted.into());
        }

        // Handle pointer to struct conversions (for struct literals)
        if let (BasicValueEnum::PointerValue(ptr_val), BasicTypeEnum::StructType(target_struct)) = (value, target_type) {
            // The pointer points to the struct data, load it
            let loaded = self.backend.builder.build_load(target_struct, ptr_val, name)
                .map_err(|e| self.error("type_conversion",
                    format!("failed to load struct from pointer: {}", e)))?;
            return Ok(loaded);
        }

        // Handle pointer to int conversions (for C FFI)
        if let (BasicValueEnum::PointerValue(ptr_val), BasicTypeEnum::IntType(target_int)) = (value, target_type) {
            // Convert pointer to integer (ptrtoint)
            let converted = self.backend.builder.build_ptr_to_int(ptr_val, target_int, &format!("{}_ptrtoint", name))
                .map_err(|e| self.error("type_conversion",
                    format!("failed to convert pointer to int: {}", e)))?;
            return Ok(converted.into());
        }

        // Handle int to pointer conversions (for C FFI)
        if let (BasicValueEnum::IntValue(int_val), BasicTypeEnum::PointerType(target_ptr)) = (value, target_type) {
            // Convert integer to pointer (inttoptr)
            let converted = self.backend.builder.build_int_to_ptr(int_val, target_ptr, &format!("{}_inttoptr", name))
                .map_err(|e| self.error("type_conversion",
                    format!("failed to convert int to pointer: {}", e)))?;
            return Ok(converted.into());
        }

        if let (BasicValueEnum::PointerValue(ptr_val), BasicTypeEnum::PointerType(_)) = (value, target_type) {
            return Ok(ptr_val.into());
        }

        // Type mismatch error with helpful information
        let value_type_str = self.type_to_string(value_type);
        let target_type_str = self.type_to_string(target_type);

        Err(self.error("type_conversion",
            format!("cannot convert value from type '{}' to type '{}'\n  = note: these types are incompatible and cannot be implicitly converted\n  = help: ensure types match or use explicit type conversion if supported",
                value_type_str, target_type_str)))
    }

    /// Convert LLVM type to human-readable Coffee spelling.
    /// Unknown LLVM kinds (vector / scalable vector) are not `int`.
    pub fn type_to_string(&self, type_: BasicTypeEnum<'ctx>) -> String {
        crate::backend::types::llvm_basic_to_coffee(type_).unwrap_or_else(|e| e)
    }

    /// Check if two types are compatible for implicit conversion
    pub fn are_types_compatible(&self, from: BasicTypeEnum<'ctx>, to: BasicTypeEnum<'ctx>) -> bool {
        match (from, to) {
            // Same types are always compatible
            (a, b) if a == b => true,

            // Integer to integer: always compatible (with truncation/extension)
            (BasicTypeEnum::IntType(_), BasicTypeEnum::IntType(_)) => true,

            // Float to float: always compatible (with truncation/extension)
            (BasicTypeEnum::FloatType(_), BasicTypeEnum::FloatType(_)) => true,

            // Integer to float: compatible
            (BasicTypeEnum::IntType(_), BasicTypeEnum::FloatType(_)) => true,

            // Float to integer: compatible
            (BasicTypeEnum::FloatType(_), BasicTypeEnum::IntType(_)) => true,

            // C `object` / `str` / `buf`: opaque ptr, and int handles via inttoptr/ptrtoint
            (BasicTypeEnum::PointerType(_), BasicTypeEnum::PointerType(_)) => true,
            (BasicTypeEnum::PointerType(_), BasicTypeEnum::IntType(_)) => true,
            (BasicTypeEnum::IntType(_), BasicTypeEnum::PointerType(_)) => true,

            // All other combinations are incompatible
            _ => false,
        }
    }
}
