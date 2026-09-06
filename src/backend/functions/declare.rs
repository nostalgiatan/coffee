use crate::coffee_debug;
use crate::parser::Function;
use crate::backend::types::TypeMapper;
use crate::backend::codegen::CodeGenerator;
use inkwell::{values::FunctionValue, AddressSpace};

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

    // C and Coffee both map `bool` to i8 (`TypeMapper`); C ABI mapper is for FFI widths.
    let param_types: Vec<inkwell::types::BasicTypeEnum<'ctx>> = func.parameters.iter()
        .filter(|p| !p.is_variadic)
        .map(|p| type_mapper.try_map_type(&p.param_type))
        .collect::<Result<Vec<_>, _>>()?;

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
        let return_type = type_mapper.try_map_type(&func.return_type)?;

        // Create function type (mark as variadic if needed)
        // Coffee resource classes return a pointer; slices and C structs return the value.
        match return_type {
            inkwell::types::BasicTypeEnum::IntType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::FloatType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::PointerType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::ArrayType(t) => t.fn_type(&param_types_ref, has_variadic),
            inkwell::types::BasicTypeEnum::StructType(t) => {
                let coffee_ty = crate::types::type_from_str(&func.return_type);
                let by_value = matches!(coffee_ty, Ok(crate::types::Type::Slice(_)))
                    || type_mapper.c_struct_names.contains(&func.return_type)
                    || type_mapper.c_union_byte_sizes.contains_key(&func.return_type);
                if by_value {
                    t.fn_type(&param_types_ref, has_variadic)
                } else {
                    context
                        .ptr_type(AddressSpace::default())
                        .fn_type(&param_types_ref, has_variadic)
                }
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
    let llvm_name = coffee_llvm_fn_name(&func.name);
    let function = module.add_function(&llvm_name, fn_type, None);

    // Set calling convention for C functions
    // Note: LLVM's default calling convention (0) is already C, which is what we want
    // For C functions, we explicitly set it to ensure compatibility
    if func.is_c {
        // C calling convention is 0 in LLVM (also the default)
        function.set_call_conventions(0);
    }

    functions.insert(func.name.clone(), function);

    coffee_debug!(
        "DEBUG: declare_function: coffee='{}' llvm='{}'",
        func.name,
        llvm_name
    );

    // Debug: print function signature
    coffee_debug!("DEBUG: declare_function: function='{}', param_count={}, return_type={:?}", 
        func.name, function.get_params().len(), function.get_type().get_return_type());
    for (i, param) in function.get_params().into_iter().enumerate() {
        coffee_debug!("DEBUG: declare_function: param {} type={:?}", i, param.get_type());
    }

    Ok(function)
}

/// LLVM symbol for a Coffee function. Must not steal libc/CRT names: after
/// `fn main` returns, the C runtime calls `exit`, not a Coffee wrapper.
pub fn coffee_llvm_fn_name(name: &str) -> String {
    if name != "main" && shadows_c_process_abi(name) {
        format!("_cf_{name}")
    } else {
        name.to_string()
    }
}

fn shadows_c_process_abi(name: &str) -> bool {
    matches!(name, "exit" | "_exit" | "abort" | "atexit")
}

/// LLVM function type from Coffee spellings using **only** `TypeMapper::with_c_abi`.
/// Runtime/clone fallbacks must not invent `i64 strlen` when libc says `int(4)+`.
pub fn c_abi_fn_type<'ctx>(
    context: &'ctx inkwell::context::Context,
    return_ty: &str,
    param_tys: &[&str],
    variadic: bool,
) -> Result<inkwell::types::FunctionType<'ctx>, String> {
    let mapper = TypeMapper::with_c_abi(context);
    c_fn_type_with(&mapper, context, return_ty, param_tys, variadic)
}

pub fn c_fn_type_with<'ctx>(
    mapper: &TypeMapper<'ctx>,
    context: &'ctx inkwell::context::Context,
    return_ty: &str,
    param_tys: &[&str],
    variadic: bool,
) -> Result<inkwell::types::FunctionType<'ctx>, String> {
    let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = param_tys
        .iter()
        .map(|p| mapper.try_map_type(p).map(Into::into))
        .collect::<Result<Vec<_>, _>>()?;
    if return_ty == "void" || return_ty == "()" {
        return Ok(context.void_type().fn_type(&param_types, variadic));
    }
    let ret = mapper.try_map_type(return_ty)?;
    Ok(match ret {
        inkwell::types::BasicTypeEnum::IntType(t) => t.fn_type(&param_types, variadic),
        inkwell::types::BasicTypeEnum::FloatType(t) => t.fn_type(&param_types, variadic),
        inkwell::types::BasicTypeEnum::PointerType(t) => t.fn_type(&param_types, variadic),
        inkwell::types::BasicTypeEnum::ArrayType(t) => t.fn_type(&param_types, variadic),
        inkwell::types::BasicTypeEnum::StructType(t) => t.fn_type(&param_types, variadic),
        inkwell::types::BasicTypeEnum::VectorType(t) => t.fn_type(&param_types, variadic),
        inkwell::types::BasicTypeEnum::ScalableVectorType(_) => context
            .ptr_type(AddressSpace::default())
            .fn_type(&param_types, variadic),
    })
}

