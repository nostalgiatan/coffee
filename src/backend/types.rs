//! Type system for LLVM code generation
//! 
//! This module provides the type mapping functionality that converts Coffee language
//! types to their corresponding LLVM types. It handles the conversion between the
//! unified Coffee type system and LLVM's type system, ensuring proper type representation
//! during code generation. The module also provides C ABI compatibility when interfacing
//! with external C libraries.
//! 
//! Uses unified Type from crate::types

use inkwell::context::Context;
use inkwell::types::{BasicTypeEnum, StructType};
use inkwell::AddressSpace;
use std::collections::HashMap;
use crate::types::Type as CoffeeType;  // Use unified Type from types module

/// Type mapper from Coffee types to LLVM types
/// 
/// The TypeMapper is responsible for converting Coffee language types to their
/// corresponding LLVM types during code generation. It maintains a cache of
/// struct types and provides different mapping strategies depending on whether
/// C ABI compatibility is required.
/// 
/// The mapper supports both Coffee's native type system and C-compatible type
/// mappings for FFI (Foreign Function Interface) operations. It handles complex
/// type conversions including integers of various sizes and signedness,
/// floating-point types, and composite types.
pub struct TypeMapper<'ctx> {
    pub context: &'ctx Context,
    /// Cache of struct types
    pub struct_types: HashMap<String, StructType<'ctx>>,
    /// Cache of struct field names to indices
    struct_fields: HashMap<String, Vec<String>>,
    /// Whether to use C type mapping (for C ABI compatibility)
    c_abi: bool,
}

impl<'ctx> TypeMapper<'ctx> {
    /// Create a new TypeMapper instance
    /// 
    /// Initializes a TypeMapper with the default Coffee type mapping behavior
    /// and without C ABI compatibility. The mapper will handle type conversions
    /// according to Coffee's native type system.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM Context to use for type creation
    /// 
    /// # Returns
    /// 
    /// A new TypeMapper instance with default settings
    pub fn new(context: &'ctx Context) -> Self {
        TypeMapper {
            context,
            struct_types: HashMap::new(),
            struct_fields: HashMap::new(),
            c_abi: false,
        }
    }

    /// Create a type mapper with C ABI compatibility
    /// 
    /// Initializes a TypeMapper configured for C ABI compatibility. This is used
    /// when interfacing with external C libraries, ensuring that Coffee types
    /// are mapped to their C equivalents for proper FFI (Foreign Function Interface)
    /// operations. The C ABI mode ensures that type sizes and representations
    /// match C conventions.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM Context to use for type creation
    /// 
    /// # Returns
    /// 
    /// A new TypeMapper instance configured for C ABI compatibility
    pub fn with_c_abi(context: &'ctx Context) -> Self {
        TypeMapper {
            context,
            struct_types: HashMap::new(),
            struct_fields: HashMap::new(),
            c_abi: true,
        }
    }

