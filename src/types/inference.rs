use super::definition::*;
use super::registry::TypeRegistry;
use super::errors::TypeSystemError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 类型推导器，合并原 ty::TypeInferencer 和增强功能
pub struct TypeInferencer {
    /// Type registry
    registry: Arc<RwLock<TypeRegistry>>,
    /// Type variables mapping
    type_vars: HashMap<String, Type>,
    /// Next type variable ID
    next_var_id: usize,
    /// Generic parameter constraints
    constraints: HashMap<String, Vec<TypeConstraint>>,
    /// Inference context (for nested expressions)
    context_stack: Vec<InferenceContext>,
}

/// 推导上下文（用于嵌套表达式和作用域）
#[derive(Debug, Clone)]
pub struct InferenceContext {
    /// Context name
    pub name: String,
    /// Local variable types in this context
    pub locals: HashMap<String, Type>,
    /// Expected return type (for functions)
    pub expected_return: Option<Type>,
    /// Generic parameters in scope
    pub generics: Vec<String>,
}

impl InferenceContext {
    pub fn new(name: impl Into<String>) -> Self {
        InferenceContext {
            name: name.into(),
            locals: HashMap::new(),
            expected_return: None,
            generics: vec![],
        }
    }

    pub fn with_generics(name: impl Into<String>, generics: Vec<String>) -> Self {
        InferenceContext {
            name: name.into(),
            locals: HashMap::new(),
            expected_return: None,
            generics,
        }
    }
}

impl TypeInferencer {
    /// 创建新的类型推导器
    pub fn new(registry: Arc<RwLock<TypeRegistry>>) -> Self {
        TypeInferencer {
            registry,
            type_vars: HashMap::new(),
            next_var_id: 0,
            constraints: HashMap::new(),
            context_stack: vec![InferenceContext::new("global")],
        }
    }

    /// 进入新的推导上下文
    pub fn enter_context(&mut self, context: InferenceContext) {
        self.context_stack.push(context);
    }

    /// 退出当前推导上下文
    pub fn exit_context(&mut self) -> Option<InferenceContext> {
        if self.context_stack.len() > 1 {
            self.context_stack.pop()
        } else {
            None
        }
    }

    /// 获取当前上下文
    fn current_context(&self) -> &InferenceContext {
        self.context_stack.last().expect("Context stack should never be empty")
    }

    /// 获取当前上下文（可变）
    fn current_context_mut(&mut self) -> &mut InferenceContext {
        self.context_stack.last_mut().expect("Context stack should never be empty")
    }

    /// Infer a type from an expression string.
    ///
    /// Tries [`crate::parser::expr::Expression::parse`] first, then falls back to
    /// string heuristics. This helper is **not** the main type checker
    /// (`crate::types::checker`); it exists for lightweight inference only.
    pub fn infer_expression(&mut self, expr: &str) -> Result<Type, TypeSystemError> {
        let expr = expr.trim();

        if let Ok(ast) = crate::parser::expr::Expression::parse(expr) {
            if let Ok(ty) = self.infer_parsed_expression(&ast) {
                return Ok(ty);
            }
        }

        self.infer_expression_str(expr)
    }

