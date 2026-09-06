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
        similar: Vec<String>,
    },
    /// 未定义的函数（来自 ty::TypeError）
    UndefinedFunction {
        name: String,
        span: Span,
        similar: Vec<String>,
    },
    /// 未定义的类型（来自 ty::TypeError）
    UndefinedType {
        name: String,
        span: Span,
        similar: Vec<String>,
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
        similar: Vec<String>,
    },
    /// 泛型参数数量不匹配（来自 ty::TypeError）
    #[allow(dead_code)]
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
    /// 类型解析错误（类型字符串语法无效）
    ParseError {
        type_str: String,
        reason: String,
    },
    /// Compiler-internal failure (e.g. poisoned registry lock), not type syntax
    Internal {
        reason: String,
    },
    /// 泛型实例化错误（新增）
    #[allow(dead_code)]
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
        similar: Vec<String>,
    },
    /// 变体不存在（新增）
    VariantNotFound {
        enum_name: String,
        variant_name: String,
        span: Span,
        similar: Vec<String>,
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
    /// Move / assign / rm while a loan is live (E303)
    BorrowViolation {
        variable: String,
        reason: String,
        span: Span,
    },
    /// Shared vs exclusive borrow conflict (E401)
    BorrowConflict {
        variable: String,
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
            TypeSystemError::OwnershipError { span, .. } |
            TypeSystemError::BorrowViolation { span, .. } |
            TypeSystemError::BorrowConflict { span, .. } => Some(*span),
            TypeSystemError::Duplicate { new: span, .. } => Some(*span),
            _ => None,
        }
    }

    /// Dummy source span covering `name` from offset 0 (no parser AST spans).
    pub fn span_for_name(name: &str) -> Span {
        Span::new(0, name.len())
    }

    /// Span from a diagnostic location, or `span_for_name` when location is empty.
    fn span_from_diagnostic(location: &crate::diagnostics::SourceLocation, fallback_name: &str) -> Span {
        if location.is_valid() || location.byte_length > 0 {
            Span::new(
                location.byte_offset,
                location.byte_offset + location.byte_length,
            )
        } else {
            TypeSystemError::span_for_name(fallback_name)
        }
    }

    /// 创建类型不匹配错误
    pub fn type_mismatch(expected: Type, found: Type, span: Span) -> Self {
        coffee_debug!("[DEBUG] type_mismatch: creating error: expected {:?}, found {:?}", expected, found);
        TypeSystemError::TypeMismatch { expected, found, span }
    }

    /// 创建未定义变量错误
    pub fn undefined_variable(name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::UndefinedVariable {
            name: name.into(),
            span,
            similar: Vec::new(),
        }
    }

    /// 创建未定义函数错误
    pub fn undefined_function(name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::UndefinedFunction {
            name: name.into(),
            span,
            similar: Vec::new(),
        }
    }

    /// 创建未定义类型错误
    pub fn undefined_type(name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::UndefinedType {
            name: name.into(),
            span,
            similar: Vec::new(),
        }
    }

    /// Attach similar-name suggestions (typos).
    pub fn with_similar(mut self, similar: Vec<String>) -> Self {
        match &mut self {
            TypeSystemError::UndefinedVariable { similar: slot, .. }
            | TypeSystemError::UndefinedFunction { similar: slot, .. }
            | TypeSystemError::UndefinedType { similar: slot, .. }
            | TypeSystemError::FieldNotFound { similar: slot, .. }
            | TypeSystemError::MethodNotFound { similar: slot, .. }
            | TypeSystemError::VariantNotFound { similar: slot, .. } => {
                *slot = similar;
            }
            _ => {}
        }
        self
    }

    /// Similar names recorded on undefined-symbol errors.
    pub fn similar_names(&self) -> Vec<String> {
        match self {
            TypeSystemError::UndefinedVariable { similar, .. }
            | TypeSystemError::UndefinedFunction { similar, .. }
            | TypeSystemError::UndefinedType { similar, .. }
            | TypeSystemError::FieldNotFound { similar, .. }
            | TypeSystemError::MethodNotFound { similar, .. }
            | TypeSystemError::VariantNotFound { similar, .. } => similar.clone(),
            _ => Vec::new(),
        }
    }

    fn is_object_type(ty: &Type) -> bool {
        matches!(ty, Type::Variadic) || ty.to_string() == "object"
    }

    /// C opaque handles / newtypes (`FILE`, `HWND`, …), not `buf` or `object`.
    fn is_file_like_named(ty: &Type) -> bool {
        match ty {
            Type::NamedType { name } if name != "buf" && name != "object" => {
                let upper = name.to_ascii_uppercase();
                upper == "FILE"
                    || upper.contains("FILE")
                    || (name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                        && name.chars().any(|c| c.is_ascii_alphabetic()))
            }
            _ => {
                let s = ty.to_string();
                let upper = s.to_ascii_uppercase();
                upper == "FILE" || upper.contains("FILE")
            }
        }
    }

    fn type_mismatch_help(expected: &Type, found: &Type) -> Option<&'static str> {
        if expected.is_int() && found.is_float() {
            Some("use int(expr) to convert the float")
        } else if expected.is_float() && found.is_int() {
            Some("use float(expr) to convert the integer")
        } else if matches!(expected, Type::Bool) && found.is_int() {
            Some("use bool(...) or the literals true / false")
        } else if expected.is_int() && matches!(found, Type::Bool) {
            Some("use bool(...) or the literals true / false")
        } else if (Self::is_file_like_named(expected) && Self::is_object_type(found))
            || (Self::is_object_type(expected) && Self::is_file_like_named(found))
        {
            Some(
                "use `as` to convert a newtype and its source (FILE as object / p as FILE); sibling handles need two hops",
            )
        } else if expected.is_int() && found.is_int() && expected != found {
            Some("annotate the binding or literal (let n: int(8)- = 16) — C uLong/size_t is int(8)-")
        } else {
            None
        }
    }

    fn libc_stdio_help(name: &str) -> Option<&'static str> {
        matches!(name, "print" | "println" | "printf").then_some(
            "use print in std, or use printf, fprintf, exit in libc of c",
        )
    }

    fn ownership_syntax_help() -> &'static str {
        "see SYNTAX for borrow (`&` / `&mut`), `mv`, `clone`, and `rm`"
    }

    /// Extra `help:` lines for `Diagnostic::format`.
    pub fn diagnostic_suggestions(&self) -> Vec<String> {
        match self {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                Self::type_mismatch_help(expected, found)
                    .map(|s| vec![s.to_string()])
                    .unwrap_or_default()
            }
            TypeSystemError::UndefinedFunction { name, .. } => {
                Self::libc_stdio_help(name)
                    .map(|s| vec![s.to_string()])
                    .unwrap_or_default()
            }
            TypeSystemError::OwnershipError { .. } | TypeSystemError::BorrowViolation { .. } => {
                vec![Self::ownership_syntax_help().to_string()]
            }
            _ => Vec::new(),
        }
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
    #[cfg(test)]
    pub fn field_not_found(type_name: impl Into<String>, field_name: impl Into<String>, span: Span) -> Self {
        TypeSystemError::FieldNotFound {
            type_name: type_name.into(),
            field_name: field_name.into(),
            span,
            similar: Vec::new(),
        }
    }

    /// 创建重复定义错误
    pub fn duplicate(name: impl Into<String>, existing: Span, new: Span) -> Self {
        TypeSystemError::Duplicate { name: name.into(), existing, new }
    }

    /// 创建未找到错误
    #[cfg(test)]
    pub fn not_found(name: impl Into<String>, kind: SpaceKind) -> Self {
        TypeSystemError::NotFound { name: name.into(), kind }
    }

    pub fn internal(reason: impl Into<String>) -> Self {
        TypeSystemError::Internal { reason: reason.into() }
    }

    // Legacy conversion functions - deprecated after module consolidation
    // pub fn from_type_error(error: crate::ty::check::TypeError) -> Self {
    }
    
