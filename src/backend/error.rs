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
use crate::parser::class::{ClassDef, ClassField};

/// Parser `ClassDef` for builtin `Error { code, note, e }` (layout prefix for `of Error`).
pub fn builtin_error_class_def() -> ClassDef {
    ClassDef {
        type_params: vec![],
        name: "Error".to_string(),
        parent: None,
        fields: vec![
            ClassField {
                name: "code".to_string(),
                field_type: "int".to_string(),
                bit_width: None,
            },
            ClassField {
                name: "note".to_string(),
                field_type: "str".to_string(),
                bit_width: None,
            },
            ClassField {
                name: "e".to_string(),
                field_type: "object".to_string(),
                bit_width: None,
            },
        ],
        methods: vec![],
        packed: false,
        has_constructor: false,
    }
}

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
    if type_mapper.struct_types.contains_key("Error") {
        return Ok(());
    }

    let i64_type = context.i64_type();
    let i8_ptr_type = context.ptr_type(AddressSpace::default());
    let field_tys = [
        i64_type.into(),    // code: int
        i8_ptr_type.into(), // note: string (pointer)
        i8_ptr_type.into(), // e: any (pointer)
    ];

    let error_type = match context.get_struct_type("Error") {
        Some(existing) => {
            if existing.is_opaque() {
                existing.set_body(&field_tys, false);
            }
            existing
        }
        None => {
            let ty = type_mapper.context.opaque_struct_type("Error");
            ty.set_body(&field_tys, false);
            ty
        }
    };

    type_mapper.struct_types.insert("Error".to_string(), error_type);
    Ok(())
}
