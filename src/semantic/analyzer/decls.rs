use crate::coffee_debug;
use super::SemanticAnalyzer;
use crate::types::definition::*;
use crate::types::errors::TypeSystemError;

impl SemanticAnalyzer {
    /// 分析变量声明
    pub fn analyze_variable_decl(&mut self, decl: &crate::parser::var::VariableDecl) -> Result<(), TypeSystemError> {
        let span = Span::new(0, decl.name.len());

        // 解析类型
        let ty = match self.type_registry.read() {
            Ok(reg) => reg.resolve_type(&decl.var_type)?,
            Err(_) => return Err(TypeSystemError::ParseError {
                type_str: decl.var_type.clone(),
                reason: "Failed to access type registry".to_string(),
            }),
        };

        // 创建实体
        let entity = Entity::Variable {
            ty,
            initialized: true,
        };

        // 在作用域中绑定
        // 不在作用域空间中绑定变量，只在符号空间中声明
        // 这样可以避免不同函数中的变量冲突
        // if let Ok(scope) = self.scope_space.write() {
        //     scope.bind(decl.name.clone(), entity.clone(), span)?;
        // }

        // 在符号空间中声明
        // 如果在函数内，使用限定名（function::variable）以避免冲突
        {
            let symbol_name = if let Some(ref func_name) = self.current_function_name {
                format!("{}::{}", func_name, decl.name)
            } else {
                decl.name.clone()
            };

            if let Ok(mut symbols) = self.symbol_space.write() {
                let binding = Binding {
                    name: symbol_name.clone(),
                    entity,
                    span,
                    mutable: false,
                    visibility: Visibility::Private,
                };
                symbols.declare(symbol_name, binding)?;
            }
        }

        // 分析初始化值的表达式（用于检查未定义符号）
        self.analyze_expression(&decl.value)?;

        Ok(())
    }

