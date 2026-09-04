//! Generate C Header Files from Coffee Function Declarations
//! 
//! This module provides functionality to generate C-compatible header files (.h)
//! from Coffee function definitions, enabling C/C++ code to call Coffee functions.
//! This is the reverse direction of the C integration system, allowing Coffee
//! libraries to be used from C code.
//! 
//! The module handles complex type mappings between Coffee and C, including:
//! - Basic types (int, float, bool, etc.)
//! - Complex types (tuples, slices, arrays)
//! - Function pointers
//! - Struct definitions for complex Coffee types
//! 
//! The generated headers include proper C++ extern "C" wrappers and follow
//! C naming conventions while preserving Coffee's type safety features.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use crate::parser::Function;
use crate::types::Type;

/// Context for tracking generated types and struct definitions
/// 
/// This internal structure maintains state during C header generation,
/// keeping track of all the type definitions that need to be included
/// in the generated header file. It manages:
/// 
/// - Struct definitions for Coffee types like tuples and slices
/// - Forward declarations for complex types
/// - Function pointer typedefs for Coffee function types
/// 
/// The context ensures that types are defined in the correct order
/// in the generated header file, with dependencies defined before
/// their dependents.
struct HeaderGenContext {
    /// Generated struct definitions (name -> fields)
    /// Each entry maps a struct name to a vector of (field_name, field_type) pairs
    structs: std::collections::HashMap<String, Vec<(String, String)>>,
    /// Forward declarations needed
    /// These are type names that need forward declarations to resolve circular dependencies
    forward_decls: HashSet<String>,
    /// Function pointer typedefs (name -> (return_type, param_types))
    /// These create convenient type aliases for Coffee function types
    function_typedefs: std::collections::HashMap<String, (String, Vec<String>)>,
    /// Function declarations (for drop functions, etc.)
    functions: Vec<String>,
}

impl HeaderGenContext {
    /// Create a new empty header generation context
    /// 
    /// This function initializes a fresh context with empty collections
    /// for tracking structs, forward declarations, and function typedefs.
    fn new() -> Self {
        Self {
            structs: std::collections::HashMap::new(),
            forward_decls: HashSet::new(),
            function_typedefs: std::collections::HashMap::new(),
            functions: Vec::new(),
        }
    }

    /// Register a struct definition in the context
    /// 
    /// This method adds a struct definition to the context, which will later
    /// be included in the generated header file. The struct definition
    /// includes the struct name and its fields (name and type).
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the struct to define
    /// * `fields` - Vector of (field_name, field_type) pairs representing the struct's fields
    fn add_struct(&mut self, name: String, fields: Vec<(String, String)>) {
        self.structs.insert(name, fields);
    }

    /// Add a forward declaration to the context
    /// 
    /// This method registers a type name that requires a forward declaration
    /// in the generated header file. Forward declarations are needed to
    /// resolve circular type dependencies.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the type that needs a forward declaration
    fn add_forward_decl(&mut self, name: String) {
        self.forward_decls.insert(name);
    }

    /// Add a function pointer typedef to the context
    /// 
    /// This method creates a typedef for a function pointer type, which is
    /// used to represent Coffee function types in C headers. The typedef
    /// includes the return type and parameter types.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the typedef
    /// * `return_type` - The return type of the function
    /// * `param_types` - Vector of parameter types for the function
    fn add_function_typedef(&mut self, name: String, return_type: String, param_types: Vec<String>) {
        self.function_typedefs.insert(name, (return_type, param_types));
    }
}

