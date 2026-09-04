use crate::coffee_debug;
use super::definition::{Span, SpaceKind, Type, type_from_str};
use std::fmt;

/// 统一的类型系统错误，合并了 TypeError 和 SpaceError
#[derive(Debug, Clone)]
pub enum TypeSystemError {
    /// 类型不匹配（来自 ty::TypeError）
    TypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },
    /// 未定义的变量（来自 ty::TypeError）
    UndefinedVariable {
        name: String,
        span: Span,
    },
    /// 未定义的函数（来自 ty::TypeError）
    UndefinedFunction {
        name: String,
        span: Span,
    },
    /// 未定义的类型（来自 ty::TypeError）
    UndefinedType {
        name: String,
        span: Span,
    },
    /// 参数数量不匹配（来自 ty::TypeError）
    ArityMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },
    /// 非法操作（来自 ty::TypeError）
    InvalidOperation {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },
    /// 不是函数类型（来自 ty::TypeError）
    NotCallable {
        ty: Type,
        span: Span,
    },
    /// 字段不存在（来自 ty::TypeError）
    FieldNotFound {
        type_name: String,
        field_name: String,
        span: Span,
    },
    /// 泛型参数数量不匹配（来自 ty::TypeError）
    GenericArgCountMismatch {
        type_name: String,
        expected: usize,
        found: usize,
        span: Span,
    },
    /// 未找到绑定（来自 space::SpaceError）
    NotFound {
        name: String,
        kind: SpaceKind,
    },
    /// 重复定义（来自 space::SpaceError）
    Duplicate {
        name: String,
        existing: Span,
        new: Span,
    },
    /// 循环依赖（来自 space::SpaceError）
    Cycle {
        path: Vec<String>,
    },
    /// 约束不满足（来自 space::SpaceError）
    ConstraintViolation {
        constraint: String,
        reason: String,
        span: Span,
    },
    /// 可见性错误（来自 space::SpaceError）
    VisibilityError {
        name: String,
        required: String,
        actual: String,
    },
    /// 类型解析错误（新增）
    ParseError {
        type_str: String,
        reason: String,
    },
    /// 泛型实例化错误（新增）
    InstantiationError {
        type_name: String,
        args: Vec<Type>,
        reason: String,
        span: Span,
    },
    /// 方法不存在（新增）
    MethodNotFound {
        type_name: String,
        method_name: String,
        span: Span,
    },
    /// 变体不存在（新增）
    VariantNotFound {
        enum_name: String,
        variant_name: String,
        span: Span,
    },
    /// 生命周期错误（新增）
    LifetimeError {
        reason: String,
        span: Span,
    },
    /// 所有权错误（新增）
    OwnershipError {
        reason: String,
        span: Span,
    },
}

impl TypeSystemError {
    /// 获取错误的位置信息
    pub fn span(&self) -> Option<Span> {
        match self {
            TypeSystemError::TypeMismatch { span, .. } |
            TypeSystemError::UndefinedVariable { span, .. } |
            TypeSystemError::UndefinedFunction { span, .. } |
            TypeSystemError::UndefinedType { span, .. } |
            TypeSystemError::ArityMismatch { span, .. } |
            TypeSystemError::InvalidOperation { span, .. } |
            TypeSystemError::NotCallable { span, .. } |
            TypeSystemError::FieldNotFound { span, .. } |
            TypeSystemError::GenericArgCountMismatch { span, .. } |
            TypeSystemError::ConstraintViolation { span, .. } |
            TypeSystemError::InstantiationError { span, .. } |
            TypeSystemError::MethodNotFound { span, .. } |
            TypeSystemError::VariantNotFound { span, .. } |
            TypeSystemError::LifetimeError { span, .. } |
            TypeSystemError::OwnershipError { span, .. } => Some(*span),
            TypeSystemError::Duplicate { new: span, .. } => Some(*span),
            _ => None,
        }
    }

    /// 创建类型不匹配错误
    pub fn type_mismatch(expected: Type, found: Type, span: Span) -> Self {
        coffee_debug!("[DEBUG] type_mismatch: creating error: expected {:?}, found {:?}", expected, found);
        TypeSystemError::TypeMismatch { expected, found, span }
    }

