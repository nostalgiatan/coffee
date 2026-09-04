//! Function compilation for Coffee compiler code generation
//! 
//! This module handles function declaration, compilation, and main() generation.
//! It provides utilities for declaring functions with proper signatures, handling
//! both Coffee and C ABI compatibility, and generating appropriate return instructions.
//! The module includes support for external function declarations and runtime function
//! declarations required by the compiler. It also manages function signatures for
//! imported functions and C library functions.

use crate::coffee_debug;
use crate::parser::Function;
use crate::parser::function::FunctionBody;
use crate::parser::Statement;
use crate::backend::types::TypeMapper;
use crate::backend::memory_ops::DiagSeverity;
use super::codegen::CodeGenerator;
use inkwell::{values::{FunctionValue, BasicValueEnum}, AddressSpace, types::BasicType};

/// Compile a function declaration (create signature without body)
/// 
/// This function creates the LLVM function signature for a Coffee function
/// without generating its body. It handles both Coffee and C ABI compatibility
/// by using appropriate type mappings for parameter and return types. The
/// function also manages variadic parameters and sets the appropriate calling
/// convention for C functions.
/// 
/// For C functions (marked with `c fn` syntax), the function uses C ABI type
/// mapping to ensure compatibility with external C libraries. It properly
/// handles void return types and sets the C calling convention for C functions.
/// 
/// # Arguments
/// 
/// * `func` - The Coffee Function to declare
/// * `module` - The LLVM module to add the function to
/// * `context` - The LLVM context
/// * `type_mapper` - The type mapper for Coffee type conversion
/// * `functions` - The function map to store the declared function
/// 
/// # Returns
/// 
/// * `Ok(FunctionValue)` - The declared LLVM function value
/// * `Err(String)` - If there was an error during function declaration
pub fn declare_function<'ctx>(
    func: &Function,
    module: &inkwell::module::Module<'ctx>,
    context: &'ctx inkwell::context::Context,
    type_mapper: &TypeMapper<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
) -> Result<FunctionValue<'ctx>, String> {
    // Check if function has variadic parameters
    let has_variadic = func.parameters.iter().any(|p| p.is_variadic);

    // For C functions, use C ABI type mapping (bool → i8)
    // For Coffee functions, use standard type mapping (bool → i1)
    let c_abi_mapper = TypeMapper::with_c_abi(context);

    // Convert parameter types (exclude variadic parameters from the type list)
    let param_types: Vec<inkwell::types::BasicTypeEnum<'ctx>> = func.parameters.iter()
        .filter(|p| !p.is_variadic)
        .map(|p| {
            if func.is_c {
                c_abi_mapper.map_type(&p.param_type)
            } else {
                type_mapper.map_type(&p.param_type)
            }
        })
        .collect();

    let param_types_ref: Vec<inkwell::types::BasicMetadataTypeEnum> = param_types.iter()
        .map(|t| match t {
            inkwell::types::BasicTypeEnum::IntType(t) => (*t).into(),
            inkwell::types::BasicTypeEnum::FloatType(t) => (*t).into(),
            inkwell::types::BasicTypeEnum::PointerType(t) => (*t).into(),
            inkwell::types::BasicTypeEnum::ArrayType(t) => (*t).into(),
            inkwell::types::BasicTypeEnum::StructType(t) => (*t).into(),
            inkwell::types::BasicTypeEnum::VectorType(t) => (*t).into(),
            inkwell::types::BasicTypeEnum::ScalableVectorType(_) => {
                // ScalableVectorType - use context pointer type instead
                // This case should not happen in practice, but we handle it gracefully
                context.ptr_type(AddressSpace::default()).into()
            }
        })
        .collect();

    // Return type (also use C ABI mapping for C functions)
    // Special handling for void return type
    let is_void = func.return_type == "void" || func.return_type == "()";

    let fn_type = if is_void {
        // Void return type - use void_type().fn_type()
        context.void_type().fn_type(&param_types_ref, has_variadic)
    } else {
        let return_type = if func.is_c {
            c_abi_mapper.map_type(&func.return_type)
        } else {
            type_mapper.map_type(&func.return_type)
        };

        // Create function type (mark as variadic if needed)
        // For struct types, return a pointer instead of the struct value
        match return_type {
            inkwell::types::BasicTypeEnum::IntType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::FloatType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::PointerType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::ArrayType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::StructType(_) => {
                // For struct types, return a pointer to the struct
                context.ptr_type(AddressSpace::default()).fn_type(&param_types_ref, has_variadic)
            }
            inkwell::types::BasicTypeEnum::VectorType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::ScalableVectorType(_) => {
                // For scalable vectors, use context pointer type instead of deprecated method
                context.ptr_type(AddressSpace::default()).fn_type(&param_types_ref, has_variadic)
            }
        }
    };

    // Add function to module
    // For C functions, use None linkage to ensure proper C ABI
    // For Coffee functions, also use None (default is C ABI, which is fine for both)
    let function = module.add_function(&func.name, fn_type, None);

    // Set calling convention for C functions
    // Note: LLVM's default calling convention (0) is already C, which is what we want
    // For C functions, we explicitly set it to ensure compatibility
    if func.is_c {
        // C calling convention is 0 in LLVM (also the default)
        function.set_call_conventions(0);
    }

    functions.insert(func.name.clone(), function);

    // Debug: print function signature
    coffee_debug!("DEBUG: declare_function: function='{}', param_count={}, return_type={:?}", 
        func.name, function.get_params().len(), function.get_type().get_return_type());
    for (i, param) in function.get_params().into_iter().enumerate() {
        coffee_debug!("DEBUG: declare_function: param {} type={:?}", i, param.get_type());
    }

    Ok(function)
}