fn declare_external_function_from_symbol<'ctx>(
    context: &'ctx inkwell::context::Context,
    module: &inkwell::module::Module<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
    qualified_name: &str,
    symbol: &crate::c::CSymbol,
    mapper: &TypeMapper<'ctx>,
) -> Result<FunctionValue<'ctx>, String> {
    let mut param_tys: Vec<&str> = Vec::new();
    for param in &symbol.parameters {
        if param.param_type == "..." || (param.name == "args" && param.param_type == "object") {
            break;
        }
        param_tys.push(param.param_type.as_str());
    }
    let fn_type = c_fn_type_with(
        mapper,
        context,
        &symbol.return_type,
        &param_tys,
        symbol.is_variadic,
    )?;

    if let Some(existing) = module.get_function(qualified_name) {
        if existing.get_name().to_str().ok() == Some(qualified_name) {
            functions
                .entry(qualified_name.to_string())
                .or_insert(existing);
            return Ok(existing);
        }
    }

    let function = module.add_function(qualified_name, fn_type, None);
    let coffee_holds_key = functions.get(qualified_name).is_some_and(|f| {
        f.get_name().to_str().ok() != Some(qualified_name)
    });
    if !coffee_holds_key {
        functions.insert(qualified_name.to_string(), function);
    }

    Ok(function)
}

/// Declare an external function (from imported module)
///
/// Signature comes only from a loaded `.cfc` table (`CSymbol`).
pub fn declare_external_function<'ctx>(
    qualified_name: &str,
    context: &'ctx inkwell::context::Context,
    module: &inkwell::module::Module<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
    cfc_symbols: &std::collections::HashMap<String, crate::c::CSymbolTable>,
    mapper: &TypeMapper<'ctx>,
) -> Result<FunctionValue<'ctx>, String> {
    if let Some(existing) = module.get_function(qualified_name) {
        if existing.get_name().to_str().ok() == Some(qualified_name) {
            return Ok(existing);
        }
    }
    if let Some(&func) = functions.get(qualified_name) {
        if func.get_name().to_str().ok() == Some(qualified_name) {
            return Ok(func);
        }
    }

    let func_name = qualified_name.split('.').last().unwrap_or(qualified_name);

    for (_library, symbol_table) in cfc_symbols {
        if let Some(symbol) = symbol_table.get(func_name) {
            return declare_external_function_from_symbol(
                context,
                module,
                functions,
                qualified_name,
                symbol,
                mapper,
            );
        }
    }

    Err(format!(
        "unknown C function '{}': not in loaded .cfc tables",
        qualified_name
    ))
}

/// Declare runtime functions from the bundled libc table (`puts`/`exit`/`malloc`/`free`).
pub fn declare_runtime_functions<'ctx>(
    context: &'ctx inkwell::context::Context,
    module: &inkwell::module::Module<'ctx>,
    functions: &mut std::collections::HashMap<String, FunctionValue<'ctx>>,
    cfc_symbols: &std::collections::HashMap<String, crate::c::CSymbolTable>,
    mapper: &TypeMapper<'ctx>,
) {
    let libc = cfc_symbols
        .get("libc")
        .expect("bundled libc symbol table missing");
    for name in ["puts", "exit", "malloc", "free"] {
        if module.get_function(name).is_some_and(|f| {
            f.get_name().to_str().ok() == Some(name)
        }) {
            continue;
        }
        let symbol = libc
            .get(name)
            .unwrap_or_else(|| panic!("libc.cfc missing runtime symbol {name}"));
        declare_external_function_from_symbol(context, module, functions, name, symbol, mapper)
            .unwrap_or_else(|e| panic!("failed to declare runtime '{name}' from libc table: {e}"));
    }
}