    /// 创建未定义变量错误
    pub fn undefined_variable(name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::UndefinedVariable { name: name.into(), span }
    }

    /// 创建未定义函数错误
    pub fn undefined_function(name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::UndefinedFunction { name: name.into(), span }
    }

    /// 创建未定义类型错误
    pub fn undefined_type(name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::UndefinedType { name: name.into(), span }
    }

    /// 创建参数数量不匹配错误
    pub fn arity_mismatch(expected: usize, found: usize, span: Span) -> Self {
        TypeSystemError::ArityMismatch { expected, found, span }
    }

    /// 创建无效操作错误
    pub fn invalid_operation(op: impl Into<String>, left: Type, right: Type, span: Span) -> Self {
        TypeSystemError::InvalidOperation { op: op.into(), left, right, span }
    }

    /// 创建不可调用错误
    pub fn not_callable(ty: Type, span: Span) -> Self {
        TypeSystemError::NotCallable { ty, span }
    }

    /// 创建字段未找到错误
    pub fn field_not_found(type_name: impl Into<String>, field_name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::FieldNotFound {
            type_name: type_name.into(),
            field_name: field_name.into(),
            span
        }
    }

    /// 创建重复定义错误
    pub fn duplicate(name: impl Into<String>, existing: Span, new: Span) -> Self {
        TypeSystemError::Duplicate { name: name.into(), existing, new }
    }

    /// 创建未找到错误
    pub fn not_found(name: impl Into<String>, kind: SpaceKind) -> Self {
        TypeSystemError::NotFound { name: name.into(), kind }
    }

    /// 创建约束违反错误
    pub fn constraint_violation(constraint: impl Into<String>, reason: impl Into<String>, span: Span) -> Self {
        TypeSystemError::ConstraintViolation {
            constraint: constraint.into(),
            reason: reason.into(),
            span
        }
    }

    // Legacy conversion functions - deprecated after module consolidation
    // pub fn from_type_error(error: crate::ty::check::TypeError) -> Self {
    }
    
    /// Convert from Diagnostic to TypeSystemError
impl From<crate::diagnostics::Diagnostic> for TypeSystemError {
    fn from(diagnostic: crate::diagnostics::Diagnostic) -> Self {
        match diagnostic.kind {
            crate::diagnostics::ErrorKind::UndefinedSymbol { name, .. } => {
                TypeSystemError::NotFound {
                    name,
                    kind: SpaceKind::Symbol,
                }
            }
            crate::diagnostics::ErrorKind::TypeMismatch { expected, found } => {
                let expected_ty = type_from_str(&expected).unwrap_or(Type::unit());
                let found_ty = type_from_str(&found).unwrap_or(Type::unit());
                TypeSystemError::TypeMismatch {
                    expected: expected_ty,
                    found: found_ty,
                    span: Span::new(0, 1),
                }
            }
            crate::diagnostics::ErrorKind::UnknownType { name } => {
                TypeSystemError::undefined_type(name, Span::new(0, 0))
            }
            crate::diagnostics::ErrorKind::ArityMismatch { expected, found } => {
                TypeSystemError::arity_mismatch(expected, found, Span::new(0, 0))
            }
            crate::diagnostics::ErrorKind::InvalidOperation { op, left, right } => {
                TypeSystemError::invalid_operation(
                    op,
                    type_from_str(&left).unwrap_or(Type::unit()),
                    type_from_str(&right).unwrap_or(Type::unit()),
                    Span::new(0, 0),
                )
            }
            crate::diagnostics::ErrorKind::NotCallable { ty } => {
                TypeSystemError::not_callable(type_from_str(&ty).unwrap_or(Type::unit()), Span::new(0, 0))
            }
            crate::diagnostics::ErrorKind::FieldNotFound { type_name, field_name } => {
                TypeSystemError::FieldNotFound {
                    type_name,
                    field_name,
                    span: Span::new(0, 0),
                }
            }
            crate::diagnostics::ErrorKind::MethodNotFound { type_name, method_name } => {
                TypeSystemError::MethodNotFound {
                    type_name,
                    method_name,
                    span: Span::new(0, 0),
                }
            }
            crate::diagnostics::ErrorKind::VariantNotFound { enum_name, variant_name } => {
                TypeSystemError::VariantNotFound {
                    enum_name,
                    variant_name,
                    span: Span::new(0, 0),
                }
            }
            crate::diagnostics::ErrorKind::InvalidType { name, reason } => {
                TypeSystemError::ParseError {
                    type_str: name,
                    reason,
                }
            }
            _ => {
                // Default mapping for other diagnostic types
                TypeSystemError::ParseError {
                    type_str: "unknown".to_string(),
                    reason: diagnostic.message,
                }
            }
        }
    }
}

