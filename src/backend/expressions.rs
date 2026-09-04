//! Expression compilation for Coffee compiler
//!
//! Compiles expressions including literals, variables, binary/unary operations,
//! function calls, array indexing, and format strings.
//!
//! Variant-specific methods live in `src/backend/expr/` and are loaded from this
//! facade so `backend/mod.rs` can keep `pub mod expressions` without a new `expr` entry.

use super::codegen::CodeGenerator;
use inkwell::values::BasicValueEnum;
use crate::parser::expr::Expression;
#[cfg(test)]
use crate::coffee_debug;
#[cfg(test)]
use crate::backend::type_inference;
#[cfg(test)]
use crate::backend::type_inference::TypeInferenceContext;
#[cfg(test)]
use inkwell::types::BasicTypeEnum;

#[path = "expr/mod.rs"]
mod expr;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile an expression from leftover source text (e.g. `VariableDecl.value`).
    pub fn compile_source_as_expr(&mut self, src: &str) -> Result<BasicValueEnum<'ctx>, String> {
        let expr = self.strip_inline_comments(src);
        let expr = expr.trim();
        if expr.is_empty() {
            return Err(self.error("compile_expr", "empty expression"));
        }
        match Expression::parse(expr) {
            Ok(tree) => self.compile_expr(&tree),
            Err(_) => self.compile_expr(&Expression::Literal(expr.to_string())),
        }
    }

    /// Compile LLVM IR from an `Expression` tree (production path).
    pub fn compile_expr(&mut self, expr: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        self.expression_depth += 1;
        if self.expression_depth > self.max_expression_depth {
            self.expression_depth -= 1;
            return Err(self.error("compile_expression",
                format!("expression nesting depth {} exceeds maximum {}",
                    self.expression_depth, self.max_expression_depth)));
        }
        let result = self.compile_expr_inner(expr);
        self.expression_depth -= 1;
        result
    }

    fn compile_expr_inner(&mut self, expr: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            Expression::Literal(s) => self.compile_literal_atom(s),
            Expression::Variable(name) => self.compile_variable_ref(name),
            Expression::FString { template, placeholders } => {
                self.compile_fstring_from_ast(template, placeholders)
            }
            Expression::Binary { left, op, right } => {
                let l = self.compile_expr(left)?;
                let r = self.compile_expr(right)?;
                self.build_binary_op(op, l, r)
            }
            Expression::Unary { op, operand } => {
                let operand_val = self.compile_expr(operand)?;
                self.compile_unary_op(op, operand_val)
            }
            Expression::Call { function, args } => self.compile_call_expr(function, args),
            Expression::ConstructorCall { class_name, args } => {
                self.compile_constructor_from_ast(class_name, args)
            }
            Expression::Member { object, field, args } => {
                self.compile_member_expr(object, field, args)
            }
            Expression::Index { array, index } => self.compile_index_expr(array, index),
            Expression::ArrayLiteral { elements } => self.compile_array_literal_expr(elements),
            Expression::TupleLiteral { elements } => self.compile_tuple_literal_expr(elements),
            Expression::StructLiteral { struct_name, fields } => {
                self.compile_struct_literal_from_ast(struct_name, fields)
            }
            Expression::Assign { object, field_name, value } => {
                self.compile_assign_expr(object, field_name, value)
            }
            Expression::TypeCast { target_type, value } => {
                self.compile_type_cast_expr(target_type, value)
            }
        }
    }

    /// Compile expression from string
    #[cfg(test)]
    pub fn compile_expression_str(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
        coffee_debug!("DEBUG: compile_expression_str: expr='{}', len={}", expr, expr.len());
        self.compile_expression_str_with_inference(expr, &TypeInferenceContext::new())
    }

    /// Compile expression from string with type inference
    #[cfg(test)]
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
    #[cfg(test)]
    fn compile_expression_str_inner_with_inference(&mut self, expr: &str, inference_ctx: &TypeInferenceContext<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        coffee_debug!("DEBUG: compile_expression_str_inner_with_inference: expr='{}', len={}", expr, expr.len());
        
        // Check if it's a simple literal value (not containing operators or function calls)
        // But also check if it's a variable reference (alphanumeric + underscore)
        // IMPORTANT: Member access (object.field) should NOT be treated as simple literal
        // IMPORTANT: Struct literals (Point { x: 10 }) should NOT be treated as simple literal
        let is_simple_literal = !expr.contains('(') && !expr.contains('[') && !expr.contains('+') && !expr.contains('-') && !expr.contains('*') && !expr.contains('/') && !expr.contains('%') && !expr.contains('=') && !expr.contains('!') && !expr.contains('&') && !expr.contains('|') && !expr.contains('^') && !expr.contains('<') && !expr.contains('>') && !expr.contains('?') && !expr.contains(':') && !expr.contains('.') && !expr.contains('{');
        
        // Check if it's a variable reference (starts with letter or underscore, contains only alphanumeric and underscore)
        // Exclude expressions with '::' (qualified names like Class::method)
        let is_variable = !expr.contains("::") && expr.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) 
            && expr.chars().all(|c| c.is_alphanumeric() || c == '_');
        
        coffee_debug!("DEBUG: compile_expression_str_inner_with_inference: is_simple_literal={}, is_variable={}", is_simple_literal, is_variable);
        
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
    #[cfg(test)]
    fn compile_expression_str_inner(&mut self, expr: &str) -> Result<BasicValueEnum<'ctx>, String> {
        coffee_debug!("DEBUG: compile_expression_str_inner: expr='{}', len={}", expr, expr.len());
        
        // IMPORTANT: Strip inline comments before processing the expression
        // This prevents comments from being parsed as part of the expression
        let expr = self.strip_inline_comments(expr);
        
        coffee_debug!("DEBUG: compile_expression_str_inner: after strip_inline_comments, expr='{}', len={}", expr, expr.len());

        // Struct literal: Point { x: 10, y: 20 }
        // MUST come before binary operation check to avoid splitting struct literals
        if let Some(pos) = expr.find('{') {
            if expr.ends_with('}') {
                let struct_name = expr[..pos].trim();
                let fields_str = &expr[pos + 1..expr.len() - 1].trim();
                coffee_debug!("DEBUG: compile_expression_str_inner: struct literal, struct_name='{}', fields_str='{}'", struct_name, fields_str);
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
        coffee_debug!("DEBUG: compile_expression_str_inner: expr='{}', expr.contains('(')={}, expr.ends_with(')')={}", expr, expr.contains('('), expr.ends_with(')'));
        if expr.contains('(') && expr.ends_with(')') {
            coffee_debug!("DEBUG: compile_expression_str_inner: expr contains '(' and ends with ')', expr='{}'", expr);

            // Check if this is a type conversion function: int(value), float(value), bool(value)
            let paren_pos = expr.find('(').unwrap();
            let func_name = &expr[..paren_pos].trim();
            if *func_name == "int" || *func_name == "float" || *func_name == "bool" {
                coffee_debug!("DEBUG: compile_expression_str_inner: type conversion function '{}'", func_name);
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
                    coffee_debug!("DEBUG: compile_expression_str_inner: checking if '{}' is an enum type", scope_str);
                    if self.enums.contains_key(&scope_str.to_string()) {
                        coffee_debug!("DEBUG: compile_expression_str_inner: '{}' is an enum type, constructing variant '{}'", scope_str, variant_name);
                        
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
                    coffee_debug!("DEBUG: compile_expression_str_inner: detected method call, object_str='{}', method_name='{}', args_str='{}'", object_str, method_name, args_str);
                    return self.compile_method_call(object_str, method_name, args_str);
                }
            }

            coffee_debug!("DEBUG: compile_expression_str_inner: expr.contains(\"::new\")={}", expr.contains("::new"));
            // Check if this is a constructor call: class::new(args)
            if expr.contains("::new") {
                let parts: Vec<&str> = expr.split("::").collect();
                coffee_debug!("DEBUG: constructor call: expr='{}', expr.len()={}, parts={:?}", expr, expr.len(), parts);
                if parts.len() == 2 {
                    coffee_debug!("DEBUG: constructor call: parts[0]='{}', parts[0].len()={}, parts[1]='{}', parts[1].len()={}", parts[0], parts[0].len(), parts[1], parts[1].len());
                    if parts[1].starts_with("new") {
                        let class_name = parts[0].trim();
                        let full_method = parts[1].trim();
                        coffee_debug!("DEBUG: constructor call: class_name='{}', full_method='{}', full_method.len()={}", class_name, full_method, full_method.len());
                        let args_str = &full_method[4..full_method.len()-1].trim(); // Remove "new(" prefix and ")" suffix
                        coffee_debug!("DEBUG: constructor call: args_str='{}', args_str.len()={}", args_str, args_str.len());

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
                    coffee_debug!("DEBUG: compile_expression_str_inner: checking if '{}' is an enum type, enums.keys() = {:?}", object_str, self.enums.keys().collect::<Vec<_>>());
                    if self.enums.contains_key(&object_str.to_string()) {
                        coffee_debug!("DEBUG: compile_expression_str_inner: '{}' is an enum type", object_str);
                        // This is an enum variant: Enum.Variant or Enum.Variant(args)
                        let variant_name = if rest.contains('(') && rest.ends_with(')') {
                            // Enum.Variant(args) - extract variant name
                            &rest[..rest.find('(').unwrap()]
                        } else {
                            // Enum.Variant - simple variant
                            rest
                        };

                        let global_name = format!("{}_{}", object_str, variant_name);
                        coffee_debug!("DEBUG: compile_expression_str_inner: loading enum variant '{}'", global_name);

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
                        coffee_debug!("DEBUG: compile_expression_str_inner: '{}' is NOT an enum type", object_str);
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
            coffee_debug!("DEBUG: compile_expression_str_inner: found variable '{}' in variables", expr);
            
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

        coffee_debug!("DEBUG: compile_expression_str_inner: variable '{}' not found in variables, keys: {:?}", expr, self.variables.keys().collect::<Vec<_>>());

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
    #[cfg(test)]
    fn try_compile_binary_op(&mut self, expr: &str) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        coffee_debug!("DEBUG: try_compile_binary_op: expr='{}'", expr);
        // Check for binary operators (simple tokenization)
        // IMPORTANT: Respect operator precedence when finding operators
        // Operators are checked from LOWEST to HIGHEST precedence
        // This ensures that higher precedence operators are evaluated first
        // Precedence (highest to lowest): !, *, /, %, +, -, <<, >>, &, |, ^, <, >, <=, >=, ==, !=, &&, ||
        for op in [" && ", " || ", " == ", " != ", " < ", " > ", " <= ", " >= ", " ^ ", " | ", " & ", " << ", " >> ", " + ", " - ", " * ", " / ", " % "] {
            if let Some(pos) = find_operator_outside_parens(expr, op) {
                coffee_debug!("DEBUG: try_compile_binary_op: found op '{}' at pos={}", op, pos);
                let left_str = &expr[..pos];
                let right_str = &expr[pos + op.len()..];

                let left = self.compile_expression_str(left_str)?;
                let right = self.compile_expression_str(right_str)?;

                let result = self.build_binary_op(op.trim(), left, right)?;
                return Ok(Some(result));
            }
        }

        coffee_debug!("DEBUG: try_compile_binary_op: no operator found");
        Ok(None)
    }
    /// Compile format string (f"...") with safe placeholder substitution
    /// This performs runtime type validation and prevents injection attacks
    #[cfg(test)]
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

    #[cfg(test)]
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
            coffee_debug!("DEBUG: compile_struct_literal: struct_name='{}', fields_str='{}'", struct_name, fields_str);
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
                        coffee_debug!("DEBUG: compile_struct_literal: found comma at paren_depth={}, brace_depth={}, current_field='{}'", paren_depth, brace_depth, current_field);
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
            coffee_debug!("DEBUG: compile_struct_literal: fields={:?}", fields);
            
            for field_pair in fields {
                coffee_debug!("DEBUG: compile_struct_literal: field_pair='{}'", field_pair);
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

                coffee_debug!("DEBUG: compile_struct_literal: field_pair='{}', colon_pos={:?}", field_pair, colon_pos);

                if let Some(pos) = colon_pos {
                    let field_name = field_pair[..pos].trim();
                    let field_value_expr = field_pair[pos + 1..].trim();
                    coffee_debug!("DEBUG: compile_struct_literal: field_name='{}', field_value_expr='{}'", field_name, field_value_expr);

                    // Compile field value
                    // Special handling for wildcard _ (skip compilation, will be ignored in match)
                    let field_value = if field_value_expr == "_" {
                        // Use a default value (0) for wildcard
                        // This will be ignored in pattern matching anyway
                        self.backend.context.i64_type().const_int(0, false).into()
                    } else {
                        match Expression::parse(field_value_expr) {
                            Ok(tree) => self.compile_expr(&tree)?,
                            Err(_) => self.compile_expr(&Expression::Literal(field_value_expr.to_string()))?,
                        }
                    };

                    // Get field index
                    let field_index = self.type_mapper.get_field_index(struct_name, field_name)
                        .ok_or_else(|| self.error("compile_struct_literal",
                            format!("field '{}' not found in struct '{}'", field_name, struct_name)))?;

                    // Get field type
                    coffee_debug!("DEBUG: compile_struct_literal: struct_type.name={:?}, field_index={}, struct_type.count_fields()={}", 
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
    #[cfg(test)]
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
                    let arg_value = self.compile_source_as_expr(arg_str)?;
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
}

/// Find the position of an operator outside of parentheses and string literals
/// Returns the first position where the operator appears outside any parentheses or string literals
#[cfg(test)]
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
