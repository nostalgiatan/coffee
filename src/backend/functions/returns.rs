#[cfg(test)]
use crate::types::Type;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{FunctionValue, BasicValueEnum};

fn zero_value_for_llvm_type<'ctx>(ty: BasicTypeEnum<'ctx>) -> BasicValueEnum<'ctx> {
    match ty {
        BasicTypeEnum::IntType(t) => t.const_zero().into(),
        BasicTypeEnum::FloatType(t) => t.const_zero().into(),
        BasicTypeEnum::PointerType(t) => t.const_null().into(),
        BasicTypeEnum::ArrayType(t) => t.const_zero().into(),
        BasicTypeEnum::StructType(t) => t.const_zero().into(),
        BasicTypeEnum::VectorType(t) => t.const_zero().into(),
        BasicTypeEnum::ScalableVectorType(t) => t.const_zero().into(),
    }
}

/// Implicit fall-through return: match the LLVM function type, not a guessed i64.
pub fn build_implicit_return<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    function: FunctionValue<'ctx>,
) -> Result<(), String> {
    match function.get_type().get_return_type() {
        None => {
            builder
                .build_return(None)
                .map_err(|e| format!("failed to build void return: {}", e))?;
        }
        Some(ty) => {
            let v = zero_value_for_llvm_type(ty);
            builder
                .build_return(Some(&v))
                .map_err(|e| format!("failed to build default return: {}", e))?;
        }
    }
    Ok(())
}