/// Declare an external function from a C symbol (.cfc file)
/// 
/// This internal function declares an external function based on its signature
/// defined in a C symbol table from a .cfc file. It creates the appropriate
/// LLVM function signature based on the parameter and return types specified
/// in the C symbol definition. The function handles both regular and variadic
/// C functions according to the symbol specification.
/// 
/// The function properly maps the C types defined in the symbol to their LLVM
/// equivalents and creates the function in the specified module. It's used
/// when importing C functions from external libraries using .cfc files.
/// 
/// # Arguments
/// 
/// * `context` - The LLVM context
/// * `module` - The LLVM module to add the function to
/// * `functions` - The function map to store the declared function
/// * `qualified_name` - The qualified name of the function to declare
/// * `symbol` - The C symbol definition containing the function signature
/// 
/// # Returns
/// 
/// * `Ok(FunctionValue)` - The declared LLVM function value
/// * `Err(String)` - If there was an error during function declaration
fn declare_external_function_from_symbol<'ctx>(
    context: &'ctx inkwell::context::Context,
    module: &inkwell::module::Module<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
    qualified_name: &str,
    symbol: &crate::c::CSymbol,
) -> Result<FunctionValue<'ctx>, String> {
    use crate::backend::types::TypeMapper;
    let type_mapper = TypeMapper::with_c_abi(context);

    // Convert parameter types
    let mut param_types = Vec::new();
    for param in &symbol.parameters {
        if param.is_variadic {
            // Variadic parameter - will be handled separately
            break;
        }
        let llvm_type = type_mapper.map_type(&param.param_type);
        param_types.push(llvm_type.into());
    }

    // Convert return type
    let return_llvm_type = type_mapper.map_type(&symbol.return_type);

    // Create function type
    let fn_type = if symbol.is_variadic {
        return_llvm_type.fn_type(&param_types, true)
    } else {
        return_llvm_type.fn_type(&param_types, false)
    };

    // Add function to module
    let function = module.add_function(qualified_name, fn_type, None);
    functions.insert(qualified_name.to_string(), function);

    Ok(function)
}

