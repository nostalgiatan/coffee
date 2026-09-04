// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License

//! Error class generation module
//!
//! This module handles the automatic generation of the Error class and its subclasses.
//! The Error class is only generated when the program uses raise statements or error handlers,
//! avoiding dead code in programs that don't need error handling.

use inkwell::context::Context;
use inkwell::AddressSpace;
use crate::backend::types::TypeMapper;

/// Generate Error class definition
///
/// The Error class has three fields:
/// - code: int (error code)
/// - note: string (error message/description)
/// - e: any (additional error context/data)
///
/// This function should only be called when the program uses raise statements
/// or error handlers, to avoid generating unnecessary code.
///
/// # Arguments
///
/// * `context` - The LLVM context
/// * `type_mapper` - The type mapper to register the Error class type
///
/// # Returns
///
/// * `Ok(())` - Successfully generated the Error class
/// * `Err(String)` - If there was an error during generation
pub fn generate_error_class<'ctx>(
    context: &'ctx Context,
    type_mapper: &mut TypeMapper<'ctx>,
) -> Result<(), String> {
    let i64_type = context.i64_type();
    let i8_ptr_type = context.ptr_type(AddressSpace::default());

    // Register the Error class type in the type mapper
    // This allows error classes to inherit from Error
    // Create Error struct type manually
    let error_type = type_mapper.context.opaque_struct_type("Error");
    error_type.set_body(&[
        i64_type.into(),   // code: int
        i8_ptr_type.into(), // note: string (pointer)
        i8_ptr_type.into(), // e: any (pointer)
    ], false);

    Ok(())
}

/// Check if a class is an error class (inherits from Error)
///
/// # Arguments
///
/// * `class_name` - The name of the class to check
/// * `_type_mapper` - The type mapper to check inheritance (unused for now)
///
/// # Returns
///
/// * `true` - If the class inherits from Error
/// * `false` - Otherwise
pub fn is_error_class(class_name: &str, _type_mapper: &TypeMapper) -> bool {
    // Check if the class type inherits from Error
    // This will be implemented when we have proper inheritance tracking
    // For now, we can check if the class name ends with "Error" or starts with "Error"
    class_name == "Error" || class_name.ends_with("Error") || class_name.starts_with("Error")
}