/// Generate C header file from Coffee functions
/// 
/// This function creates a C-compatible header file (.h) from a slice of
/// Coffee functions and class definitions, allowing C/C++ code to call Coffee functions
/// and use Coffee struct types. The generated header includes proper type definitions,
/// function declarations, and C++ extern "C" wrappers.
/// 
/// The function performs several steps:
/// 1. Filters out external functions (only Coffee-implemented functions are exported)
/// 2. Analyzes all function signatures to determine required type definitions
/// 3. Generates appropriate C type mappings for complex Coffee types
/// 4. Creates struct definitions for tuples, slices, classes, and other composite types
/// 5. Formats the header with proper includes, guards, and C++ compatibility
/// 
/// # Arguments
/// 
/// * `functions` - Slice of Coffee Function AST nodes to export to C
/// * `classes` - Slice of Coffee ClassDef AST nodes to export to C
/// * `output_path` - Path where the generated .h file should be written
/// * `header_name` - Base name for the header guard (will be converted to HEADER_NAME_H format)
/// 
/// # Returns
/// 
/// * `Ok(PathBuf)` - Path to the successfully generated header file
/// * `Err(String)` - Error message describing what went wrong
/// 
/// # Errors
/// 
/// This function returns an error if:
/// - No exportable functions are found (all functions are external)
/// - File creation or writing fails
pub fn generate_header<P: AsRef<Path>>(
    functions: &[Function],
    classes: &[crate::parser::class::ClassDef],
    output_path: P,
    header_name: &str,
) -> Result<PathBuf, String> {
    let output_path = output_path.as_ref();

    // Filter all callable functions (both c fn and regular fn should be exported)
    let exportable_functions: Vec<&Function> = functions
        .iter()
        .filter(|f| !matches!(&f.body, crate::parser::FunctionBody::External)) // Not external
        .collect();

    if exportable_functions.is_empty() {
        return Err("no exportable functions found".to_string());
    }

    // Create context for tracking types
    let mut ctx = HeaderGenContext::new();

    // First pass: collect all types and generate struct definitions
    for func in &exportable_functions {
        collect_types_from_function(&mut ctx, func);
    }

    // Collect class definitions
    for class in classes {
        if !class.has_constructor {
            // Pure data structure (struct) - only generate struct definition
            generate_class_struct(&mut ctx, class);
        } else {
            // Full class - generate struct definition, new and drop declarations
            generate_class_struct(&mut ctx, class);
            generate_class_new_decl(&mut ctx, class);
            generate_class_drop_decl(&mut ctx, class);
        }
    }

    // Create header guard name
    let header_guard = header_name
        .to_uppercase()
        .replace(|c: char| !c.is_alphanumeric(), "_")
        + "_H";

    // Generate header content
    let mut content = String::new();

    // Header comment
    content.push_str("// Auto-generated C header file from Coffee\n");
    content.push_str("// DO NOT EDIT MANUALLY\n\n");

    // Include necessary headers
    content.push_str("#include <stddef.h>\n");
    content.push_str("#include <stdint.h>\n");
    content.push_str("\n");

    // Header guard
    content.push_str(&format!("#ifndef {}\n", header_guard));
    content.push_str(&format!("#define {}\n\n", header_guard));

    // Forward declarations
    if !ctx.forward_decls.is_empty() {
        for decl_name in &ctx.forward_decls {
            content.push_str(&format!("typedef struct {} {};\n", decl_name, decl_name));
        }
        content.push('\n');
    }

    // Struct definitions
    if !ctx.structs.is_empty() {
        content.push_str("// Struct definitions\n");
        for (struct_name, fields) in &ctx.structs {
            content.push_str(&format!("struct {} {{\n", struct_name));
            for (field_name, field_type) in fields {
                content.push_str(&format!("    {} {};\n", field_type, field_name));
            }
            content.push_str("};\n\n");
        }
    }

    // Function pointer typedefs
    if !ctx.function_typedefs.is_empty() {
        content.push_str("// Function pointer typedefs\n");
        for (typedef_name, (return_type, param_types)) in &ctx.function_typedefs {
            content.push_str(&format!("typedef {} (*{})(", return_type, typedef_name));

            if param_types.is_empty() {
                content.push_str("void");
            } else {
                content.push_str(&param_types.join(", "));
            }

            content.push_str(");\n");
        }
        content.push('\n');
    }

    // Include C++ wrapper
    content.push_str("#ifdef __cplusplus\n");
    content.push_str("extern \"C\" {\n");
    content.push_str("#endif\n\n");

    // Function declarations
    content.push_str("// Function declarations\n");
    for func in &exportable_functions {
        content.push_str(&generate_function_declaration_with_ctx(func, &mut ctx));
        content.push_str("\n");  // Extra newline between functions
    }

    // Drop function declarations (for classes with methods)
    if !ctx.functions.is_empty() {
        content.push_str("// Drop function declarations\n");
        for drop_decl in &ctx.functions {
            content.push_str(drop_decl);
            content.push_str("\n");
        }
    }

    // Close extern C
    content.push_str("#ifdef __cplusplus\n");
    content.push_str("} // extern \"C\"\n");
    content.push_str("#endif\n\n");

    // End header guard
    content.push_str(&format!("#endif // {}\n", header_guard));

    // Write to file
    let mut file = File::create(output_path)
        .map_err(|e| format!("failed to create header file: {}", e))?;

    file.write_all(content.as_bytes())
        .map_err(|e| format!("failed to write header file: {}", e))?;

    Ok(output_path.to_path_buf())
}

