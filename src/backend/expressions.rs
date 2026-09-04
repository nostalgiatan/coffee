//! Expression compilation for Coffee compiler
//!
//! Compiles expressions including literals, variables, binary/unary operations,
//! function calls, array indexing, and format strings.

use super::codegen::CodeGenerator;
use inkwell::values::{BasicValueEnum, PointerValue};
use crate::backend::type_inference::TypeInferenceContext;
use crate::backend::type_inference;
use inkwell::types::BasicTypeEnum;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile expression from string
    pub fn compile_expression_str(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
        eprintln!("DEBUG: compile_expression_str: expr='{}', len={}", expr, expr.len());
        self.compile_expression_str_with_inference(expr, &TypeInferenceContext::new())
    }

    /// Compile expression from string with type inference
    pub fn compile_expression_str_with_inference(&mut self, expr: &str, inference_ctx: &TypeInferenceContext<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        // HIGH-13: Track expression depth to prevent stack overflow
        self.expression_depth += 1;
        if self.expression_depth > self.max_expression_depth {
            self.expression_depth -= 1;
            return Err(self.error("compile_expression",
                format!("expression nesting depth {} exceeds maximum {}",
                    self.expression_depth, self.max_expression_depth)));
        }

        let result = self.compile_expression_str_inner_with_inference(expr, inference_ctx);

        // Always decrement depth before returning
        self.expression_depth -= 1;
        result
    }

    /// Inner expression compilation with type inference
    fn compile_expression_str_inner_with_inference(&mut self, expr: &str, inference_ctx: &TypeInferenceContext<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        eprintln!("DEBUG: compile_expression_str_inner_with_inference: expr='{}', len={}", expr, expr.len());
        
        // Check if it's a simple literal value (not containing operators or function calls)
        // But also check if it's a variable reference (alphanumeric + underscore)
        // IMPORTANT: Member access (object.field) should NOT be treated as simple literal
        // IMPORTANT: Struct literals (Point { x: 10 }) should NOT be treated as simple literal
        let is_simple_literal = !expr.contains('(') && !expr.contains('[') && !expr.contains('+') && !expr.contains('-') && !expr.contains('*') && !expr.contains('/') && !expr.contains('%') && !expr.contains('=') && !expr.contains('!') && !expr.contains('&') && !expr.contains('|') && !expr.contains('^') && !expr.contains('<') && !expr.contains('>') && !expr.contains('?') && !expr.contains(':') && !expr.contains('.') && !expr.contains('{');
        
        // Check if it's a variable reference (starts with letter or underscore, contains only alphanumeric and underscore)
        // Exclude expressions with '::' (qualified names like Class::method)
        let is_variable = !expr.contains("::") && expr.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) 
            && expr.chars().all(|c| c.is_alphanumeric() || c == '_');
        
        eprintln!("DEBUG: compile_expression_str_inner_with_inference: is_simple_literal={}, is_variable={}", is_simple_literal, is_variable);
        
        if is_simple_literal && !is_variable {
            // It's a simple literal (number, boolean, string) - use type inference
            return type_inference::compile_literal_with_inference(
                self.backend.context,
                &self.backend.builder,
                expr,
                inference_ctx
            );
        }

        // For complex expressions or variable references, fall back to normal compilation
        self.compile_expression_str_inner(expr)
    }

    /// Inner expression compilation (called by compile_expression_str)
    /// This doesn't manage depth tracking - that's handled by the outer function
    fn compile_expression_str_inner(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
        eprintln!("DEBUG: compile_expression_str_inner: expr='{}', len={}", expr, expr.len());
        
        // IMPORTANT: Strip inline comments before processing the expression
        // This prevents comments from being parsed as part of the expression
        let expr = self.strip_inline_comments(expr);
        
        eprintln!("DEBUG: compile_expression_str_inner: after strip_inline_comments, expr='{}', len={}", expr, expr.len());

        // Struct literal: Point { x: 10, y: 20 }
        // MUST come before binary operation check to avoid splitting struct literals
        if let Some(pos) = expr.find('{') {
            if expr.ends_with('}') {
                let struct_name = expr[..pos].trim();
                let fields_str = &expr[pos + 1..expr.len() - 1].trim();
                eprintln!("DEBUG: compile_expression_str_inner: struct literal, struct_name='{}', fields_str='{}'", struct_name, fields_str);
                return self.compile_struct_literal(struct_name, fields_str);
            }
        }
        
                // Handle unary operators (-, !)
                if expr.starts_with('-') || expr.starts_with('!') {
                    let op = &expr[..1];
                    let operand_str = expr[1..].trim();
                    let operand = self.compile_expression_str(operand_str)?;
        
                    return match op {
                        "-" => {
                            // Negation operation with overflow checking
                            match operand {
                                BasicValueEnum::IntValue(i) => {
                                    Ok(self.arithmetic_ctx.build_int_neg(
                                        self.backend.context,
                                        &self.backend.builder,
                                        i
                                    )?.into())
                                }
                                BasicValueEnum::FloatValue(f) => {
                                    // Float negation (no overflow possible for floats)
                                    let negated = self.backend.builder.build_float_neg(f, "fneg")
                                        .map_err(|e| self.error("compile_expression",
                                            format!("failed to build float negation: {}", e)))?;
                                    Ok(negated.into())
                                }
                                _ => Err(self.error("compile_expression",
                                    format!("negation not supported for this type: {}", operand_str))),
                            }
                        }
                        "!" => {
                            // Logical NOT
                            match operand {
                                BasicValueEnum::IntValue(i) => {
                                    // XOR with 1 to flip boolean
                                    let one = i.get_type().const_int(1, false);
                                    Ok(self.backend.builder.build_xor(i, one, "not")
                                        .map_err(|e| self.error("compile_expression",
                                            format!("failed to build logical NOT: {}", e)))?
                                        .into())
                                }
                                _ => Err(self.error("compile_expression",
                                    format!("logical NOT not supported for this type: {}", operand_str))),
                            }
                        }
                        _ => Err(self.error("compile_expression",
                            format!("unknown unary operator: {}", op))),
                    };
                }
        
                // Integer literal
                if let Ok(i) = expr.parse::<i64>() {
                    // For now, use i64 as default, but in the future, we should infer the type from context
                    return Ok(self.backend.context.i64_type().const_int(i as u64, true).into());
                }
        
                // Float literal
                if let Ok(f) = expr.parse::<f64>() {
                    return Ok(self.backend.context.f64_type().const_float(f).into());
                }
        
                // Boolean literal (i8 for C compatibility)
                if expr == "true" {
                    return Ok(self.backend.context.i8_type().const_int(1, false).into());
                }
                if expr == "false" {
                    return Ok(self.backend.context.i8_type().const_int(0, false).into());
                }
        
                // Check for invalid operators before function call parsing
                if expr.contains("**") {
                    return Err(self.error("compile_expression",
                        format!("syntax error: '**' is not a valid operator in Coffee\n  = note: Coffee does not have a power operator\n  = help: use a function or implement your own power function")));
                }
        
                if expr.contains("@") {
                    return Err(self.error("compile_expression",
                        format!("syntax error: '@' is not a valid operator in Coffee\n  = note: '@' is not a supported operator\n  = help: use supported operators: +, -, *, /, %, <<, >>, ==, !=, <, >, <=, >=")));
                }
        
                // Check for increment/decrement operators
                if expr.contains("++") || expr.contains("--") {
                    return Err(self.error("compile_expression",
                        format!("syntax error: '{}' is not valid in Coffee expressions\n  = note: Coffee does not support increment/decrement operators\n  = help: use 'x = x + 1' or 'x = x - 1' instead", if expr.contains("++") { "++" } else { "--" })));
                }
        
        
                // Format string (f"...")
                if expr.starts_with('f') && expr.len() > 2 {
                    let inner = &expr[1..]; // Remove 'f' prefix
                    if (inner.starts_with('"') && inner.ends_with('"')) || (inner.starts_with('\'') && inner.ends_with('\'')) {
                        let string_content = &inner[1..inner.len()-1];
                        return self.compile_fstring(string_content);
                    }
                }
        
                // String literal
                if expr.starts_with('"') && expr.ends_with('"') {
                    let string_content = &expr[1..expr.len()-1];
                    return self.build_string_constant(string_content);
                }
        
                // Tuple literal: (10, 20) or (10, 20, 30)
        // MUST come before parenthesized expression check to avoid conflicts
        if expr.starts_with('(') && expr.ends_with(')') {
            let inner = &expr[1..expr.len() - 1].trim();
            
            // Check if this is a tuple literal (contains comma)
            if inner.contains(',') {
                // Parse tuple elements
                let mut elements = Vec::new();
                let mut current = String::new();
                let mut depth = 0;
                let mut in_string = false;
                let mut escape_next = false;
                
                for ch in inner.chars() {
                    if escape_next {
                        current.push(ch);
                        escape_next = false;
                        continue;
                    }
                    
                    match ch {
                        '\\' => {
                            escape_next = true;
                            current.push(ch);
                        }
                        '"' if !in_string => {
                            in_string = true;
                            current.push(ch);
                        }
                        '"' if in_string => {
                            in_string = false;
                            current.push(ch);
                        }
                        '(' if !in_string => {
                            depth += 1;
                            current.push(ch);
                        }
                        ')' if !in_string => {
                            depth -= 1;
                            current.push(ch);
                        }
                        ',' if !in_string && depth == 0 => {
                            let elem = current.trim().to_string();
                            if !elem.is_empty() {
                                elements.push(elem);
                            }
                            current.clear();
                        }
                        _ => {
                            current.push(ch);
                        }
                    }
                }
                if !current.trim().is_empty() {
                    elements.push(current.trim().to_string());
                }
                
                if !elements.is_empty() {
                    // Compile each element
                    let mut compiled_elements = Vec::new();
                    for elem in elements {
                        let compiled = self.compile_expression_str(&elem)
                            .map_err(|e| self.error("compile_expression",
                                format!("failed to compile tuple element '{}': {}", elem, e)))?;
                        compiled_elements.push(compiled);
                    }
                    
                    // Create tuple type
                    let element_types: Vec<BasicTypeEnum> = compiled_elements.iter()
                        .map(|v| v.get_type())
                        .collect();
                    
                    let tuple_type = self.backend.context.struct_type(&element_types, false);
                    
                    // Allocate space for the tuple
                    let tuple_ptr = self.backend.builder.build_alloca(tuple_type, "tuple")
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to allocate space for tuple: {}", e)))?;
                    
                    // Insert each element into the tuple using insert_value
                    let mut current_value = tuple_type.const_zero();
                    for (i, elem) in compiled_elements.iter().enumerate() {
                        let inserted = self.backend.builder.build_insert_value(
                            current_value,
                            *elem,
                            i as u32,
                            &format!("tuple_insert_{}", i)
                        ).map_err(|e| self.error("compile_expression",
                            format!("failed to insert tuple element {}: {}", i, e)))?;
                        current_value = match inserted {
                            inkwell::values::AggregateValueEnum::StructValue(v) => v,
                            _ => return Err(self.error("compile_expression",
                                format!("insert_value did not return a StructValue"))),
                        };
                    }
                    
                    // Store the tuple value
                    self.backend.builder.build_store(tuple_ptr, current_value)
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to store tuple value: {}", e)))?;
                    
                    // Load the tuple
                    let loaded = self.backend.builder.build_load(tuple_type, tuple_ptr, "tuple_loaded")
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to load tuple: {}", e)))?;
                    
                    return Ok(loaded);
                }
            }
        }
        
                // Parenthesized expression: strip outer parentheses
                if expr.starts_with('(') && expr.ends_with(')') {
                    // Check if parentheses are balanced
                    let mut depth = 0;
                    let mut all_balanced = true;
                    for (i, ch) in expr.chars().enumerate() {
                        if ch == '(' {
                            depth += 1;
                        } else if ch == ')' {
                            depth -= 1;
                            // If depth becomes 0 before the end, the outer parens don't match
                            if depth == 0 && i < expr.len() - 1 {
                                all_balanced = false;
                                break;
                            }
                        }
                    }
        
                    // Only strip if the outer parentheses are the ONLY pair wrapping the whole expression
                    if all_balanced && depth == 0 {
                        let inner = &expr[1..expr.len()-1];
                        return self.compile_expression_str(inner);
                    }
                }
        
                // Binary operation
                if let Some(result) = self.try_compile_binary_op(&expr)? {
                    return Ok(result);
                }
        // Tuple literal: (10, 20) or (10, 20, 30)
        // MUST come before struct literal check to avoid conflicts
        if expr.starts_with('(') && expr.ends_with(')') {
            let inner = &expr[1..expr.len() - 1].trim();
            
            // Check if this is a tuple literal (contains comma)
            if inner.contains(',') {
                // Parse tuple elements
                let mut elements = Vec::new();
                let mut current = String::new();
                let mut depth = 0;
                let mut in_string = false;
                let mut escape_next = false;
                
                for ch in inner.chars() {
                    if escape_next {
                        current.push(ch);
                        escape_next = false;
                        continue;
                    }
                    
                    match ch {
                        '\\' => {
                            escape_next = true;
                            current.push(ch);
                        }
                        '"' if !in_string => {
                            in_string = true;
                            current.push(ch);
                        }
                        '"' if in_string => {
                            in_string = false;
                            current.push(ch);
                        }
                        '(' if !in_string => {
                            depth += 1;
                            current.push(ch);
                        }
                        ')' if !in_string => {
                            depth -= 1;
                            current.push(ch);
                        }
                        ',' if !in_string && depth == 0 => {
                            let elem = current.trim().to_string();
                            if !elem.is_empty() {
                                elements.push(elem);
                            }
                            current.clear();
                        }
                        _ => {
                            current.push(ch);
                        }
                    }
                }
                if !current.trim().is_empty() {
                    elements.push(current.trim().to_string());
                }
                
                if !elements.is_empty() {
                    // Compile each element
                    let mut compiled_elements = Vec::new();
                    for elem in elements {
                        let compiled = self.compile_expression_str(&elem)
                            .map_err(|e| self.error("compile_expression",
                                format!("failed to compile tuple element '{}': {}", elem, e)))?;
                        compiled_elements.push(compiled);
                    }
                    
                    // Create tuple type
                    let element_types: Vec<BasicTypeEnum> = compiled_elements.iter()
                        .map(|v| v.get_type())
                        .collect();
                    
                    let tuple_type = self.backend.context.struct_type(&element_types, false);
                    
                    // Create tuple value
                    let tuple_value = tuple_type.const_named_struct(&compiled_elements);
                    
                    // Allocate space for the tuple
                    let tuple_ptr = self.backend.builder.build_alloca(tuple_type, "tuple")
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to allocate space for tuple: {}", e)))?;
                    
                    // Store the tuple value
                    self.backend.builder.build_store(tuple_ptr, tuple_value)
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to store tuple value: {}", e)))?;
                    
                    // Load the tuple
                    let loaded = self.backend.builder.build_load(tuple_type, tuple_ptr, "tuple_loaded")
                        .map_err(|e| self.error("compile_expression",
                            format!("failed to load tuple: {}", e)))?;
                    
                    return Ok(loaded);
                }
            }
        }
        
        // Struct literal: Point { x: 10, y: 20 }
        // MUST come before binary operation check to avoid splitting struct literals
        if let Some(pos) = expr.find('{') {
            if expr.ends_with('}') {
                let struct_name = expr[..pos].trim();
                let fields_str = &expr[pos + 1..expr.len() - 1].trim();
                return self.compile_struct_literal(struct_name, fields_str);
            }
        }

        // Array indexing: array[index]
        // Must come before function calls since [ has higher precedence
        if let Some(pos) = expr.find('[') {
            if expr.ends_with(']') {
                let array_str = &expr[..pos].trim();
                let index_str = &expr[pos + 1..expr.len() - 1].trim();

                return self.compile_array_index(array_str, index_str);
            }
        }

        // Function call and constructor call
        // IMPORTANT: Must come before member access to avoid parsing function calls as member access
        // Example: printf("Point: %d, %d\n", p.x, p.y) should be parsed as function call, not member access
        eprintln!("DEBUG: compile_expression_str_inner: expr='{}', expr.contains('(')={}, expr.ends_with(')')={}", expr, expr.contains('('), expr.ends_with(')'));
        if expr.contains('(') && expr.ends_with(')') {
            eprintln!("DEBUG: compile_expression_str_inner: expr contains '(' and ends with ')', expr='{}'", expr);

            // Check if this is a type conversion function: int(value), float(value), bool(value)
            let paren_pos = expr.find('(').unwrap();
            let func_name = &expr[..paren_pos].trim();
            if *func_name == "int" || *func_name == "float" || *func_name == "bool" {
                eprintln!("DEBUG: compile_expression_str_inner: type conversion function '{}'", func_name);
                let args_str = &expr[paren_pos + 1..expr.len() - 1].trim();
                let value = self.compile_expression_str(args_str)?;
                
                match *func_name {
                    "int" => {
                        // Convert float to int
                        match value {
                            BasicValueEnum::FloatValue(f) => {
                                let result = self.backend.builder.build_float_to_signed_int(
                                    f,
                                    self.backend.context.i64_type(),
                                    "fptosi"
                                ).map_err(|e| self.error("compile_expression",
                                    format!("failed to convert float to int: {}", e)))?;
                                return Ok(result.into());
                            }
                            BasicValueEnum::IntValue(i) => {
                                // Already an int, just return it
                                return Ok(i.into());
                            }
                            _ => {
                                return Err(self.error("compile_expression",
                                    format!("cannot convert type to int: unsupported type")));
                            }
                        }
                    }
                    "float" => {
                        // Convert int to float
                        match value {
                            BasicValueEnum::IntValue(i) => {
                                let result = self.backend.builder.build_signed_int_to_float(
                                    i,
                                    self.backend.context.f64_type(),
                                    "sitofp"
                                ).map_err(|e| self.error("compile_expression",
                                    format!("failed to convert int to float: {}", e)))?;
                                return Ok(result.into());
                            }
                            BasicValueEnum::FloatValue(f) => {
                                // Already a float, just return it
                                return Ok(f.into());
                            }
                            _ => {
                                return Err(self.error("compile_expression",
                                    format!("cannot convert type to float: unsupported type")));
                            }
                        }
                    }
                    "bool" => {
                        // Convert to bool (i8)
                        match value {
                            BasicValueEnum::IntValue(i) => {
                                // Truncate or extend to i8
                                let result = self.backend.builder.build_int_truncate_or_bit_cast(
                                    i,
                                    self.backend.context.i8_type(),
                                    "trunc"
                                ).map_err(|e| self.error("compile_expression",
                                    format!("failed to convert to bool: {}", e)))?;
                                return Ok(result.into());
                            }
                            BasicValueEnum::FloatValue(f) => {
                                // Convert float to i64, then to i8
                                let i64_val = self.backend.builder.build_float_to_signed_int(
                                    f,
                                    self.backend.context.i64_type(),
                                    "fptosi"
                                ).map_err(|e| self.error("compile_expression",
                                    format!("failed to convert float to bool: {}", e)))?;
                                let result = self.backend.builder.build_int_truncate_or_bit_cast(
                                    i64_val,
                                    self.backend.context.i8_type(),
                                    "trunc"
                                ).map_err(|e| self.error("compile_expression",
                                    format!("failed to convert to bool: {}", e)))?;
                                return Ok(result.into());
                            }
                            _ => {
                                return Err(self.error("compile_expression",
                                    format!("cannot convert type to bool: unsupported type")));
                            }
                        }
                    }
                    _ => {
                        return Err(self.error("compile_expression",
                            format!("unknown type conversion function: {}", func_name)));
                    }
                }
            }

            // Check if this is an enum variant: Enum::Variant(args)
            if expr.contains("::") && !expr.contains("::new") {
                let paren_pos = expr.find('(').unwrap();
                let func_name = &expr[..paren_pos];

                if func_name.contains("::") {
                    // Find the last "::" to get the scope and variant name
                    let scope_pos = func_name.rfind("::").unwrap();
                    let scope_str = &func_name[..scope_pos];
                    let variant_name = &func_name[scope_pos + 2..]; // Skip "::"

                    // Check if scope_str is an enum type
                    eprintln!("DEBUG: compile_expression_str_inner: checking if '{}' is an enum type", scope_str);
                    if self.enums.contains_key(&scope_str.to_string()) {
                        eprintln!("DEBUG: compile_expression_str_inner: '{}' is an enum type, constructing variant '{}'", scope_str, variant_name);
                        
                        // Compile arguments
                        let args_str = &expr[paren_pos + 1..expr.len() - 1].trim();
                        let args: Vec<&str> = if args_str.is_empty() {
                            vec![]
                        } else {
                            args_str.split(',').map(|s| s.trim()).collect()
                        };
                        
                        // Get the enum type
                        if let Some(enum_def) = self.enums.get(&scope_str.to_string()) {
                            // Find the variant
                            if let Some(variant) = enum_def.variants.iter().find(|v| v.name == variant_name) {
                                // Compile arguments
                                let compiled_args: Vec<BasicValueEnum<'ctx>> = args.iter()
                                    .map(|arg| self.compile_expression_str(arg))
                                    .collect::<Result<Vec<_>, _>>()?;
                                
                                // Create a struct for the variant (tag + fields)
                                // Tag is stored as the first field (variant index)
                                let tag_value = self.backend.context.i64_type().const_int(1, false);
                                
                                // Create fields for the variant
                                let mut field_values = vec![BasicValueEnum::IntValue(tag_value)];
                                field_values.extend(compiled_args);
                                
                                // Get LLVM types for fields
                                let field_types: Vec<BasicTypeEnum<'ctx>> = field_values.iter()
                                    .map(|v| v.get_type())
                                    .collect();
                                
                                let variant_type = self.backend.context.struct_type(&field_types, false);
                                
                                // Create variant value
                                let variant_value = variant_type.const_named_struct(&field_values);
                                
                                // Allocate space for the variant
                                let variant_ptr = self.backend.builder.build_alloca(variant_type, &format!("{}_{}", scope_str, variant_name))
                                    .map_err(|e| self.error("compile_expression",
                                        format!("failed to allocate space for enum variant: {}", e)))?;
                                
                                // Store the variant value
                                self.backend.builder.build_store(variant_ptr, variant_value)
                                    .map_err(|e| self.error("compile_expression",
                                        format!("failed to store enum variant value: {}", e)))?;
                                
                                // Load the variant
                                let loaded = self.backend.builder.build_load(variant_type, variant_ptr, &format!("{}_{}_loaded", scope_str, variant_name))
                                    .map_err(|e| self.error("compile_expression",
                                        format!("failed to load enum variant: {}", e)))?;
                                
                                return Ok(loaded);
                            }
                        }
                    }
                }
            }

            // Check if this is a method call: object.method(args)
            // Method calls have the form: object.method(args) where object.method contains a dot
            // but NOT ::new (constructor call)
            if expr.contains('.') && !expr.contains("::new") {
                let paren_pos = expr.find('(').unwrap();
                let func_name = &expr[..paren_pos];

                // Check if func_name is a variable name (e.g., p.move is not a variable, but p is)
                if !self.variables.contains_key(func_name) && func_name.contains('.') {
                    // This is likely a method call
                    let dot_pos = func_name.rfind('.').unwrap();
                    let object_str = &func_name[..dot_pos];
                    let method_name = &func_name[dot_pos + 1..];
                    let args_str = &expr[paren_pos + 1..expr.len() - 1].trim();
                    eprintln!("DEBUG: compile_expression_str_inner: detected method call, object_str='{}', method_name='{}', args_str='{}'", object_str, method_name, args_str);
                    return self.compile_method_call(object_str, method_name, args_str);
                }
            }

            eprintln!("DEBUG: compile_expression_str_inner: expr.contains(\"::new\")={}", expr.contains("::new"));
            // Check if this is a constructor call: class::new(args)
            if expr.contains("::new") {
                let parts: Vec<&str> = expr.split("::").collect();
                eprintln!("DEBUG: constructor call: expr='{}', expr.len()={}, parts={:?}", expr, expr.len(), parts);
                if parts.len() == 2 {
                    eprintln!("DEBUG: constructor call: parts[0]='{}', parts[0].len()={}, parts[1]='{}', parts[1].len()={}", parts[0], parts[0].len(), parts[1], parts[1].len());
                    if parts[1].starts_with("new") {
                        let class_name = parts[0].trim();
                        let full_method = parts[1].trim();
                        eprintln!("DEBUG: constructor call: class_name='{}', full_method='{}', full_method.len()={}", class_name, full_method, full_method.len());
                        let args_str = &full_method[4..full_method.len()-1].trim(); // Remove "new(" prefix and ")" suffix
                        eprintln!("DEBUG: constructor call: args_str='{}', args_str.len()={}", args_str, args_str.len());

                        return self.compile_constructor_call(class_name, args_str);
                    }
                }
            }

            return self.compile_function_call(&expr);
        }

        // Member access: object.field or object.method(args)
        // Check for '.' that is not part of a float literal
        // IMPORTANT: Skip this check if expression contains '::' (constructor call or enum variant)
        if !expr.starts_with('"') && !expr.ends_with('"') && !expr.contains("::") {
            if let Some(pos) = expr.rfind('.') {
                let object_str = &expr[..pos].trim();
                let rest = &expr[pos + 1..].trim();

                // Check if this is actually a float literal
                let is_float_literal = object_str.parse::<f64>().is_ok() && rest.parse::<u64>().is_ok();

                // If it's not a float literal, treat as member access
                if !is_float_literal {
                    // Check if this is an enum variant: Enum.Variant or Enum.Variant(args)
                    // Convert to Enum::Variant syntax for the backend
                    eprintln!("DEBUG: compile_expression_str_inner: checking if '{}' is an enum type, enums.keys() = {:?}", object_str, self.enums.keys().collect::<Vec<_>>());
                    if self.enums.contains_key(&object_str.to_string()) {
                        eprintln!("DEBUG: compile_expression_str_inner: '{}' is an enum type", object_str);
                        // This is an enum variant: Enum.Variant or Enum.Variant(args)
                        let variant_name = if rest.contains('(') && rest.ends_with(')') {
                            // Enum.Variant(args) - extract variant name
                            &rest[..rest.find('(').unwrap()]
                        } else {
                            // Enum.Variant - simple variant
                            rest
                        };

                        let global_name = format!("{}_{}", object_str, variant_name);
                        eprintln!("DEBUG: compile_expression_str_inner: loading enum variant '{}'", global_name);

                        // Try to load the global constant
                        if let Some(global) = self.backend.module.get_global(&global_name) {
                            let value = global.as_pointer_value();
                            let loaded = self.backend.builder.build_load(
                                self.backend.context.i64_type(),
                                value,
                                &global_name
                            ).map_err(|e| self.error("compile_expression",
                                    format!("failed to load enum variant '{}': {}", expr, e)))?;
                            return Ok(loaded);
                        }
                    } else {
                        eprintln!("DEBUG: compile_expression_str_inner: '{}' is NOT an enum type", object_str);
                    }

                    // Check if this is a method call: object.method(args)
                    if rest.contains('(') && rest.ends_with(')') {
                        let method_name = &rest[..rest.find('(').unwrap()];
                        let args_str = &rest[method_name.len() + 1..rest.len() - 1].trim();
                        return self.compile_method_call(object_str, method_name, args_str);
                    } else {
                        // Field access: object.field
                        return self.compile_field_access(object_str, rest);
                    }
                }
            }
        }
        // Enum variant: Enum::Variant or Enum::Variant(args)
        if expr.contains("::") {
            let parts: Vec<&str> = expr.split("::").collect();
            if parts.len() == 2 {
                let enum_name = parts[0].trim();
                let variant_part = parts[1].trim();
                
                // Check if this is a variant with parameters: Variant(args)
                let (variant_name, args_str) = if variant_part.contains('(') {
                    let paren_pos = variant_part.find('(').unwrap();
                    let name = &variant_part[..paren_pos];
                    let args_part = &variant_part[paren_pos..];
                    if args_part.ends_with(')') {
                        (name, &args_part[1..args_part.len()-1])
                    } else {
                        (variant_part, "")
                    }
                } else {
                    (variant_part, "")
                };
                
                let global_name = format!("{}_{}", enum_name, variant_name);
                
                // Check if variant has parameters
                if !args_str.is_empty() {
                    // Get enum definition to check variant fields
                    if let Some(enum_def) = self.enums.get(enum_name) {
                        if let Some(variant) = enum_def.variants.iter().find(|v| v.name == variant_name) {
                            if !variant.fields.is_empty() {
                                // This is a variant with parameters - create a struct value
                                // First, get or create the struct type for this variant
                                let struct_name = format!("{}_{}", enum_name, variant_name);
                                
                                // Compile arguments
                                let mut args = Vec::new();
                                if !args_str.is_empty() {
                                    let mut current_arg = String::new();
                                    let mut depth = 0;
                                    for ch in args_str.chars() {
                                        match ch {
                                            '(' if !current_arg.is_empty() => depth += 1,
                                            ')' if depth > 0 => depth -= 1,
                                            ',' if depth == 0 => {
                                                if !current_arg.trim().is_empty() {
                                                    args.push(current_arg.trim().to_string());
                                                }
                                                current_arg.clear();
                                            }
                                            _ => current_arg.push(ch),
                                        }
                                    }
                                    if !current_arg.trim().is_empty() {
                                        args.push(current_arg.trim().to_string());
                                    }
                                }
                                
                                // Compile argument values
                                let mut compiled_args = Vec::new();
                                for arg in &args {
                                    compiled_args.push(self.compile_expression_str(arg)?);
                                }
                                
                                // Get or create struct type
                                let field_types: Vec<BasicTypeEnum> = compiled_args.iter().map(|arg| arg.get_type()).collect();
                                let struct_type = self.backend.context.struct_type(&field_types, false);
                                
                                // Create struct value
                                let struct_value = struct_type.const_named_struct(&compiled_args);
                                
                                return Ok(struct_value.into());
                            }
                        }
                    }
                    
                    // If we get here, the variant doesn't have fields but arguments were provided
                    return Err(self.error("compile_expression",
                        format!("enum variant '{}' does not take arguments", variant_name)));
                }
                
                // Simple variant without parameters - load global constant
                if let Some(global) = self.backend.module.get_global(&global_name) {
                    let value = global.as_pointer_value();
                    let loaded = self.backend.builder.build_load(
                        self.backend.context.i64_type(),
                        value,
                        &global_name
                    ).map_err(|e| self.error("compile_expression",
                        format!("failed to load enum variant '{}': {}", expr, e)))?;
                    return Ok(loaded);
                }
            }
        }

        // Command-line argument reference (arg1, arg2, etc.)
        if let Some(arg_num) = Self::parse_arg_number(&expr) {
            // Check if we're in main function and have access to argc/argv
            if let (Some(argv), Some(argc)) = (self.main_argv, self.main_argc) {
                return self.get_command_line_arg(argv, argc, arg_num);
            } else {
                return Err(self.error("compile_expression",
                    format!("command-line argument '{}' can only be used in the main() statement\n  = help: Use 'main(entry_function(arg1, arg2, ...))' to pass command-line arguments to your entry function\n  = example: main(my_entry(arg1)) will pass the first command-line argument to my_entry()", expr)));
            }
        }

        // Variable reference
        if let Some(&(ptr, var_type)) = self.variables.get(&expr) {
            eprintln!("DEBUG: compile_expression_str_inner: found variable '{}' in variables", expr);
            
            // HIGH-10 FIX: Mark variable as used when referenced
            self.used_variables.insert(expr.clone());

            // 生命周期跟踪：记录变量使用
            self.memory_ctx.record_use(&expr);

            // Check if variable has been moved (use-after-move protection)
            if self.memory_ctx.is_moved(&expr) {
                return Err(self.error("compile_expression",
                    format!("use of moved variable: '{}'\n  = note: variable was moved and is no longer valid\n  = help: consider using 'clone' before move to preserve the original value", expr)));
            }

            // Check if variable has been dropped (use-after-free protection)
            if self.memory_ctx.is_dropped(&expr) {
                return Err(self.error("compile_expression",
                    format!("use of dropped variable: '{}'\n  = note: variable was removed/freed and is no longer valid\n  = error: this operation would cause a use-after-free vulnerability", expr)));
            }

            // For struct types, return the pointer directly (not loaded)
            // This is needed because struct return types are pointers
            if let BasicTypeEnum::StructType(_) = var_type {
                return Ok(ptr.into());
            }

            let value = self.backend.builder.build_load(
                var_type,
                ptr,
                &expr
            ).map_err(|e| self.error("compile_expression",
                format!("failed to load variable '{}': {}", expr, e)))?;
            return Ok(value);
        }

        eprintln!("DEBUG: compile_expression_str_inner: variable '{}' not found in variables, keys: {:?}", expr, self.variables.keys().collect::<Vec<_>>());

        // Build helpful error message similar to rustc
        let mut help_msg = String::new();

        if !self.variables.is_empty() {
            help_msg.push_str(&format!("\n  = note: available variables in current scope: {}",
                self.variables.keys().map(|k| format!("'{}'", k)).collect::<Vec<_>>().join(", ")));
        }

        // Check for similar variable names (simple edit distance)
        let similar: Vec<_> = self.variables.keys()
            .filter(|k| {
                let dist = Self::edit_distance(&expr, k);
                dist <= 2 && dist < k.len()
            })
            .map(|k| format!("'{}'", k))
            .collect();

        if !similar.is_empty() {
            help_msg.push_str(&format!("\n  = help: did you mean {}?", similar.join(" or ")));
        }

        // Check for common mistakes
        if expr.contains('#') {
            help_msg.push_str(&format!("\n  = error: invalid comment syntax detected\n  = note: Coffee uses '/#/' for comments, not '#'\n  = example: let x: int = 5  /#/ this is a comment"));
        }

        if expr.contains("//") {
            help_msg.push_str(&format!("\n  = error: invalid comment syntax detected\n  = note: Coffee uses '/#/' for comments, not '//'\n  = example: let x: int = 5  /#/ this is a comment"));
        }

        // Check for potential operator issues
        if expr.contains("**") {
            help_msg.push_str(&format!("\n  = error: '**' is not a valid operator in Coffee\n  = note: Coffee does not have a power operator\n  = help: use a function or implement your own power function"));
        }

        if expr.contains("@") {
            help_msg.push_str(&format!("\n  = error: '@' is not a valid operator in Coffee\n  = note: '@' is not a supported operator\n  = help: use supported operators: +, -, *, /, %, <<, >>, ==, !=, <, >, <=, >="));
        }

        if expr.contains("++") || expr.contains("--") {
            let op = if expr.contains("++") { "++" } else { "--" };
            help_msg.push_str(&format!("\n  = error: '{}' is not valid in Coffee\n  = note: Coffee does not support increment/decrement operators\n  = help: use 'x = x + 1' or 'x = x - 1' instead", op));
        }

        Err(self.error("compile_expression",
            format!("cannot find value '{}' in this scope{}", expr, help_msg)))
    }

    /// Try to compile binary operation
    fn try_compile_binary_op(&mut self, expr: &str) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        eprintln!("DEBUG: try_compile_binary_op: expr='{}'", expr);
        // Check for binary operators (simple tokenization)
        // IMPORTANT: Respect operator precedence when finding operators
        // Operators are checked from LOWEST to HIGHEST precedence
        // This ensures that higher precedence operators are evaluated first
        // Precedence (highest to lowest): !, *, /, %, +, -, <<, >>, &, |, ^, <, >, <=, >=, ==, !=, &&, ||
        for op in [" && ", " || ", " == ", " != ", " < ", " > ", " <= ", " >= ", " ^ ", " | ", " & ", " << ", " >> ", " + ", " - ", " * ", " / ", " % "] {
            if let Some(pos) = find_operator_outside_parens(expr, op) {
                eprintln!("DEBUG: try_compile_binary_op: found op '{}' at pos={}", op, pos);
                let left_str = &expr[..pos];
                let right_str = &expr[pos + op.len()..];

                let left = self.compile_expression_str(left_str)?;
                let right = self.compile_expression_str(right_str)?;

                let result = self.build_binary_op(op.trim(), left, right)?;
                return Ok(Some(result));
            }
        }

        eprintln!("DEBUG: try_compile_binary_op: no operator found");
        Ok(None)
    }

    /// Build binary operation using arithmetic module
    fn build_binary_op(&mut self, op: &str, left: BasicValueEnum<'ctx>, right: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        use inkwell::IntPredicate;
        match (left, right) {
            (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => {
                // HIGH-11 FIX: Use safe shift operations that check for negative amounts and overflow
                let result: BasicValueEnum<'ctx> = match op {
                    "<<" => self.arithmetic_ctx.build_int_shl(self.backend.context, &self.backend.builder, l, r)?.into(),
                    ">>" => self.arithmetic_ctx.build_int_shr(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "&" => self.backend.builder.build_and(l, r, "bitwise_and")
                        .map_err(|e| self.error("binary_operation", format!("failed to build bitwise and operation: {}", e)))?.into(),
                    "|" => self.backend.builder.build_or(l, r, "bitwise_or")
                        .map_err(|e| self.error("binary_operation", format!("failed to build bitwise or operation: {}", e)))?.into(),
                    "^" => self.backend.builder.build_xor(l, r, "bitwise_xor")
                        .map_err(|e| self.error("binary_operation", format!("failed to build bitwise xor operation: {}", e)))?.into(),
                    "+" => self.arithmetic_ctx.build_int_add(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "-" => self.arithmetic_ctx.build_int_sub(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "*" => self.arithmetic_ctx.build_int_mul(self.backend.context, &self.backend.builder, l, r)?.into(),
                    "/" => self.arithmetic_ctx.build_int_div(self.backend.context, &self.backend.builder, l, r)?,
                    "%" => self.arithmetic_ctx.build_int_mod(self.backend.context, &self.backend.builder, l, r)?,
                    "&&" => self.backend.builder.build_and(l, r, "and")
                        .map_err(|e| self.error("binary_operation", format!("failed to build and operation: {}", e)))?.into(),
                    "||" => self.backend.builder.build_or(l, r, "or")
                        .map_err(|e| self.error("binary_operation", format!("failed to build or operation: {}", e)))?.into(),
                    "==" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::EQ, l, r, "eq")?.into(),
                    "!=" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::NE, l, r, "ne")?.into(),
                    "<" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SLT, l, r, "lt")?.into(),
                    ">" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SGT, l, r, "gt")?.into(),
                    "<=" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SLE, l, r, "le")?.into(),
                    ">=" => self.arithmetic_ctx.build_int_compare(&self.backend.builder, IntPredicate::SGE, l, r, "ge")?.into(),
                    _ => return Err(self.error("binary_operation",
                        format!("unknown integer operator '{}'\n  = note: supported operators: <<, >>, &, |, ^, +, -, *, /, %, ==, !=, <, >, <=, >=", op))),
                };
                Ok(result)
            }
            (BasicValueEnum::FloatValue(l), BasicValueEnum::FloatValue(r)) => {
                use inkwell::FloatPredicate;
                // Arithmetic operations return FloatValue
                if matches!(op, "+" | "-" | "*" | "/") {
                    let result = match op {
                        "+" => self.arithmetic_ctx.build_float_add(&self.backend.builder, l, r)?,
                        "-" => self.arithmetic_ctx.build_float_sub(&self.backend.builder, l, r)?,
                        "*" => self.arithmetic_ctx.build_float_mul(&self.backend.builder, l, r)?,
                        "/" => self.arithmetic_ctx.build_float_div(self.backend.context, &self.backend.builder, l, r)?,
                        _ => unreachable!(),
                    };
                    Ok(result.into())
                } else {
                    // Comparison operations return IntValue (i1)
                    let pred = match op {
                        "==" => FloatPredicate::OEQ,
                        "!=" => FloatPredicate::ONE,
                        "<" => FloatPredicate::OLT,
                        ">" => FloatPredicate::OGT,
                        "<=" => FloatPredicate::OLE,
                        ">=" => FloatPredicate::OGE,
                        _ => return Err(self.error("binary_operation",
                            format!("unknown float operator '{}'\n  = note: supported operators: +, -, *, /, ==, !=, <, >, <=, >=", op))),
                    };
                    let result = self.arithmetic_ctx.build_float_compare(&self.backend.builder, pred, l, r, "fcmp")
                        .map_err(|e| self.error("binary_operation",
                            format!("failed to build float comparison {}: {}", op, e)))?;
                    Ok(result.into())
                }
            }
            // Handle int and float mixed operations - no implicit conversion
            (BasicValueEnum::IntValue(int_val), BasicValueEnum::FloatValue(float_val)) => {
                let left_type_str = self.type_to_string(int_val.get_type().into());
                let right_type_str = self.type_to_string(float_val.get_type().into());

                Err(self.error("binary_operation",
                    format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: operator '{}' does not support implicit type conversion between int and float\n  = help: use explicit type conversion or ensure both operands are of the same type",
                        op, left_type_str, right_type_str, op)))
            }
            (BasicValueEnum::FloatValue(float_val), BasicValueEnum::IntValue(int_val)) => {
                let left_type_str = self.type_to_string(float_val.get_type().into());
                let right_type_str = self.type_to_string(int_val.get_type().into());

                Err(self.error("binary_operation",
                    format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: operator '{}' does not support implicit type conversion between float and int\n  = help: use explicit type conversion or ensure both operands are of the same type",
                        op, left_type_str, right_type_str, op)))
            }
            (BasicValueEnum::PointerValue(left_ptr), BasicValueEnum::PointerValue(right_ptr)) => {
                // Check if both are string pointers (i8*)
                if op == "+" {
                    // String concatenation
                    return self.build_string_concat(left_ptr, right_ptr);
                } else {
                    let left_type_str = self.type_to_string(left_ptr.get_type().into());
                    let right_type_str = self.type_to_string(right_ptr.get_type().into());

                    Err(self.error("binary_operation",
                        format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: only '+' operator is supported for string concatenation",
                            op, left_type_str, right_type_str)))
                }
            }
            (left_val, right_val) => {
                let left_type_str = self.type_to_string(left_val.get_type());
                let right_type_str = self.type_to_string(right_val.get_type());

                Err(self.error("binary_operation",
                    format!("type mismatch: cannot apply operator '{}' to types '{}' and '{}'\n  = note: operator '{}' requires compatible numeric types\n  = help: ensure both operands are of the same numeric type (int or float)",
                        op, left_type_str, right_type_str, op)))
            }
        }
    }

    /// Compile function call
    fn compile_function_call(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
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

        eprintln!("DEBUG: compile_function_call: func_name='{}', args_str='{}', args_list={:?}", func_name, args_str, args_list);

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

    /// Convert a value to a target type
    /// Performs safe type conversions when possible
    pub fn convert_value_to_type(&self, value: BasicValueEnum<'ctx>, target_type: BasicTypeEnum<'ctx>, name: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let value_type = value.get_type();

        // If types match, no conversion needed
        if value_type == target_type {
            return Ok(value);
        }

        // Handle integer type conversions
        if let (BasicValueEnum::IntValue(int_val), BasicTypeEnum::IntType(target_int)) = (value, target_type) {
            let current_width = int_val.get_type().get_bit_width();
            let target_width = target_int.get_bit_width();

            if current_width == target_width {
                return Ok(value);
            }

            // Truncate to smaller type (safe if value fits)
            if target_width < current_width {
                let truncated = self.backend.builder.build_int_truncate(
                    int_val,
                    target_int,
                    &format!("{}_trunc", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to truncate integer: {}", e)))?;
                return Ok(truncated.into());
            }

            // Extend to larger type (sign extension for signed)
            if target_width > current_width {
                let extended = self.backend.builder.build_int_s_extend(
                    int_val,
                    target_int,
                    &format!("{}_sext", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to sign-extend integer: {}", e)))?;
                return Ok(extended.into());
            }
        }

        // Handle float type conversions
        if let (BasicValueEnum::FloatValue(float_val), BasicTypeEnum::FloatType(target_float)) = (value, target_type) {
            let current_width = float_val.get_type().get_bit_width();
            let target_width = target_float.get_bit_width();

            if current_width == target_width {
                return Ok(value);
            }

            // Truncate to f32
            if target_width < current_width {
                let truncated = self.backend.builder.build_float_trunc(
                    float_val,
                    target_float,
                    &format!("{}_trunc", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to truncate float: {}", e)))?;
                return Ok(truncated.into());
            }

            // Extend to f64
            if target_width > current_width {
                let extended = self.backend.builder.build_float_ext(
                    float_val,
                    target_float,
                    &format!("{}_ext", name)
                ).map_err(|e| self.error("type_conversion",
                    format!("failed to extend float: {}", e)))?;
                return Ok(extended.into());
            }
        }

        // Handle int to float conversions
        if let (BasicValueEnum::IntValue(int_val), BasicTypeEnum::FloatType(target_float)) = (value, target_type) {
            let converted = self.backend.builder.build_signed_int_to_float(
                int_val,
                target_float,
                &format!("{}_sitofp", name)
            ).map_err(|e| self.error("type_conversion",
                format!("failed to convert int to float: {}", e)))?;
            return Ok(converted.into());
        }

        // Handle float to int conversions
        if let (BasicValueEnum::FloatValue(float_val), BasicTypeEnum::IntType(target_int)) = (value, target_type) {
            let converted = self.backend.builder.build_float_to_signed_int(
                float_val,
                target_int,
                &format!("{}_fptosi", name)
            ).map_err(|e| self.error("type_conversion",
                format!("failed to convert float to int: {}", e)))?;
            return Ok(converted.into());
        }

        // Handle pointer to struct conversions (for struct literals)
        if let (BasicValueEnum::PointerValue(ptr_val), BasicTypeEnum::StructType(target_struct)) = (value, target_type) {
            // The pointer points to the struct data, load it
            let loaded = self.backend.builder.build_load(target_struct, ptr_val, name)
                .map_err(|e| self.error("type_conversion",
                    format!("failed to load struct from pointer: {}", e)))?;
            return Ok(loaded);
        }

        // Handle pointer to int conversions (for C FFI)
        if let (BasicValueEnum::PointerValue(ptr_val), BasicTypeEnum::IntType(target_int)) = (value, target_type) {
            // Convert pointer to integer (ptrtoint)
            let converted = self.backend.builder.build_ptr_to_int(ptr_val, target_int, &format!("{}_ptrtoint", name))
                .map_err(|e| self.error("type_conversion",
                    format!("failed to convert pointer to int: {}", e)))?;
            return Ok(converted.into());
        }

        // Handle int to pointer conversions (for C FFI)
        if let (BasicValueEnum::IntValue(int_val), BasicTypeEnum::PointerType(target_ptr)) = (value, target_type) {
            // Convert integer to pointer (inttoptr)
            let converted = self.backend.builder.build_int_to_ptr(int_val, target_ptr, &format!("{}_inttoptr", name))
                .map_err(|e| self.error("type_conversion",
                    format!("failed to convert int to pointer: {}", e)))?;
            return Ok(converted.into());
        }

        // Type mismatch error with helpful information
        let value_type_str = self.type_to_string(value_type);
        let target_type_str = self.type_to_string(target_type);

        Err(self.error("type_conversion",
            format!("cannot convert value from type '{}' to type '{}'\n  = note: these types are incompatible and cannot be implicitly converted\n  = help: ensure types match or use explicit type conversion if supported",
                value_type_str, target_type_str)))
    }

    /// Convert LLVM type to human-readable string
    pub fn type_to_string(&self, type_: BasicTypeEnum<'ctx>) -> String {
        match type_ {
            BasicTypeEnum::IntType(t) => {
                let width = t.get_bit_width();
                match width {
                    1 => "i1".to_string(),  // LLVM native bool (not used by Coffee)
                    8 => "int(1)".to_string(),  // Coffee bool/int(1) = i8
                    16 => "int(2)".to_string(),
                    32 => "int(4)".to_string(),
                    64 => "int(8)".to_string(),
                    128 => "int(16)".to_string(),
                    _ => format!("int({})", width / 8),
                }
            }
            BasicTypeEnum::FloatType(t) => {
                let width = t.get_bit_width();
                match width {
                    32 => "float(4)".to_string(),
                    64 => "float(8)".to_string(),
                    _ => format!("float({})", width / 8),
                }
            }
            BasicTypeEnum::PointerType(_) => {
                format!("ptr")
            }
            BasicTypeEnum::ArrayType(t) => {
                let len = t.len();
                let element_type = t.get_element_type();
                format!("[{}; {}]", self.type_to_string(element_type), len)
            }
            BasicTypeEnum::StructType(t) => {
                if t.is_packed() {
                    format!("packed struct")
                } else {
                    format!("struct")
                }
            }
            BasicTypeEnum::VectorType(t) => {
                // Vector type has num_elements field
                format!("[{}]", self.type_to_string(t.get_element_type()))
            }
            _ => "unknown".to_string(),
        }
    }

    /// Check if two types are compatible for implicit conversion
    pub fn are_types_compatible(&self, from: BasicTypeEnum<'ctx>, to: BasicTypeEnum<'ctx>) -> bool {
        match (from, to) {
            // Same types are always compatible
            (a, b) if a == b => true,

            // Integer to integer: always compatible (with truncation/extension)
            (BasicTypeEnum::IntType(_), BasicTypeEnum::IntType(_)) => true,

            // Float to float: always compatible (with truncation/extension)
            (BasicTypeEnum::FloatType(_), BasicTypeEnum::FloatType(_)) => true,

            // Integer to float: compatible
            (BasicTypeEnum::IntType(_), BasicTypeEnum::FloatType(_)) => true,

            // Float to integer: compatible
            (BasicTypeEnum::FloatType(_), BasicTypeEnum::IntType(_)) => true,

            // All other combinations are incompatible
            _ => false,
        }
    }

    /// Compile an f-string from AST (template and placeholders already parsed)
    pub fn compile_fstring_from_ast(&mut self, template: &str, placeholders: &[String]) -> Result<BasicValueEnum<'ctx>, String> {
        // Build format string and load placeholder values
        if placeholders.is_empty() {
            // No placeholders, just return the string constant
            return self.build_string_constant(template);
        }

        // Get or declare snprintf function
        if !self.functions.contains_key("snprintf") {
            self.used_c_functions.insert("snprintf".to_string());
            super::functions::declare_builtin_c_function(
                "snprintf",
                self.backend.context,
                &self.backend.module,
                &mut self.functions,
            )?;
        }

        let snprintf_fn = *self.functions.get("snprintf")
            .ok_or_else(|| self.error("fstring", "snprintf function not declared"))?;

        // Calculate required buffer size
        // Estimate: template length + placeholder values max length
        // For safety, use a reasonable limit (4KB)
        let template_len = template.len();
        let estimated_size = template_len + 1024; // Extra space for placeholder values
        
        // Cap buffer size to prevent stack overflow
        const MAX_FSTRING_SIZE: usize = 4096; // 4KB max
        let buffer_size = std::cmp::min(estimated_size, MAX_FSTRING_SIZE);
        
        // Allocate stack buffer for the formatted string
        let buffer_size_const = self.backend.context.i64_type().const_int(buffer_size as u64, false);
        let buffer_type = self.backend.context.i8_type().array_type(buffer_size as u32);
        let buffer_alloca = self.backend.builder.build_alloca(buffer_type, "fstring_buffer")
            .map_err(|e| self.error("fstring",
                format!("failed to allocate fstring buffer: {}", e)))?;

        // Get pointer to the buffer using GEP
        let buffer_ptr = unsafe {
            self.backend.builder.build_in_bounds_gep(
                buffer_type,
                buffer_alloca,
                &[self.backend.context.i32_type().const_int(0, false), self.backend.context.i32_type().const_int(0, false)],
                "fstring_buffer_ptr"
            ).map_err(|e| self.error("fstring",
                format!("failed to get buffer pointer: {}", e)))?
        };

        // Build format string
        let format_str = self.build_string_constant(template)?;

        // Build snprintf arguments
        let mut snprintf_args: Vec<inkwell::values::BasicMetadataValueEnum<'_>> = vec![
            buffer_ptr.into(),
            buffer_size_const.into(),
            format_str.into()
        ];

        // Load each placeholder variable
        for placeholder in placeholders {
            // HIGH-10 FIX: Mark placeholder variable as used
            self.used_variables.insert(placeholder.clone());

            // Record variable use for lifetime tracking
            // Update current line to ensure it's not zero
            self.memory_ctx.set_line(self.memory_ctx.current_line + 1);
            self.memory_ctx.record_use(placeholder);

            let &(ptr, var_type) = self.variables.get(placeholder)
                .ok_or_else(|| self.error("fstring",
                    format!("variable '{}' not found in scope", placeholder)))?;

            let value = self.backend.builder.build_load(
                var_type,
                ptr,
                placeholder
            ).map_err(|e| self.error("fstring",
                    format!("failed to load variable '{}': {}", placeholder, e)))?;

            snprintf_args.push(value.into());
        }

        // Call snprintf to format the string
        let snprintf_call = self.backend.builder.build_call(
            snprintf_fn,
            &snprintf_args,
            "fstring_snprintf"
        ).map_err(|e| self.error("fstring",
            format!("failed to build snprintf call: {}", e)))?;

        // Get the return value (number of characters written, excluding null terminator)
        let bytes_written = match snprintf_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("fstring", "snprintf returned void"));
            }
        };

        let bytes_written = match bytes_written {
            inkwell::values::BasicValueEnum::IntValue(val) => val,
            _ => return Err(self.error("fstring", "snprintf returned unexpected type")),
        };

        // Create a constant 0 for comparison
        let zero = self.backend.context.i32_type().const_int(0, false);
        
        // Check if snprintf returned negative value (error)
        let is_negative = self.backend.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            bytes_written,
            zero,
            "is_negative"
        ).map_err(|e| self.error("fstring",
            format!("failed to build comparison: {}", e)))?;

        // Build error message for buffer overflow
        let error_msg = self.build_string_constant(
            "Error: f-string buffer overflow - formatted string too long\n"
        )?;

        // Get printf function for error reporting
        if !self.functions.contains_key("printf") {
            self.used_c_functions.insert("printf".to_string());
            super::functions::declare_builtin_c_function(
                "printf",
                self.backend.context,
                &self.backend.module,
                &mut self.functions,
            )?;
        }

        let printf_fn = *self.functions.get("printf")
            .ok_or_else(|| self.error("fstring", "printf function not declared"))?;

        // If snprintf failed (returned negative), print error and use empty string
        let error_block = self.backend.context.append_basic_block(
            self.backend.builder.get_insert_block().unwrap().get_parent().unwrap(),
            "fstring_error"
        );
        
        let success_block = self.backend.context.append_basic_block(
            self.backend.builder.get_insert_block().unwrap().get_parent().unwrap(),
            "fstring_success"
        );

        // Conditional branch: if negative, go to error block
        self.backend.builder.build_conditional_branch(
            is_negative,
            error_block,
            success_block
        ).map_err(|e| self.error("fstring",
            format!("failed to build conditional branch: {}", e)))?;

        // Error block: print error message and use empty string
        self.backend.builder.position_at_end(error_block);
        self.backend.builder.build_call(
            printf_fn,
            &[error_msg.into()],
            "print_error"
        ).map_err(|e| self.error("fstring",
            format!("failed to build printf call: {}", e)))?;
        
        // Set first character to null (empty string)
        let null_val = self.backend.context.i8_type().const_int(0, false);
        self.backend.builder.build_store(
            buffer_ptr,
            null_val
        ).map_err(|e| self.error("fstring",
            format!("failed to store null terminator: {}", e)))?;

        self.backend.builder.build_unconditional_branch(success_block)
            .map_err(|e| self.error("fstring",
                format!("failed to build unconditional branch: {}", e)))?;

        // Success block: continue with the formatted string
        self.backend.builder.position_at_end(success_block);

        // Return the buffer pointer (string type)
        Ok(buffer_ptr.into())
    }

    /// Compile format string (f"...") with safe placeholder substitution
    /// This performs runtime type validation and prevents injection attacks
    fn compile_fstring(&mut self, template: &str) -> Result<BasicValueEnum<'ctx>, String> {
        use std::fmt::Write;

        // Parse template to extract placeholders and build format string
        let mut result = String::new();
        let mut placeholders = Vec::new();
        let mut placeholder_types = Vec::new(); // Track types of placeholders
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    result.push('{');
                    continue;
                }

                let mut placeholder = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '}' {
                        chars.next();
                        break;
                    }
                    if next.is_alphanumeric() || next == '_' {
                        placeholder.push(chars.next().unwrap());
                    } else {
                        return Err(self.error("fstring",
                            format!("invalid character '{}' in placeholder", next)));
                    }
                }

                if placeholder.is_empty() {
                    return Err(self.error("fstring", "empty placeholder {{}}"));
                }

                // Validate placeholder exists and get its type
                if !self.variables.contains_key(&placeholder) {
                    return Err(self.error("fstring",
                        format!("undefined variable '{}'", placeholder)));
                }

                let &(_, var_type) = self.variables.get(&placeholder)
                    .ok_or_else(|| self.error("fstring",
                        format!("variable '{}' not found in scope", placeholder)))?;

                // Add placeholder and its type to lists
                placeholders.push(placeholder.clone());
                placeholder_types.push(var_type);

                // Use appropriate format specifier based on type
                let format_spec = match var_type {
                    BasicTypeEnum::IntType(t) => {
                        if t.get_bit_width() == 64 { "%lld" } else { "%d" }
                    }
                    BasicTypeEnum::FloatType(t) => {
                        if t.get_bit_width() == 64 { "%lf" } else { "%f" }
                    }
                    BasicTypeEnum::PointerType(_) => "%s",
                    _ => "%s", // Default to string for other types
                };
                write!(result, "{}", format_spec).unwrap();
            } else if c == '}' {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    result.push('}');
                } else {
                    return Err(self.error("fstring", "unmatched '}}'"));
                }
            } else {
                // HIGH-2 FIX: Escape '%' to prevent format string injection
                // Any '%' in user input must be escaped to '%%' to prevent
                // printf from interpreting it as a format specifier
                if c == '%' {
                    result.push_str("%%");
                } else {
                    result.push(c);
                }
            }
        }

        // Build format string and load placeholder values
        if placeholders.is_empty() {
            // No placeholders, just return the string constant
            return self.build_string_constant(&result);
        }

        // Delegate to compile_fstring_from_ast to generate string value
        self.compile_fstring_from_ast(&result, &placeholders)
    }

    /// Process escape sequences in a string literal
    fn process_escape_sequences(s: &str) -> Vec<u8> {
        let mut result = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    match next {
                        'n' => result.push(b'\n'),
                        't' => result.push(b'\t'),
                        'r' => result.push(b'\r'),
                        '\\' => result.push(b'\\'),
                        '"' => result.push(b'"'),
                        '\'' => result.push(b'\''),
                        '0' => result.push(b'\0'),
                        'x' => {
                            // Hex escape: \xHH
                            let mut hex_str = String::new();
                            for _ in 0..2 {
                                if let Some(&h) = chars.peek() {
                                    if h.is_ascii_hexdigit() {
                                        hex_str.push(chars.next().unwrap());
                                    }
                                }
                            }
                            if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                                result.push(byte);
                            }
                        }
                        _ => {
                            // Unknown escape, keep as-is
                            result.push(b'\\');
                            result.push(next as u8);
                        }
                    }
                } else {
                    result.push(b'\\');
                }
            } else {
                result.push(c as u8);
            }
        }

        result
    }

    /// Build string constant
    /// Build string concatenation: str1 + str2
    fn build_string_concat(&mut self, left: PointerValue<'ctx>, right: PointerValue<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        // Get or declare strlen function
        if !self.functions.contains_key("strlen") {
            self.used_c_functions.insert("strlen".to_string());
        }
        let strlen_fn = if self.functions.contains_key("strlen") {
            *self.functions.get("strlen").unwrap()
        } else {
            // Declare it
            self.declare_external_function("strlen")?
        };

        // Get or declare strcpy function
        if !self.functions.contains_key("strcpy") {
            self.used_c_functions.insert("strcpy".to_string());
        }
        let strcpy_fn = if self.functions.contains_key("strcpy") {
            *self.functions.get("strcpy").unwrap()
        } else {
            // Declare it
            self.declare_external_function("strcpy")?
        };

        // Get or declare strcat function
        if !self.functions.contains_key("strcat") {
            self.used_c_functions.insert("strcat".to_string());
        }
        let strcat_fn = if self.functions.contains_key("strcat") {
            *self.functions.get("strcat").unwrap()
        } else {
            // Declare it
            self.declare_external_function("strcat")?
        };

        // Get or declare malloc function
        if !self.functions.contains_key("malloc") {
            self.used_c_functions.insert("malloc".to_string());
        }
        let malloc_fn = if self.functions.contains_key("malloc") {
            *self.functions.get("malloc").unwrap()
        } else {
            // Declare it
            self.declare_external_function("malloc")?
        };

        // Calculate lengths of both strings
        let left_len_call = self.backend.builder.build_call(strlen_fn, &[left.into()], "left_len")
            .map_err(|e| self.error("string_concat", format!("failed to call strlen: {}", e)))?;

        let left_len = match left_len_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("string_concat", "strlen returned void"));
            }
        };

        let right_len_call = self.backend.builder.build_call(strlen_fn, &[right.into()], "right_len")
            .map_err(|e| self.error("string_concat", format!("failed to call strlen: {}", e)))?;

        let right_len = match right_len_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("string_concat", "strlen returned void"));
            }
        };

        // Calculate total length (left_len + right_len + 1 for null terminator)
        let total_len = self.backend.builder.build_int_add(
            left_len.into_int_value(),
            right_len.into_int_value(),
            "total_len"
        ).map_err(|e| self.error("string_concat", format!("failed to build int add: {}", e)))?;
        let total_len = self.backend.builder.build_int_add(
            total_len,
            self.backend.context.i64_type().const_int(1, false),
            "total_len_with_null"
        ).map_err(|e| self.error("string_concat", format!("failed to build int add: {}", e)))?;

        // Allocate memory for concatenated string
        let result_ptr_call = self.backend.builder.build_call(malloc_fn, &[total_len.into()], "result_ptr")
            .map_err(|e| self.error("string_concat", format!("failed to call malloc: {}", e)))?;

        let result_ptr = match result_ptr_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("string_concat", "malloc returned void"));
            }
        };

        let result_ptr = result_ptr.into_pointer_value();

        // Copy left string to result
        self.backend.builder.build_call(strcpy_fn, &[result_ptr.into(), left.into()], "strcpy_left")
            .map_err(|e| self.error("string_concat", format!("failed to call strcpy: {}", e)))?;

        // Concatenate right string to result
        self.backend.builder.build_call(strcat_fn, &[result_ptr.into(), right.into()], "strcat_right")
            .map_err(|e| self.error("string_concat", format!("failed to call strcat: {}", e)))?;

        Ok(result_ptr.into())
    }

    fn build_string_constant(&mut self, s: &str) -> Result<BasicValueEnum<'ctx>, String> {
        if let Some(&ptr) = self.string_constants.get(s) {
            return Ok(ptr.into());
        }

        // HIGH-6 FIX: Limit string length to prevent DoS via huge strings
        const MAX_STRING_LENGTH: usize = 10 * 1024 * 1024; // 10 MB max string
        if s.len() > MAX_STRING_LENGTH {
            return Err(self.error("build_string_constant",
                format!("String constant exceeds maximum length\n  = note: string length: {} bytes, maximum: {} bytes\n  = help: use shorter strings",
                    s.len(), MAX_STRING_LENGTH)));
        }

        // LOW-2 FIX: Limit number of string constants to prevent DoS
        const MAX_STRING_CONSTANTS: usize = 10000;
        if self.string_constants.len() >= MAX_STRING_CONSTANTS {
            return Err(self.error("build_string_constant",
                format!("Too many string constants in program\n  = note: current string constants: {}, maximum: {}\n  = help: reduce number of string literals or reuse existing ones",
                    self.string_constants.len(), MAX_STRING_CONSTANTS)));
        }

        // Process escape sequences
        let bytes = Self::process_escape_sequences(s);
        let string_val = self.backend.context.const_string(&bytes, true);
        let global = self.backend.module.add_global(
            string_val.get_type(),
            Some(inkwell::AddressSpace::default()),
            "str"
        );
        global.set_initializer(&string_val);
        global.set_constant(true);

        let ptr = global.as_pointer_value();
        self.string_constants.insert(s.to_string(), ptr);

        Ok(ptr.into())
    }

    /// Compile struct literal: Point { x: 10, y: 20 }
    fn compile_struct_literal(&mut self, struct_name: &str, fields_str: &str) -> Result<BasicValueEnum<'ctx>, String> {
        // Get class definition
        let class_def = self.classes.get(struct_name)
            .ok_or_else(|| self.error("compile_struct_literal",
                format!("unknown class '{}'", struct_name)))?;

        // Get struct type from class definition
        let struct_type = self.type_mapper.get_or_create_struct_type_from_class(struct_name, class_def, &self.classes)
            .ok_or_else(|| self.error("compile_struct_literal",
                format!("failed to create struct type for '{}'", struct_name)))?;

        // Allocate struct on stack
        let alloca = self.backend.builder.build_alloca(struct_type, &format!("{}_literal", struct_name))
            .map_err(|e| self.error("compile_struct_literal",
                format!("failed to allocate struct '{}': {}", struct_name, e)))?;

        // Parse and initialize fields
        if !fields_str.trim().is_empty() {
            eprintln!("DEBUG: compile_struct_literal: struct_name='{}', fields_str='{}'", struct_name, fields_str);
            // Smart field parsing that handles commas in expressions
            let mut fields = Vec::new();
            let mut current_field = String::new();
            let mut paren_depth = 0;  // Track parentheses depth
            let mut brace_depth = 0;  // Track braces depth
            let mut in_string = false;

            for ch in fields_str.chars() {
                match ch {
                    '"' if !in_string => {
                        in_string = true;
                        current_field.push(ch);  // Add opening quote
                    },
                    '"' if in_string => {
                        in_string = false;
                        current_field.push(ch);  // Add closing quote
                    },
                    '(' if !in_string => {
                        paren_depth += 1;
                        current_field.push(ch);
                    },
                    ')' if !in_string => {
                        paren_depth -= 1;
                        current_field.push(ch);
                    },
                    '{' if !in_string => {
                        brace_depth += 1;
                        current_field.push(ch);
                    },
                    '}' if !in_string => {
                        brace_depth -= 1;
                        current_field.push(ch);
                    },
                    ',' if !in_string && paren_depth == 0 && brace_depth == 0 => {
                        eprintln!("DEBUG: compile_struct_literal: found comma at paren_depth={}, brace_depth={}, current_field='{}'", paren_depth, brace_depth, current_field);
                        fields.push(current_field.trim().to_string());
                        current_field.clear();
                    }
                    '\n' | '\r' | '\t' | ' ' => {
                        // Skip whitespace characters
                        if !current_field.is_empty() {
                            current_field.push(' ');
                        }
                    }
                    _ => current_field.push(ch),
                }
            }
            if !current_field.trim().is_empty() {
                fields.push(current_field.trim().to_string());
            }
            eprintln!("DEBUG: compile_struct_literal: fields={:?}", fields);
            
            for field_pair in fields {
                eprintln!("DEBUG: compile_struct_literal: field_pair='{}'", field_pair);
                let field_pair = field_pair.trim();
                if field_pair.is_empty() {
                    continue;
                }

                // Find the first colon that is not part of :: (double colon)
                // This handles cases like "field: Point::new(x, y)"
                let mut colon_pos = None;
                let mut chars = field_pair.chars().enumerate();
                while let Some((i, ch)) = chars.next() {
                    if ch == ':' {
                        // Check if this is part of ::
                        if let Some(next_ch) = field_pair.chars().nth(i + 1) {
                            if next_ch == ':' {
                                // This is ::, skip both characters
                                chars.next(); // skip the second colon
                            } else {
                                // This is a single colon, this is the field separator
                                colon_pos = Some(i);
                                break;
                            }
                        } else {
                            // Single colon at the end
                            colon_pos = Some(i);
                            break;
                        }
                    }
                }

                eprintln!("DEBUG: compile_struct_literal: field_pair='{}', colon_pos={:?}", field_pair, colon_pos);

                if let Some(pos) = colon_pos {
                    let field_name = field_pair[..pos].trim();
                    let field_value_expr = field_pair[pos + 1..].trim();
                    eprintln!("DEBUG: compile_struct_literal: field_name='{}', field_value_expr='{}'", field_name, field_value_expr);

                    // Compile field value
                    // Special handling for wildcard _ (skip compilation, will be ignored in match)
                    let field_value = if field_value_expr == "_" {
                        // Use a default value (0) for wildcard
                        // This will be ignored in pattern matching anyway
                        self.backend.context.i64_type().const_int(0, false).into()
                    } else {
                        self.compile_expression_str(field_value_expr)?
                    };

                    // Get field index
                    let field_index = self.type_mapper.get_field_index(struct_name, field_name)
                        .ok_or_else(|| self.error("compile_struct_literal",
                            format!("field '{}' not found in struct '{}'", field_name, struct_name)))?;

                    // Get field type
                    eprintln!("DEBUG: compile_struct_literal: struct_type.name={:?}, field_index={}, struct_type.count_fields()={}", 
                        struct_type.get_name(), field_index, struct_type.count_fields());
                    let field_type = struct_type.get_field_type_at_index(field_index as u32)
                        .ok_or_else(|| self.error("compile_struct_literal",
                            format!("field '{}' not found in struct '{}'", field_name, struct_name)))?;

                    // Get pointer to field
                    let field_ptr = unsafe {
                        self.backend.builder.build_in_bounds_gep(
                            struct_type,
                            alloca,
                            &[
                                self.backend.context.i32_type().const_int(0, false),
                                self.backend.context.i32_type().const_int(field_index as u64, false),
                            ],
                            &format!("{}_{}_ptr", struct_name, field_name)
                        ).map_err(|e| self.error("compile_struct_literal",
                            format!("failed to get field pointer: {}", e)))?
                    };

                    // Store field value
                    let converted_value = self.convert_value_to_type(field_value, field_type, field_name)?;
                    self.backend.builder.build_store(field_ptr, converted_value)
                        .map_err(|e| self.error("compile_struct_literal",
                            format!("failed to store field '{}': {}", field_name, e)))?;
                }
            }
        }

        Ok(alloca.into())
    }

    /// Compile constructor call: class::new(args)
    fn compile_constructor_call(&mut self, class_name: &str, args_str: &str) -> Result<BasicValueEnum<'ctx>, String> {
        // Construct function name: class_name + "_new"
        let ctor_name = format!("{}_new", class_name);
        
        // Get constructor function
        let ctor_fn = *self.functions.get(&ctor_name)
            .ok_or_else(|| self.error("compile_constructor_call",
                format!("constructor not found: '{}'", ctor_name)))?;
        
        // Parse and compile arguments
        let mut args = Vec::new();
        if !args_str.is_empty() {
            for arg_str in args_str.split(',') {
                let arg_str = arg_str.trim();
                if !arg_str.is_empty() {
                    let arg_value = self.compile_expression_str(arg_str)?;
                    args.push(arg_value);
                }
            }
        }
        
        // Call constructor
        let call_result = self.backend.builder.build_call(
            ctor_fn,
            &args.iter().map(|a| (*a).into()).collect::<Vec<_>>(),
            &format!("{}_call", ctor_name)
        ).map_err(|e| self.error("compile_constructor_call",
            format!("failed to call constructor '{}': {}", ctor_name, e)))?;
        
        // Extract return value
        let return_value = match call_result.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => val,
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(self.error("compile_constructor_call",
                    "constructor returned void (should return object)"));
            }
        };
        
        // Check if return value is a pointer (heap-allocated object)
        match return_value {
            inkwell::values::BasicValueEnum::PointerValue(ptr) => {
                // Constructor returns a pointer to heap-allocated object
                Ok(ptr.into())
            }
            inkwell::values::BasicValueEnum::IntValue(int_val) => {
                // Constructor returns struct by value (as i64 for simplicity)
                // This is a workaround - in real implementation, we'd need proper struct handling
                // For now, just return the int value as a pointer
                let ptr_type = self.backend.context.i64_type().ptr_type(inkwell::AddressSpace::default());
                let ptr = self.backend.builder.build_int_to_ptr(int_val, ptr_type, &format!("{}_ptr", ctor_name))
                    .map_err(|e| self.error("compile_constructor_call",
                        format!("failed to convert int to ptr: {}", e)))?;
                Ok(ptr.into())
            }
            inkwell::values::BasicValueEnum::StructValue(struct_val) => {
                // Constructor returns struct by value (stack-allocated)
                // Allocate heap memory and copy the struct
                let struct_type = struct_val.get_type();
                
                // Get malloc function
                let malloc_fn = self.functions.get("malloc")
                    .copied()
                    .ok_or_else(|| self.error("compile_constructor_call",
                        "malloc function not found for heap allocation"))?;
                
                // Calculate struct size (simplified approach - use 64 bytes as default)
                let size = 64u64;
                
                // Call malloc to allocate heap memory
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
                        "malloc returned unexpected value"))?,
                };
                
                let heap_ptr = match heap_ptr_value {
                    inkwell::values::BasicValueEnum::PointerValue(ptr) => ptr,
                    _ => return Err(self.error("compile_constructor_call",
                        "malloc did not return a pointer"))?,
                };
                
                // Copy struct to heap memory
                self.backend.builder.build_store(heap_ptr, struct_val)
                    .map_err(|e| self.error("compile_constructor_call",
                        format!("failed to store struct to heap: {}", e)))?;
                
                // Mark the heap pointer as heap-allocated for proper cleanup
                // This will be done when the variable is declared
                Ok(heap_ptr.into())
            }
            _ => return Err(self.error("compile_constructor_call",
                "constructor did not return an object (should return struct or pointer)"))?,
        }
    }

    /// Compile field access: object.field
    pub fn compile_field_access(&mut self, object_str: &str, field_name: &str) -> Result<BasicValueEnum<'ctx>, String> {
        // Compile the object expression
        let object_value = self.compile_expression_str(object_str)?;
        
        // Get the object type
        let object_type = object_value.get_type();
        
        eprintln!("DEBUG: compile_field_access: object_str='{}', field_name='{}', object_type={:?}",
            object_str, field_name, object_type);
        
        match object_type {
            BasicTypeEnum::PointerType(ptr_type) => {
                // Object is a pointer (heap-allocated or stack-allocated struct)
                // Check if this is a struct pointer by looking at variable_types
                if let Some(class_name) = self.variable_types.get(object_str) {
                    // Get the struct type from struct_types cache
                    if let Some(&struct_type) = self.type_mapper.struct_types.get(class_name) {
                        // Get field index
                        let field_index = self.get_field_index(&struct_type, field_name)?;
                        
                        // Get pointer to the field using GEP
                        let zero = self.backend.context.i64_type().const_int(0, false);
                        let field_index_val = self.backend.context.i32_type().const_int(field_index as u64, false);
                        
                        let field_ptr = unsafe {
                            self.backend.builder.build_in_bounds_gep(
                                struct_type,
                                object_value.into_pointer_value(),
                                &[zero, field_index_val],
                                &format!("{}_{}_ptr", object_str, field_name)
                            ).map_err(|e| self.error("compile_field_access",
                                format!("failed to get field pointer: {}", e)))?
                        };
                        
                        // Load the field value
                        let field_value = self.backend.builder.build_load(
                            struct_type.get_field_type_at_index(field_index as u32).unwrap(),
                            field_ptr,
                            &format!("{}_{}", object_str, field_name)
                        ).map_err(|e| self.error("compile_field_access",
                            format!("failed to load field '{}': {}", field_name, e)))?;
                        
                        return Ok(field_value);
                    }
                }
                
                // Fallback: try to load as a generic i64 value
                let i64_type = self.backend.context.i64_type();
                let loaded_value = self.backend.builder.build_load(
                    i64_type,
                    object_value.into_pointer_value(),
                    &format!("{}_loaded", object_str)
                ).map_err(|e| self.error("compile_field_access",
                    format!("failed to load object: {}", e)))?;
                
                Ok(loaded_value)
            }
            BasicTypeEnum::StructType(struct_type) => {
                // Object is a struct value (stack-allocated)
                let field_index = self.get_field_index(&struct_type, field_name)?;
                
                // For struct values, we need to extract the field directly
                let field_value = self.backend.builder.build_extract_value(
                    object_value.into_struct_value(),
                    field_index as u32,
                    &format!("{}_{}", object_str, field_name)
                ).map_err(|e| self.error("compile_field_access",
                    format!("failed to extract field '{}': {}", field_name, e)))?;
                
                Ok(field_value)
            }
            _ => Err(self.error("compile_field_access",
                format!("field access on non-struct type: {}", self.type_to_string(object_type))))
        }
    }

    /// Compile field access to get field pointer (for assignment): object.field
    pub fn compile_field_access_ptr(&mut self, object_str: &str, field_name: &str) -> Result<PointerValue<'ctx>, String> {
        // Compile the object expression
        let object_value = self.compile_expression_str(object_str)?;
        
        // Get the object type
        let object_type = object_value.get_type();
        
        match object_type {
            BasicTypeEnum::PointerType(ptr_type) => {
                // Object is a pointer (heap-allocated or stack-allocated struct)
                let object_ptr = object_value.into_pointer_value();
                
                // Try to get the struct type from the variable table
                let var_type = if let Some(&(_, var_type)) = self.variables.get(object_str) {
                    var_type
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("variable '{}' not found", object_str)));
                };
                
                // Get field index
                let field_index = match var_type {
                    BasicTypeEnum::StructType(s) => self.get_field_index(&s, field_name)?,
                    BasicTypeEnum::PointerType(inner_ptr_type) => {
                        // Try to get the class name from variable_types
                        if let Some(class_name) = self.variable_types.get(object_str) {
                            // Get the struct type from struct_types cache
                            if let Some(&struct_type) = self.type_mapper.struct_types.get(class_name) {
                                self.get_field_index(&struct_type, field_name)?
                            } else {
                                return Err(self.error("compile_field_access_ptr",
                                    format!("class '{}' not found in struct_types cache", class_name)));
                            }
                        } else {
                            return Err(self.error("compile_field_access_ptr",
                                format!("cannot get class name for variable '{}'", object_str)));
                        }
                    }
                    _ => {
                        return Err(self.error("compile_field_access_ptr",
                            format!("variable '{}' does not have a struct type", object_str)));
                    }
                };
                
                // Get pointer to the field using GEP
                let zero = self.backend.context.i64_type().const_int(0, false);
                let field_index_val = self.backend.context.i32_type().const_int(field_index as u64, false);
                
                // Get the struct type from struct_types cache
                let class_name = if let Some(name) = self.variable_types.get(object_str) {
                    name.clone()
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("cannot get class name for variable '{}'", object_str)));
                };
                
                let struct_type = if let Some(&struct_type) = self.type_mapper.struct_types.get(&class_name) {
                    struct_type
                } else {
                    return Err(self.error("compile_field_access_ptr",
                        format!("class '{}' not found in struct_types cache", class_name)));
                };
                
                let field_ptr = unsafe {
                    self.backend.builder.build_in_bounds_gep(
                        struct_type,
                        object_ptr,
                        &[zero, field_index_val],
                        &format!("{}_{}_ptr", object_str, field_name)
                    )
                }.map_err(|e| self.error("compile_field_access_ptr",
                    format!("failed to build field GEP: {}", e)))?;
                
                Ok(field_ptr)
            }
            BasicTypeEnum::StructType(struct_type) => {
                // Object is a struct value (stack-allocated)
                // We need to get a pointer to the struct first
                // This is more complex - for now, return an error
                Err(self.error("compile_field_access_ptr",
                    format!("field access on struct value not supported for assignment (use pointer)")))
            }
            _ => Err(self.error("compile_field_access_ptr",
                format!("field access on non-struct type: {}", self.type_to_string(object_type))))
        }
    }

    /// Compile method call: object.method(args)
    pub fn compile_method_call(&mut self, object_str: &str, method_name: &str, args_str: &str) -> Result<BasicValueEnum<'ctx>, String> {
        // Compile the object expression
        let object_value = self.compile_expression_str(object_str)?;
        
        // Get the object type
        let object_type = object_value.get_type();
        
        // Determine the class name from the variable type
        let class_name = if let Some(class_name) = self.variable_types.get(object_str) {
            class_name.clone()
        } else if let Some(&(_, var_type)) = self.variables.get(object_str) {
            match var_type {
                BasicTypeEnum::PointerType(ptr_type) => {
                    // Try to get the struct type from the pointer type
                    // Since get_element_type is not available, we'll use a different approach
                    // We'll try to infer the class name from the variable type
                    // For now, we'll use the variable name as a fallback
                    // This is not ideal, but it should work for simple cases
                    object_str.to_string()
                }
                BasicTypeEnum::StructType(_) => {
                    object_str.to_string()
                }
                _ => object_str.to_string()
            }
        } else {
            object_str.to_string()
        };
        
        eprintln!("DEBUG: compile_method_call: variable_types keys: {:?}", self.variable_types.keys().collect::<Vec<_>>());
        eprintln!("DEBUG: compile_method_call: object_str='{}', variable_types.get(object_str)={:?}", 
            object_str, self.variable_types.get(object_str));
        
        // Parse arguments
        let mut args = Vec::new();
        if !args_str.is_empty() {
            let mut current_arg = String::new();
            let mut depth = 0;
            let mut in_string = false;
            
            for ch in args_str.chars() {
                match ch {
                    '"' if !in_string => in_string = true,
                    '"' if in_string => in_string = false,
                    '(' if !in_string => depth += 1,
                    ')' if !in_string => depth -= 1,
                    ',' if !in_string && depth == 0 => {
                        if !current_arg.trim().is_empty() {
                            args.push(current_arg.trim().to_string());
                        }
                        current_arg.clear();
                    }
                    _ => current_arg.push(ch),
                }
            }
            if !current_arg.trim().is_empty() {
                args.push(current_arg.trim().to_string());
            }
        }
        
        // Compile arguments
        let mut compiled_args = Vec::new();
        for arg in &args {
            compiled_args.push(self.compile_expression_str(arg)?);
        }
        
        // Construct the full function name: ClassName_method
        let full_method_name = format!("{}_{}", class_name, method_name);
        
        eprintln!("DEBUG: compile_method_call: object_str='{}', method_name='{}', class_name='{}', full_method_name='{}'", 
            object_str, method_name, class_name, full_method_name);
        eprintln!("DEBUG: compile_method_call: functions keys: {:?}", self.functions.keys().collect::<Vec<_>>());
        
        // Try to get the function
        let function = self.functions.get(&full_method_name)
            .copied()
            .or_else(|| self.functions.get(method_name).copied())
            .ok_or_else(|| self.error("compile_method_call",
                format!("method '{}' not found\n  = help: ensure the method is defined in the class", method_name)))?;
        
        // Debug: print function signature
        eprintln!("DEBUG: compile_method_call: function='{}', param_count={}", function.get_name().to_str().unwrap_or("unknown"), function.get_params().len());
        for (i, param) in function.get_params().into_iter().enumerate() {
            eprintln!("DEBUG: compile_method_call: param {} type={:?}", i, param.get_type());
        }
        
        // Build the call with object as first argument (self)
        let mut call_args = vec![object_value];
        call_args.extend(compiled_args);
        
        // Convert arguments to BasicMetadataValueEnum
        let metadata_args: Vec<inkwell::values::BasicMetadataValueEnum> = call_args.iter()
            .map(|v| (*v).into())
            .collect();
        
        let call_result = self.backend.builder.build_call(
            function,
            &metadata_args,
            &format!("{}_call", method_name)
        ).map_err(|e| self.error("compile_method_call",
            format!("failed to call method '{}': {}", method_name, e)))?;
        
        // Return the result
        match call_result.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => Ok(val),
            inkwell::values::ValueKind::Instruction(_) => Ok(inkwell::values::BasicValueEnum::IntValue(
                self.backend.context.i64_type().const_int(0, false)
            )),
        }
    }

    /// Get field index by name in a struct type
    fn get_field_index(&self, struct_type: &inkwell::types::StructType<'ctx>, field_name: &str) -> Result<usize, String> {
        // Try to get field names from class definitions
        for (class_name, class_def) in &self.classes {
            if struct_type.get_name().map_or(false, |n| n.to_string_lossy().contains(class_name)) {
                for (i, field) in class_def.fields.iter().enumerate() {
                    if field.name == field_name {
                        return Ok(i);
                    }
                }
            }
        }
        
        // Fallback: use field count and assume sequential naming
        let count = struct_type.count_fields() as usize;
        for i in 0..count {
            // Try to find the field by index
            if struct_type.get_field_type_at_index(i as u32).is_some() {
                // If we can't get the name, just return the index
                // The caller will handle any type mismatches
                return Ok(i);
            }
        }
        
        Err(format!("field '{}' not found in struct", field_name))
    }
}

/// Find the position of an operator outside of parentheses and string literals
/// Returns the first position where the operator appears outside any parentheses or string literals
fn find_operator_outside_parens(expr: &str, op: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let expr_bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();

    let mut i = 0;
    while i < expr_bytes.len() {
        let ch = expr_bytes[i] as char;

        if escape_next {
            escape_next = false;
            i += 1;
        } else if ch == '\\' {
            escape_next = true;
            i += 1;
        } else if ch == '"' && !in_string {
            in_string = true;
            i += 1;
        } else if ch == '"' && in_string {
            in_string = false;
            i += 1;
        } else if !in_string && ch == '(' {
            depth += 1;
            i += 1;
        } else if !in_string && ch == ')' {
            depth -= 1;
            i += 1;
        } else if !in_string && depth == 0 && i + op_bytes.len() <= expr_bytes.len() {
            // Check if operator matches at this position
            if &expr_bytes[i..i + op_bytes.len()] == op_bytes {
                return Some(i);
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    None
}