/// Parse a diagnostic type string; on failure keep the original text as a name
/// (same as `Type::from_str`'s unknown-ident path), never invent `()`.
fn type_from_diag_str(s: &str) -> Type {
    type_from_str(s).unwrap_or_else(|_| Type::NamedType {
        name: s.to_string(),
    })
}

    /// Convert from Diagnostic to TypeSystemError
impl From<crate::diagnostics::Diagnostic> for TypeSystemError {
    fn from(diagnostic: crate::diagnostics::Diagnostic) -> Self {
        let location = diagnostic.location;
        match diagnostic.kind {
            crate::diagnostics::ErrorKind::UndefinedSymbol { name, .. } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &name);
                TypeSystemError::undefined_variable(name, span)
            }
            crate::diagnostics::ErrorKind::TypeMismatch { expected, found } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &expected);
                let expected_ty = type_from_diag_str(&expected);
                let found_ty = type_from_diag_str(&found);
                TypeSystemError::TypeMismatch {
                    expected: expected_ty,
                    found: found_ty,
                    span,
                }
            }
            crate::diagnostics::ErrorKind::UnknownType { name } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &name);
                TypeSystemError::undefined_type(name, span)
            }
            crate::diagnostics::ErrorKind::ArityMismatch { expected, found } => {
                let span = TypeSystemError::span_from_diagnostic(
                    &location,
                    &format!("{}/{}", expected, found),
                );
                TypeSystemError::arity_mismatch(expected, found, span)
            }
            crate::diagnostics::ErrorKind::InvalidOperation { op, left, right } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &op);
                TypeSystemError::invalid_operation(
                    op,
                    type_from_diag_str(&left),
                    type_from_diag_str(&right),
                    span,
                )
            }
            crate::diagnostics::ErrorKind::NotCallable { ty } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &ty);
                TypeSystemError::not_callable(type_from_diag_str(&ty), span)
            }
            crate::diagnostics::ErrorKind::FieldNotFound { type_name, field_name } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &field_name);
                TypeSystemError::FieldNotFound {
                    type_name,
                    field_name,
                    span,
                    similar: Vec::new(),
                }
            }
            crate::diagnostics::ErrorKind::MethodNotFound { type_name, method_name } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &method_name);
                TypeSystemError::MethodNotFound {
                    type_name,
                    method_name,
                    span,
                    similar: Vec::new(),
                }
            }
            crate::diagnostics::ErrorKind::VariantNotFound { enum_name, variant_name } => {
                let span = TypeSystemError::span_from_diagnostic(&location, &variant_name);
                TypeSystemError::VariantNotFound {
                    enum_name,
                    variant_name,
                    span,
                    similar: Vec::new(),
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
        let _ = self.span();
        match self {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                let help = TypeSystemError::type_mismatch_help(expected, found).unwrap_or(
                    "check the expression or provide a value of the correct type",
                );
                write!(f, "type mismatch: expected '{}', found '{}'
  = note: these types are incompatible and cannot be implicitly converted
  = help: {}", expected, found, help)
            }
            TypeSystemError::UndefinedVariable { name, similar, .. } => {
                write!(f, "undefined variable: '{}'", name)?;
                if !similar.is_empty() {
                    write!(
                        f,
                        "\n  = help: did you mean {}?",
                        similar.iter().map(|s| format!("`{}`", s)).collect::<Vec<_>>().join(", ")
                    )?;
                }
                Ok(())
            }
            TypeSystemError::UndefinedFunction { name, similar, .. } => {
                write!(f, "undefined function: '{}'", name)?;
                if let Some(help) = TypeSystemError::libc_stdio_help(name) {
                    write!(f, "\n  = help: {}", help)?;
                }
                if !similar.is_empty() {
                    write!(
                        f,
                        "\n  = help: did you mean {}?",
                        similar.iter().map(|s| format!("`{}`", s)).collect::<Vec<_>>().join(", ")
                    )?;
                }
                Ok(())
            }
            TypeSystemError::UndefinedType { name, similar, .. } => {
                write!(f, "undefined type: '{}'", name)?;
                if !similar.is_empty() {
                    write!(
                        f,
                        "\n  = help: did you mean {}?",
                        similar.iter().map(|s| format!("`{}`", s)).collect::<Vec<_>>().join(", ")
                    )?;
                }
                Ok(())
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
                write!(f, "type '{}' is not callable
  = note: only functions and function pointers can be called
  = help: did you mean a named fn?", ty)
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
            TypeSystemError::Internal { reason } => {
                write!(f, "internal type-system error: {}", reason)
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
  = help: {}
  = help: review the ownership rules and ensure proper memory management", reason, TypeSystemError::ownership_syntax_help())
            }
            TypeSystemError::BorrowViolation { reason, .. } => {
                write!(f, "{}\n  = help: {}", reason, TypeSystemError::ownership_syntax_help())
            }
            TypeSystemError::BorrowConflict { reason, .. } => {
                write!(f, "{}", reason)
            }
        }
    }
}

impl std::error::Error for TypeSystemError {}

/// 诊断信息，用于详细的错误报告（来自 space 模块）
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub span: Option<Span>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
}

#[cfg(test)]
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

    /// 设置位置信息
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
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

#[cfg(test)]
impl fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticLevel::Error => write!(f, "error"),
        }
    }
}