    fn infer_parsed_expression(&mut self, expr: &crate::parser::expr::Expression) -> Result<Type, TypeSystemError> {
        use crate::parser::expr::Expression;
        match expr {
            Expression::Literal(value) => {
                self.infer_literal(value).ok_or_else(|| TypeSystemError::ParseError {
                    type_str: value.clone(),
                    reason: "Unable to infer literal type".to_string(),
                })
            }
            Expression::Variable(name) => {
                if let Some(ty) = self.lookup_variable(name) {
                    return Ok(ty);
                }
                if let Ok(registry) = self.registry.read() {
                    if let Ok(ty) = registry.resolve_type(name) {
                        return Ok(ty);
                    }
                }
                Ok(Type::NamedType { name: name.clone() })
            }
            Expression::FString { .. } => Ok(Type::String),
            Expression::Binary { left, op, right } => {
                let left_type = self.infer_parsed_expression(left)?;
                let right_type = self.infer_parsed_expression(right)?;
                self.infer_binary_op(op, left_type, right_type)
            }
            Expression::Unary { operand, .. } => self.infer_parsed_expression(operand),
            Expression::Call { function, args } => {
                let func_name = match function.as_ref() {
                    Expression::Variable(name) => name.clone(),
                    _ => {
                        return Err(TypeSystemError::ParseError {
                            type_str: format!("{:?}", function),
                            reason: "Call target is not an identifier".to_string(),
                        });
                    }
                };
                let mut arg_types = Vec::new();
                for arg in args {
                    arg_types.push(self.infer_parsed_expression(arg)?);
                }
                if let Ok(registry) = self.registry.read() {
                    if let Ok(Type::Function { params, return_type }) = registry.resolve_type(&func_name) {
                        if params.len() != arg_types.len() {
                            return Err(TypeSystemError::ArityMismatch {
                                expected: params.len(),
                                found: arg_types.len(),
                                span: Span::new(0, 0),
                            });
                        }
                        return Ok(*return_type);
                    }
                }
                self.infer_generic_instantiation(&func_name, arg_types)
            }
            Expression::ConstructorCall { class_name, .. } => Ok(Type::NamedType {
                name: class_name.clone(),
            }),
            Expression::Index { array, index } => {
                let container_type = self.infer_parsed_expression(array)?;
                let index_type = self.infer_parsed_expression(index)?;
                match container_type {
                    Type::Array { elem, .. } | Type::Slice(elem) if index_type.is_int() => Ok(*elem),
                    other => Err(TypeSystemError::InvalidOperation {
                        op: "[]".to_string(),
                        left: other,
                        right: index_type,
                        span: Span::new(0, 0),
                    }),
                }
            }
            Expression::Member { object, field, .. } => {
                let object_type = self.infer_parsed_expression(object)?;
                match object_type {
                    Type::NamedType { name, .. } => {
                        if let Ok(registry) = self.registry.read() {
                            for f in registry.get_fields(&name) {
                                if f.name == *field {
                                    return registry.resolve_type(&f.ty).map_err(|_| TypeSystemError::ParseError {
                                        type_str: f.ty.clone(),
                                        reason: "Failed to resolve field type".to_string(),
                                    });
                                }
                            }
                            return Err(TypeSystemError::field_not_found(&name, field, Span::new(0, 0)));
                        }
                    }
                    _ => {}
                }
                Err(TypeSystemError::ParseError {
                    type_str: field.clone(),
                    reason: "Invalid field access".to_string(),
                })
            }
            Expression::ArrayLiteral { elements } => {
                let elem = if let Some(first) = elements.first() {
                    self.infer_parsed_expression(first)?
                } else {
                    Type::int()
                };
                Ok(Type::Array {
                    elem: Box::new(elem),
                    size: elements.len(),
                })
            }
            Expression::TypeCast { target_type, .. } => {
                if let Ok(registry) = self.registry.read() {
                    registry.resolve_type(target_type)
                } else {
                    Err(TypeSystemError::ParseError {
                        type_str: target_type.clone(),
                        reason: "Failed to access type registry".to_string(),
                    })
                }
            }
            Expression::TupleLiteral { .. }
            | Expression::StructLiteral { .. }
            | Expression::Assign { .. } => Err(TypeSystemError::ParseError {
                type_str: format!("{:?}", expr),
                reason: "Fall back to string inference".to_string(),
            }),
        }
    }

    fn infer_expression_str(&mut self, expr: &str) -> Result<Type, TypeSystemError> {

        // 字面量推导
        if let Some(ty) = self.infer_literal(expr) {
            return Ok(ty);
        }

        // 二元表达式推导
        if let Ok(ty) = self.infer_binary_expression(expr) {
            return Ok(ty);
        }

        // 函数调用推导
        if expr.contains('(') && expr.ends_with(')') {
            return self.infer_function_call(expr);
        }

        // 数组/切片访问
        if expr.contains('[') && expr.ends_with(']') {
            return self.infer_indexing(expr);
        }

        // 字段访问
        if expr.contains('.') {
            return self.infer_field_access(expr);
        }

        // 变量引用
        if let Some(ty) = self.lookup_variable(expr) {
            return Ok(ty);
        }

        // 未知标识符
        if self.is_valid_identifier(expr) {
            // 尝试从注册器解析类型
            if let Ok(registry) = self.registry.read() {
                if let Ok(ty) = registry.resolve_type(expr) {
                    return Ok(ty);
                }
            }

            // 作为命名类型返回
            return Ok(Type::NamedType {
                name: expr.to_string(),
            });
        }

        Err(TypeSystemError::ParseError {
            type_str: expr.to_string(),
            reason: "Unable to infer type".to_string(),
        })
    }