/// `exit(1)` typed to the libc table's exit parameter (usually `int` → i64).
pub(crate) fn const_exit_status_one<'ctx>(
    exit_func: inkwell::values::FunctionValue<'ctx>,
    context: &'ctx inkwell::context::Context,
) -> inkwell::values::BasicValueEnum<'ctx> {
    if let Some(p) = exit_func.get_nth_param(0) {
        if p.is_int_value() {
            return p.into_int_value().get_type().const_int(1, false).into();
        }
    }
    context.i32_type().const_int(1, false).into()
}

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// True if `func_name` is a `use … of c` import or a name in any loaded `.cfc` table.
    pub fn is_c_library_function(&self, func_name: &str) -> bool {
        for c_import in &self.c_imports {
            if c_import.contains(':') {
                let parts: Vec<&str> = c_import.split(':').collect();
                if parts.len() == 2 && parts[1] == func_name {
                    return true;
                }
            } else if c_import == func_name {
                return true;
            }
        }

        self.cfc_symbols
            .values()
            .any(|table| table.get(func_name).is_some())
    }

    pub(crate) fn declare_runtime_functions(&mut self) {
        crate::backend::functions::declare_runtime_functions(
            self.backend.context,
            &self.backend.module,
            &mut self.functions,
            &self.cfc_symbols,
            &self.type_mapper,
        );
    }

    pub(crate) fn declare_used_c_functions(&mut self) -> Result<(), String> {
        for func_name in self.used_c_functions.iter() {
            let already = self.backend.module.get_function(func_name).is_some_and(|f| {
                f.get_name().to_str().ok() == Some(func_name.as_str())
            });
            if already {
                continue;
            }
            crate::backend::functions::declare_external_function(
                func_name,
                self.backend.context,
                &self.backend.module,
                &mut self.functions,
                &self.cfc_symbols,
                &self.type_mapper,
            )?;
        }

        Ok(())
    }

    /// Declare a Coffee function even if libc already reserved the same source name
    /// (`exit`). Runtime `declare_runtime_functions` inserts C `exit` first; skipping
    /// then would compile the Coffee body into the libc symbol and hang after `main`.
    pub(crate) fn ensure_coffee_function_declared(
        &mut self,
        func: &Function,
    ) -> Result<(), String> {
        let want = coffee_llvm_fn_name(&func.name);
        let already = self.functions.get(&func.name).is_some_and(|f| {
            f.get_name().to_str().ok() == Some(want.as_str())
        });
        if already {
            return Ok(());
        }
        self.declare_function(func)?;
        Ok(())
    }

    pub(crate) fn declare_function(&mut self, func: &Function) -> Result<FunctionValue<'ctx>, String> {
        crate::backend::functions::declare_function(
            func,
            &self.backend.module,
            self.backend.context,
            &self.type_mapper,
            &mut self.functions,
        )
    }

    pub fn declare_external_function(&mut self, qualified_name: &str) -> Result<FunctionValue<'ctx>, String> {
        crate::backend::functions::declare_external_function(
            qualified_name,
            self.backend.context,
            &self.backend.module,
            &mut self.functions,
            &self.cfc_symbols,
            &self.type_mapper,
        )
    }

}

#[cfg(test)]
mod tests {
    use super::declare_function;
    use crate::backend::types::TypeMapper;
    use crate::parser::function::{Function, FunctionBody, Parameter};
    use inkwell::context::Context;
    use std::collections::HashMap;

    fn coffee_fn(name: &str, params: Vec<Parameter>, return_type: &str) -> Function {
        Function {
            type_params: vec![],
            name: name.to_string(),
            parameters: params,
            return_type: return_type.to_string(),
            error_handler: None,
            body: FunctionBody::External,
            is_c: false,
        }
    }

    #[test]
    fn declare_function_rejects_unparsable_param_type() {
        let context = Context::create();
        let module = context.create_module("t");
        let mapper = TypeMapper::new(&context);
        let mut functions = HashMap::new();
        let func = coffee_fn(
            "f",
            vec![Parameter {
                name: "xs".to_string(),
                param_type: "List<int>".to_string(),
                is_variadic: false,
            }],
            "void",
        );
        let err = declare_function(&func, &module, &context, &mapper, &mut functions)
            .expect_err("List<int> is not a Coffee type");
        assert!(
            err.contains("List<int>") || err.contains("generic") || err.contains("parse"),
            "error should mention the bad type: {err}"
        );
    }

    #[test]
    fn declare_function_rejects_unsupported_int_return_width() {
        let context = Context::create();
        let module = context.create_module("t");
        let mapper = TypeMapper::new(&context);
        let mut functions = HashMap::new();
        let func = coffee_fn("f", vec![], "int(3)+");
        declare_function(&func, &module, &context, &mapper, &mut functions)
            .expect_err("int(3)+ must not map to i64");
    }

    #[test]
    fn declare_function_mangles_exit_so_crt_can_call_libc() {
        let context = Context::create();
        let module = context.create_module("t");
        let mapper = TypeMapper::new(&context);
        let mut functions = HashMap::new();
        let func = coffee_fn(
            "exit",
            vec![Parameter {
                name: "code".to_string(),
                param_type: "int".to_string(),
                is_variadic: false,
            }],
            "void",
        );
        let f = declare_function(&func, &module, &context, &mapper, &mut functions).unwrap();
        assert_eq!(f.get_name().to_str().unwrap(), "_cf_exit");
        assert!(functions.contains_key("exit"));
        assert_eq!(super::coffee_llvm_fn_name("main"), "main");
        assert_eq!(super::coffee_llvm_fn_name("print"), "print");
    }
}
