use inkwell::values::BasicValueEnum;
use inkwell::IntPredicate;

/// Convert a value to a boolean condition
///
/// This function converts a given LLVM value to a boolean condition suitable
/// for use in control flow operations like if statements and loops. It handles
/// different types of values by comparing them to their appropriate zero value:
///
/// - For integers: compares with 0 (non-zero values are true)
/// - For floats: compares with 0.0 (non-zero values are true)
/// - For pointers: compares with null pointer (non-null pointers are true)
///
/// The function returns an i1 integer value (LLVM's boolean type) that represents
/// the truthiness of the input value. This is essential for generating correct
/// conditional branches in the compiled code.
///
/// # Arguments
///
/// * `value` - The LLVM value to convert to a boolean condition
/// * `builder` - The LLVM builder to use for generating the comparison instruction
///
/// # Returns
///
/// * `Ok(IntValue)` - An i1 integer value representing the boolean condition
/// * `Err(String)` - If the value type cannot be converted to a boolean
pub fn value_to_bool<'ctx>(value: BasicValueEnum<'ctx>, builder: &inkwell::builder::Builder<'ctx>) -> Result<inkwell::values::IntValue<'ctx>, String> {
    match value {
        BasicValueEnum::IntValue(i) => {
            let zero = i.get_type().const_zero();
            Ok(builder.build_int_compare(
                IntPredicate::NE,
                i,
                zero,
                "tobool"
            ).map_err(|e| format!("failed to build boolean conversion: {}", e))?)
        }
        BasicValueEnum::FloatValue(f) => {
            let zero = f.get_type().const_zero();
            let cmp = builder.build_float_compare(
                inkwell::FloatPredicate::ONE,
                f,
                zero,
                "toboolf"
            ).map_err(|e| format!("failed to build float boolean conversion: {}", e))?;
            Ok(cmp)
        }
        BasicValueEnum::PointerValue(p) => {
            let null_ptr = p.get_type().const_null();
            let i64_type = builder.get_insert_block().unwrap().get_context().i64_type();
            let int_ptr = builder.build_ptr_to_int(p, i64_type, "ptr_to_int")
                .map_err(|e| format!("failed to convert pointer to int: {}", e))?;
            let int_null = builder.build_ptr_to_int(null_ptr, i64_type, "null_to_int")
                .map_err(|e| format!("failed to convert null to int: {}", e))?;
            Ok(builder.build_int_compare(
                inkwell::IntPredicate::NE,
                int_ptr,
                int_null,
                "toptrbool"
            ).map_err(|e| format!("failed to build pointer boolean conversion: {}", e))?)
        }
        _ => Err(format!("cannot convert {:?} to boolean", value))
    }
}
