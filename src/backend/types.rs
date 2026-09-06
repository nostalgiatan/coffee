//! Type system for LLVM code generation
//! 
//! This module provides the type mapping functionality that converts Coffee language
//! types to their corresponding LLVM types. It handles the conversion between the
//! unified Coffee type system and LLVM's type system, ensuring proper type representation
//! during code generation. The module also provides C ABI compatibility when interfacing
//! with external C libraries.
//! 
//! Uses unified Type from crate::types

use crate::coffee_debug;
use inkwell::context::Context;
use inkwell::types::{BasicTypeEnum, StructType};
use inkwell::AddressSpace;
use std::collections::{HashMap, HashSet};
use crate::types::Type as CoffeeType;  // Use unified Type from types module

/// LLVM `BasicTypeEnum` → Coffee type spelling used in backend diagnostics.
///
/// Vector / scalable-vector kinds are not Coffee types and must not stringify
/// as `int` (or a Coffee slice of int).
pub fn llvm_basic_to_coffee(type_: BasicTypeEnum<'_>) -> Result<String, String> {
    match type_ {
        BasicTypeEnum::IntType(t) => {
            let width = t.get_bit_width();
            Ok(match width {
                1 => "i1".to_string(),
                8 => "int(1)".to_string(),
                16 => "int(2)".to_string(),
                32 => "int(4)".to_string(),
                64 => "int(8)".to_string(),
                128 => "int(16)".to_string(),
                _ => format!("int({})", width / 8),
            })
        }
        BasicTypeEnum::FloatType(t) => {
            let width = t.get_bit_width();
            Ok(match width {
                32 => "float(4)".to_string(),
                64 => "float(8)".to_string(),
                _ => format!("float({})", width / 8),
            })
        }
        BasicTypeEnum::PointerType(_) => Ok("ptr".to_string()),
        BasicTypeEnum::ArrayType(t) => {
            let len = t.len();
            let element = llvm_basic_to_coffee(t.get_element_type())?;
            Ok(format!("[{}; {}]", element, len))
        }
        BasicTypeEnum::StructType(t) => {
            if t.is_packed() {
                Ok("packed struct".to_string())
            } else {
                Ok("struct".to_string())
            }
        }
        BasicTypeEnum::VectorType(_) => {
            Err("unsupported LLVM type: vector".to_string())
        }
        BasicTypeEnum::ScalableVectorType(_) => {
            Err("unsupported LLVM type: scalable vector".to_string())
        }
    }
}

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
    /// Interned tuple LLVM structs keyed by mapped field-type debug names
    tuple_types: std::cell::RefCell<HashMap<String, StructType<'ctx>>>,
    /// Whether to use C type mapping (for C ABI compatibility)
    c_abi: bool,
    /// Complete C unions: LLVM `[i8 x size]` overlay storage.
    pub(crate) c_union_byte_sizes: HashMap<String, u32>,
    /// C enums: integer ABI (`i32`).
    pub(crate) c_enum_names: HashSet<String>,
    /// Complete `c class` names: LLVM struct by value (not Coffee heap ptr).
    pub(crate) c_struct_names: HashSet<String>,
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
            tuple_types: std::cell::RefCell::new(HashMap::new()),
            c_abi: false,
            c_union_byte_sizes: HashMap::new(),
            c_enum_names: HashSet::new(),
            c_struct_names: HashSet::new(),
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
            tuple_types: std::cell::RefCell::new(HashMap::new()),
            c_abi: true,
            c_union_byte_sizes: HashMap::new(),
            c_enum_names: HashSet::new(),
            c_struct_names: HashSet::new(),
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
    /// - int (8-byte default) → i64 / long long
    /// - float, float(8) → double
    /// - float(4) → float
    /// - bool → i8 (_Bool)
    /// - str / object → ptr (const char* / void*)
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
            CoffeeType::String | CoffeeType::Variadic => {
                self.context.ptr_type(AddressSpace::default()).into()
            }
            CoffeeType::NamedType { name } if name == "buf" => {
                self.context.ptr_type(AddressSpace::default()).into()
            }

            // Pointers and arrays. C has no Coffee `[T]` fat-pointer ABI.
            CoffeeType::Ref { .. } | CoffeeType::Array { .. } | CoffeeType::Slice(_) =>
                self.context.ptr_type(AddressSpace::default()).into(),

            _ => return None,
        })
    }

    /// Coffee type string → LLVM, without silent ptr/i64/f32 fallbacks.
    pub fn try_map_type(&self, type_str: &str) -> Result<BasicTypeEnum<'ctx>, String> {
        let coffee_type = crate::types::type_from_str(type_str).map_err(|e| {
            format!("cannot map Coffee type '{type_str}' to LLVM: {e}")
        })?;
        self.try_map_unified_type(&coffee_type)
    }

    /// Coffee `[T]` fat pointer: `{ ptr, i64 len }`.
    pub fn slice_fat_type(&self) -> StructType<'ctx> {
        self.context.struct_type(
            &[
                self.context.ptr_type(AddressSpace::default()).into(),
                self.context.i64_type().into(),
            ],
            false,
        )
    }

    /// Unified Coffee `Type` → LLVM. Unknown integer/float widths are `Err`.
    /// Named classes, `str`, refs, arrays, and fn types map to opaque ptr (ABI).
    /// `[T]` maps to `{ ptr, i64 }` (not C ABI).
    pub fn try_map_unified_type(&self, coffee_type: &CoffeeType) -> Result<BasicTypeEnum<'ctx>, String> {
        if self.c_abi {
            if let Some(llvm_type) = self.map_c_type(coffee_type) {
                return Ok(llvm_type);
            }
        }

        let ptr = || self.context.ptr_type(AddressSpace::default()).into();

        match coffee_type {
            CoffeeType::Int { bits, .. } => match bits {
                8 => Ok(self.context.i8_type().into()),
                16 => Ok(self.context.i16_type().into()),
                32 => Ok(self.context.i32_type().into()),
                64 => Ok(self.context.i64_type().into()),
                128 => Ok(self.context.i128_type().into()),
                _ => Err(format!(
                    "cannot map Coffee type '{coffee_type}' to LLVM: unsupported integer width {bits} bits"
                )),
            },
            CoffeeType::Float { bits } => match bits {
                32 => Ok(self.context.f32_type().into()),
                64 => Ok(self.context.f64_type().into()),
                _ => Err(format!(
                    "cannot map Coffee type '{coffee_type}' to LLVM: unsupported float width {bits} bits"
                )),
            },
            CoffeeType::Variadic => Ok(ptr()),
            CoffeeType::Bool => Ok(self.context.i8_type().into()),
            CoffeeType::String => Ok(ptr()),
            CoffeeType::Unit => Ok(self.context.i8_type().into()),
            CoffeeType::Void => Ok(self.context.i8_type().into()),
            CoffeeType::Tuple(types) => {
                let mut field_types = Vec::with_capacity(types.len());
                for t in types {
                    field_types.push(self.try_map_unified_type(t)?);
                }
                let key = format!("{:?}", types);
                if let Some(st) = self.tuple_types.borrow().get(&key).copied() {
                    return Ok(st.into());
                }
                let st = self.context.struct_type(&field_types, false);
                self.tuple_types.borrow_mut().insert(key, st);
                Ok(st.into())
            }
            CoffeeType::Slice(_) => Ok(self.slice_fat_type().into()),
            CoffeeType::Array { .. }
            | CoffeeType::Ref { .. }
            | CoffeeType::Function { .. }
            | CoffeeType::App { .. } => Ok(ptr()),
            CoffeeType::NamedType { name } => {
                if self.c_struct_names.contains(name) {
                    if let Some(st) = self.struct_types.get(name) {
                        return Ok((*st).into());
                    }
                }
                if let Some(&size) = self.c_union_byte_sizes.get(name) {
                    return Ok(self.context.i8_type().array_type(size).into());
                }
                if self.c_enum_names.contains(name) {
                    return Ok(self.context.i32_type().into());
                }
                Ok(ptr())
            }
        }
    }

    /// Coffee `Type` → LLVM using the same table as codegen (`try_map_unified_type`).
    /// Drop/clone must not keep a second match.
    pub fn llvm_abi_of(context: &'ctx Context, ty: &CoffeeType) -> BasicTypeEnum<'ctx> {
        TypeMapper::new(context)
            .try_map_unified_type(ty)
            .unwrap_or_else(|e| panic!("llvm_abi_of {ty}: {e}"))
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
    /// * `Ok(StructType)` - The created or cached struct type
    /// * `Err(String)` - If a field type cannot be mapped to LLVM
    pub fn get_or_create_struct_type_from_class(
        &mut self,
        class_name: &str,
        class: &crate::parser::class::ClassDef,
        all_classes: &std::collections::HashMap<String, crate::parser::class::ClassDef>,
    ) -> Result<StructType<'ctx>, String> {
        // Check cache first
        if let Some(&struct_type) = self.struct_types.get(class_name) {
            return Ok(struct_type);
        }

        let flattened = super::class_layout::flatten_class_fields(class, all_classes);
        let mut field_types = Vec::new();
        let mut field_names = Vec::new();
        for field in &flattened {
            let mapped = self.try_map_type(&field.field_type).map_err(|e| {
                format!(
                    "cannot map field '{}' of class '{}': {e}",
                    field.name, class_name
                )
            })?;
            field_types.push(mapped);
            field_names.push(field.name.clone());
        }

        // Opaque then a single set_body: parent fields, then self, declaration order.
        // compile_class must not call set_body again with a packed/reordered permutation.
        let struct_type = self.context.opaque_struct_type(class_name);
        struct_type.set_body(&field_types, class.packed);

        // Cache the struct type and field names
        self.struct_types.insert(class_name.to_string(), struct_type);
        self.struct_fields.insert(class_name.to_string(), field_names);

        Ok(struct_type)
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
        coffee_debug!("DEBUG: get_field_index: struct_name='{}', field_name='{}'", struct_name, field_name);
        coffee_debug!("DEBUG: get_field_index: struct_fields.keys()={:?}", self.struct_fields.keys().collect::<Vec<_>>());
        if let Some(fields) = self.struct_fields.get(struct_name) {
            coffee_debug!("DEBUG: get_field_index: struct '{}' has fields: {:?}", struct_name, fields);
            let result = fields.iter().position(|name| name == field_name);
            coffee_debug!("DEBUG: get_field_index: position result={:?}", result);
            result
        } else {
            coffee_debug!("DEBUG: get_field_index: struct '{}' not found in struct_fields", struct_name);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{llvm_basic_to_coffee, TypeMapper};
    use crate::types::type_from_str;
    use inkwell::context::Context;
    use inkwell::types::BasicTypeEnum;

    #[test]
    fn try_map_type_rejects_unparsable_coffee_type_instead_of_ptr() {
        let context = Context::create();
        let mapper = TypeMapper::new(&context);
        let err = mapper
            .try_map_type("List<int>")
            .expect_err("generics are not Coffee types");
        assert!(
            !err.contains("ptr") && !err.eq_ignore_ascii_case("int"),
            "error must not look like a silent ptr/int fallback: {err}"
        );
        assert!(
            err.contains("List<int>") || err.contains("generic") || err.contains("parse"),
            "error should mention the bad type: {err}"
        );
    }

    #[test]
    fn try_map_type_rejects_unsupported_int_width_instead_of_i64() {
        let context = Context::create();
        let mapper = TypeMapper::new(&context);
        let err = mapper
            .try_map_type("int(3)+")
            .expect_err("24-bit int is not a mapped LLVM integer");
        assert!(
            !err.contains("i64") && !err.eq_ignore_ascii_case("int"),
            "must not silently map to i64/int: {err}"
        );
    }

    #[test]
    fn try_map_type_maps_plain_int() {
        let context = Context::create();
        let mapper = TypeMapper::new(&context);
        let ty = mapper.try_map_type("int").expect("int is valid");
        assert!(ty.is_int_type());
        assert_eq!(ty.into_int_type().get_bit_width(), 64);
    }

    #[test]
    fn try_map_type_rejects_unsupported_float_width_instead_of_f32() {
        let context = Context::create();
        let mapper = TypeMapper::new(&context);
        let err = mapper
            .try_map_type("float(3)")
            .expect_err("24-bit float is not a mapped LLVM float");
        assert!(
            !err.contains("f32") && !err.contains("float(4)"),
            "must not silently map to f32: {err}"
        );
    }

    #[test]
    fn try_map_type_rejects_unsupported_int_width_inside_tuple() {
        let context = Context::create();
        let mapper = TypeMapper::new(&context);
        let err = mapper
            .try_map_type("(int(3)+, int)")
            .expect_err("tuple field int(3)+ must not fall through to i64");
        assert!(
            !err.contains("i64"),
            "must not silently map tuple field to i64: {err}"
        );
    }

    #[test]
    fn try_map_type_maps_named_class_and_str_to_ptr() {
        let context = Context::create();
        let mapper = TypeMapper::new(&context);
        for ty in ["Point", "str", "object", "buf"] {
            let llvm = mapper.try_map_type(ty).unwrap_or_else(|e| panic!("{ty}: {e}"));
            assert!(llvm.is_pointer_type(), "{ty} ABI is pointer, got {llvm:?}");
        }
        let c_abi = TypeMapper::with_c_abi(&context);
        let obj = c_abi.try_map_type("object").expect("C object");
        assert!(obj.is_pointer_type(), "C object is ptr, not i8/i64");
        let buf = c_abi.try_map_type("buf").expect("C buf");
        assert!(buf.is_pointer_type(), "C buf is ptr");
    }

    #[test]
    fn c_abi_strlen_matches_libc_int4_not_invented_i64() {
        let context = Context::create();
        let ty = crate::backend::functions::c_abi_fn_type(&context, "int(4)+", &["string"], false)
            .expect("strlen");
        let ret = ty.get_return_type().expect("strlen returns int").into_int_type();
        assert_eq!(ret.get_bit_width(), 32, "libc.cfc strlen => int(4)+");
    }

    #[test]
    fn try_map_type_maps_slice_to_fat_pointer() {
        let context = Context::create();
        let mapper = TypeMapper::new(&context);
        let llvm = mapper.try_map_type("[int]").expect("[int] maps");
        let st = llvm.into_struct_type();
        assert_eq!(st.count_fields(), 2, "fat pointer is {{ ptr, i64 }}");
        assert!(st.get_field_type_at_index(0).unwrap().is_pointer_type());
        let len = st.get_field_type_at_index(1).unwrap().into_int_type();
        assert_eq!(len.get_bit_width(), 64);
    }

    #[test]
    fn struct_from_class_rejects_unparsable_field_instead_of_ptr() {
        use crate::parser::class::{ClassDef, ClassField};
        use std::collections::HashMap;

        let context = Context::create();
        let mut mapper = TypeMapper::new(&context);
        let class = ClassDef {
            type_params: vec![],
            name: "Bad".to_string(),
            parent: None,
            fields: vec![ClassField {
                name: "xs".to_string(),
                field_type: "List<int>".to_string(),
                bit_width: None,
            }],
            methods: vec![],
            packed: false,
            has_constructor: false,
        };
        let all = HashMap::new();
        let err = mapper
            .get_or_create_struct_type_from_class("Bad", &class, &all)
            .expect_err("unparsable field type must not become an LLVM ptr member");
        assert!(
            err.contains("List<int>") || err.contains("generic") || err.contains("parse"),
            "error should mention the unmappable field type: {err}"
        );
        assert!(
            !mapper.struct_types.contains_key("Bad"),
            "failed mapping must not cache a partial LLVM struct"
        );
    }

    #[test]
    fn llvm_basic_to_coffee_does_not_map_vector_to_int() {
        let context = Context::create();
        let vec_ty: BasicTypeEnum = context.i32_type().vec_type(4).into();
        let err = llvm_basic_to_coffee(vec_ty).expect_err("SIMD vector is not a Coffee type");
        assert_ne!(err, "int");
        assert!(!err.starts_with("int("), "vector must not stringify as int: {err}");
        assert!(
            type_from_str(&err)
                .ok()
                .is_none_or(|t| !matches!(t, crate::types::Type::Int { .. })),
            "error text must not parse as Coffee int: {err}"
        );
    }

    #[test]
    fn llvm_basic_to_coffee_does_not_map_scalable_vector_to_int() {
        let context = Context::create();
        let vec_ty: BasicTypeEnum = context.i32_type().scalable_vec_type(4).into();
        let err = llvm_basic_to_coffee(vec_ty).expect_err("scalable vector is not a Coffee type");
        assert_ne!(err, "int");
        assert!(!err.starts_with("int("), "scalable vector must not stringify as int: {err}");
    }

    #[test]
    fn llvm_basic_to_coffee_maps_i64() {
        let context = Context::create();
        let ty: BasicTypeEnum = context.i64_type().into();
        assert_eq!(llvm_basic_to_coffee(ty).expect("i64"), "int(8)");
    }
}
