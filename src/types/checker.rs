use crate::coffee_debug;
use super::definition::*;
use super::registry::TypeRegistry;
use super::errors::{TypeSystemError, Diagnostic};
use crate::parser;
use crate::semantic::SemanticAnalyzer;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Type bounds for primitive types
#[derive(Debug, Clone)]
pub struct TypeBounds {
    /// Integer minimum value
    pub int_min: i64,
    /// Integer maximum value
    pub int_max: i64,
    /// Float minimum value
    pub float_min: f64,
    /// Float maximum value
    pub float_max: f64,
}

impl Default for TypeBounds {
    fn default() -> Self {
        TypeBounds {
            int_min: i64::MIN,
            int_max: i64::MAX,
            float_min: f64::MIN,
            float_max: f64::MAX,
        }
    }
}

/// State of a value for ownership checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueState {
    /// Value is alive and owned
    Alive,
    /// Value has been moved
    Moved,
    /// Value has been dropped/freed
    Dropped { location: String },
    /// Value is borrowed
    Borrowed { immutable: bool },
}

/// Value ownership information
#[derive(Debug, Clone)]
pub struct ValueInfo {
    /// Name of the value
    pub name: String,
    /// Type of the value
    pub ty: Type,
    /// Current state
    pub state: ValueState,
    /// Defining scope
    pub scope: SpaceId,
    /// Declaration location
    pub location: Span,
    /// Lifetime information
    pub lifetime: Option<LifetimeInfo>,
}

/// Lifetime information for a value
#[derive(Debug, Clone)]
pub struct LifetimeInfo {
    /// Lifetime start point
    pub start_point: usize,
    /// Expected end point
    pub end_point: usize,
    /// Whether value is returned
    pub returned: bool,
}

/// Import information for circular dependency checking
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Module being imported
    pub module: String,
    /// Source file doing the import
    pub source_file: String,
    /// Import location
    pub location: Span,
}

/// Current import stack for circular detection
#[derive(Debug, Clone)]
pub struct ImportStack {
    /// Stack of imports
    stack: Vec<ImportInfo>,
}

impl ImportStack {
    pub fn new() -> Self {
        ImportStack { stack: Vec::new() }
    }

    /// Push an import onto the stack
    pub fn push(&mut self, import: ImportInfo) -> Result<(), TypeSystemError> {
        // Check for circular imports
        for existing in &self.stack {
            if existing.module == import.module {
                let mut path: Vec<String> = self.stack.iter().map(|i| i.module.clone()).collect();
                path.push(import.module.clone());

                return Err(TypeSystemError::Cycle { path });
            }
        }
        self.stack.push(import);
        Ok(())
    }

    /// Pop from the stack
    pub fn pop(&mut self) -> Option<ImportInfo> {
        self.stack.pop()
    }

    /// Get current path
    pub fn path(&self) -> Vec<String> {
        self.stack.iter().map(|i| i.module.clone()).collect()
    }
}

/// Checking mode for different levels of analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckingMode {
    /// Simple type checking only (like SimpleTypeChecker)
    Simple,
    /// Comprehensive checking with ownership and lifetime analysis
    Comprehensive,
}

/// 统一类型检查器，合并 SimpleTypeChecker 和 space::TypeChecker 功能
pub struct TypeChecker {
    /// Type registry
    registry: Arc<RwLock<TypeRegistry>>,
    /// Semantic analyzer for symbol lookup
    analyzer: Option<Arc<RwLock<SemanticAnalyzer>>>,
    /// Checking mode
    mode: CheckingMode,
    /// Current return type (for return statement checking)
    current_return_type: Option<Type>,
    /// Type bounds
    bounds: TypeBounds,
    /// Value ownership tracking
    values: HashMap<String, ValueInfo>,
    /// Import stack for circular detection
    import_stack: Arc<RwLock<ImportStack>>,
    /// Imported symbols tracking
    imported_symbols: HashMap<String, Vec<String>>,
    /// Search paths for imports
    import_search_paths: Vec<String>,
    /// Error collection
    errors: Vec<TypeSystemError>,
}

impl TypeChecker {
    /// Create new type checker with specified mode
    pub fn new(registry: Arc<RwLock<TypeRegistry>>, mode: CheckingMode) -> Self {
        Self::with_analyzer(registry, mode, None)
    }

    /// Create new type checker with semantic analyzer
    pub fn with_analyzer(registry: Arc<RwLock<TypeRegistry>>, mode: CheckingMode, analyzer: Option<Arc<RwLock<SemanticAnalyzer>>>) -> Self {
        let mut import_search_paths = Vec::new();
        import_search_paths.push(".".to_string());
        import_search_paths.push("examples".to_string());

        TypeChecker {
            registry,
            analyzer,
            mode,
            current_return_type: None,
            bounds: TypeBounds::default(),
            values: HashMap::new(),
            import_stack: Arc::new(RwLock::new(ImportStack::new())),
            imported_symbols: HashMap::new(),
            import_search_paths,
            errors: Vec::new(),
        }
    }

    /// Create simple type checker (equivalent to SimpleTypeChecker)
    pub fn simple(registry: Arc<RwLock<TypeRegistry>>) -> Self {
        Self::new(registry, CheckingMode::Simple)
    }

    /// Create comprehensive type checker (equivalent to space::TypeChecker)
    pub fn comprehensive(registry: Arc<RwLock<TypeRegistry>>) -> Self {
        Self::new(registry, CheckingMode::Comprehensive)
    }

    /// Set the semantic analyzer
    pub fn set_analyzer(&mut self, analyzer: Arc<RwLock<SemanticAnalyzer>>) {
        self.analyzer = Some(analyzer);
    }

    /// Get collected errors
    pub fn errors(&self) -> &[TypeSystemError] {
        &self.errors
    }

    /// Clear errors
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// Add an error
    fn add_error(&mut self, error: TypeSystemError) {
        coffee_debug!("[DEBUG] add_error: adding error: {:?}", error);
        self.errors.push(error);
    }

    /// Set current return type (for function checking)
    pub fn set_return_type(&mut self, ty: Type) {
        self.current_return_type = Some(ty);
    }

    /// Look up function from symbol space
    fn lookup_function(&self, func_name: &str) -> Option<crate::parser::function::Function> {
        if let Some(ref analyzer) = self.analyzer {
            if let Ok(analyzer) = analyzer.read() {
                // Try to find function in C symbols
                if let Ok(cfc_symbols) = analyzer.get_cfc_symbols() {
                    for (lib_name, symbol_table) in cfc_symbols.iter() {
                        if let Some(c_symbol) = symbol_table.symbols.get(func_name) {
                            // Return a placeholder function - in reality we'd need to properly
                            // convert the C symbol to a Function definition
                            return None; // Placeholder - need proper implementation
                        }
                    }
                }
            }
        }
        None
    }

    /// Set function return type from type string
    pub fn set_function_return_type(&mut self, type_str: &str) -> Result<(), TypeSystemError> {
        let return_type = self.registry.read().unwrap().resolve_type(type_str)
            .map_err(|_| TypeSystemError::ParseError {
                type_str: type_str.to_string(),
                reason: "Failed to resolve return type".to_string(),
            })?;
        self.current_return_type = Some(return_type);
        Ok(())
    }