#[cfg(test)]
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

    #[test]
    fn from_diagnostic_named_errors_cover_identifier() {
        use crate::diagnostics::{Diagnostic, ErrorKind, Severity};

        let unknown: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::UnknownType {
                name: "Nope".to_string(),
            },
            "unknown",
        )
        .into();
        assert_eq!(unknown.span(), Some(TypeSystemError::span_for_name("Nope")));

        let field: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::FieldNotFound {
                type_name: "Point".to_string(),
                field_name: "z".to_string(),
            },
            "field",
        )
        .into();
        assert_eq!(field.span(), Some(TypeSystemError::span_for_name("z")));
    }

    #[test]
    fn from_diagnostic_maps_dummy_spans_via_span_for_name() {
        use crate::diagnostics::{Diagnostic, ErrorKind, Severity, SymbolType};

        let undef: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::UndefinedSymbol {
                name: "missing_var".to_string(),
                symbol_type: SymbolType::Variable,
            },
            "undef",
        )
        .into();
        assert_eq!(
            undef.span(),
            Some(TypeSystemError::span_for_name("missing_var"))
        );

        let mismatch: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::TypeMismatch {
                expected: "int".to_string(),
                found: "bool".to_string(),
            },
            "mismatch",
        )
        .into();
        assert_eq!(
            mismatch.span(),
            Some(TypeSystemError::span_for_name("int"))
        );

        let arity: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::ArityMismatch {
                expected: 2,
                found: 1,
            },
            "arity",
        )
        .into();
        assert_eq!(
            arity.span(),
            Some(TypeSystemError::span_for_name(&format!("{}/{}", 2, 1)))
        );
    }

    fn named(name: &str) -> Type {
        Type::NamedType {
            name: name.to_string(),
        }
    }

    #[test]
    fn from_diagnostic_unparsable_type_strings_become_named_type() {
        use crate::diagnostics::{Diagnostic, ErrorKind, Severity};

        let mismatch: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::TypeMismatch {
                expected: "List<int>".to_string(),
                found: "Foo<bar>".to_string(),
            },
            "mismatch",
        )
        .into();
        match mismatch {
            TypeSystemError::TypeMismatch {
                expected, found, ..
            } => {
                assert_eq!(expected, named("List<int>"));
                assert_eq!(found, named("Foo<bar>"));
            }
            other => panic!("expected TypeMismatch, got {:?}", other),
        }

        let op: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::InvalidOperation {
                op: "+".to_string(),
                left: "List<int>".to_string(),
                right: "Map<str>".to_string(),
            },
            "op",
        )
        .into();
        match op {
            TypeSystemError::InvalidOperation { left, right, .. } => {
                assert_eq!(left, named("List<int>"));
                assert_eq!(right, named("Map<str>"));
            }
            other => panic!("expected InvalidOperation, got {:?}", other),
        }

        let call: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::NotCallable {
                ty: "List<int>".to_string(),
            },
            "call",
        )
        .into();
        match call {
            TypeSystemError::NotCallable { ty, .. } => {
                assert_eq!(ty, named("List<int>"));
            }
            other => panic!("expected NotCallable, got {:?}", other),
        }
    }

    #[test]
    fn from_diagnostic_uses_source_location_span() {
        use crate::diagnostics::{Diagnostic, ErrorKind, Severity, SourceLocation};

        let loc = SourceLocation {
            byte_offset: 10,
            byte_length: 3,
            ..SourceLocation::default()
        };
        let err: TypeSystemError = Diagnostic::new(
            Severity::Error,
            ErrorKind::TypeMismatch {
                expected: "int".to_string(),
                found: "bool".to_string(),
            },
            "mismatch",
        )
        .with_location(loc)
        .into();
        assert_eq!(err.span(), Some(Span::new(10, 13)));
    }

    #[test]
    fn type_mismatch_file_vs_object_suggests_as() {
        let error = TypeSystemError::type_mismatch(
            named("FILE"),
            Type::Variadic,
            Span::new(0, 1),
        );
        let msg = error.to_string();
        assert!(msg.contains("as"), "help should mention `as`: {msg}");
        assert!(msg.contains("FILE"));
        assert!(msg.contains("object"));

        let reverse = TypeSystemError::type_mismatch(
            Type::Variadic,
            named("FILE"),
            Span::new(0, 1),
        );
        assert!(reverse.to_string().contains("as"));
    }

    #[test]
    fn type_mismatch_c_int_width_suggests_annotation() {
        let error = TypeSystemError::type_mismatch(
            Type::Int {
                bits: 64,
                signed: false,
            },
            Type::int(),
            Span::new(0, 1),
        );
        let msg = error.to_string();
        assert!(msg.contains("int(8)-"), "{msg}");
        assert!(msg.contains("annotate"), "{msg}");
    }

    #[test]
    fn undefined_print_mentions_std_and_libc() {
        let error = TypeSystemError::undefined_function("print", Span::new(0, 5));
        let msg = error.to_string();
        assert!(msg.contains("use print in std"), "{msg}");
        assert!(msg.contains("libc"), "{msg}");
    }
}