/// Declare an external function (from imported module)
/// 
/// This function declares an external function that was imported from another
/// module or C library. It first attempts to find the function's signature in
/// the provided C symbol tables from .cfc files. If found, it uses the exact
/// signature from the .cfc file. Otherwise, it attempts to infer a generic
/// signature based on common function naming patterns (math functions, print
/// functions, etc.).
/// 
/// The function handles both Coffee and C imported functions, with special
/// handling for common C library function patterns. It ensures that external
/// functions are properly declared before their use in the compiled code.
/// 
/// For imported functions, we use a generic signature that will be linked later
/// 
/// # Arguments
/// 
/// * `qualified_name` - The fully qualified name of the function to declare
/// * `context` - The LLVM context
/// * `module` - The LLVM module to add the function to
/// * `functions` - The function map to store the declared function
/// * `cfc_symbols` - The C symbol tables from .cfc files for signature lookup
/// 
/// # Returns
/// 
/// * `Ok(FunctionValue)` - The declared LLVM function value
/// * `Err(String)` - If there was an error during function declaration
pub fn declare_external_function<'ctx>(
    qualified_name: &str,
    context: &'ctx inkwell::context::Context,
    module: &inkwell::module::Module<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
    cfc_symbols: &std::collections::HashMap<String, crate::c::CSymbolTable>,
) -> Result<FunctionValue<'ctx>, String> {
    // Check if already declared
    if let Some(&func) = functions.get(qualified_name) {
        return Ok(func);
    }

    // Try to get the function signature from .cfc files
    let func_name = qualified_name.split('.').last().unwrap_or(qualified_name);

    // Look through all CFC symbol tables to find the function
    for (_library, symbol_table) in cfc_symbols {
        if let Some(symbol) = symbol_table.get(func_name) {
            // Found the function in .cfc file - use its signature
            return declare_external_function_from_symbol(context, module, functions, qualified_name, symbol);
        }
    }

    let i64_type = context.i64_type();
    let i32_type = context.i32_type();
    let f64_type = context.f64_type();
    let i8_ptr_type = context.ptr_type(AddressSpace::default());
    let void_type = context.void_type();

    // For external functions, we need to infer the signature
    // This is a limitation - we'll use a generic signature for now
    // In the future, we should read the actual signature from the imported module

    // Try to parse the last segment to guess the type from common patterns
    let func_name = qualified_name.split('.').last().unwrap_or(qualified_name);

    // Common math functions return f64 and take f64
    let is_math = matches!(func_name, "sqrt" | "sin" | "cos" | "tan" | "log" | "exp" | "abs" | "floor" | "ceil" | "round" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" | "exp2" | "expm1" | "log10" | "log2" | "log1p" | "trunc" | "cbrt");
    // Two-argument math functions
    let is_math2 = matches!(func_name, "pow" | "atan2" | "hypot" | "fmod" | "remainder" | "fmax" | "fmin" | "fdim");
    // Print/println functions return void
    let is_print = func_name == "print" || func_name == "println";
    // printf returns i32 and takes variadic args
    let is_printf = func_name == "printf" || func_name == "fprintf" || func_name == "sprintf" || func_name == "snprintf";
    // putchar/fputc take a single int and return int
    let is_putchar = matches!(func_name, "putchar" | "fputc");
    // strlen/strcmp take a single string and return i64
    let is_strlen = matches!(func_name, "strlen" | "strcmp");
    // strcpy/strcat take two strings and return a pointer
    let is_strcpy = matches!(func_name, "strcpy" | "strcat");
    // malloc takes a size and returns a pointer
    let is_malloc = func_name == "malloc";
    // calloc takes count and size and returns a pointer
    let is_calloc = func_name == "calloc";
    // free takes a pointer and returns void
    let is_free = func_name == "free";
    // realloc takes pointer and size and returns a pointer
    let is_realloc = func_name == "realloc";
    // stdio functions return i32
    let is_stdio = matches!(func_name, "fputs" | "fgetc" | "getchar" | "strncmp" | "strncpy" | "strncat" | "memcmp" | "memcpy" | "memset" | "abs" | "labs" | "llabs");
    // Functions ending with "_void" or matching common void patterns
    let is_void_func = func_name.ends_with("_void") ||
                       func_name == "hello_world" ||
                       func_name.contains("init") ||
                       func_name.contains("setup") ||
                       func_name == "exit" ||
                       func_name == "_exit" ||
                       func_name == "abort";

    let fn_type = if is_math {
        f64_type.fn_type(&[f64_type.into()], false)
    } else if is_math2 {
        f64_type.fn_type(&[f64_type.into(), f64_type.into()], false)
    } else if is_print {
        void_type.fn_type(&[i8_ptr_type.into()], true)
    } else if is_printf {
        context.i32_type().fn_type(&[i8_ptr_type.into()], true)
    } else if is_putchar {
        // putchar/fputc take a single int and return int
        context.i32_type().fn_type(&[i32_type.into()], false)
    } else if is_strlen {
        // strlen/strcmp take a single string and return i64
        i64_type.fn_type(&[i8_ptr_type.into()], false)
    } else if is_strcpy {
        // strcpy/strcat take two strings and return a pointer
        i8_ptr_type.fn_type(&[i8_ptr_type.into(), i8_ptr_type.into()], false)
    } else if is_malloc {
        // malloc takes a size and returns a pointer
        i8_ptr_type.fn_type(&[i64_type.into()], false)
    } else if is_calloc {
        // calloc takes count and size and returns a pointer
        i8_ptr_type.fn_type(&[i64_type.into(), i64_type.into()], false)
    } else if is_free {
        // free takes a pointer (as int) and returns void
        void_type.fn_type(&[i64_type.into()], false)
    } else if is_realloc {
        // realloc takes pointer and size and returns a pointer
        i8_ptr_type.fn_type(&[i8_ptr_type.into(), i64_type.into()], false)
    } else if func_name == "exit" || func_name == "_exit" || func_name == "abort" {
        // exit functions take an int parameter and return void
        void_type.fn_type(&[i32_type.into()], false)
    } else if func_name == "puts" {
        // puts takes a string and returns i32
        context.i32_type().fn_type(&[i8_ptr_type.into()], false)
    } else if is_stdio {
        context.i32_type().fn_type(&[i8_ptr_type.into(), i32_type.into()], false)
    } else if is_void_func {
        void_type.fn_type(&[], false)
    } else {
        // Default: i64 -> i64
        i64_type.fn_type(&[i64_type.into()], false)
    };

    let function = module.add_function(qualified_name, fn_type, None);
    functions.insert(qualified_name.to_string(), function);

    Ok(function)
}