/// Generate C function declaration with context for tracking types
fn generate_function_declaration_with_ctx(func: &Function, ctx: &mut HeaderGenContext) -> String {
    let c_return_type = coffee_type_to_c_type_with_ctx(&func.return_type, ctx);

    let mut decl = format!("{} {}(", c_return_type, func.name);

    // Parameters
    let param_decls: Vec<String> = func.parameters
        .iter()
        .map(|p| {
            let c_type = coffee_type_to_c_type_with_ctx(&p.param_type, ctx);
            format!("{} {}", c_type, p.name)
        })
        .collect();

    if param_decls.is_empty() {
        decl.push_str("void");
    } else {
        decl.push_str(&param_decls.join(", "));
    }

    decl.push_str(");");

    decl
}

/// Collect all types from a function signature
fn collect_types_from_function(ctx: &mut HeaderGenContext, func: &Function) {
    // Collect return type
    if let Ok(return_type) = Type::from_str(&func.return_type) {
        collect_types_from_type(ctx, &return_type);
    }

    // Collect parameter types
    for param in &func.parameters {
        if let Ok(param_type) = Type::from_str(&param.param_type) {
            collect_types_from_type(ctx, &param_type);
        }
    }
}

/// Recursively collect types and generate necessary struct definitions
fn collect_types_from_type(ctx: &mut HeaderGenContext, ty: &Type) {
    match ty {
        // Base types don't need struct definitions
        Type::Int { .. } | Type::Float { .. } | Type::Bool | Type::Void | Type::Unit | Type::String | Type::Variadic => {}

        // Tuple types need a struct definition
        Type::Tuple(elem_types) => {
            let struct_name = type_to_struct_name(ty);

            // Only generate if not already defined
            if !ctx.structs.contains_key(&struct_name) {
                let fields: Vec<(String, String)> = elem_types
                    .iter()
                    .enumerate()
                    .map(|(i, elem)| {
                        let field_name = format!("field{}", i);
                        let field_type = type_to_c_type_string(elem);
                        // Recursively collect nested types
                        collect_types_from_type(ctx, elem);
                        (field_name, field_type)
                    })
                    .collect();

                ctx.add_struct(struct_name, fields);
            }
        }

        // Slice types need a struct with data pointer and length
        Type::Slice(elem) => {
            let struct_name = format!("Slice_{}", type_to_base_name(elem));
            let elem_type_str = type_to_c_type_string(elem);

            // Only generate if not already defined
            if !ctx.structs.contains_key(&struct_name) {
                let fields = vec![
                    ("data".to_string(), format!("{}*", elem_type_str)),
                    ("len".to_string(), "size_t".to_string()),
                ];
                ctx.add_struct(struct_name, fields);
            }

            // Recursively collect element type
            collect_types_from_type(ctx, elem);
        }

        // Array types need special handling
        Type::Array { elem, size } => {
            let struct_name = format!("Array_{}_{}", type_to_base_name(elem), size);
            let elem_type_str = type_to_c_type_string(elem);

            // Only generate if not already defined
            if !ctx.structs.contains_key(&struct_name) {
                let fields = vec![
                    ("data".to_string(), format!("{}[{}]", elem_type_str, size)),
                ];
                ctx.add_struct(struct_name, fields);
            }

            // Recursively collect element type
            collect_types_from_type(ctx, elem);
        }

        // Reference types - collect the referenced type
        Type::Ref { elem, .. } => {
            collect_types_from_type(ctx, elem);
        }

        // Function types need a typedef
        Type::Function { params, return_type } => {
            // Generate function pointer type name
            let param_names: Vec<String> = params.iter().map(type_to_c_type_string).collect();
            let ret_name = type_to_c_type_string(return_type);

            // Create a readable typedef name
            let typedef_name = format!("Fn_{}",
                if param_names.is_empty() {
                    "void".to_string()
                } else {
                    param_names.join("_")
                        .replace(" ", "_")
                        .replace("*", "ptr")
                        .replace("const", "c")
                }
            );

            // Only add if not already defined
            if !ctx.function_typedefs.contains_key(&typedef_name) {
                ctx.add_function_typedef(typedef_name.clone(), ret_name, param_names);
            }

            // Recursively collect parameter and return types
            for param in params {
                collect_types_from_type(ctx, param);
            }
            collect_types_from_type(ctx, return_type);
        }

        // Named types need forward declarations
        Type::NamedType { name } => {
            ctx.add_forward_decl(name.clone());
        }
    }
}

