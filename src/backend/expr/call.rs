//! Call, constructor, and named-function compilation.

use crate::backend::codegen::CodeGenerator;
use crate::parser::expr::Expression;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;
#[cfg(test)]
use crate::coffee_debug;
#[cfg(test)]
use crate::backend::type_inference::TypeInferenceContext;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_call_expr(&mut self, function: &Expression, args: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        match function {
            Expression::Variable(name) if name.contains("::") => {
                let parts: Vec<&str> = name.split("::").collect();
                if parts.len() == 2 {
                    return self.compile_enum_variant_call(parts[0], parts[1], args);
                }
                self.compile_named_call(name, args)
            }
            Expression::Variable(name) => self.compile_named_call(name, args),
            Expression::Member { object, field, args: member_args } if member_args.is_empty() => {
                let mut all_args = Vec::new();
                all_args.extend_from_slice(args);
                self.compile_member_expr(object, field, &all_args)
            }
            other => {
                let name = other.to_string();
                self.compile_named_call(&name, args)
            }
        }
    }

    pub(crate) fn compile_enum_variant_call(&mut self, scope_str: &str, variant_name: &str, args: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        if self.enums.contains_key(scope_str) {
            let tag_index = self.enums.get(scope_str)
                .and_then(|enum_def| enum_def.variants.iter().position(|v| v.name == variant_name))
                .unwrap_or(0) as u64;
            let compiled_args: Vec<BasicValueEnum<'ctx>> = args.iter()
                .map(|arg| self.compile_enum_payload_arg(arg))
                .collect::<Result<Vec<_>, _>>()?;
            let tag_value = self.backend.context.i64_type().const_int(tag_index, false);
            let mut field_values: Vec<BasicValueEnum<'ctx>> = vec![BasicValueEnum::IntValue(tag_value)];
            field_values.extend(compiled_args);
            let field_types: Vec<BasicTypeEnum<'ctx>> = field_values.iter().map(|v| v.get_type()).collect();
            let variant_type = self.backend.context.struct_type(&field_types, false);
            // Runtime payloads (literals, nested variants) are not LLVM constants, so
            // build via alloca + insertvalue rather than const_named_struct.
            let variant_ptr = self.backend.builder.build_alloca(variant_type, &format!("{}_{}", scope_str, variant_name))
                .map_err(|e| self.error("compile_expression",
                    format!("failed to allocate space for enum variant: {}", e)))?;
            let mut current_value = variant_type.const_zero();
            for (i, field) in field_values.iter().enumerate() {
                let inserted = self.backend.builder.build_insert_value(
                    current_value,
                    *field,
                    i as u32,
                    &format!("{}_{}_insert_{}", scope_str, variant_name, i)
                ).map_err(|e| self.error("compile_expression",
                    format!("failed to insert enum variant field {}: {}", i, e)))?;
                current_value = match inserted {
                    inkwell::values::AggregateValueEnum::StructValue(v) => v,
                    _ => return Err(self.error("compile_expression",
                        "insert_value did not return a StructValue".to_string())),
                };
            }
            self.backend.builder.build_store(variant_ptr, current_value)
                .map_err(|e| self.error("compile_expression",
                    format!("failed to store enum variant value: {}", e)))?;
            // Named enum types map to ptr; return the alloca rather than a loaded struct.
            return Ok(variant_ptr.into());
        }
        self.compile_named_call(&format!("{}::{}", scope_str, variant_name), args)
    }

    /// Compile a payload field of `Enum.Variant(...)`.
    /// Unbound identifiers (match bindings such as `Option.Some(x)`) become
    /// i64 zeros and are registered so arm bodies can mention the name.
    fn compile_enum_payload_arg(&mut self, arg: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        if let Expression::Variable(name) = arg {
            if name != "_" && !self.variables.contains_key(name) && !name.contains('.') {
                let ty = self.backend.context.i64_type();
                let alloca = self.backend.builder.build_alloca(ty, name)
                    .map_err(|e| self.error("compile_expression",
                        format!("failed to allocate enum pattern binding '{}': {}", name, e)))?;
                let zero = ty.const_zero();
                self.backend.builder.build_store(alloca, zero)
                    .map_err(|e| self.error("compile_expression",
                        format!("failed to store enum pattern binding '{}': {}", name, e)))?;
                self.variables.insert(name.clone(), (alloca, ty.into()));
                return Ok(zero.into());
            }
        }
        self.compile_expr(arg)
    }

    pub(crate) fn compile_constructor_from_ast(&mut self, class_name: &str, args: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        let ctor_name = format!("{}_new", class_name);
        let ctor_fn = *self.functions.get(&ctor_name)
            .ok_or_else(|| self.error("compile_constructor_call",
                format!("constructor not found: '{}'", ctor_name)))?;
        let mut compiled = Vec::new();
        for arg in args {
            compiled.push(self.compile_expr(arg)?);
        }
        let call_result = self.backend.builder.build_call(
            ctor_fn,
            &compiled.iter().map(|a| (*a).into()).collect::<Vec<_>>(),
            &format!("{}_call", ctor_name)
        ).map_err(|e| self.error("compile_constructor_call",
            format!("failed to call constructor '{}': {}", ctor_name, e)))?;
        let return_value = match call_result.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("compile_constructor_call",
                    "constructor returned void (should return object)"));
            }
        };
        match return_value {
            BasicValueEnum::PointerValue(ptr) => Ok(ptr.into()),
            BasicValueEnum::IntValue(int_val) => {
                let ptr_type = self.backend.context.ptr_type(inkwell::AddressSpace::default());
                let ptr = self.backend.builder.build_int_to_ptr(int_val, ptr_type, &format!("{}_ptr", ctor_name))
                    .map_err(|e| self.error("compile_constructor_call",
                        format!("failed to convert int to ptr: {}", e)))?;
                Ok(ptr.into())
            }
            BasicValueEnum::StructValue(struct_val) => {
                let malloc_fn = self.functions.get("malloc")
                    .copied()
                    .ok_or_else(|| self.error("compile_constructor_call",
                        "malloc function not found for heap allocation"))?;
                let size = 64u64;
                let size_value = self.backend.context.i64_type().const_int(size, false);
                let heap_ptr = self.backend.builder.build_call(
                    malloc_fn,
                    &[size_value.into()],
                    &format!("{}_malloc", ctor_name)
                ).map_err(|e| self.error("compile_constructor_call",
                    format!("failed to call malloc: {}", e)))?;
                let heap_ptr_value = match heap_ptr.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(val) => val,
                    _ => return Err(self.error("compile_constructor_call",
                        "malloc returned unexpected value")),
                };
                let heap_ptr = match heap_ptr_value {
                    BasicValueEnum::PointerValue(ptr) => ptr,
                    _ => return Err(self.error("compile_constructor_call",
                        "malloc did not return a pointer")),
                };
                self.backend.builder.build_store(heap_ptr, struct_val)
                    .map_err(|e| self.error("compile_constructor_call",
                        format!("failed to store struct to heap: {}", e)))?;
                Ok(heap_ptr.into())
            }
            _ => Err(self.error("compile_constructor_call",
                "constructor did not return an object (should return struct or pointer)")),
        }
    }

    pub(crate) fn compile_named_call(&mut self, func_name: &str, args: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        if func_name.contains('.') && !func_name.contains("::") {
            let dot_pos = func_name.rfind('.').unwrap();
            let object_str = &func_name[..dot_pos];
            let method_name = &func_name[dot_pos + 1..];
            if self.variables.contains_key(object_str) {
                return self.compile_method_call_from_ast(object_str, method_name, args);
            }
        }
        self.compile_named_function_call(func_name, args)
    }

    pub(crate) fn compile_named_function_call(&mut self, func_name: &str, args: &[Expression]) -> Result<BasicValueEnum<'ctx>, String> {
        let function = match self.functions.get(func_name).copied() {
            Some(f) => f,
            None => {
                let qualified_name = if let Some(current_fn) = self.current_function {
                    let current_name = current_fn.get_name().to_str().unwrap_or("");
                    if current_name.contains('.') {
                        let parts: Vec<&str> = current_name.rsplitn(2, '.').collect();
                        if parts.len() == 2 {
                            let module_path = parts[1];
                            let qualified = format!("{}.{}", module_path, func_name);
                            if self.functions.contains_key(&qualified) {
                                Some(qualified)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(qual_name) = qualified_name {
                    *self.functions.get(&qual_name).unwrap()
                } else if func_name.contains('.') && !func_name.contains("::") {
                    self.declare_external_function(func_name)?
                } else if self.is_c_library_function(func_name) {
                    self.declare_external_function(func_name)?
                } else {
                    let mut help = String::new();
                    let candidates: Vec<String> = self.functions.keys()
                        .filter(|k| {
                            !k.starts_with("coffee_") && *k != "printf" && *k != "puts" && *k != "malloc" && *k != "free"
                        })
                        .cloned()
                        .collect();
                    let similar = crate::diagnostics::find_similar_names(func_name, &candidates, 2, 3);
                    if !similar.is_empty() {
                        help.push_str(&format!("\n  = help: did you mean {}?", similar.join(" or ")));
                    }
                    return Err(self.error("function_call", format!("cannot find function '{}' in this scope{}", func_name, help)));
                }
            }
        };

        let fixed_param_count = function.count_params() as usize;
        let mut param_types = Vec::new();
        for i in 0..args.len() {
            let param_type = if i < fixed_param_count {
                function.get_nth_param(i as u32)
                    .map(|p| p.get_type())
                    .ok_or_else(|| self.error("function_call",
                        format!("failed to get type for parameter {} of function '{}'", i, func_name)))?
            } else {
                self.backend.context.i64_type().into()
            };
            param_types.push(param_type);
        }

        let compiled_args: Vec<BasicValueEnum<'ctx>> = args.iter().enumerate()
            .map(|(i, arg)| {
                self.compile_expr(arg)
                    .map_err(|e| self.error("function_call",
                        format!("failed to compile argument {} in call to '{}': {}", i + 1, func_name, e)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if compiled_args.len() < fixed_param_count {
            return Err(self.error("function_call",
                format!("wrong number of arguments for function '{}'\n  = note: expected at least {} arguments, got {} arguments",
                    func_name, fixed_param_count, compiled_args.len())));
        }

        let mut converted_args = Vec::new();
        for (i, arg) in compiled_args.iter().enumerate() {
            let param_type = param_types[i];
            let arg_type = arg.get_type();
            if !self.are_types_compatible(arg_type, param_type) {
                let arg_type_str = self.type_to_string(arg_type);
                let param_type_str = self.type_to_string(param_type);
                return Err(self.error("function_call",
                    format!("type mismatch in argument {} of function '{}'\n  = note: expected type '{}', found type '{}'",
                        i + 1, func_name, param_type_str, arg_type_str)));
            }
            let converted_arg = self.convert_value_to_type(*arg, param_type, &format!("arg_{}", i))?;
            converted_args.push(converted_arg);
        }

        let args_ref: Vec<_> = converted_args.iter().map(|a| (*a).into()).collect();
        let call = self.backend.builder
            .build_call(function, &args_ref, "call")
            .map_err(|e| self.error("function_call",
                format!("failed to build call to '{}': {}", func_name, e)))?;
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => Ok(val),
            inkwell::values::ValueKind::Instruction(_) => {
                Ok(self.backend.context.i64_type().const_zero().into())
            }
        }
    }

    /// Compile function call from leftover source (test string compiler only).
    #[cfg(test)]
    pub(crate) fn compile_function_call(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let paren_pos = expr.find('(')
            .ok_or_else(|| self.error("function_call", "invalid function call syntax - missing '('"))?;
        let func_name = &expr[..paren_pos];
        let args_str = &expr[paren_pos+1..expr.len()-1];

        // Try to get the function
        let function = match self.functions.get(func_name).copied() {
            Some(f) => f,
            None => {
                // Try to resolve with current module prefix
                // If we're in a module function (e.g., std.math.sqrt), try to find func_name within same module
                let qualified_name = if let Some(current_fn) = self.current_function {
                    let current_name = current_fn.get_name().to_str().unwrap_or("");
                    if current_name.contains('.') {
                        // Extract module path (e.g., "std.math" from "std.math.sqrt")
                        let parts: Vec<&str> = current_name.rsplitn(2, '.').collect();
                        if parts.len() == 2 {
                            let module_path = parts[1];
                            let qualified = format!("{}.{}", module_path, func_name);
                            if self.functions.contains_key(&qualified) {
                                Some(qualified)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(qual_name) = qualified_name {
                    *self.functions.get(&qual_name).unwrap()
                } else if func_name.contains('.') && !func_name.contains("::") {
                    // Check if this is a method call: object.method
                    let dot_pos = func_name.rfind('.').unwrap();
                    let object_str = &func_name[..dot_pos];
                    let method_name = &func_name[dot_pos + 1..];

                    // Check if object_str is a variable
                    if self.variables.contains_key(object_str) {
                        // This is a method call, delegate to compile_method_call
                        return self.compile_method_call(object_str, method_name, args_str);
                    }

                    // Check if this is a qualified name (e.g., lib.utils.add)
                    // Create an external declaration for the imported function
                    self.declare_external_function(func_name)?
                } else if self.is_c_library_function(func_name) {
                    // Check if this is a C library function
                    // Declare it as an external function with C linkage
                    self.declare_external_function(func_name)?
                } else {
                    // Not found and not a qualified name - error
                    let mut help = String::new();

                    // Find similar function names
                    let candidates: Vec<String> = self.functions.keys()
                        .filter(|k| {
                            // Skip internal functions
                            !k.starts_with("coffee_") && *k != "printf" && *k != "puts" && *k != "malloc" && *k != "free"
                        })
                        .cloned()
                        .collect();

                    let similar = crate::diagnostics::find_similar_names(func_name, &candidates, 2, 3);

                    if !similar.is_empty() {
                        help.push_str(&format!("\n  = help: did you mean {}?", similar.join(" or ")));
                    }

                    // List available functions
                    let available: Vec<_> = self.functions.keys()
                        .filter(|k| !k.starts_with("coffee_") && *k != "printf" && *k != "puts" && *k != "malloc" && *k != "free")
                        .map(|k| format!("'{}'", k))
                        .collect();

                    if !available.is_empty() {
                        help.push_str(&format!("\n  = note: available functions: {}", available.join(", ")));
                    }

                    return Err(self.error("function_call", format!("cannot find function '{}' in this scope{}", func_name, help)));
                }
            }
        };

        // Parse arguments handling nested parentheses
        let args_list = if args_str.trim().is_empty() {
            vec![]
        } else {
            self.split_function_args(args_str)?
        };

        coffee_debug!("DEBUG: compile_function_call: func_name='{}', args_str='{}', args_list={:?}", func_name, args_str, args_list);

        // Type inference: Get parameter types first for context
        let mut param_types = Vec::new();
        let fixed_param_count = function.count_params() as usize;

        for i in 0..args_list.len() {
            let param_type = if i < fixed_param_count {
                // Fixed parameter - get type from function signature
                function.get_nth_param(i as u32)
                    .map(|p| p.get_type())
                    .ok_or_else(|| self.error("function_call",
                        format!("failed to get type for parameter {} of function '{}'", i, func_name)))?
            } else {
                // Variadic parameter - infer type from argument
                // For printf-like functions, we need to infer the type from the argument itself
                // Try to infer type from the argument string
                let arg_str = args_list[i].trim();
                if arg_str.starts_with('"') && arg_str.ends_with('"') {
                    // String literal
                    self.backend.context.ptr_type(inkwell::AddressSpace::default()).into()
                } else if arg_str.parse::<i64>().is_ok() {
                    // Integer literal
                    self.backend.context.i64_type().into()
                } else if arg_str.parse::<f64>().is_ok() {
                    // Float literal
                    self.backend.context.f64_type().into()
                } else {
                    // Variable - try to get type from symbol table
                    if let Some(&(_ptr, var_type)) = self.variables.get(arg_str) {
                        // var_type is already a BasicTypeEnum
                        var_type
                    } else {
                        // Default to i64 for unknown types
                        self.backend.context.i64_type().into()
                    }
                }
            };
            param_types.push(param_type);
        }

        // Compile arguments with type inference context
        let args: Vec<BasicValueEnum<'ctx>> = args_list.iter().enumerate()
            .map(|(i, arg)| {
                // Create type inference context with expected parameter type
                let inference_ctx = TypeInferenceContext::with_expected_type(param_types[i]);
                self.compile_expression_str_with_inference(arg.trim(), &inference_ctx)
                    .map_err(|e| self.error("function_call",
                        format!("failed to compile argument {} in call to '{}': {}", i + 1, func_name, e)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // HIGH-7 FIX: Validate argument count matches function signature
        // Calling a function with wrong number of arguments causes undefined behavior
        // For variadic functions (like printf), allow more arguments than fixed parameters
        let func_param_count = function.count_params() as usize;
        if args.len() < func_param_count {
            return Err(self.error("function_call",
                format!("wrong number of arguments for function '{}'\n  = note: expected at least {} arguments, got {} arguments\n  = help: check function signature and provide correct number of arguments",
                    func_name, func_param_count, args.len())));
        }

        // Type inference: Convert arguments to match parameter types
        let mut converted_args = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let param_type = param_types[i];

            let arg_type = arg.get_type();

            // Check if types are compatible before conversion
            if !self.are_types_compatible(arg_type, param_type) {
                let arg_type_str = self.type_to_string(arg_type);
                let param_type_str = self.type_to_string(param_type);

                return Err(self.error("function_call",
                    format!("type mismatch in argument {} of function '{}'\n  = note: expected type '{}', found type '{}'\n  = help: these types are incompatible and cannot be implicitly converted\n  = help: check the function signature or provide a value of the correct type",
                        i + 1, func_name, param_type_str, arg_type_str)));
            }

            let converted_arg = self.convert_value_to_type(*arg, param_type, &format!("arg_{}", i))?;
            converted_args.push(converted_arg);
        }

        let args_ref: Vec<_> = converted_args.iter().map(|a| (*a).into()).collect();

        let call = self.backend.builder
            .build_call(function, &args_ref, "call")
            .map_err(|e| self.error("function_call",
                format!("failed to build call to '{}': {}", func_name, e)))?;

        // Handle return value - try to get basic value from call
        // For void functions, return a dummy value
        let return_value = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                // Void return - return a dummy value
                self.backend.context.i64_type().const_zero().into()
            }
        };

        // No return value conversion - semantic analysis should have already verified type compatibility
        Ok(return_value)
    }
}