impl fmt::Display for TypeSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                write!(f, "type mismatch: expected '{}', found '{}'
  = note: these types are incompatible and cannot be implicitly converted
  = help: check the expression or provide a value of the correct type", expected, found)
            }
            TypeSystemError::UndefinedVariable { name, .. } => {
                write!(f, "undefined variable: '{}'", name)
            }
            TypeSystemError::UndefinedFunction { name, .. } => {
                write!(f, "undefined function: '{}'", name)
            }
            TypeSystemError::UndefinedType { name, .. } => {
                write!(f, "undefined type: '{}'", name)
            }
            TypeSystemError::ArityMismatch { expected, found, .. } => {
                write!(f, "arity mismatch: expected {} arguments, found {}
  = note: incorrect number of arguments provided to the function
  = help: check the function signature and provide the correct number of arguments", expected, found)
            }
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                write!(f, "invalid operation: {} {} {}
  = note: this operation is not supported for the given types
  = help: check the types of operands and ensure they support this operation", left, op, right)
            }
            TypeSystemError::NotCallable { ty, .. } => {
                write!(f, "type '{} is not callable
  = note: only functions and function pointers can be called
  = help: check the type of the expression and ensure it is a function", ty)
            }
            TypeSystemError::FieldNotFound { type_name, field_name, .. } => {
                write!(f, "field '{} not found in type '{}'
  = note: the type does not have this field
  = help: check the field name or the type definition", field_name, type_name)
            }
            TypeSystemError::GenericArgCountMismatch { type_name, expected, found, .. } => {
                write!(f, "type '{} expected {} generic arguments, found {}
  = note: incorrect number of generic arguments
  = help: check the type definition and provide the correct number of generic arguments", type_name, expected, found)
            }
            TypeSystemError::NotFound { name, kind } => {
                write!(f, "{} not found in {} space
  = note: the item is not defined in the specified space
  = help: check the name or import the item", name, kind)
            }
            TypeSystemError::Duplicate { name, .. } => {
                write!(f, "duplicate definition of '{}'
  = note: this name is already defined in the current scope
  = help: use a different name or remove the duplicate definition", name)
            }
            TypeSystemError::Cycle { path } => {
                write!(f, "cycle detected: {}
  = note: there is a circular dependency
  = help: break the cycle by removing one of the dependencies", path.join(" -> "))
            }
            TypeSystemError::ConstraintViolation { constraint, reason, .. } => {
                write!(f, "constraint '{} violated: {}
  = note: the value does not satisfy the constraint
  = help: check the value and ensure it meets the constraint requirements", constraint, reason)
            }
            TypeSystemError::VisibilityError { name, required, actual } => {
                write!(f, "visibility error for '{}': required {}, found {}
  = note: the item has insufficient visibility
  = help: check the visibility modifiers and ensure proper access", name, required, actual)
            }
            TypeSystemError::ParseError { type_str, reason } => {
                write!(f, "failed to parse type '{}': {}
  = note: the type syntax is invalid
  = help: check the type syntax and ensure it follows the Coffee language specification", type_str, reason)
            }
            TypeSystemError::InstantiationError { type_name, reason, .. } => {
                write!(f, "failed to instantiate type '{}': {}
  = note: the type could not be instantiated
  = help: check the type parameters and ensure they are valid", type_name, reason)
            }
            TypeSystemError::MethodNotFound { type_name, method_name, .. } => {
                write!(f, "method '{} not found in type '{}'
  = note: the type does not have this method
  = help: check the method name or the type definition", method_name, type_name)
            }
            TypeSystemError::VariantNotFound { enum_name, variant_name, .. } => {
                write!(f, "variant '{} not found in enum '{}'
  = note: the enum does not have this variant
  = help: check the variant name or the enum definition", variant_name, enum_name)
            }
            TypeSystemError::LifetimeError { reason, .. } => {
                write!(f, "Lifetime error: {}
  = note: this operation violates lifetime rules
  = help: review the lifetime rules and ensure proper reference management", reason)
            }
            TypeSystemError::OwnershipError { reason, .. } => {
                write!(f, "Ownership error: {}
  = note: this operation violates Coffee's ownership rules
  = help: review the ownership rules and ensure proper memory management", reason)
            }
        }
    }
}

impl std::error::Error for TypeSystemError {}

/// 诊断信息，用于详细的错误报告（来自 space 模块）
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub span: Option<Span>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
}

impl Diagnostic {
    /// 创建错误诊断
    pub fn error(message: impl Into<String>) -> Self {
        Diagnostic {
            level: DiagnosticLevel::Error,
            message: message.into(),
            span: None,
            notes: vec![],
            help: None,
        }
    }

    /// 创建警告诊断
    pub fn warning(message: impl Into<String>) -> Self {
        Diagnostic {
            level: DiagnosticLevel::Warning,
            message: message.into(),
            span: None,
            notes: vec![],
            help: None,
        }
    }

    /// 设置位置信息
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// 添加注释
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// 添加帮助信息
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// 从 TypeSystemError 创建诊断
    pub fn from_error(error: &TypeSystemError) -> Self {
        let mut diag = Diagnostic::error(error.to_string());
        if let Some(span) = error.span() {
            diag = diag.with_span(span);
        }

        // 根据错误类型添加帮助信息
        match error {
            TypeSystemError::UndefinedType { name, .. } => {
                diag = diag.with_help(format!("Did you forget to define type '{}'?", name));
            }
            TypeSystemError::UndefinedVariable { name, .. } => {
                diag = diag.with_help(format!("Did you forget to declare variable '{}'?", name));
            }
            TypeSystemError::UndefinedFunction { name, .. } => {
                diag = diag.with_help(format!("Did you forget to define function '{}'?", name));
            }
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                if expected.can_coerce_from(found) {
                    diag = diag.with_help("These types can be implicitly converted");
                } else {
                    diag = diag.with_help("Consider adding an explicit type conversion");
                }
            }
            _ => {}
        }

        diag
    }
}

impl fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticLevel::Error => write!(f, "error"),
            DiagnosticLevel::Warning => write!(f, "warning"),
            DiagnosticLevel::Note => write!(f, "note"),
            DiagnosticLevel::Help => write!(f, "help"),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.level, self.message)?;

        if let Some(span) = self.span {
            write!(f, " at {}:{}", span.start, span.end)?;
        }

        for note in &self.notes {
            write!(f, "\n  note: {}", note)?;
        }

        if let Some(help) = &self.help {
            write!(f, "\n  help: {}", help)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = TypeSystemError::type_mismatch(
            Type::int(),
            Type::bool(),
            Span::new(10, 20),
        );

        assert!(error.to_string().contains("type mismatch"));
        assert!(error.to_string().contains("int"));
        assert!(error.to_string().contains("bool"));
    }

    #[test]
    fn test_diagnostic_from_error() {
        let error = TypeSystemError::undefined_variable("x", Span::new(5, 6));
        let diag = Diagnostic::from_error(&error);

        assert_eq!(diag.level, DiagnosticLevel::Error);
        assert!(diag.message.contains("undefined variable"));
        assert!(diag.help.is_some());
        assert_eq!(diag.span, Some(Span::new(5, 6)));
    }

    #[test]
    fn test_error_span() {
        let error = TypeSystemError::undefined_function("foo", Span::new(15, 25));
        assert_eq!(error.span(), Some(Span::new(15, 25)));

        let error2 = TypeSystemError::not_found("SomeType", SpaceKind::Type);
        assert_eq!(error2.span(), None);
    }
}