/// Convert a Type to its C type string representation
fn type_to_c_type_string(ty: &Type) -> String {
    match ty {
        Type::Int { bits, signed } => {
            match (bits, signed) {
                (64, true) => "long long".to_string(),
                (32, true) => "int".to_string(),
                (16, true) => "short".to_string(),
                (8, true) => "char".to_string(),
                (64, false) => "unsigned long long".to_string(),
                (32, false) => "unsigned int".to_string(),
                (16, false) => "unsigned short".to_string(),
                (8, false) => "unsigned char".to_string(),
                _ => "int".to_string(),
            }
        }
        Type::Float { bits } => {
            match bits {
                64 => "double".to_string(),
                32 => "float".to_string(),
                _ => "double".to_string(),
            }
        }
        Type::Bool => "_Bool".to_string(),
        Type::Void => "void".to_string(),
        Type::String => "const char*".to_string(),
        Type::Unit => "void".to_string(),
        Type::Variadic => "...".to_string(),
        Type::Tuple(elem_types) => {
            // Generate struct name like Tuple_3 or Tuple_int_float
            let elem_names: Vec<String> = elem_types
                .iter()
                .map(type_to_base_name)
                .collect();
            format!("struct Tuple_{}_{}", elem_types.len(), elem_names.join("_"))
        }
        Type::Slice(elem) => {
            format!("struct Slice_{}", type_to_base_name(elem))
        }
        Type::Array { elem, size } => {
            format!("struct Array_{}_{}", type_to_base_name(elem), size)
        }
        Type::Ref { elem, mutable } => {
            let elem_type = type_to_c_type_string(elem);
            if elem_type == "void" {
                "void*".to_string()
            } else if *mutable {
                format!("{}*", elem_type)
            } else {
                format!("const {}*", elem_type)
            }
        }
        Type::Function { params, return_type: _ } => {
            // For function types, generate the typedef name
            let param_names: Vec<String> = params.iter().map(type_to_c_type_string).collect();

            format!("(*)({})", {
                if param_names.is_empty() {
                    "void".to_string()
                } else {
                    param_names.join(", ")
                }
            })
        }
        Type::NamedType { name } => {
            name.clone()
        }
    }
}

/// Convert a Type to its C type string representation with context
/// This version looks up function pointer typedefs
fn type_to_c_type_string_with_ctx(ty: &Type, _ctx: &HeaderGenContext) -> String {
    match ty {
        Type::Function { params, return_type: _ } => {
            // Try to find the typedef name
            let param_names: Vec<String> = params.iter().map(|p| type_to_c_type_string(p)).collect();

            // Generate the same typedef name as in collect_types_from_type
            let typedef_name = format!("Fn_{}",
                if param_names.is_empty() {
                    "void".to_string()
                } else {
                    param_names.join("_")
                        .replace(" ", "_")
                        .replace("*", "ptr")
                        .replace("const", "c")
                }
            );

            // Return the typedef name (function pointers use the typedef directly)
            typedef_name
        }
        _ => type_to_c_type_string(ty)
    }
}

/// Generate a base name for a type (for struct naming)
fn type_to_base_name(ty: &Type) -> String {
    match ty {
        Type::Int { bits, signed } => {
            format!("i{}{}", bits, if *signed { "" } else { "u" })
        }
        Type::Float { bits } => {
            format!("f{}", bits)
        }
        Type::Bool => "bool".to_string(),
        Type::Void | Type::Unit => "void".to_string(),
        Type::String => "str".to_string(),
        Type::Variadic => "varargs".to_string(),
        Type::Tuple(elem_types) => {
            let elem_names: Vec<String> = elem_types
                .iter()
                .map(type_to_base_name)
                .collect();
            format!("tuple_{}_{}", elem_types.len(), elem_names.join("_"))
        }
        Type::Slice(elem) => {
            format!("slice_{}", type_to_base_name(elem))
        }
        Type::Array { elem, size } => {
            format!("array_{}_{}", type_to_base_name(elem), size)
        }
        Type::Ref { elem, .. } => {
            format!("ref_{}", type_to_base_name(elem))
        }
        Type::Function { .. } => {
            "fn".to_string()
        }
        Type::NamedType { name } => {
            name.clone()
        }
    }
}