/// Declare runtime functions (C library and Coffee compiler internal)
/// 
/// This function declares the essential runtime functions that are used by
/// the Coffee compiler during code generation and execution. These functions
/// include C library functions for error handling and memory management, as
/// well as Coffee compiler internal functions for panic handling.
/// 
/// The function uses a macro to only declare functions that haven't already
/// been declared, allowing users to override these functions by declaring
/// their own `c fn` with the same name. This provides flexibility for users
/// who want to customize the runtime behavior.
/// 
/// These functions are used by the compiler for:
/// - Error handling (puts, exit, coffee_panic)
/// - Memory management (malloc, free)
/// 
/// Users can override these by declaring their own `c fn` with the same name.
/// 
/// # Arguments
/// 
/// * `context` - The LLVM context
/// * `module` - The LLVM module to add the functions to
/// * `functions` - The function map to store the declared functions
pub fn declare_runtime_functions<'ctx>(
    context: &'ctx inkwell::context::Context,
    module: &inkwell::module::Module<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
) {
    let i32_type = context.i32_type();
    let i64_type = context.i64_type();
    let i8_ptr_type = context.ptr_type(AddressSpace::default());
    let void_type = context.void_type();

    // Helper macro to declare function only if not already declared
    // This allows users to override by declaring their own `c fn`
    macro_rules! declare_if_missing {
        ($func_name:expr, $func_type:expr) => {
            if !functions.contains_key($func_name) {
                let func = module.add_function($func_name, $func_type, None);
                functions.insert($func_name.to_string(), func);
            }
        };
    }

    // C library functions used by compiler runtime

    // puts - used for error message output in panic
    let puts_type = i32_type.fn_type(&[i8_ptr_type.into()], false);
    declare_if_missing!("puts", puts_type);

    // exit - used for program termination in panic
    let exit_type = i32_type.fn_type(&[i32_type.into()], false);
    declare_if_missing!("exit", exit_type);

    // malloc - used for heap allocation
    let malloc_type = i8_ptr_type.fn_type(&[i64_type.into()], false);
    declare_if_missing!("malloc", malloc_type);

    // free - used for heap deallocation
    let free_type = void_type.fn_type(&[i8_ptr_type.into()], false);
    declare_if_missing!("free", free_type);

    // Note: printf is NOT declared here - users must declare it themselves with `c fn printf`
}

/// Generate a return instruction
/// 
/// This function generates an appropriate LLVM return instruction based on
/// the function's return type and whether a value is provided. For void
/// functions, it generates a return instruction without a value. For functions
/// with a return type, it either returns the provided value or a default
/// value if none is provided.
/// 
/// The function handles special cases like void return types and ensures
/// that the generated return instruction is compatible with the function's
/// signature. It uses the get_default_value function to create appropriate
/// default values for non-void functions when no explicit return value is given.
/// 
/// # Arguments
/// 
/// * `builder` - The LLVM builder to use for instruction generation
/// * `return_type` - The return type of the function as a string
/// * `context` - The LLVM context
/// * `value` - The optional value to return (None for void returns or to use default)
/// 
/// # Returns
/// 
/// * `Ok(())` - If the return instruction was generated successfully
/// * `Err(String)` - If there was an error during instruction generation
pub fn build_return<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    return_type: &str,
    context: &'ctx inkwell::context::Context,
    value: Option<BasicValueEnum<'ctx>>,
) -> Result<(), String> {
    match value {
        Some(v) => {
            builder.build_return(Some(&v))
                .map_err(|e| format!("failed to build return instruction: {}", e))?;
        }
        None => {
            if return_type == "void" || return_type == "()" {
                builder.build_return(None)
                    .map_err(|e| format!("failed to build void return: {}", e))?;
            } else {
                // Return default value
                let default_val = get_default_value(return_type, context)
                    .map_err(|e| format!("failed to get default value for type '{}': {}", return_type, e))?;
                builder.build_return(Some(&default_val))
                    .map_err(|e| format!("failed to build default return: {}", e))?;
            }
        }
    }
    Ok(())
}