    /// 声明函数（不分析函数体）
    /// 这个函数只声明函数签名，不分析函数体
    /// 用于支持互递归函数
    pub fn declare_function_decl(&mut self, func: &crate::parser::function::Function) -> Result<(), TypeSystemError> {
        let span = Span::new(0, func.name.len());

        // 检查函数是否已经被声明
        let function_already_declared = if let Ok(symbols) = self.symbol_space.read() {
            symbols.lookup(&func.name).is_some()
        } else {
            false
        };

        if function_already_declared {
            coffee_debug!("DEBUG: declare_function_decl: function '{}' already declared, skipping", func.name);
            return Ok(());
        }

        // 获取类型注册器读锁一次（避免在循环中重复获取）
        let type_registry = self.type_registry.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: func.name.clone(),
                reason: "Failed to access type registry".to_string(),
            })?;

        // 解析参数类型
        let mut param_types = Vec::new();
        for (_idx, param) in func.parameters.iter().enumerate() {
            let ty = type_registry.resolve_type(&param.param_type)?;
            param_types.push(ty);
        }

        // 解析返回类型
        let return_type = type_registry.resolve_type(&func.return_type)?;

        // 释放锁后再进行后续操作
        drop(type_registry);

        // 创建函数实体
        let entity = Entity::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
            generics: vec![],
        };

        // 在作用域中绑定
        if let Ok(scope) = self.scope_space.write() {
            scope.bind(func.name.clone(), entity.clone(), span)?;
        }

        // 在符号空间中声明
        if let Ok(mut symbols) = self.symbol_space.write() {
            let binding = Binding {
                name: func.name.clone(),
                entity,
                span,
                mutable: false,
                visibility: Visibility::Public,
            };
            symbols.declare(func.name.clone(), binding)?;
        }

        // 在类型注册器中注册函数类型
        let func_type = Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        };

        if let Ok(registry) = self.type_registry.write() {
            registry.define_alias(func.name.clone(), func_type)?;
        }

        coffee_debug!("DEBUG: declare_function_decl: declared function '{}'", func.name);
        Ok(())
    }

    /// 分析函数声明
    pub fn analyze_function_decl(&mut self, func: &crate::parser::function::Function) -> Result<(), TypeSystemError> {
        let span = Span::new(0, func.name.len());

        // 获取类型注册器读锁一次（避免在循环中重复获取）
        let type_registry = self.type_registry.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: func.name.clone(),
                reason: "Failed to access type registry".to_string(),
            })?;

        // 解析参数类型
        let mut param_types = Vec::new();
        for (_idx, param) in func.parameters.iter().enumerate() {
            let ty = type_registry.resolve_type(&param.param_type)?;
            param_types.push(ty);
        }

        // 解析返回类型
        let return_type = type_registry.resolve_type(&func.return_type)?;

        // 释放锁后再进行后续操作
        drop(type_registry);

        // 创建函数实体
        let entity = Entity::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
            generics: vec![],
        };

        // 在类型注册器中注册函数类型
        let func_type = Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        };

        // 检查函数是否已经被声明（在 declare_function_decl 中）
        // 如果已经声明，跳过声明步骤，直接进入函数体分析
        let function_already_declared = if let Ok(symbols) = self.symbol_space.read() {
            symbols.lookup(&func.name).is_some()
        } else {
            false
        };

        coffee_debug!("DEBUG: analyze_function_decl: function_already_declared={} for function '{}'", function_already_declared, func.name);

        if !function_already_declared {
            // 在作用域中绑定
            if let Ok(scope) = self.scope_space.write() {
                scope.bind(func.name.clone(), entity.clone(), span)?;
            }

            // 在符号空间中声明
            if let Ok(mut symbols) = self.symbol_space.write() {
                let binding = Binding {
                    name: func.name.clone(),
                    entity,
                    span,
                    mutable: false,
                    visibility: Visibility::Public,
                };
                symbols.declare(func.name.clone(), binding)?;
            }

            if let Ok(registry) = self.type_registry.write() {
                registry.define_alias(func.name.clone(), func_type.clone())?;
            }
        }

        // 设置当前函数名称，以便在分析函数体内的表达式时使用限定名查找
        self.current_function_name = Some(func.name.clone());

        // 在函数作用域中绑定参数
        // 注意：这里需要重新获取 type_registry，因为之前已经 drop 了
        let type_registry = self.type_registry.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: func.name.clone(),
                reason: "Failed to access type registry".to_string(),
            })?;

        coffee_debug!("DEBUG: analyze_function_decl: binding parameters for function '{}', params: {:?}", func.name, func.parameters.iter().map(|p| &p.name).collect::<Vec<_>>());

        // 在符号空间中绑定参数，使用限定名（function::parameter）
        if let Ok(mut symbols) = self.symbol_space.write() {
            for param in &func.parameters {
                let param_type = type_registry.resolve_type(&param.param_type)?;
                let param_entity = Entity::Variable {
                    ty: param_type.clone(),
                    initialized: true,
                };
                let qualified_name = format!("{}::{}", func.name, param.name);
                coffee_debug!("DEBUG: analyze_function_decl: binding parameter '{}' as '{}' in function '{}'", param.name, qualified_name, func.name);
                let binding = Binding {
                    name: qualified_name.clone(),
                    entity: param_entity,
                    span,
                    mutable: false,
                    visibility: Visibility::Private,
                };
                symbols.declare(qualified_name, binding)?;
            }
        }

        drop(type_registry);

        // Note: func_type is already registered in declare_function_decl (first pass)
        // We don't need to register it again here
        // Borrow checking is `types::borrow`; do not register function names as LifetimeParam.

        // Create a child scope for the function
        let function_scope = self.enter_scope(&func.name);

        // Add function parameters to the function scope
        for param in &func.parameters {
            let param_span = Span::new(0, param.name.len());
            let param_type = match self.type_registry.read() {
                Ok(reg) => reg.resolve_type(&param.param_type)?,
                Err(_) => return Err(TypeSystemError::ParseError {
                    type_str: param.param_type.clone(),
                    reason: "Failed to access type registry".to_string(),
                }),
            };

            let param_entity = Entity::Variable {
                ty: param_type,
                initialized: true,
            };

            // Add to function scope (not global scope)
            if let Ok(scope) = function_scope.write() {
                scope.bind(param.name.clone(), param_entity.clone(), param_span)?;
            }

            // Note: Parameters are already bound to symbol_space with qualified names (function::parameter)
            // earlier in this function (lines 3653-3670), so we don't need to bind them again here
        }

        // Analyze function body statements to check for undefined symbols
        // Set current function return type and name for void return validation
        let old_return_type = self.current_function_return_type.clone();
        let old_function_name = self.current_function_name.clone();
        self.current_function_return_type = Some(func.return_type.clone());
        self.current_function_name = Some(func.name.clone());

        if let crate::parser::function::FunctionBody::Block(statements) = &func.body {
            for stmt in statements {
                self.analyze_statement(stmt)?;
            }
        }

        // Restore previous return type and function name
        self.current_function_return_type = old_return_type;
        self.current_function_name = old_function_name;

        // Clean up function-local variables from symbol space
        // Remove variables with qualified names (function::variable)
        if let Ok(mut symbols) = self.symbol_space.write() {
            let qualified_prefix = format!("{}::", func.name);
            let keys_to_remove: Vec<String> = symbols.declared_symbols()
                .iter()
                .filter(|name| name.starts_with(&qualified_prefix))
                .cloned()
                .collect();
            
            for key in keys_to_remove {
                let _ = symbols.remove(&key);
            }
        }

        Ok(())
    }

    /// 分析类型定义
    pub fn analyze_type_def(&mut self, name: &str, type_def: TypeDef) -> Result<(), TypeSystemError> {
        let span = Span::new(0, name.len());

        // 在类型注册器中注册
        if let Ok(registry) = self.type_registry.write() {
            registry.define_type(name, type_def.clone())?;
        }

        // 创建类型实体
        let entity = Entity::TypeDef { def: type_def };

        // 在符号空间中声明
        if let Ok(mut symbols) = self.symbol_space.write() {
            let binding = Binding {
                name: name.to_string(),
                entity,
                span,
                mutable: false,
                visibility: Visibility::Public,
            };
            symbols.declare(name.to_string(), binding)?;
        }

        Ok(())
    }

    /// 分析符号引用
    pub fn analyze_symbol_reference(&mut self, name: &str, span: Span) -> Result<Entity, TypeSystemError> {
        // 在符号空间中记录引用
        if let Ok(mut symbols) = self.symbol_space.write() {
            symbols.reference(name.to_string(), span);

            // 查找符号声明
            if let Some(binding) = symbols.lookup(name) {
                return Ok(binding.entity.clone());
            }
        }

        // 如果符号空间中没有，尝试作用域空间
        if let Ok(scope) = self.scope_space.read() {
            if let Some(binding) = scope.get(name) {
                return Ok(binding.entity);
            }
        }

        Err(TypeSystemError::NotFound {
            name: name.to_string(),
            kind: SpaceKind::Symbol,
        })
    }
}