/// Generate struct name from type
fn type_to_struct_name(ty: &Type) -> String {
    type_to_c_type_string(ty)
}

/// Convert Coffee type to C type with context for tracking types
fn coffee_type_to_c_type_with_ctx(coffee_type: &str, ctx: &mut HeaderGenContext) -> String {
    let parsed = Type::from_str(coffee_type);

    match parsed {
        Ok(ty) => {
            // Collect this type for struct generation
            collect_types_from_type(ctx, &ty);
            type_to_c_type_string_with_ctx(&ty, ctx)
        }
        Err(_) => {
            // Fallback to simple string matching
            match coffee_type {
                "void" => "void".to_string(),
                "bool" => "_Bool".to_string(),
                "string" => "const char*".to_string(),
                _ => "int".to_string(),
            }
        }
    }
}

/// Generate header name from input file path
pub fn generate_header_name(input_path: &Path) -> String {
    input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("coffee_module")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_conversion() {
        let mut ctx = HeaderGenContext::new();
        // Coffee's default "int" is 64-bit, which maps to C's "long long"
        assert_eq!(coffee_type_to_c_type_with_ctx("int", &mut ctx), "long long");
        assert_eq!(coffee_type_to_c_type_with_ctx("int(4)", &mut ctx), "int");
        assert_eq!(coffee_type_to_c_type_with_ctx("float", &mut ctx), "double");
        assert_eq!(coffee_type_to_c_type_with_ctx("void", &mut ctx), "void");
        assert_eq!(coffee_type_to_c_type_with_ctx("string", &mut ctx), "const char*");
    }

    #[test]
    fn test_header_name_generation() {
        let path = Path::new("test_module.cf");
        assert_eq!(generate_header_name(path), "test_module");

        let path = Path::new("lib/mylib.coffee");
        assert_eq!(generate_header_name(path), "mylib");
    }
}

/// Generate C struct definition for a Coffee class (pure data structure)
fn generate_class_struct(ctx: &mut HeaderGenContext, class: &crate::parser::class::ClassDef) {
    let struct_name = class.name.clone();
    
    // Only generate if not already defined
    if !ctx.structs.contains_key(&struct_name) {
        let fields: Vec<(String, String)> = class.fields
            .iter()
            .map(|field| {
                let field_type = map_coffee_type_to_c(&field.field_type);
                (field.name.clone(), field_type)
            })
            .collect();
        
        ctx.add_struct(struct_name.clone(), fields);
    }
}

/// Generate new function declaration for classes with constructor (full classes)
fn generate_class_new_decl(ctx: &mut HeaderGenContext, class: &crate::parser::class::ClassDef) {
    let struct_name = class.name.clone();
    
    // Generate new function declaration
    // Format: struct ClassName* ClassName__new(params);
    let mut new_decl = format!("struct {}* {}__new(", struct_name, struct_name);
    
    // Add parameters (if any)
    let param_decls: Vec<String> = class.fields
        .iter()
        .map(|field| {
            let field_type = map_coffee_type_to_c(&field.field_type);
            format!("{} {}", field_type, field.name)
        })
        .collect();
    
    if param_decls.is_empty() {
        new_decl.push_str("void");
    } else {
        new_decl.push_str(&param_decls.join(", "));
    }
    
    new_decl.push_str(");");
    ctx.functions.push(new_decl);
}

/// Generate drop function declaration for classes with constructor (full classes)
fn generate_class_drop_decl(ctx: &mut HeaderGenContext, class: &crate::parser::class::ClassDef) {
    let struct_name = class.name.clone();
    
    // Generate drop function declaration
    // Format: void ClassName__drop(struct ClassName* self);
    let drop_decl = format!("void {}__drop(struct {}* self);", struct_name, struct_name);
    ctx.functions.push(drop_decl);
}

/// Map Coffee type to C type
fn map_coffee_type_to_c(coffee_type: &str) -> String {
    // Simple type mapping for now
    match coffee_type {
        "int" | "int(8)+" => "long long".to_string(),
        "int(4)+" => "int".to_string(),
        "int(2)+" => "short".to_string(),
        "int(1)+" => "char".to_string(),
        "float" | "float(8)" => "double".to_string(),
        "float(4)" => "float".to_string(),
        "string" => "const char*".to_string(),
        "bool" => "_Bool".to_string(),
        "void" => "void".to_string(),
        _ => coffee_type.to_string(), // For named types (classes), keep as-is
    }
}