    /// Map unified Type to C type for FFI
    /// 
    /// This internal method converts Coffee types to their corresponding C types
    /// for use in Foreign Function Interface (FFI) operations. The mapping ensures
    /// compatibility with C's type system, which is necessary when calling C
    /// functions from Coffee or exposing Coffee functions to C code.
    /// 
    /// Coffee types → C types:
    /// - int(8)+, int(16)+, int(32)+, int(64)+ → int8_t, int16_t, int32_t, int64_t
    /// - int(8)-, int(16)-, int(32)-, int(64)- → uint8_t, uint16_t, uint32_t, uint64_t
    /// - int (defaults to i32) → int (32-bit on most platforms)
    /// - float, float(8) → double
    /// - float(4) → float
    /// - bool → _Bool (C99) or int (C89)
    /// - string → const char*
    /// 
    /// # Arguments
    /// 
    /// * `coffee_type` - The CoffeeType to convert to a C-compatible type
    /// 
    /// # Returns
    /// 
    /// * `Some(BasicTypeEnum)` - The corresponding C-compatible LLVM type
    /// * `None` - If no C mapping exists for the given Coffee type
    fn map_c_type(&self, coffee_type: &CoffeeType) -> Option<BasicTypeEnum<'ctx>> {
        Some(match coffee_type {
            // Fixed-width integers (C99 stdint.h)
            CoffeeType::Int { bits: 8, signed: true } => self.context.i8_type().into(),    // int8_t
            CoffeeType::Int { bits: 16, signed: true } => self.context.i16_type().into(),  // int16_t
            CoffeeType::Int { bits: 32, signed: true } => self.context.i32_type().into(),  // int32_t
            CoffeeType::Int { bits: 64, signed: true } => self.context.i64_type().into(),  // int64_t / long
            CoffeeType::Int { bits: 128, signed: true } => self.context.i128_type().into(), // int128_t (GNU extension)

            // Unsigned variants (C99 stdint.h)
            CoffeeType::Int { bits: 8, signed: false } => self.context.i8_type().into(),    // uint8_t
            CoffeeType::Int { bits: 16, signed: false } => self.context.i16_type().into(),  // uint16_t
            CoffeeType::Int { bits: 32, signed: false } => self.context.i32_type().into(),  // uint32_t
            CoffeeType::Int { bits: 64, signed: false } => self.context.i64_type().into(),  // uint64_t / unsigned long
            CoffeeType::Int { bits: 128, signed: false } => self.context.i128_type().into(), // uint128_t (GNU extension)

            // Floating point
            CoffeeType::Float { bits: 32 } => self.context.f32_type().into(),   // float
            CoffeeType::Float { bits: 64 } => self.context.f64_type().into(),   // double

            // Other types
            CoffeeType::Bool => self.context.i8_type().into(),   // _Bool or int
            CoffeeType::String => self.context.ptr_type(AddressSpace::default()).into(), // const char*

            // Pointers, arrays, and slices
            CoffeeType::Ref { .. } | CoffeeType::Array { .. } | CoffeeType::Slice(_) =>
                self.context.ptr_type(AddressSpace::default()).into(),

            _ => return None,
        })
    }

    /// Convert Coffee type string to LLVM type with signed/unsigned support
    /// 
    /// This method converts a Coffee type string (such as "int(32)+", "float(64)", 
    /// "bool", etc.) to its corresponding LLVM type. The method uses the unified
    /// Type system from crate::types for validation and proper type system support.
    /// 
    /// The conversion handles various Coffee type formats including basic types,
    /// sized integers with signedness, floating-point types, and composite types.
    /// The method provides fallback behavior for unknown types by returning a
    /// pointer type.
    /// 
    /// Uses unified Type from crate::types for validation and type system support
    /// 
    /// # Arguments
    /// 
    /// * `type_str` - The Coffee type string to convert (e.g., "int(32)+", "float")
    /// 
    /// # Returns
    /// 
    /// The corresponding LLVM BasicTypeEnum for the given Coffee type string
    pub fn map_type(&self, type_str: &str) -> BasicTypeEnum<'ctx> {
        // Parse type string using unified type system
        let parsed_type = crate::types::type_from_str(type_str);

        let coffee_type = match parsed_type {
            Ok(t) => t,
            Err(_) => {
                // Fallback for unknown types - use pointer type
                return self.context.ptr_type(AddressSpace::default()).into();
            }
        };

        // Handle based on the unified Type enum
        self.map_unified_type(&coffee_type)
    }

    /// Map unified Type enum to LLVM type
    /// 
    /// This internal method converts a CoffeeType enum value to its corresponding
    /// LLVM type. The method handles the full range of Coffee types including
    /// integers of various sizes and signedness, floating-point types, and
    /// composite types. In C ABI mode, it prioritizes C-compatible mappings.
    /// 
    /// The method implements Coffee's type system semantics while ensuring
    /// compatibility with LLVM's type system. It handles special cases like
    /// variadic arguments and provides appropriate fallbacks for complex types.
    /// 
    /// # Arguments
    /// 
    /// * `coffee_type` - The CoffeeType enum value to convert
    /// 
    /// # Returns
    /// 
    /// The corresponding LLVM BasicTypeEnum for the given CoffeeType
    fn map_unified_type(&self, coffee_type: &CoffeeType) -> BasicTypeEnum<'ctx> {
        // If in C ABI mode, try map_c_type first for better C compatibility
        if self.c_abi {
            if let Some(llvm_type) = self.map_c_type(coffee_type) {
                return llvm_type;
            }
        }

        // Default Coffee type mapping
        match coffee_type {
            // Variadic arguments - represent as i8 marker (handled separately in function declaration)
            CoffeeType::Variadic => self.context.i8_type().into(),

            // Signed integers
            CoffeeType::Int { bits, signed: true } => match bits {
                8 => self.context.i8_type().into(),
                16 => self.context.i16_type().into(),
                32 => self.context.i32_type().into(),
                64 => self.context.i64_type().into(),
                128 => self.context.i128_type().into(),
                _ => self.context.i64_type().into(),
            },

            // Unsigned integers
            CoffeeType::Int { bits, signed: false } => match bits {
                8 => self.context.i8_type().into(),
                16 => self.context.i16_type().into(),
                32 => self.context.i32_type().into(),
                64 => self.context.i64_type().into(),
                128 => self.context.i128_type().into(),
                _ => self.context.i64_type().into(),
            },

            // Floating point
            CoffeeType::Float { bits } => match bits {
                32 => self.context.f32_type().into(),
                64 => self.context.f64_type().into(),
                _ => self.context.f32_type().into(),
            },

            // Other types
            CoffeeType::Bool => self.context.i8_type().into(),  // bool = i8 (统一，C兼容)
            CoffeeType::String => self.context.ptr_type(AddressSpace::default()).into(),
            CoffeeType::Unit => self.context.i8_type().into(),  // () as i8
            CoffeeType::Void => self.context.i8_type().into(),  // void - placeholder, will be handled specially

            // Composite types
            CoffeeType::Tuple(types) => {
                // Convert tuple types to LLVM struct type
                let field_types: Vec<BasicTypeEnum> = types
                    .iter()
                    .map(|t| self.map_unified_type(t))
                    .collect();
                
                // Create a struct type for the tuple
                // Use a unique name based on the tuple's field types
                let type_name = format!("tuple_{}", types.len());
                self.context.struct_type(&field_types, false).into()
            }
            CoffeeType::Array { .. } => self.context.ptr_type(AddressSpace::default()).into(),
            CoffeeType::Slice(_) => self.context.ptr_type(AddressSpace::default()).into(),
            CoffeeType::Ref { .. } => self.context.ptr_type(AddressSpace::default()).into(),
            CoffeeType::Function { .. } => self.context.ptr_type(AddressSpace::default()).into(),

            // Named types
            CoffeeType::NamedType { name } => {
                if let Some(&struct_type) = self.struct_types.get(name) {
                    // For named types (classes), return a pointer to the struct
                    // This is because class instances are passed by reference
                    struct_type.ptr_type(AddressSpace::default()).into()
                } else {
                    self.context.ptr_type(AddressSpace::default()).into()
                }
            },
        }
    }