    /// 推导字面量类型
    fn infer_literal(&self, expr: &str) -> Option<Type> {
        // 浮点字面量 - 先检查，确保 0.0 被识别为浮点数
        if expr.contains('.') {
            if let Ok(_) = expr.parse::<f64>() {
                return Some(Type::f64());
            }
        }

        // 整数字面量 - Coffee 默认使用 64 位整数
        if let Ok(_) = expr.parse::<i64>() {
            return Some(Type::int());
        }

        // 布尔字面量
        if expr == "true" || expr == "false" {
            return Some(Type::bool());
        }

        // 字符串字面量
        if expr.starts_with('"') && expr.ends_with('"') {
            return Some(Type::String);
        }

        // 字符字面量
        if expr.starts_with('\'') && expr.ends_with('\'') && expr.len() == 3 {
            return Some(Type::Int { bits: 8, signed: false }); // char as u8
        }

        // 单元类型
        if expr == "()" {
            return Some(Type::Unit);
        }

        None
    }

    /// 推导二元表达式类型
    fn infer_binary_expression(&mut self, expr: &str) -> Result<Type, TypeSystemError> {
        // 按优先级顺序查找操作符
        let operators = [
            ("||", 1),
            ("&&", 2),
            ("==", 3), ("!=", 3),
            ("<", 4), (">", 4), ("<=", 4), (">=", 4),
            ("+", 5), ("-", 5),
            ("*", 6), ("/", 6), ("%", 6),
        ];

        for (op, _precedence) in operators.iter() {
            if let Some(pos) = find_operator_position(expr, op) {
                let left = &expr[..pos].trim();
                let right = &expr[pos + op.len()..].trim();

                let left_type = self.infer_expression(left)?;
                let right_type = self.infer_expression(right)?;

                return self.infer_binary_op(op, left_type, right_type);
            }
        }

        Err(TypeSystemError::ParseError {
            type_str: expr.to_string(),
            reason: "Not a binary expression".to_string(),
        })
    }

    /// 推导函数调用类型
    fn infer_function_call(&mut self, expr: &str) -> Result<Type, TypeSystemError> {
        if let Some(paren_pos) = expr.find('(') {
            let func_name = expr[..paren_pos].trim();
            let args_str = &expr[paren_pos + 1..expr.len() - 1];

            // 推导参数类型
            let arg_types = if args_str.trim().is_empty() {
                vec![]
            } else {
                let args = split_arguments(args_str);
                let mut types = Vec::new();
                for arg in args {
                    types.push(self.infer_expression(&arg)?);
                }
                types
            };

            // 查找函数类型
            if let Ok(registry) = self.registry.read() {
                if let Ok(Type::Function { params, return_type }) = registry.resolve_type(func_name) {
                    // 检查参数数量
                    if params.len() != arg_types.len() {
                        return Err(TypeSystemError::ArityMismatch {
                            expected: params.len(),
                            found: arg_types.len(),
                            span: Span::new(0, expr.len()),
                        });
                    }

                    // 检查参数类型兼容性
                    for (i, (expected, found)) in params.iter().zip(arg_types.iter()).enumerate() {
                        if !self.types_compatible(expected, found)? {
                            return Err(TypeSystemError::TypeMismatch {
                                expected: expected.clone(),
                                found: found.clone(),
                                span: Span::new(paren_pos + i * 10, paren_pos + (i + 1) * 10),
                            });
                        }
                    }

                    return Ok(*return_type);
                }
            }

            // 尝试泛型实例化
            return self.infer_generic_instantiation(func_name, arg_types);
        }

        Err(TypeSystemError::ParseError {
            type_str: expr.to_string(),
            reason: "Invalid function call syntax".to_string(),
        })
    }

    /// 推导索引访问类型
    fn infer_indexing(&mut self, expr: &str) -> Result<Type, TypeSystemError> {
        if let Some(bracket_pos) = expr.find('[') {
            let container_expr = &expr[..bracket_pos].trim();
            let index_expr = &expr[bracket_pos + 1..expr.len() - 1].trim();

            let container_type = self.infer_expression(container_expr)?;
            let index_type = self.infer_expression(index_expr)?;

            match container_type {
                Type::Array { elem, .. } => {
                    if index_type.is_int() {
                        Ok(*elem)
                    } else {
                        Err(TypeSystemError::TypeMismatch {
                            expected: Type::int(),
                            found: index_type,
                            span: Span::new(bracket_pos, expr.len()),
                        })
                    }
                }
                Type::Slice(elem) => {
                    if index_type.is_int() {
                        Ok(*elem)
                    } else {
                        Err(TypeSystemError::TypeMismatch {
                            expected: Type::int(),
                            found: index_type,
                            span: Span::new(bracket_pos, expr.len()),
                        })
                    }
                }
                _ => Err(TypeSystemError::InvalidOperation {
                    op: "[]".to_string(),
                    left: container_type,
                    right: index_type,
                    span: Span::new(0, expr.len()),
                })
            }
        } else {
            Err(TypeSystemError::ParseError {
                type_str: expr.to_string(),
                reason: "Invalid indexing syntax".to_string(),
            })
        }
    }