    /// Check a return statement
    pub fn check_return_statement(&mut self, return_stmt: &parser::var::ReturnStmt) -> Result<(), TypeSystemError> {
        coffee_debug!("[DEBUG] check_return_statement: checking return statement");
        if let Some(ref expr) = return_stmt.value {
            coffee_debug!("[DEBUG] check_return_statement: inferring type for return value '{}'", expr);
            let expr_type = match crate::parser::expr::parse_expression(expr) {
                Ok(parsed) => self.check_expression(&parsed)?,
                Err(_) => self.infer_value_type(expr)?,
            };
            coffee_debug!("[DEBUG] check_return_statement: inferred return value type as {:?}", expr_type);
            
            // Check against expected return type
            if let Some(ref expected_type) = self.current_return_type {
                coffee_debug!("[DEBUG] check_return_statement: expected return type is {:?}", expected_type);
                if !self.types_compatible(&expr_type, expected_type)? {
                    coffee_debug!("[DEBUG] check_return_statement: return type mismatch");
                    let error = TypeSystemError::type_mismatch(
                        expected_type.clone(),
                        expr_type,
                        Span::new(0, expr.len())
                    );
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Check a variable declaration
    pub fn check_variable_decl(&mut self, decl: &parser::var::VariableDecl) -> Result<(), TypeSystemError> {
        let location = Span::new(0, decl.name.len());

        coffee_debug!("[DEBUG] check_variable_decl: checking variable '{}' with type '{}', value '{}'", decl.name, decl.var_type, decl.value);

        // Check if variable already exists (comprehensive mode only)
        if self.mode == CheckingMode::Comprehensive {
            if let Some(existing) = self.values.get(&decl.name) {
                if existing.state == ValueState::Alive {
                    let error = TypeSystemError::Duplicate {
                        name: decl.name.clone(),
                        existing: existing.location,
                        new: location,
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
        }

        // Resolve type
        let ty = {
            let registry_result = self.registry.read();
            match registry_result {
                Ok(reg) => reg.resolve_type(&decl.var_type),
                Err(_) => {
                    drop(registry_result); // Explicitly drop the registry lock
                    let error = TypeSystemError::ParseError {
                        type_str: decl.var_type.clone(),
                        reason: "Failed to access registry".to_string(),
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
        }?;

        coffee_debug!("[DEBUG] check_variable_decl: resolved type to {:?}", ty);

        // Check value type if value is provided
        if !decl.value.is_empty() {
            let value_type = match crate::parser::expr::parse_expression(&decl.value) {
                Ok(expr) => self.check_expression(&expr)?,
                Err(_) => self.infer_value_type(&decl.value)?,
            };
            coffee_debug!("[DEBUG] check_variable_decl: inferred value type as {:?}", value_type);
            if !self.types_compatible(&value_type, &ty)? {
                coffee_debug!("[DEBUG] check_variable_decl: types NOT compatible: {:?} vs {:?}", value_type, ty);
                let error = TypeSystemError::TypeMismatch {
                    expected: ty.clone(),
                    found: value_type.clone(),
                    span: Span::new(0, decl.value.len()),
                };
                coffee_debug!("[DEBUG] check_variable_decl: creating TypeMismatch error: expected {:?}, found {:?}", ty, value_type);
                self.add_error(error.clone());
                return Err(error);
            }
        }

        // Check type bounds if value is a literal (comprehensive mode)
        if self.mode == CheckingMode::Comprehensive && !decl.value.is_empty() {
            self.check_value_bounds(&ty, &decl.value, location)?;
        }

        // Track the value
        if self.mode == CheckingMode::Comprehensive {
            self.values.insert(decl.name.clone(), ValueInfo {
                name: decl.name.clone(),
                ty,
                state: ValueState::Alive,
                scope: SpaceId::new(),
                location,
                lifetime: None,
            });
        }

        Ok(())
    }

    /// Type-check a statement, recursing into function / if / while / match bodies
    /// so nested `let` and memory ops are checked. Compiler hookup: call this from
    /// `CompilationPipeline::type_check_statement`.
    pub fn check_statement(&mut self, statement: &parser::Statement) -> Result<(), TypeSystemError> {
        match statement {
            parser::Statement::VariableDecl(decl) => self.check_variable_decl(decl),
            parser::Statement::MemoryOp(op) => self.check_memory_op(op).map_err(Into::into),
            parser::Statement::Function(func) => self.check_function(func),
            parser::Statement::If(if_expr) => self.check_if(if_expr),
            parser::Statement::While(while_loop) => self.check_while(while_loop),
            parser::Statement::Match(match_expr) => self.check_match(match_expr),
            parser::Statement::For(for_loop) => self.check_stmt_list(&for_loop.body),
            parser::Statement::Expr(expr) => self.check_expr_stmt(expr),
            parser::Statement::Return(ret) => self.check_return_statement(ret),
            parser::Statement::Assignment(_, value) => self.check_expr_stmt(value),
            parser::Statement::Raise(_)
            | parser::Statement::Import(_)
            | parser::Statement::Main(_)
            | parser::Statement::Class(_)
            | parser::Statement::Enum(_)
            | parser::Statement::Break(_)
            | parser::Statement::Continue(_)
            | parser::Statement::SingleLineComment(_)
            | parser::Statement::MultiLineComment(_) => Ok(()),
        }
    }

    fn check_stmt_list(&mut self, statements: &[parser::Statement]) -> Result<(), TypeSystemError> {
        let mut first_err = None;
        for stmt in statements {
            if let Err(e) = self.check_statement(stmt) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn check_expr_stmt(&mut self, expr: &crate::parser::expr::Expression) -> Result<(), TypeSystemError> {
        match self.check_expression(expr) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.add_error(e.clone());
                Err(e)
            }
        }
    }

    fn check_function(&mut self, func: &parser::Function) -> Result<(), TypeSystemError> {
        let prev_return = self.current_return_type.clone();
        let prev_values = self.values.clone();
        if let Err(e) = self.set_function_return_type(&func.return_type) {
            self.add_error(e.clone());
            self.current_return_type = prev_return;
            return Err(e);
        }
        for param in &func.parameters {
            let ty = match self.registry.read() {
                Ok(reg) => reg.resolve_type(&param.param_type).unwrap_or(Type::unit()),
                Err(_) => Type::unit(),
            };
            self.values.insert(param.name.clone(), ValueInfo {
                name: param.name.clone(),
                ty,
                state: ValueState::Alive,
                scope: SpaceId::new(),
                location: Span::new(0, param.name.len()),
                lifetime: None,
            });
        }
        let result = match &func.body {
            parser::FunctionBody::Block(stmts) => self.check_stmt_list(stmts),
            parser::FunctionBody::Expression(expr) => self.check_expr_stmt(expr),
            parser::FunctionBody::External => Ok(()),
        };
        self.current_return_type = prev_return;
        self.values = prev_values;
        result
    }

    fn check_if(&mut self, if_expr: &parser::IfExpr) -> Result<(), TypeSystemError> {
        self.check_expr_stmt(&if_expr.condition)?;
        self.check_stmt_list(&if_expr.body)?;
        for elif in &if_expr.elifs {
            self.check_expr_stmt(&elif.condition)?;
            self.check_stmt_list(&elif.body)?;
        }
        if let Some(else_body) = &if_expr.else_body {
            self.check_stmt_list(else_body)?;
        }
        Ok(())
    }

    fn check_while(&mut self, while_loop: &parser::WhileLoop) -> Result<(), TypeSystemError> {
        self.check_expr_stmt(&while_loop.condition)?;
        self.check_stmt_list(&while_loop.body)
    }

    fn check_match(&mut self, match_expr: &parser::MatchExpr) -> Result<(), TypeSystemError> {
        self.check_expr_stmt(&match_expr.value)?;
        for arm in &match_expr.arms {
            let _ = self.check_expression(&arm.pattern);
            if let Some(guard) = &arm.guard {
                self.check_expr_stmt(guard)?;
            }
            self.check_stmt_list(&arm.body)?;
        }
        Ok(())
    }

    /// Check an expression
    pub fn check_expression(&mut self, expr: &crate::parser::expr::Expression) -> Result<Type, TypeSystemError> {
        match expr {
            crate::parser::expr::Expression::Literal(value) => {
                self.infer_value_type(value)
            },
            crate::parser::expr::Expression::Variable(name) => {
                self.infer_value_type(name)
            },
            crate::parser::expr::Expression::Binary { left, op, right } => {
                let left_type = self.check_expression(left)?;
                let right_type = self.check_expression(right)?;
                self.check_binary_operation(&left_type, op, &right_type)
            },
            crate::parser::expr::Expression::Call { function, args } => {
                // Extract function name
                let function_name = match function.as_ref() {
                    crate::parser::expr::Expression::Variable(name) => name.clone(),
                    _ => return Err(TypeSystemError::ParseError {
                        type_str: "function call".to_string(),
                        reason: "Complex function expressions not yet supported".to_string(),
                    }),
                };

                // Convert to FunctionCall for existing logic
                let function_call = crate::parser::function::FunctionCall {
                    name: function_name,
                    args: args.iter().map(|arg| {
                        match arg {
                            crate::parser::expr::Expression::Literal(value) => value.clone(),
                            crate::parser::expr::Expression::Variable(name) => name.clone(),
                            _ => "placeholder".to_string(),
                        }
                    }).collect(),
                };

                self.check_function_call(&function_call)
            },
            crate::parser::expr::Expression::Member { object, field, args } => {
                let object_type = self.check_expression(object)?;
                let class_name = match &object_type {
                    Type::NamedType { name } => name.clone(),
                    _ => {
                        return Err(TypeSystemError::ParseError {
                            type_str: "member access".to_string(),
                            reason: format!("Cannot access field '{}' on non-class type {:?}", field, object_type),
                        })
                    }
                };

                if !args.is_empty() {
                    return self.check_method_call(&class_name, field, args);
                }

                // Field access. Zero-arg `obj.method()` is also Member with empty args;
                // if there is no field, fall back to a method lookup.
                match self.lookup_field_type(&class_name, field) {
                    Ok(field_type) => Ok(field_type),
                    Err(TypeSystemError::FieldNotFound { .. }) => {
                        match self.lookup_method(&class_name, field) {
                            Some(method) => Ok(method.return_type),
                            None => Ok(Type::void()),
                        }
                    }
                    Err(e) => Err(e),
                }
            },
            crate::parser::expr::Expression::StructLiteral { struct_name, .. } => {
                // Return the struct type
                Ok(crate::types::Type::NamedType { name: struct_name.clone() })
            },
            crate::parser::expr::Expression::Unary { op, operand } => {
                let operand_type = self.check_expression(operand)?;
                self.check_unary_operation(op, &operand_type)
            },
            crate::parser::expr::Expression::FString { template: _, placeholders } => {
                // Validate that all placeholders exist in current scope
                for placeholder in placeholders {
                    if !self.values.contains_key(placeholder) {
                        return Err(TypeSystemError::UndefinedVariable {
                            name: placeholder.clone(),
                            span: Span::new(0, 0), // Will be updated with proper location
                        });
                    }

                    // Validate that the placeholder has a safe-to-format type
                    let value_info = &self.values[placeholder];
                    match &value_info.ty {
                        Type::Int { .. } | Type::Float { .. } | Type::Bool | Type::String => {
                            // These types are safe for string formatting
                        }
                        _ => {
                            return Err(TypeSystemError::ParseError {
                                type_str: format!("f-string placeholder '{}'", placeholder),
                                reason: format!("type {:?} cannot be formatted into string", value_info.ty),
                            });
                        }
                    }
                }
                Ok(Type::string())
            },
            crate::parser::expr::Expression::ConstructorCall { class_name, .. } => {
                // Return the class type
                Ok(crate::types::Type::NamedType { name: class_name.clone() })
            },
            crate::parser::expr::Expression::ArrayLiteral { elements } => {
                // Check all elements have the same type
                if elements.is_empty() {
                    // Empty array - default to int array
                    return Ok(Type::Array { elem: Box::new(Type::int()), size: 0 });
                }

                // Infer type from first element
                let first_type = self.check_expression(&elements[0])?;

                // Check all other elements have the same type
                for element in &elements[1..] {
                    let elem_type = self.check_expression(element)?;
                    if elem_type != first_type {
                        return Err(TypeSystemError::TypeMismatch {
                            expected: first_type.clone(),
                            found: elem_type,
                            span: Span::new(0, 0),
                        });
                    }
                }

                // Return array type
                Ok(Type::Array {
                    elem: Box::new(first_type),
                    size: elements.len(),
                })
            },
            crate::parser::expr::Expression::TupleLiteral { elements } => {
                // Check all element expressions
                let mut elem_types = Vec::new();
                for element in elements {
                    let elem_type = self.check_expression(element)?;
                    elem_types.push(elem_type);
                }

                // Return tuple type
                Ok(Type::Tuple(elem_types))
            },
            crate::parser::expr::Expression::Index { array, index } => {
                // Check array expression
                let array_type = self.check_expression(array)?;

                // Check index expression
                let index_type = self.check_expression(index)?;

                // Validate that index is an integer
                if !index_type.is_int() {
                    return Err(TypeSystemError::TypeMismatch {
                        expected: Type::int(),
                        found: index_type.clone(),
                        span: Span::new(0, 0),
                    });
                }

                // Return the element type of the array
                match array_type {
                    Type::Array { ref elem, .. } => Ok(*elem.clone()),
                    Type::Slice(ref elem) => Ok(*elem.clone()),
                    _ => Err(TypeSystemError::ParseError {
                        type_str: format!("array indexing"),
                        reason: format!("cannot index non-array type {:?}", array_type),
                    }),
                }
            },
            crate::parser::expr::Expression::Assign { object, field_name, value } => {
                // Check that object is 'self'
                match object.as_ref() {
                    crate::parser::expr::Expression::Variable(name) if name == "self" => {
                        // Check the value expression
                        self.check_expression(value)?;
                        // Assign expressions don't produce a value
                        Ok(Type::void())
                    }
                    _ => Err(TypeSystemError::ParseError {
                        type_str: "assign".to_string(),
                        reason: format!("assign can only be used with 'self', found {}", object),
                    }),
                }
            },
            crate::parser::expr::Expression::TypeCast { target_type, value } => {
                // Check the value expression
                self.check_expression(value)?;
                // Return the target type
                match target_type.as_str() {
                    "int" => Ok(Type::int()),
                    "float" => Ok(Type::float()),
                    "bool" => Ok(Type::bool()),
                    _ => Err(TypeSystemError::ParseError {
                        type_str: "type cast".to_string(),
                        reason: format!("unknown target type: {}", target_type),
                    }),
                }
            },
        }
    }

    fn lookup_field_type(&self, class_name: &str, field: &str) -> Result<Type, TypeSystemError> {
        let registry = self.registry.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: "member access".to_string(),
                reason: "Failed to access type registry".to_string(),
            })?;

        let fields = match registry.get_type(class_name) {
            Some(TypeDef::Class { fields, .. }) | Some(TypeDef::Struct { fields, .. }) => fields,
            Some(_) => {
                return Err(TypeSystemError::ParseError {
                    type_str: "member access".to_string(),
                    reason: format!("'{}' is not a class", class_name),
                })
            }
            None => {
                return Err(TypeSystemError::UndefinedType {
                    name: class_name.to_string(),
                    span: Span::new(0, 0),
                })
            }
        };

        let field_type_str = match fields.iter().find(|f| f.name == field) {
            Some(field_def) => field_def.ty.clone(),
            None => {
                return Err(TypeSystemError::FieldNotFound {
                    type_name: class_name.to_string(),
                    field_name: field.to_string(),
                    span: Span::new(0, 0),
                })
            }
        };

        registry.resolve_type(&field_type_str)
            .map_err(|_| TypeSystemError::ParseError {
                type_str: field_type_str.clone(),
                reason: format!("Failed to resolve field type '{}'", field_type_str),
            })
    }

    fn lookup_method(&self, class_name: &str, method_name: &str) -> Option<MethodSignature> {
        let registry = self.registry.read().ok()?;
        registry.get_methods(class_name).into_iter().find(|m| m.name == method_name)
    }

    fn check_method_call(
        &mut self,
        class_name: &str,
        method_name: &str,
        args: &[crate::parser::expr::Expression],
    ) -> Result<Type, TypeSystemError> {
        let method = self.lookup_method(class_name, method_name).ok_or_else(|| {
            TypeSystemError::MethodNotFound {
                type_name: class_name.to_string(),
                method_name: method_name.to_string(),
                span: Span::new(0, 0),
            }
        })?;

        let param_types: Vec<Type> = match method.params.first() {
            Some(Type::NamedType { name }) if name == class_name => method.params[1..].to_vec(),
            _ => method.params.clone(),
        };

        if args.len() != param_types.len() {
            let error = TypeSystemError::arity_mismatch(param_types.len(), args.len(), Span::new(0, 0));
            self.add_error(error.clone());
            return Err(error);
        }

        for (arg, expected) in args.iter().zip(param_types.iter()) {
            let arg_type = self.check_expression(arg)?;
            if !self.types_compatible(&arg_type, expected)? {
                let error = TypeSystemError::type_mismatch(expected.clone(), arg_type, Span::new(0, 0));
                self.add_error(error.clone());
                return Err(error);
            }
        }

        Ok(method.return_type)
    }

    fn check_unary_operation(&self, op: &str, operand_type: &Type) -> Result<Type, TypeSystemError> {
        let location = Span::new(0, 1);
        match op {
            "!" => {
                if *operand_type == Type::bool() {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::invalid_operation(op, operand_type.clone(), Type::unit(), location))
                }
            }
            "-" => {
                if operand_type.is_numeric() {
                    Ok(operand_type.clone())
                } else {
                    Err(TypeSystemError::invalid_operation(op, operand_type.clone(), Type::unit(), location))
                }
            }
            _ => Err(TypeSystemError::invalid_operation(op, operand_type.clone(), Type::unit(), location)),
        }
    }

    /// Check binary operation with proper types
    fn check_binary_operation(&self, left_type: &Type, op: &str, right_type: &Type) -> Result<Type, TypeSystemError> {
        let location = Span::new(0, 1); // Simplified location

        match op {
            "+" | "-" | "*" | "/" | "%" => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    Ok(self.promote_numeric_types(left_type, right_type))
                } else {
                    Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                if self.types_compatible(left_type, right_type)? {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
                }
            }
            "&&" | "||" => {
                if *left_type == Type::bool() && *right_type == Type::bool() {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
                }
            }
            _ => {
                Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
            }
        }
    }

    /// Check function call
    pub fn check_function_call(&mut self, call: &parser::function::FunctionCall) -> Result<Type, TypeSystemError> {
        let location = Span::new(0, call.name.len());

        // Check if this is an enum variant: Enum::Variant(args)
        if call.name.contains("::") {
            let parts: Vec<&str> = call.name.split("::").collect();
            if parts.len() == 2 {
                let enum_name = parts[0].trim().to_string();
                let variant_name = parts[1].trim();
                
                // Check if this is a variant with parameters
                let variant_name_clean = if variant_name.contains('(') {
                    let paren_pos = variant_name.find('(').unwrap();
                    &variant_name[..paren_pos]
                } else {
                    variant_name
                };
                let variant_name_clean = variant_name_clean.to_string();
                
                // Try to resolve enum type
                let variant_info = {
                    let registry_result = self.registry.read();
                    match registry_result {
                        Ok(reg) => {
                            if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(&enum_name) {
                                // Check if this variant exists
                                if let Some(variant) = variants.iter().find(|v| v.name == variant_name_clean) {
                                    Some((enum_name.clone(), variant.clone()))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        },
                        Err(_) => None
                    }
                };
                
                if let Some((enum_name, variant)) = variant_info {
                    // Check argument count
                    if call.args.len() != variant.fields.len() {
                        let error = TypeSystemError::arity_mismatch(variant.fields.len(), call.args.len(), location);
                        self.add_error(error.clone());
                        return Err(error);
                    }
                    
                    // Check argument types
                    for (i, (arg, field)) in call.args.iter().zip(variant.fields.iter()).enumerate() {
                        let expected_type = {
                            let registry_result = self.registry.read();
                            match registry_result {
                                Ok(reg) => {
                                    match field {
                                        crate::types::definition::VariantField::Positional(t) => {
                                            reg.resolve_type(t).unwrap_or(Type::unit())
                                        }
                                        crate::types::definition::VariantField::Named { ty, .. } => {
                                            reg.resolve_type(ty).unwrap_or(Type::unit())
                                        }
                                    }
                                },
                                Err(_) => Type::unit()
                            }
                        };
                        
                        let arg_type = self.infer_value_type(arg)?;
                        if !self.types_compatible(&arg_type, &expected_type)? {
                            let arg_location = Span::new(location.start + i * 10, location.start + (i + 1) * 10);
                            let error = TypeSystemError::type_mismatch(expected_type, arg_type, arg_location);
                            self.add_error(error.clone());
                            return Err(error);
                        }
                    }
                    
                    // Return enum type
                    return Ok(Type::NamedType { name: enum_name });
                }
            }
        }
        
        // Get function type from registry or symbol space
        let function_type = {
            let registry_result = self.registry.read();
            match registry_result {
                Ok(reg) => {
                    // Check if it's a defined function in registry
                    if let Ok(func_type) = reg.resolve_type(&call.name) {
                        Ok(func_type)
                    } else {
                        Err("not_found")
                    }
                }
                Err(_) => Err("registry_error")
            }
        };

        let function_type = match function_type {
            Ok(ty) => ty,
            Err("not_found") => {
                // Try to find function in semantic analyzer's C symbols
                if let Some(ref analyzer) = self.analyzer {
                    if let Ok(analyzer) = analyzer.read() {
                        // Check C function symbols
                        if let Ok(cfc_symbols) = analyzer.get_cfc_symbols() {
                            for (lib_name, symbol_table) in cfc_symbols.iter() {
                                if let Some(c_symbol) = symbol_table.symbols.get(&call.name) {
                                    // Build function type from C symbol
                                    let mut param_types = Vec::new();
                                    for param in &c_symbol.parameters {
                                        if let Ok(param_type) = self.registry.read().unwrap().resolve_type(&param.param_type) {
                                            param_types.push(param_type);
                                        }
                                    }
                                    let return_type = match self.registry.read().unwrap().resolve_type(&c_symbol.return_type) {
                                        Ok(ty) => ty,
                                        Err(_) => Type::unit(),
                                    };
                                    return Ok(Type::Function {
                                        params: param_types,
                                        return_type: Box::new(return_type),
                                    });
                                }
                            }
                        }
                    }
                }
                
                let error = TypeSystemError::undefined_function(&call.name, location);
                self.add_error(error.clone());
                return Err(error);
            }
            Err(_) => {
                let error = TypeSystemError::ParseError {
                    type_str: call.name.clone(),
                    reason: "Failed to access registry".to_string(),
                };
                self.add_error(error.clone());
                return Err(error);
            }
        };

        // Extract parameter types and return type from function type
        let (param_types, return_type) = match function_type {
            Type::Function { params, return_type } => (params, *return_type),
            _ => {
                let error = TypeSystemError::not_callable(function_type, location);
                self.add_error(error.clone());
                return Err(error);
            }
        };

        // Check arity
        if call.args.len() != param_types.len() {
            let error = TypeSystemError::arity_mismatch(param_types.len(), call.args.len(), location);
            self.add_error(error.clone());
            return Err(error);
        }

        // Check argument types
        for (i, (arg, expected_type)) in call.args.iter().zip(param_types.iter()).enumerate() {
            coffee_debug!("[DEBUG] check_function_call: checking arg {} '{}' with expected type {:?}", i, arg, expected_type);
            let arg_type = self.infer_value_type(arg)?;
            coffee_debug!("[DEBUG] check_function_call: inferred arg type as {:?}", arg_type);
            if !self.types_compatible(&arg_type, expected_type)? {
                let arg_location = Span::new(location.start + i * 10, location.start + (i + 1) * 10);
                let error = TypeSystemError::type_mismatch(expected_type.clone(), arg_type, arg_location);
                self.add_error(error.clone());
                return Err(error);
            }
        }

        Ok(return_type)
    }

    /// Check binary expression
    pub fn check_binary_expr(&mut self, left: &str, op: &str, right: &str) -> Result<Type, TypeSystemError> {
        let left_type = self.infer_value_type(left)?;
        let right_type = self.infer_value_type(right)?;
        let location = Span::new(0, left.len() + op.len() + right.len());

        match op {
            "+" | "-" | "*" | "/" | "%" => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    // Division by zero check (comprehensive mode)
                    if self.mode == CheckingMode::Comprehensive && op == "/" {
                        if let Ok(val) = right.parse::<f64>() {
                            if val == 0.0 {
                                let error = TypeSystemError::InvalidOperation {
                                    op: op.to_string(),
                                    left: left_type,
                                    right: right_type,
                                    span: location,
                                };
                                self.add_error(error.clone());
                                return Err(error);
                            }
                        }
                    }
                    Ok(self.promote_numeric_types(&left_type, &right_type))
                } else {
                    let error = TypeSystemError::invalid_operation(op, left_type, right_type, location);
                    self.add_error(error.clone());
                    Err(error)
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                if self.types_compatible(&left_type, &right_type)? {
                    Ok(Type::bool())
                } else {
                    let error = TypeSystemError::invalid_operation(op, left_type, right_type, location);
                    self.add_error(error.clone());
                    Err(error)
                }
            }
            "&&" | "||" => {
                if left_type == Type::bool() && right_type == Type::bool() {
                    Ok(Type::bool())
                } else {
                    let error = TypeSystemError::invalid_operation(op, left_type, right_type, location);
                    self.add_error(error.clone());
                    Err(error)
                }
            }
            _ => {
                let error = TypeSystemError::invalid_operation(op, left_type, right_type, location);
                self.add_error(error.clone());
                Err(error)
            }
        }
    }

    /// Infer type from value string
    fn infer_value_type(&self, value: &str) -> Result<Type, TypeSystemError> {
        let value = value.trim();
        if let Some(rest) = value.strip_prefix('!') {
            let inner = self.infer_value_type(rest.trim())?;
            return self.check_unary_operation("!", &inner);
        }
        if let Some(rest) = value.strip_prefix('-') {
            if !rest.is_empty() {
                let inner = self.infer_value_type(rest.trim())?;
                return self.check_unary_operation("-", &inner);
            }
        }

        // Check for function calls (e.g., "sin(0.0)")
        if value.contains('(') && value.ends_with(')') {
            // Extract function name and arguments
            if let Some(paren_pos) = value.find('(') {
                let func_name = &value[..paren_pos].trim();
                let args_str = &value[paren_pos + 1..value.len() - 1];
                
                // Try to find function type
                if let Some(ref analyzer) = self.analyzer {
                    if let Ok(analyzer) = analyzer.read() {
                        // Check C function symbols
                        if let Ok(cfc_symbols) = analyzer.get_cfc_symbols() {
                            coffee_debug!("[DEBUG] infer_value_type: found {} C symbol tables", cfc_symbols.len());
                            for (lib_name, symbol_table) in cfc_symbols.iter() {
                                coffee_debug!("[DEBUG] infer_value_type: checking library '{}', {} symbols", lib_name, symbol_table.symbols.len());
                                if let Some(c_symbol) = symbol_table.symbols.get(*func_name) {
                                    coffee_debug!("[DEBUG] infer_value_type: found C symbol '{}' with return type '{}', {} parameters", func_name, c_symbol.return_type, c_symbol.parameters.len());
                                    
                                    // Check argument types
                                    if !args_str.is_empty() {
                                        let args: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
                                        if args.len() != c_symbol.parameters.len() {
                                            coffee_debug!("[DEBUG] infer_value_type: argument count mismatch: expected {}, got {}", c_symbol.parameters.len(), args.len());
                                        }
                                        
                                        for (i, (arg, param)) in args.iter().zip(c_symbol.parameters.iter()).enumerate() {
                                            let expected_type = match self.registry.read().unwrap().resolve_type(&param.param_type) {
                                                Ok(ty) => ty,
                                                Err(_) => Type::unit(),
                                            };
                                            let arg_type = self.infer_value_type(arg)?;
                                            coffee_debug!("[DEBUG] infer_value_type: checking arg {} '{}' vs expected type {:?}", i, arg, expected_type);
                                            coffee_debug!("[DEBUG] infer_value_type: inferred arg type as {:?}", arg_type);
                                            if !self.types_compatible(&arg_type, &expected_type)? {
                                                coffee_debug!("[DEBUG] infer_value_type: argument type mismatch at position {}", i);
                                                let error = TypeSystemError::TypeMismatch {
                                                    expected: expected_type,
                                                    found: arg_type,
                                                    span: Span::new(0, arg.len()),
                                                };
                                                return Err(error);
                                            }
                                        }
                                    }
                                    
                                    // Return the function's return type
                                    return match self.registry.read().unwrap().resolve_type(&c_symbol.return_type) {
                                        Ok(ty) => {
                                            coffee_debug!("[DEBUG] infer_value_type: resolved return type to {:?}", ty);
                                            Ok(ty)
                                        },
                                        Err(e) => {
                                            coffee_debug!("[DEBUG] infer_value_type: failed to resolve return type: {:?}", e);
                                            Ok(Type::unit())
                                        },
                                    };
                                }
                            }
                        }
                    }
                }
                
                // Check if this is an enum variant: Enum::Variant(args)
                if func_name.contains("::") {
                    let parts: Vec<&str> = func_name.split("::").collect();
                    if parts.len() == 2 {
                        let enum_name = parts[0].trim().to_string();
                        let variant_name = parts[1].trim().to_string();
                        
                        // Try to resolve enum type
                        match self.registry.read() {
                            Ok(reg) => {
                                if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(&enum_name) {
                                    // Check if this variant exists
                                    if let Some(variant) = variants.iter().find(|v| v.name == variant_name) {
                                        // Check argument count
                                        let args: Vec<&str> = if args_str.is_empty() {
                                            Vec::new()
                                        } else {
                                            args_str.split(',').map(|s| s.trim()).collect()
                                        };
                                        
                                        if args.len() != variant.fields.len() {
                                            coffee_debug!("[DEBUG] infer_value_type: enum variant argument count mismatch: expected {}, got {}", variant.fields.len(), args.len());
                                            let error = TypeSystemError::arity_mismatch(variant.fields.len(), args.len(), Span::new(0, value.len()));
                                            return Err(error);
                                        }
                                        
                                        // Check argument types
                                        for (i, (arg, field)) in args.iter().zip(variant.fields.iter()).enumerate() {
                                            let expected_type = match field {
                                                crate::types::definition::VariantField::Positional(t) => {
                                                    reg.resolve_type(t).unwrap_or(Type::unit())
                                                }
                                                crate::types::definition::VariantField::Named { ty, .. } => {
                                                    reg.resolve_type(ty).unwrap_or(Type::unit())
                                                }
                                            };
                                            
                                            let arg_type = self.infer_value_type(arg)?;
                                            if !self.types_compatible(&arg_type, &expected_type)? {
                                                let error = TypeSystemError::type_mismatch(expected_type, arg_type, Span::new(0, arg.len()));
                                                return Err(error);
                                            }
                                        }
                                        
                                        // Return enum type
                                        coffee_debug!("[DEBUG] infer_value_type: enum variant '{}' returns type '{}'", func_name, enum_name);
                                        return Ok(Type::NamedType { name: enum_name });
                                    }
                                }
                            },
                            Err(_) => {}
                        }
                    }
                }
                
                // If not found in C symbols, try registry
                match self.registry.read() {
                    Ok(reg) => {
                        if let Ok(Type::Function { return_type, params }) = reg.resolve_type(*func_name) {
                            coffee_debug!("[DEBUG] infer_value_type: found function in registry, return type {:?}", return_type);
                            
                            // Check argument types
                            if !args_str.is_empty() {
                                let args: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
                                if args.len() != params.len() {
                                    coffee_debug!("[DEBUG] infer_value_type: argument count mismatch: expected {}, got {}", params.len(), args.len());
                                }
                                
                                for (i, (arg, expected_type)) in args.iter().zip(params.iter()).enumerate() {
                                    let arg_type = self.infer_value_type(arg)?;
                                    coffee_debug!("[DEBUG] infer_value_type: checking arg {} '{}' vs expected type {:?}", i, arg, expected_type);
                                    coffee_debug!("[DEBUG] infer_value_type: inferred arg type as {:?}", arg_type);
                                    if !self.types_compatible(&arg_type, expected_type)? {
                                        coffee_debug!("[DEBUG] infer_value_type: argument type mismatch at position {}", i);
                                        let error = TypeSystemError::TypeMismatch {
                                            expected: expected_type.clone(),
                                            found: arg_type,
                                            span: Span::new(0, arg.len()),
                                        };
                                        return Err(error);
                                    }
                                }
                            }
                            
                            Ok(*return_type)
                        } else {
                            coffee_debug!("[DEBUG] infer_value_type: function '{}' not found in registry", func_name);
                            Err(TypeSystemError::undefined_function(func_name.to_string(), Span::new(0, value.len())))
                        }
                    },
                    Err(_) => Err(TypeSystemError::ParseError {
                        type_str: value.to_string(),
                        reason: "Failed to access registry".to_string(),
                    }),
                }
            } else {
                Err(TypeSystemError::ParseError {
                    type_str: value.to_string(),
                    reason: "Invalid function call syntax".to_string(),
                })
            }
        }
        // Try to parse as float literal (only if it contains a decimal point)
        else if value.contains('.') {
            if let Ok(val) = value.parse::<f64>() {
                coffee_debug!("[DEBUG] infer_value_type: '{}' parsed as float {}", value, val);
                if self.mode == CheckingMode::Comprehensive {
                    if val < self.bounds.float_min || val > self.bounds.float_max {
                        return Err(TypeSystemError::ParseError {
                            type_str: value.to_string(),
                            reason: format!("Float {} out of bounds [{}, {}]", val, self.bounds.float_min, self.bounds.float_max),
                        });
                    }
                }
                Ok(Type::float())
            } else {
                // Contains '.' but not a valid float, try integer
                if let Ok(val) = value.parse::<i64>() {
                    coffee_debug!("[DEBUG] infer_value_type: '{}' parsed as integer {}", value, val);
                    if self.mode == CheckingMode::Comprehensive {
                        if val < self.bounds.int_min || val > self.bounds.int_max {
                            return Err(TypeSystemError::ParseError {
                                type_str: value.to_string(),
                                reason: format!("Integer {} out of bounds [{}, {}]", val, self.bounds.int_min, self.bounds.int_max),
                            });
                        }
                    }
                    Ok(Type::int())
                } else {
                    Err(TypeSystemError::ParseError {
                        type_str: value.to_string(),
                        reason: format!("Invalid numeric literal: {}", value),
                    })
                }
            }
        }
        // Try to parse as integer literal
        else if let Ok(val) = value.parse::<i64>() {
            coffee_debug!("[DEBUG] infer_value_type: '{}' parsed as integer {}", value, val);
            if self.mode == CheckingMode::Comprehensive {
                if val < self.bounds.int_min || val > self.bounds.int_max {
                    return Err(TypeSystemError::ParseError {
                        type_str: value.to_string(),
                        reason: format!("Integer {} out of bounds [{}, {}]", val, self.bounds.int_min, self.bounds.int_max),
                    });
                }
            }
            Ok(Type::int())
        }
        // Boolean literals
        else if value == "true" || value == "false" {
            Ok(Type::bool())
        }
        // String literals
        else if value.starts_with('"') && value.ends_with('"') {
            Ok(Type::string())
        }
        // Enum variant: Enum::Variant or Enum::Variant(args)
        else if value.contains("::") {
            let parts: Vec<&str> = value.split("::").collect();
            if parts.len() == 2 {
                let enum_name = parts[0].trim();
                let variant_part = parts[1].trim();
                
                // Check if this is a variant with parameters
                let variant_name = if variant_part.contains('(') {
                    let paren_pos = variant_part.find('(').unwrap();
                    &variant_part[..paren_pos]
                } else {
                    variant_part
                };
                
                // Try to resolve enum type
                match self.registry.read() {
                    Ok(reg) => {
                        if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(enum_name) {
                            // Check if this variant has fields
                            if let Some(variant) = variants.iter().find(|v| v.name == variant_name) {
                                if variant.fields.is_empty() {
                                    // Unit variant - return enum type
                                    Ok(Type::NamedType { name: enum_name.to_string() })
                                } else {
                                    // Variant with fields - return a struct type
                                    // For simplicity, we'll treat it as a tuple of field types
                                    let field_types: Vec<Type> = variant.fields.iter().map(|f| {
                                        match f {
                                            crate::types::definition::VariantField::Positional(t) => {
                                                reg.resolve_type(t).unwrap_or(Type::unit())
                                            }
                                            crate::types::definition::VariantField::Named { ty, .. } => {
                                                reg.resolve_type(ty).unwrap_or(Type::unit())
                                            }
                                        }
                                    }).collect();
                                    Ok(Type::Tuple(field_types))
                                }
                            } else {
                                // Variant not found
                                Err(TypeSystemError::undefined_variable(value.to_string(), Span::new(0, value.len())))
                            }
                        } else {
                            // Enum not found
                            Err(TypeSystemError::undefined_variable(value.to_string(), Span::new(0, value.len())))
                        }
                    },
                    Err(_) => Err(TypeSystemError::ParseError {
                        type_str: value.to_string(),
                        reason: "Failed to access registry".to_string(),
                    }),
                }
            } else {
                // Invalid enum syntax
                Err(TypeSystemError::ParseError {
                    type_str: value.to_string(),
                    reason: "Invalid enum syntax".to_string(),
                })
            }
        }
        // Try to resolve as variable name
        else if self.mode == CheckingMode::Comprehensive {
            if let Some(info) = self.values.get(value) {
                Ok(info.ty.clone())
            } else {
                // Try to resolve as type from registry
                match self.registry.read() {
                    Ok(reg) => reg.resolve_type(value).map_err(|_| TypeSystemError::undefined_variable(value.to_string(), Span::new(0, value.len()))),
                    Err(_) => Err(TypeSystemError::ParseError {
                        type_str: value.to_string(),
                        reason: "Failed to access registry".to_string(),
                    }),
                }
            }
        }
        // Try to resolve as type from registry
        else {
            match self.registry.read() {
                Ok(reg) => reg.resolve_type(value).map_err(|_| TypeSystemError::undefined_variable(value.to_string(), Span::new(0, value.len()))),
                Err(_) => Err(TypeSystemError::ParseError {
                    type_str: value.to_string(),
                    reason: "Failed to access registry".to_string(),
                }),
            }
        }
    }

    /// Check if types are compatible
    fn types_compatible(&self, ty1: &Type, ty2: &Type) -> Result<bool, TypeSystemError> {
        coffee_debug!("[DEBUG] types_compatible: comparing {:?} vs {:?}", ty1, ty2);
        if ty1 == ty2 {
            coffee_debug!("[DEBUG] types_compatible: types are equal, returning true");
            return Ok(true);
        }

        // Check coercion
        if ty1.can_coerce_from(ty2) || ty2.can_coerce_from(ty1) {
            coffee_debug!("[DEBUG] types_compatible: types can be coerced, returning true");
            return Ok(true);
        }

        // In comprehensive mode, check registry for inheritance
        if self.mode == CheckingMode::Comprehensive {
            match self.registry.read() {
                Ok(reg) => {
                    let is_compatible = reg.is_compatible(ty1, ty2);
                    coffee_debug!("[DEBUG] types_compatible: registry says {} (comprehensive mode)", is_compatible);
                    Ok(is_compatible)
                },
                Err(_) => {
                    coffee_debug!("[DEBUG] types_compatible: failed to read registry, returning false");
                    Ok(false)
                },
            }
        } else {
            coffee_debug!("[DEBUG] types_compatible: not comprehensive mode, returning false");
            Ok(false)
        }
    }

    /// Promote numeric types for arithmetic operations
    fn promote_numeric_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Float { .. }, _) | (_, Type::Float { .. }) => Type::float(),
            (Type::Int { bits: b1, signed: s1 }, Type::Int { bits: b2, signed: s2 }) => {
                let bits = (*b1).max(*b2);
                let signed = *s1 || *s2;
                Type::Int { bits, signed }
            }
            _ => left.clone(),
        }
    }

    /// Check value bounds (comprehensive mode only)
    fn check_value_bounds(&self, ty: &Type, value: &str, location: Span) -> Result<(), TypeSystemError> {
        match ty {
            Type::Int { .. } => {
                if let Ok(val) = value.parse::<i64>() {
                    if val < self.bounds.int_min || val > self.bounds.int_max {
                        return Err(TypeSystemError::ConstraintViolation {
                            constraint: "integer bounds".to_string(),
                            reason: format!("Value {} exceeds bounds [{}, {}]", val, self.bounds.int_min, self.bounds.int_max),
                            span: location,
                        });
                    }
                }
            }
            Type::Float { .. } => {
                if let Ok(val) = value.parse::<f64>() {
                    if val < self.bounds.float_min || val > self.bounds.float_max {
                        return Err(TypeSystemError::ConstraintViolation {
                            constraint: "float bounds".to_string(),
                            reason: format!("Value {} exceeds bounds [{}, {}]", val, self.bounds.float_min, self.bounds.float_max),
                            span: location,
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Check memory operation (comprehensive mode only)
    ///
    /// Coffee language memory operations:
    /// - clone x y: Deep copy - creates independent copy with new lifetime
    /// - copy x y: Shallow copy - creates shared reference to same data
    /// - mv x y: Move - transfers ownership (x becomes invalid, y takes over)
    /// - rm x: Remove - deletes variable and frees resources
    pub fn check_memory_operation(&mut self, var_name: &str, operation: &str) -> Result<(), TypeSystemError> {
        if self.mode != CheckingMode::Comprehensive {
            return Ok(());
        }

        let location = Span::new(0, var_name.len());

        match operation {
            "move" | "mv" => {
                // Move operation: source becomes invalid
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot move '{}' - value already moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot move '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            value_info.state = ValueState::Moved;
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "clone" => {
                // Clone operation: source remains valid, creates deep copy
                // Only check that source exists and is accessible
                if let Some(value_info) = self.values.get(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot clone '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot clone '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            // Clone is always safe - source remains valid
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "copy" => {
                // Copy operation: creates shared reference (shallow copy)
                // Source remains valid, both variables point to same data
                if let Some(value_info) = self.values.get(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot copy '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot copy '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            // Copy is safe - creates shared reference
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "remove" | "rm" => {
                // Remove operation: frees the variable
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot remove '{}' - value was already moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { .. } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot remove '{}' - value was already dropped", var_name),
                                span: location,
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            value_info.state = ValueState::Dropped {
                                location: "memory operation".to_string(),
                            };
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "remove_multiple" => {
                // Remove multiple operation: frees multiple variables
                // This is handled by the backend, we just need to check that the first variable exists
                // The actual removal will be done by the backend
                if !var_name.is_empty() {
                    if let Some(value_info) = self.values.get_mut(var_name) {
                        match value_info.state {
                            ValueState::Moved => {
                                return Err(TypeSystemError::OwnershipError {
                                    reason: format!("Cannot remove '{}' - value was already moved", var_name),
                                    span: location,
                                });
                            }
                            ValueState::Dropped { .. } => {
                                return Err(TypeSystemError::OwnershipError {
                                    reason: format!("Cannot remove '{}' - value was already dropped", var_name),
                                    span: location,
                                });
                            }
                            ValueState::Alive | ValueState::Borrowed { .. } => {
                                value_info.state = ValueState::Dropped {
                                    location: "memory operation".to_string(),
                                };
                            }
                        }
                    } else {
                        return Err(TypeSystemError::undefined_variable(var_name, location));
                    }
                }
            }

            "borrow" => {
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot borrow '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot borrow '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive => {
                            value_info.state = ValueState::Borrowed { immutable: true };
                        }
                        ValueState::Borrowed { .. } => {
                            // Already borrowed
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "borrow_mut" => {
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot mutably borrow '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot mutably borrow '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive => {
                            value_info.state = ValueState::Borrowed { immutable: false };
                        }
                        ValueState::Borrowed { .. } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot mutably borrow '{}' - already borrowed", var_name),
                                span: location,
                            });
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            _ => {
                return Err(TypeSystemError::OwnershipError {
                    reason: format!("Unknown memory operation: '{}'", operation),
                    span: location,
                });
            }
        }

        Ok(())
    }

    /// Check import statement (comprehensive mode only)
    pub fn check_import(&mut self, module: &str, source_file: &str) -> Result<(), TypeSystemError> {
        if self.mode != CheckingMode::Comprehensive {
            return Ok(());
        }

        let import_info = ImportInfo {
            module: module.to_string(),
            source_file: source_file.to_string(),
            location: Span::new(0, module.len()),
        };

        if let Ok(mut stack) = self.import_stack.write() {
            stack.push(import_info)?;
        }

        // Add to imported symbols
        self.imported_symbols.entry(source_file.to_string())
            .or_insert_with(Vec::new)
            .push(module.to_string());

        Ok(())
    }

    /// Check main entry point arguments
    pub fn check_main_entry(&mut self, main_entry: &parser::main::MainEntry) -> Result<(), TypeSystemError> {
        coffee_debug!("[DEBUG] check_main_entry: checking main entry '{}'", main_entry.entry_function);
        
        // Find the function type
        let function_type = {
            let registry_result = self.registry.read();
            match registry_result {
                Ok(reg) => {
                    coffee_debug!("[DEBUG] check_main_entry: accessing registry");
                    if let Ok(func_type) = reg.resolve_type(&main_entry.entry_function) {
                        coffee_debug!("[DEBUG] check_main_entry: found function '{}' in registry", main_entry.entry_function);
                        Ok(func_type)
                    } else {
                        coffee_debug!("[DEBUG] check_main_entry: function '{}' not found in registry", main_entry.entry_function);
                        Err("not_found")
                    }
                }
                Err(_) => {
                    coffee_debug!("[DEBUG] check_main_entry: failed to access registry");
                    Err("registry_error")
                }
            }
        };

        let param_types = match function_type {
            Ok(Type::Function { params, return_type: _ }) => {
                coffee_debug!("[DEBUG] check_main_entry: function has {} parameters", params.len());
                Ok(params)
            },
            Ok(_) => {
                coffee_debug!("[DEBUG] check_main_entry: '{}' is not a function", main_entry.entry_function);
                let error = TypeSystemError::ParseError {
                    type_str: main_entry.entry_function.clone(),
                    reason: format!("'{}' is not a function", main_entry.entry_function),
                };
                self.add_error(error.clone());
                return Err(error);
            },
            Err("not_found") => {
                // Try to find function in semantic analyzer's C symbols
                coffee_debug!("[DEBUG] check_main_entry: trying to find '{}' in C symbols", main_entry.entry_function);
                let mut c_param_types = None;
                if let Some(ref analyzer) = self.analyzer {
                    if let Ok(analyzer) = analyzer.read() {
                        if let Ok(cfc_symbols) = analyzer.get_cfc_symbols() {
                            coffee_debug!("[DEBUG] check_main_entry: found {} C symbol tables", cfc_symbols.len());
                            for (lib_name, symbol_table) in cfc_symbols.iter() {
                                coffee_debug!("[DEBUG] check_main_entry: checking library '{}', {} symbols", lib_name, symbol_table.symbols.len());
                                if let Some(c_symbol) = symbol_table.symbols.get(&main_entry.entry_function) {
                                    let mut param_types = Vec::new();
                                    for param in &c_symbol.parameters {
                                        if let Ok(param_type) = self.registry.read().unwrap().resolve_type(&param.param_type) {
                                            param_types.push(param_type);
                                        }
                                    }
                                    let return_type = match self.registry.read().unwrap().resolve_type(&c_symbol.return_type) {
                                        Ok(ty) => ty,
                                        Err(_) => Type::unit(),
                                    };
                                    coffee_debug!("[DEBUG] check_main_entry: found C function '{}' with {} parameters, return type {:?}", main_entry.entry_function, param_types.len(), return_type);
                                    c_param_types = Some(param_types);
                                    break;
                                }
                            }
                        }
                    }
                }
                
                match c_param_types {
                    Some(param_types) => Ok(param_types),
                    None => {
                        coffee_debug!("[DEBUG] check_main_entry: function '{}' not found in C symbols", main_entry.entry_function);
                        let error = TypeSystemError::undefined_function(&main_entry.entry_function, Span::new(0, main_entry.entry_function.len()));
                        self.add_error(error.clone());
                        Err(error)
                    }
                }
            },
            Err(_) => {
                let error = TypeSystemError::ParseError {
                    type_str: main_entry.entry_function.clone(),
                    reason: "Failed to access registry".to_string(),
                };
                self.add_error(error.clone());
                Err(error)
            }
        };

        match param_types {
            Ok(param_types) => {
                coffee_debug!("[DEBUG] check_main_entry: found function with {} parameters", param_types.len());
                self.check_main_args(&main_entry.args, &param_types)
            },
            Err(error) => Err(error)
        }
    }

    /// Check main entry point arguments against parameter types
    fn check_main_args(&mut self, args: &[String], param_types: &[Type]) -> Result<(), TypeSystemError> {
        coffee_debug!("[DEBUG] check_main_args: checking {} args against {} param_types", args.len(), param_types.len());
        if args.len() != param_types.len() {
            let error = TypeSystemError::arity_mismatch(param_types.len(), args.len(), Span::new(0, 0));
            self.add_error(error.clone());
            return Err(error);
        }

        for (i, (arg, expected_type)) in args.iter().zip(param_types.iter()).enumerate() {
            coffee_debug!("[DEBUG] check_main_args: checking arg {} '{}' vs expected type {:?}", i, arg, expected_type);
            let arg_type = self.infer_value_type(arg)?;
            coffee_debug!("[DEBUG] check_main_args: inferred arg type as {:?}", arg_type);
            if !self.types_compatible(&arg_type, expected_type)? {
                coffee_debug!("[DEBUG] check_main_args: argument type mismatch at position {}", i);
                let error = TypeSystemError::TypeMismatch {
                    expected: expected_type.clone(),
                    found: arg_type,
                    span: Span::new(0, arg.len()),
                };
                self.add_error(error.clone());
                return Err(error);
            }
        }

        coffee_debug!("[DEBUG] check_main_args: all arguments checked successfully");
        Ok(())
    }

    /// Get diagnostics for all errors
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.errors.iter().map(|error| Diagnostic::from_error(error)).collect()
    }

    /// Clear all tracking state
    pub fn reset(&mut self) {
        self.errors.clear();
        self.values.clear();
        self.imported_symbols.clear();
        self.current_return_type = None;
    }

    /// Report a diagnostic error (for compatibility)
    pub fn report(&mut self, error: crate::diagnostics::Diagnostic) {
        // Convert diagnostic to TypeSystemError and add to errors
        self.errors.push(crate::types::TypeSystemError::from(error));
    }

    /// Check memory operation (alias for check_memory_operation for compatibility)
    pub fn check_memory_op(&mut self, op: &crate::parser::memory::MemoryOp) -> Result<(), crate::diagnostics::Diagnostic> {
        let (operation, var_name) = match op {
            crate::parser::memory::MemoryOp::Clone { source, target: _ } => ("clone", source.as_str()),
            crate::parser::memory::MemoryOp::Copy { source, target: _ } => ("copy", source.as_str()),
            crate::parser::memory::MemoryOp::Move { source, target: _ } => ("move", source.as_str()),
            crate::parser::memory::MemoryOp::Remove { target } => ("remove", target.as_str()),
            crate::parser::memory::MemoryOp::RemoveMultiple { targets } => {
                if !targets.is_empty() {
                    ("remove_multiple", targets[0].as_str())
                } else {
                    ("remove_multiple", "")
                }
            }
            crate::parser::memory::MemoryOp::CleanOut { targets, except_mode: _ } => {
                if let Some(vars) = targets {
                    if !vars.is_empty() {
                        ("clean_out", vars[0].as_str())
                    } else {
                        ("clean_out", "")
                    }
                } else {
                    ("clean_out", "")
                }
            }
        };

        self.check_memory_operation(var_name, operation)
            .map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_type_checker() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let checker = TypeChecker::simple(registry);

        // Test simple type inference
        assert_eq!(checker.infer_value_type("123").unwrap(), Type::int());
        assert_eq!(checker.infer_value_type("3.14").unwrap(), Type::float());
        assert_eq!(checker.infer_value_type("true").unwrap(), Type::bool());
        assert_eq!(checker.infer_value_type("\"hello\"").unwrap(), Type::string());
    }

    #[test]
    fn test_comprehensive_type_checker() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);

        // Test ownership tracking
        let decl = parser::var::VariableDecl {
            name: "x".to_string(),
            var_type: "int".to_string(),
            value: "42".to_string(),
        };

        checker.check_variable_decl(&decl).unwrap();
        assert!(checker.values.contains_key("x"));

        // Test memory operation
        checker.check_memory_operation("x", "move").unwrap();
        assert_eq!(checker.values.get("x").unwrap().state, ValueState::Moved);
    }

    #[test]
    fn test_unary_not_and_neg() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let not_true = parser::expr::Expression::Unary {
            op: "!".to_string(),
            operand: Box::new(parser::expr::Expression::Literal("true".to_string())),
        };
        assert_eq!(checker.check_expression(&not_true).unwrap(), Type::bool());

        let neg = parser::expr::Expression::Unary {
            op: "-".to_string(),
            operand: Box::new(parser::expr::Expression::Literal("10".to_string())),
        };
        assert_eq!(checker.check_expression(&neg).unwrap(), Type::int());

        let bad = parser::expr::Expression::Unary {
            op: "!".to_string(),
            operand: Box::new(parser::expr::Expression::Literal("1".to_string())),
        };
        assert!(checker.check_expression(&bad).is_err());
    }
}