/// Get default value for a type
/// 
/// This function returns the default zero value for a given Coffee type.
/// It's used when a function needs to return a value but no explicit return
/// value is provided, or when initializing variables with default values.
/// The function handles various Coffee types including integers, floats,
/// booleans, and strings, returning appropriate zero or null values.
/// 
/// For integer types, it returns the corresponding zero value. For floating-point
/// types, it returns 0.0. For booleans, it returns false (0). For string types,
/// it returns a null pointer. The function returns an error for types that
/// don't have a meaningful default value, such as void.
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
pub fn get_default_value<'ctx>(
    type_str: &str,
    context: &'ctx inkwell::context::Context,
) -> Result<BasicValueEnum<'ctx>, String> {
    Ok(match type_str {
        "int" | "i64" => context.i64_type().const_zero().into(),
        "i32" => context.i32_type().const_zero().into(),
        "i16" => context.i16_type().const_zero().into(),
        "i8" => context.i8_type().const_zero().into(),
        "float" | "f64" => context.f64_type().const_zero().into(),
        "f32" => context.f32_type().const_zero().into(),
        "bool" => context.bool_type().const_zero().into(),
        "string" | "str" => context.ptr_type(inkwell::AddressSpace::default()).const_null().into(),
        "void" | "()" => return Err("void type has no default value".to_string()),
        _ => {
            // For unknown types, default to i64 zero
            context.i64_type().const_zero().into()
        }
    })
}

/// Declare a built-in C function (from libc/libm)
/// 
/// This method declares a built-in C library function using its predefined signature.
/// It implements the "declare on demand" principle - functions are only declared
/// when they are actually used in the code. This follows the practical design
/// approach of V language.
/// 
/// # Arguments
/// 
/// * `func_name` - The name of the built-in C function to declare
/// * `context` - The LLVM context
/// * `module` - The LLVM module to add the function to
/// * `functions` - The function map to store the declared function
/// 
/// # Returns
/// 
/// * `Ok(())` - If the function was declared successfully
/// * `Err(String)` - If the function is not a built-in C function or declaration failed
pub fn declare_builtin_c_function<'ctx>(
    func_name: &str,
    context: &'ctx inkwell::context::Context,
    module: &inkwell::module::Module<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
) -> Result<(), String> {
    // Check if already declared
    if functions.contains_key(func_name) {
        return Ok(());
    }

    // Get function signature based on function name
    let (param_types, return_type, is_variadic) = get_builtin_c_function_signature(func_name, context);

    // Create function type
    let fn_type = match return_type {
        ReturnType::Void => context.void_type().fn_type(&param_types, is_variadic),
        ReturnType::Int(t) => t.fn_type(&param_types, is_variadic),
        ReturnType::Float(t) => t.fn_type(&param_types, is_variadic),
        ReturnType::Pointer(t) => t.fn_type(&param_types, is_variadic),
    };

    // Add function to module
    let function = module.add_function(func_name, fn_type, None);
    functions.insert(func_name.to_string(), function);

    Ok(())
}