    /// 推导字段访问类型
    fn infer_field_access(&mut self, expr: &str) -> Result<Type, TypeSystemError> {
        if let Some(dot_pos) = expr.rfind('.') {
            let object_expr = &expr[..dot_pos].trim();
            let field_name = &expr[dot_pos + 1..].trim();

            let object_type = self.infer_expression(object_expr)?;

            match object_type {
                Type::NamedType { name, .. } => {
                    if let Ok(registry) = self.registry.read() {
                        let fields = registry.get_fields(&name);
                        for field in fields {
                            if field.name == *field_name {
                                return registry.resolve_type(&field.ty).map_err(|_|
                                    TypeSystemError::ParseError {
                                        type_str: field.ty.clone(),
                                        reason: "Failed to resolve field type".to_string(),
                                    }
                                );
                            }
                        }
                        return Err(TypeSystemError::field_not_found(&name, *field_name, Span::new(dot_pos, expr.len())));
                    }
                }
                _ => {}
            }
        }

        Err(TypeSystemError::ParseError {
            type_str: expr.to_string(),
            reason: "Invalid field access syntax".to_string(),
        })
    }

    /// 推导二元操作符类型
    fn infer_binary_op(&mut self, op: &str, left: Type, right: Type) -> Result<Type, TypeSystemError> {
        match op {
            "+" | "-" | "*" | "/" | "%" => {
                if left.is_numeric() && right.is_numeric() {
                    Ok(self.promote_numeric_types(&left, &right))
                } else {
                    self.unify_types(&left, &right)
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                if self.types_compatible(&left, &right)? {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::TypeMismatch {
                        expected: left,
                        found: right,
                        span: Span::new(0, 0),
                    })
                }
            }
            "&&" | "||" => {
                if left == Type::bool() && right == Type::bool() {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::TypeMismatch {
                        expected: Type::bool(),
                        found: if left != Type::bool() { left } else { right },
                        span: Span::new(0, 0),
                    })
                }
            }
            _ => {
                Err(TypeSystemError::InvalidOperation {
                    op: op.to_string(),
                    left,
                    right,
                    span: Span::new(0, 0),
                })
            }
        }
    }

    /// 统一两个类型
    fn unify_types(&mut self, ty1: &Type, ty2: &Type) -> Result<Type, TypeSystemError> {
        if ty1 == ty2 {
            return Ok(ty1.clone());
        }

        // 检查是否可以隐式转换
        if ty1.can_coerce_from(ty2) {
            return Ok(ty1.clone());
        }

        if ty2.can_coerce_from(ty1) {
            return Ok(ty2.clone());
        }

        // 类型不匹配
        Err(TypeSystemError::TypeMismatch {
            expected: ty1.clone(),
            found: ty2.clone(),
            span: Span::new(0, 0),
        })
    }

    /// 提升数值类型
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

    /// 检查类型兼容性
    fn types_compatible(&self, ty1: &Type, ty2: &Type) -> Result<bool, TypeSystemError> {
        if ty1 == ty2 {
            return Ok(true);
        }

        if ty1.can_coerce_from(ty2) || ty2.can_coerce_from(ty1) {
            return Ok(true);
        }

        // 检查注册器中的兼容性
        if let Ok(registry) = self.registry.read() {
            Ok(registry.is_compatible(ty1, ty2))
        } else {
            Ok(false)
        }
    }

    /// 推导类型实例化（不再支持泛型）
    fn infer_generic_instantiation(&mut self, name: &str, _arg_types: Vec<Type>) -> Result<Type, TypeSystemError> {
        // 不再支持泛型，忽略arg_types
        if let Ok(registry) = self.registry.read() {
            if let Some(_) = registry.get_type(name) {
                return Ok(Type::NamedType {
                    name: name.to_string(),
                });
            }
        }

        Err(TypeSystemError::undefined_type(name, Span::new(0, name.len())))
    }

    /// 查找变量类型
    fn lookup_variable(&self, name: &str) -> Option<Type> {
        // 先查找当前上下文中的局部变量
        for context in self.context_stack.iter().rev() {
            if let Some(ty) = context.locals.get(name) {
                return Some(ty.clone());
            }
        }

        // 查找类型变量
        if let Some(ty) = self.type_vars.get(name) {
            return Some(ty.clone());
        }

        None
    }

    /// 检查是否是有效标识符
    fn is_valid_identifier(&self, s: &str) -> bool {
        !s.is_empty() &&
        s.chars().next().unwrap().is_alphabetic() &&
        s.chars().all(|c| c.is_alphanumeric() || c == '_')
    }

    /// 创建新的类型变量
    fn fresh_type_var(&mut self) -> String {
        let id = self.next_var_id;
        self.next_var_id += 1;
        format!("T{}", id)
    }

    /// 在当前上下文中绑定变量
    pub fn bind_var(&mut self, name: String, ty: Type) {
        self.current_context_mut().locals.insert(name, ty);
    }

    /// 推导并绑定变量
    pub fn infer_and_bind(&mut self, name: String, expr: &str) -> Result<Type, TypeSystemError> {
        let ty = self.infer_expression(expr)?;
        self.bind_var(name, ty.clone());
        Ok(ty)
    }

    /// 添加类型约束
    pub fn add_constraint(&mut self, type_param: String, constraint: TypeConstraint) {
        self.constraints.entry(type_param).or_insert_with(Vec::new).push(constraint);
    }

    /// 解析类型变量
    pub fn resolve_type_var(&self, var: &str) -> Option<Type> {
        self.type_vars.get(var).cloned()
    }

    /// 清空所有推导状态
    pub fn reset(&mut self) {
        self.type_vars.clear();
        self.constraints.clear();
        self.next_var_id = 0;
        self.context_stack.clear();
        self.context_stack.push(InferenceContext::new("global"));
    }
}