/// Get or create a struct type from a class definition
    /// 
    /// This method creates an LLVM struct type for a Coffee class, using the class
    /// definition to determine the number, order, and types of fields. The struct
    /// type is cached to avoid recreating it on subsequent calls.
    /// 
    /// # Arguments
    /// 
    /// * `class_name` - The name of the Coffee class
    /// * `class` - The class definition containing field information
    /// * `all_classes` - All class definitions (for inheritance support)
    /// 
    /// # Returns
    /// 
    /// * `Some(StructType)` - The created or cached struct type
    /// * `None` - If the class cannot be converted to a struct type
    pub fn get_or_create_struct_type_from_class(
        &mut self,
        class_name: &str,
        class: &crate::parser::class::ClassDef,
        all_classes: &std::collections::HashMap<String, crate::parser::class::ClassDef>,
    ) -> Option<StructType<'ctx>> {
        // Check cache first
        if let Some(&struct_type) = self.struct_types.get(class_name) {
            return Some(struct_type);
        }

        // Convert Coffee field types to LLVM types
        let mut field_types = Vec::new();
        let mut field_names = Vec::new();

        // Handle inheritance: add parent fields first
        if let Some(ref parent_name) = class.parent {
            if let Some(parent_class) = all_classes.get(parent_name) {
                for field in &parent_class.fields {
                    let llvm_type = self.map_type(&field.field_type);
                    field_types.push(llvm_type);
                    field_names.push(field.name.clone());
                }
            }
        }

        // Add current class fields
        for field in &class.fields {
            let llvm_type = self.map_type(&field.field_type);
            field_types.push(llvm_type);
            field_names.push(field.name.clone());
        }

        // Create opaque struct type
        let struct_type = self.context.opaque_struct_type(class_name);

        // Set struct body with field types
        struct_type.set_body(&field_types, class.packed);

        // Cache the struct type and field names
        self.struct_types.insert(class_name.to_string(), struct_type);
        self.struct_fields.insert(class_name.to_string(), field_names);

        Some(struct_type)
    }

    /// Get field index by name in a struct type
    /// 
    /// # Arguments
    /// 
    /// * `struct_name` - The name of the struct type
    /// * `field_name` - The name of the field
    /// 
    /// # Returns
    /// 
    /// * `Some(usize)` - The index of the field
    /// * `None` - If the field is not found
    pub fn get_field_index(&self, struct_name: &str, field_name: &str) -> Option<usize> {
        eprintln!("DEBUG: get_field_index: struct_name='{}', field_name='{}'", struct_name, field_name);
        eprintln!("DEBUG: get_field_index: struct_fields.keys()={:?}", self.struct_fields.keys().collect::<Vec<_>>());
        if let Some(fields) = self.struct_fields.get(struct_name) {
            eprintln!("DEBUG: get_field_index: struct '{}' has fields: {:?}", struct_name, fields);
            let result = fields.iter().position(|name| name == field_name);
            eprintln!("DEBUG: get_field_index: position result={:?}", result);
            result
        } else {
            eprintln!("DEBUG: get_field_index: struct '{}' not found in struct_fields", struct_name);
            None
        }
    }
}