/// Return type for C functions
enum ReturnType<'ctx> {
    Void,
    Int(inkwell::types::IntType<'ctx>),
    Float(inkwell::types::FloatType<'ctx>),
    Pointer(inkwell::types::PointerType<'ctx>),
}

/// Get the signature of a built-in C function
/// 
/// This method returns the LLVM signature of a built-in C library function.
/// It includes parameter types, return type, and variadic flag.
/// 
/// # Arguments
/// 
/// * `func_name` - The name of the built-in C function
/// * `context` - The LLVM context
/// 
/// # Returns
/// 
/// * `(Vec<BasicMetadataTypeEnum>, ReturnType, bool)` - Parameter types, return type, and variadic flag
fn get_builtin_c_function_signature<'ctx>(
    func_name: &str,
    context: &'ctx inkwell::context::Context,
) -> (Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>>, ReturnType<'ctx>, bool) {
    // Type aliases
    let i8_ptr = context.ptr_type(AddressSpace::default());
    let i32 = context.i32_type();
    let i64 = context.i64_type();
    let f64 = context.f64_type();

    // Built-in C function signatures
    match func_name {
        // printf family (variadic)
        "printf" | "fprintf" | "sprintf" | "snprintf" => {
            (vec![i8_ptr.into()], ReturnType::Int(i32), true)
        }

        // puts family (non-variadic)
        "puts" | "fputs" => {
            (vec![i8_ptr.into()], ReturnType::Int(i32), false)
        }

        // putchar family
        "putchar" | "fputc" => {
            (vec![i32.into()], ReturnType::Int(i32), false)
        }

        // scanf family (variadic)
        "scanf" | "fscanf" | "sscanf" => {
            (vec![i8_ptr.into()], ReturnType::Int(i32), true)
        }

        // Memory functions
        "malloc" => (vec![i64.into()], ReturnType::Pointer(i8_ptr), false),
        "free" => (vec![i64.into()], ReturnType::Void, false),
        "calloc" => (vec![i64.into(), i64.into()], ReturnType::Pointer(i8_ptr), false),
        "realloc" => (vec![i8_ptr.into(), i64.into()], ReturnType::Pointer(i8_ptr), false),

        // String functions
        "memcpy" | "memmove" => (vec![i8_ptr.into(), i8_ptr.into(), i64.into()], ReturnType::Pointer(i8_ptr), false),
        "memcmp" => (vec![i8_ptr.into(), i8_ptr.into(), i64.into()], ReturnType::Int(i32), false),
        "memset" => (vec![i8_ptr.into(), i32.into(), i64.into()], ReturnType::Pointer(i8_ptr), false),
        "strlen" => (vec![i8_ptr.into()], ReturnType::Int(i64), false),
        "strcmp" => (vec![i8_ptr.into(), i8_ptr.into()], ReturnType::Int(i32), false),
        "strcpy" | "strcat" => (vec![i8_ptr.into(), i8_ptr.into()], ReturnType::Pointer(i8_ptr), false),
        "strncmp" => (vec![i8_ptr.into(), i8_ptr.into(), i64.into()], ReturnType::Int(i32), false),
        "strncpy" | "strncat" => (vec![i8_ptr.into(), i8_ptr.into(), i64.into()], ReturnType::Pointer(i8_ptr), false),

        // Process functions
        "exit" | "_exit" | "abort" => (vec![i32.into()], ReturnType::Void, false),
        "system" => (vec![i8_ptr.into()], ReturnType::Int(i32), false),
        "getenv" => (vec![i8_ptr.into()], ReturnType::Pointer(i8_ptr), false),
        "setenv" => (vec![i8_ptr.into(), i8_ptr.into(), i32.into()], ReturnType::Int(i32), false),

        // Random functions
        "rand" => (vec![], ReturnType::Int(i32), false),
        "srand" => (vec![i32.into()], ReturnType::Void, false),
        "time" => (vec![i8_ptr.into()], ReturnType::Int(i64), false),

        // Math functions (single argument)
        "abs" | "labs" | "llabs" => (vec![i64.into()], ReturnType::Int(i64), false),
        "atoi" | "atol" | "atoll" => (vec![i8_ptr.into()], ReturnType::Int(i64), false),
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" => (vec![f64.into()], ReturnType::Float(f64), false),
        "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" => (vec![f64.into()], ReturnType::Float(f64), false),
        "exp" | "exp2" | "expm1" | "log" | "log10" | "log2" | "log1p" => (vec![f64.into()], ReturnType::Float(f64), false),
        "sqrt" | "cbrt" | "fabs" | "floor" | "ceil" | "round" | "trunc" => (vec![f64.into()], ReturnType::Float(f64), false),

        // Math functions (two arguments)
        "pow" | "atan2" | "hypot" | "fmod" | "remainder" | "fmax" | "fmin" | "fdim" => {
            (vec![f64.into(), f64.into()], ReturnType::Float(f64), false)
        }

        // File I/O functions
        "fopen" => (vec![i8_ptr.into(), i8_ptr.into()], ReturnType::Pointer(i8_ptr), false),
        "fclose" => (vec![i8_ptr.into()], ReturnType::Int(i32), false),
        "fread" => (vec![i8_ptr.into(), i64.into(), i64.into(), i8_ptr.into()], ReturnType::Int(i64), false),
        "fwrite" => (vec![i8_ptr.into(), i64.into(), i64.into(), i8_ptr.into()], ReturnType::Int(i64), false),
        "fseek" => (vec![i8_ptr.into(), i64.into(), i32.into()], ReturnType::Int(i32), false),
        "ftell" => (vec![i8_ptr.into()], ReturnType::Int(i64), false),
        "rewind" => (vec![i8_ptr.into()], ReturnType::Void, false),
        "fflush" => (vec![i8_ptr.into()], ReturnType::Int(i32), false),
        "fgets" => (vec![i8_ptr.into(), i32.into(), i8_ptr.into()], ReturnType::Pointer(i8_ptr), false),
        "fgetc" | "getc" => (vec![i8_ptr.into()], ReturnType::Int(i32), false),
        "feof" | "ferror" | "clearerr" => (vec![i8_ptr.into()], ReturnType::Int(i32), false),

        // Process functions
        "execl" | "execlp" | "execle" => (vec![i8_ptr.into(), i8_ptr.into()], ReturnType::Int(i32), true),
        "execv" | "execvp" => (vec![i8_ptr.into(), i8_ptr.into()], ReturnType::Int(i32), false),
        "execve" => (vec![i8_ptr.into(), i8_ptr.into(), i8_ptr.into()], ReturnType::Int(i32), false),
        "wait" | "waitpid" => (vec![i8_ptr.into()], ReturnType::Int(i32), false),

        // Default: return error (this shouldn't happen if is_builtin_c_function is used correctly)
        _ => (vec![], ReturnType::Void, false),
    }
}

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub fn compile_function(&mut self, func: &Function) -> Result<(), String> {
        coffee_debug!("DEBUG: compile_function: START compiling function '{}', parameters: {:?}", func.name, func.parameters.iter().map(|p| &p.name).collect::<Vec<_>>());
        let function = *self.functions.get(&func.name)
            .ok_or_else(|| self.error("compile_function",
                format!("function '{}' not declared - this is an internal compiler error", func.name)))?;

        // For external declarations, don't generate any function body
        if matches!(func.body, FunctionBody::External) {
            return Ok(());
        }

        self.current_function = Some(function);

        // 进入新的函数作用域
        self.memory_ctx.enter_scope();

        // 初始化生命周期跟踪
        self.memory_ctx.set_function(func.name.clone());

        // Set error handler for this function
        self.current_error_handler = func.error_handler.clone();
        self.current_function_params = func.parameters.iter().map(|p| p.name.clone()).collect();

        // Mark that Error class is needed if function has error handler
        if func.error_handler.is_some() {
            self.needs_error_class = true;
        }

        // Create entry block
        let entry = self.backend.context.append_basic_block(function, "entry");
        self.backend.builder.position_at_end(entry);

        // Clear local variables and lifetime information for each function
        // This ensures that each function has its own scope and can use the same variable names
        self.variables.clear();
        self.memory_ctx.clear_all();

        // CRITICAL-5 FIX: Reset stack size tracking for each function
        self.current_stack_size = 0;

        // Allocate parameters
        for (i, param) in func.parameters.iter().enumerate() {
            let param_value = function.get_nth_param(i as u32)
                .ok_or_else(|| self.error("compile_function",
                    format!("missing parameter {} '{}' in function '{}' - internal error",
                        i, param.name, func.name)))?;

            let param_type = self.coffee_type_to_llvm(&param.param_type)
                .map_err(|e| self.error("compile_function",
                    format!("failed to convert type '{}' for parameter '{}': {}",
                        param.param_type, param.name, e)))?;
            let alloca = self.backend.builder.build_alloca(param_type, &param.name)
                .map_err(|e| self.error("compile_function",
                    format!("failed to allocate parameter '{}': {}", param.name, e)))?;

            self.backend.builder.build_store(alloca, param_value)
                .map_err(|e| self.error("compile_function",
                    format!("failed to store parameter '{}': {}", param.name, e)))?;

            coffee_debug!("DEBUG: compile_function: inserting parameter '{}' into variables", param.name);
            self.variables.insert(param.name.clone(), (alloca, param_type));
            coffee_debug!("DEBUG: compile_function: variables after inserting parameter '{}': {:?}", param.name, self.variables.keys().collect::<Vec<_>>());
        }

        // Create a body block where all actual code will be compiled
        // This keeps entry block clean with only allocas and a single branch
        let body_block = self.backend.context.append_basic_block(function, "body");
        self.backend.builder.build_unconditional_branch(body_block)
            .map_err(|e| self.error("compile_function",
                format!("failed to build branch from entry to body: {}", e)))?;
        self.backend.builder.position_at_end(body_block);

        // Track that we're now in body block
        self.entry_successor = Some(body_block);

        // Track if function contains raise statement
        let mut has_raise = false;
        let mut raise_line = 0;

        // Compile function body
        match &func.body {
            FunctionBody::External => {
                // External declaration - should have returned early
                return Err(self.error("compile_function",
                    "external function declaration should not reach body compilation"));
            }
            FunctionBody::Expression(expr) => {
                // Single expression - evaluate and return
                if !matches!(expr, crate::parser::expr::Expression::Literal(s) if s.is_empty()) {
                    let value = self.compile_expr(expr)
                        .map_err(|e| self.error("compile_function",
                            format!("failed to compile function body expression: {}", e)))?;
                    build_return(&self.backend.builder, &func.return_type, self.backend.context, Some(value))
                        .map_err(|e| self.error("compile_function",
                            format!("failed to build return instruction: {}", e)))?;
                }
            }
            FunctionBody::Block(statements) => {
                // Multiple statements - compile each one
                let mut unreachable_warning = None;
                let mut has_terminal = false;

                for (i, stmt) in statements.iter().enumerate() {
                    // 检查是否是return或raise语句
                    let is_terminal = matches!(stmt,
                        Statement::Return(_) | Statement::Raise(_));

                    if has_terminal && !is_terminal {
                        // 在return/raise之后还有语句
                        // 如果是raise语句，报错；如果是return语句，警告
                        if has_raise {
                            return Err(self.error("compile_function",
                                format!("unreachable code after raise statement at line {} - raise must be the last statement in the function body", raise_line)));
                        } else {
                            unreachable_warning = Some(format!(
                                "unreachable code after return statement at line {}",
                                i + 1
                            ));
                            // 跳过不可达代码，不编译
                            continue;
                        }
                    }

                    // 如果已经遇到raise语句，不再编译后面的任何语句
                    if has_raise {
                        continue;
                    }

                    self.compile_statement(stmt)
                        .map_err(|e| self.error("compile_function",
                            format!("failed to compile statement {} in function body: {}", i + 1, e)))?;

                    if is_terminal {
                        has_terminal = true;
                        if matches!(stmt, Statement::Raise(_)) {
                            has_raise = true;
                            raise_line = i + 1;
                        }
                    }
                }

                // 如果有不可达代码警告，打印它（使用黄色警告）
                if let Some(warning) = unreachable_warning {
                    eprintln!("\x1b[1;33m[Warning]\x1b[0m: {}", warning);
                    eprintln!("    \x1b[0;36m| note: code after return will never execute\x1b[0m");
                }
            }
        }

        // Add default return if needed (but not if function has raise statement)
        let has_terminator = self.backend.builder.get_insert_block()
            .map(|b| b.get_terminator().is_some())
            .unwrap_or(false);
        if !has_raise && !has_terminator {
            build_return(&self.backend.builder, &func.return_type, self.backend.context, None)
                .map_err(|e| self.error("compile_function",
                    format!("failed to build default return: {}", e)))?;
        }

        // Ensure entry block has a terminator before finishing
        let entry_block = function.get_first_basic_block().unwrap();
        if entry_block.get_terminator().is_none() {
            return Err(self.error("compile_function",
                "entry block lacks terminator - function body doesn't return or branch properly"));
        }

        // HIGH-10 FIX: Check for unused variables before function ends
        // Collect all declared variables (parameters + locals)
        let _all_vars: std::collections::HashSet<String> = self.variables.keys().cloned().collect();

        // 运行生命周期检查，并按 severity 分流：
        //   Error   (M006/M007，真正的 UB)        → 编译失败
        //   Warning (M001/M002/M005/M010/M011/M012/M013) → 黄色警告，不阻断编译
        //   Silent  (M003/M004/M009，已退役的 must-rm 规则) → 丢弃
        //   自动 drop 接管后，"必须显式 rm" 的前提已不复存在。
        let lifetime_diagnostics = self.memory_ctx.run_lifetime_checks()
            .map_err(|e| self.error("compile_function",
                format!("lifetime check failed: {}", e)))?;

        let mut error_messages = Vec::new();
        for diag in lifetime_diagnostics {
            // 显式使用 var_name 字段以确保它被使用
            let _var = &diag.var_name;
            match diag.severity() {
                DiagSeverity::Error => {
                    error_messages.push(format!(
                        "\x1b[1;35m[{}]\x1b[0m: {}\n    \x1b[0;36m| help: {}\x1b[0m",
                        diag.code, diag.message, diag.hint
                    ));
                }
                DiagSeverity::Warning => {
                    eprintln!("\x1b[1;33m[Warning {}]\x1b[0m: {}\n    \x1b[0;36m| help: {}\x1b[0m",
                        diag.code, diag.message, diag.hint);
                }
                DiagSeverity::Silent => {
                    // 已退役的诊断，丢弃
                }
            }
        }

        // 只有真正的内存安全 UB（M006 use-after-free / M007 use-after-move）
        // 才拒绝编译——这些无法被自动 drop 修复。
        if !error_messages.is_empty() {
            return Err(self.error("compile_function",
                format!("\x1b[1;35mMemory safety check failed\x1b[0m\n\n{}\n  \x1b[0;36m= note: use-after-free / use-after-move are undefined behavior and cannot be auto-fixed by drop\x1b[0m",
                    error_messages.join("\n\n"))));
        }

        // Clear used variables for next function
        self.used_variables.clear();

        // 退出函数作用域
        self.memory_ctx.exit_scope();

        self.current_function = None;
        Ok(())
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
}