/// 查找操作符在表达式中的位置（考虑括号）
fn find_operator_position(expr: &str, op: &str) -> Option<usize> {
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let chars: Vec<char> = expr.chars().collect();

    for i in 0..chars.len() {
        match chars[i] {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            _ => {
                if paren_depth == 0 && bracket_depth == 0 {
                    if i + op.len() <= chars.len() {
                        let potential_op: String = chars[i..i + op.len()].iter().collect();
                        if potential_op == op {
                            return Some(i);
                        }
                    }
                }
            }
        }
    }

    None
}

/// 分割函数参数
fn split_arguments(args_str: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut paren_depth = 0;
    let mut bracket_depth = 0;

    for ch in args_str.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current_arg.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current_arg.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                current_arg.push(ch);
            }
            ']' => {
                bracket_depth -= 1;
                current_arg.push(ch);
            }
            ',' if paren_depth == 0 && bracket_depth == 0 => {
                args.push(current_arg.trim().to_string());
                current_arg.clear();
            }
            _ => {
                current_arg.push(ch);
            }
        }
    }

    if !current_arg.trim().is_empty() {
        args.push(current_arg.trim().to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_literals() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut inferencer = TypeInferencer::new(registry);

        assert_eq!(inferencer.infer_expression("42").unwrap(), Type::int());
        assert_eq!(inferencer.infer_expression("3.14").unwrap(), Type::f64());
        assert_eq!(inferencer.infer_expression("true").unwrap(), Type::bool());
        assert_eq!(inferencer.infer_expression("\"hello\"").unwrap(), Type::String);
    }

    #[test]
    fn test_infer_binary_op() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut inferencer = TypeInferencer::new(registry);

        assert_eq!(
            inferencer.infer_expression("1 + 2").unwrap(),
            Type::int()
        );
        assert_eq!(
            inferencer.infer_expression("1.0 + 2.0").unwrap(),
            Type::float()
        );
        assert_eq!(
            inferencer.infer_expression("1 == 2").unwrap(),
            Type::bool()
        );
    }

    #[test]
    fn test_context_management() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut inferencer = TypeInferencer::new(registry);

        // 全局上下文
        inferencer.bind_var("global_var".to_string(), Type::int());

        // 进入函数上下文
        inferencer.enter_context(InferenceContext::new("function"));
        inferencer.bind_var("local_var".to_string(), Type::bool());

        // 本地变量可见
        assert_eq!(inferencer.lookup_variable("local_var"), Some(Type::bool()));
        // 全局变量可见
        assert_eq!(inferencer.lookup_variable("global_var"), Some(Type::int()));

        // 退出上下文
        inferencer.exit_context();

        // 本地变量不可见
        assert_eq!(inferencer.lookup_variable("local_var"), None);
        // 全局变量仍可见
        assert_eq!(inferencer.lookup_variable("global_var"), Some(Type::int()));
    }

    #[test]
    fn test_infer_and_bind() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut inferencer = TypeInferencer::new(registry);

        inferencer.infer_and_bind("x".to_string(), "42").unwrap();
        assert_eq!(inferencer.lookup_variable("x"), Some(Type::int()));
    }
}