/// Get default value for a type
/// 
/// This function returns the default zero value for a given Coffee type.
/// It's used when a function needs to return a value but no explicit return
/// value is provided, or when initializing variables with default values.
/// The function handles integers (supported LLVM widths), 32- and 64-bit floats,
/// and booleans. Unsupported float widths and non-scalar types error.
/// 
/// # Arguments
/// 
/// * `type_str` - The Coffee type as a string (e.g., "int", "float", "bool")
/// * `context` - The LLVM context
/// 
/// # Returns
/// 
/// * `Ok(BasicValueEnum)` - The default zero value for the given type
/// * `Err(String)` - If the type doesn't have a meaningful default value
#[cfg(test)]
fn get_default_value<'ctx>(
    type_str: &str,
    context: &'ctx inkwell::context::Context,
) -> Result<BasicValueEnum<'ctx>, String> {
    let ty = crate::types::type_from_str(type_str)?;
    match ty {
        Type::Void | Type::Unit => Err("void type has no default value".to_string()),
        Type::Int { bits, .. } => {
            let width = match bits {
                8 | 16 | 32 | 64 | 128 => bits as u32,
                _ => {
                    return Err(format!(
                        "cannot get default value for type '{type_str}': unsupported integer width {bits} bits"
                    ));
                }
            };
            Ok(context.custom_width_int_type(width).const_zero().into())
        }
        Type::Float { bits: 32 } => Ok(context.f32_type().const_zero().into()),
        Type::Float { bits: 64 } => Ok(context.f64_type().const_zero().into()),
        Type::Float { bits } => Err(format!(
            "cannot get default value for type '{type_str}': unsupported float width {bits} bits"
        )),
        Type::Bool => Ok(context.i8_type().const_zero().into()),
        _ => Err(format!(
            "cannot get default value for non-scalar type '{type_str}'"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::values::AnyValue;

    #[test]
    fn test_get_default_value_int() {
        let context = inkwell::context::Context::create();
        let val = get_default_value("int", &context).unwrap();

        match val {
            BasicValueEnum::IntValue(i) => {
                assert_eq!(i.get_zero_extended_constant(), Some(0));
            }
            _ => panic!("Expected int value"),
        }
    }

    #[test]
    fn test_get_default_value_float() {
        let context = inkwell::context::Context::create();
        let val = get_default_value("float", &context).unwrap();

        match val {
            BasicValueEnum::FloatValue(f) => {
                // Should be 0.0, but LLVM prints it as "double 0.000000e+00"
                let printed = f.print_to_string().to_string();
                assert!(printed.contains("0.0") || printed.contains("0.000000e+00"),
                        "Expected float value to be 0.0, got: {}", printed);
            }
            _ => panic!("Expected float value"),
        }
    }

    #[test]
    fn test_get_default_value_void() {
        let context = inkwell::context::Context::create();
        let result = get_default_value("void", &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_default_value_int4_is_i32_not_ptr() {
        let context = inkwell::context::Context::create();
        let val = get_default_value("int(4)+", &context).unwrap();
        match val {
            BasicValueEnum::IntValue(i) => {
                assert_eq!(i.get_type().get_bit_width(), 32);
                assert_eq!(i.get_zero_extended_constant(), Some(0));
            }
            other => panic!("expected i32 zero, got {:?}", other),
        }
    }

    #[test]
    fn test_get_default_value_rejects_unparsable_type_instead_of_ptr() {
        let context = inkwell::context::Context::create();
        let err = get_default_value("List<int>", &context)
            .expect_err("unparsable type must not become NamedType + null ptr");
        assert!(
            !err.contains("ptr") && !err.contains("null"),
            "error must not look like a silent pointer fallback: {err}"
        );
        assert!(
            err.contains("List<int>") || err.contains("generic") || err.contains("parse"),
            "error should mention the bad type: {err}"
        );
    }

    #[test]
    fn test_get_default_value_rejects_zero_int_width_instead_of_i64() {
        let context = inkwell::context::Context::create();
        let err = get_default_value("int(0)+", &context)
            .expect_err("0-bit int must not coerce to i64");
        assert!(
            !err.contains("i64") && !err.eq_ignore_ascii_case("int"),
            "must not silently map to i64/int: {err}"
        );
    }

    #[test]
    fn test_get_default_value_rejects_unsupported_int_width_instead_of_i64() {
        let context = inkwell::context::Context::create();
        let err = get_default_value("int(3)+", &context)
            .expect_err("24-bit int is not a mapped LLVM integer");
        assert!(
            !err.contains("i64") && !err.eq_ignore_ascii_case("int"),
            "must not silently map to i64/int: {err}"
        );
    }

    #[test]
    fn test_get_default_value_float4_is_f32() {
        let context = inkwell::context::Context::create();
        let val = get_default_value("float(4)", &context).unwrap();
        match val {
            BasicValueEnum::FloatValue(f) => {
                let printed = f.print_to_string().to_string();
                assert!(
                    printed.contains("float") && !printed.contains("double"),
                    "expected f32 zero, got: {printed}"
                );
            }
            other => panic!("expected f32 zero, got {:?}", other),
        }
    }

    #[test]
    fn test_get_default_value_rejects_unsupported_float_width_instead_of_f32() {
        let context = inkwell::context::Context::create();
        let err = get_default_value("float(3)", &context)
            .expect_err("24-bit float is not a mapped LLVM float");
        assert!(
            !err.contains("f32") && !err.contains("float(4)"),
            "must not silently map to f32: {err}"
        );
    }

    #[test]
    fn test_get_default_value_rejects_str_instead_of_ptr() {
        let context = inkwell::context::Context::create();
        let err = get_default_value("str", &context)
            .expect_err("str must not become a null pointer default");
        assert!(
            !err.contains("ptr") && !err.contains("null"),
            "must not silently map to opaque ptr: {err}"
        );
    }

    #[test]
    fn test_get_default_value_rejects_named_type_instead_of_ptr() {
        let context = inkwell::context::Context::create();
        let err = get_default_value("Foo", &context)
            .expect_err("NamedType must not become a null pointer default");
        assert!(
            !err.contains("ptr") && !err.contains("null"),
            "must not silently map to opaque ptr: {err}"
        );
    }

    #[test]
    fn test_get_default_value_rejects_ref_array_tuple_instead_of_ptr() {
        let context = inkwell::context::Context::create();
        for ty in ["&int", "[int; 2]", "(int, float)"] {
            let err = get_default_value(ty, &context)
                .expect_err("non-scalar types must not become a null pointer default");
            assert!(
                !err.contains("ptr") && !err.contains("null"),
                "must not silently map {ty} to opaque ptr: {err}"
            );
        }
    }
}
