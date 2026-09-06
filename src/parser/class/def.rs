use super::super::function::{Function, FunctionBody, Parameter};
use super::super::var::ReturnStmt;
use super::super::Statement;

/// Class definition
/// class Name<T> of Parent:
///     field:type
///     field2:type
///
///     fn method(params:type) => return_type:
///         body
#[derive(Debug, PartialEq, Clone)]
pub struct ClassDef {
    pub name: String,
    /// Generic type parameters (`class List<T>`). Empty when the class is not generic.
    pub type_params: Vec<String>,
    pub parent: Option<String>,
    pub fields: Vec<ClassField>,
    pub methods: Vec<MethodDef>,
    pub packed: bool,  // If true, use packed layout (no padding)
    pub has_constructor: bool,  // If true, class has a custom constructor (heap-allocated)
}

/// Class field
#[derive(Debug, PartialEq, Clone)]
pub struct ClassField {
    pub name: String,
    pub field_type: String,
    pub bit_width: Option<u8>,  // Bit width for bit fields (e.g., 3 for a 3-bit field)
}

/// Method definition (inside class)
#[derive(Debug, PartialEq, Clone)]
pub struct MethodDef {
    pub name: String,
    pub parameters: Vec<MethodParameter>,
    pub return_type: String,
    pub body: Vec<Statement>,
}

impl MethodDef {
    /// LLVM / MIR name `{class}_{method}`. A `self` parameter is kept only when
    /// the source listed it; `new` and other associated functions have no receiver.
    /// A trailing value expression (`Point { ... }`) becomes `return` so codegen does
    /// not emit a typed default return (`i64 0` vs class `ptr`).
    pub fn to_standalone_function(&self, class_name: &str) -> Function {
        let method_parameters = self.parameters.iter().map(|p| Parameter {
            name: p.name.clone(),
            param_type: if p.name == "self" && p.param_type.is_empty() {
                class_name.to_string()
            } else {
                p.param_type.clone()
            },
            is_variadic: false,
        }).collect();

        let mut body_stmts = self.body.clone();
        if self.return_type != "void" && self.return_type != "()" {
            if matches!(body_stmts.last(), Some(Statement::Expr(_))) {
                if let Some(Statement::Expr(expr)) = body_stmts.pop() {
                    body_stmts.push(Statement::Return(ReturnStmt { value: Some(*expr) }));
                }
            }
        }

        Function {
            name: format!("{}_{}", class_name, self.name),
            type_params: vec![],
            parameters: method_parameters,
            return_type: self.return_type.clone(),
            error_handler: None,
            is_c: false,
            body: FunctionBody::Block(body_stmts),
        }
    }
}

/// Method parameter
#[derive(Debug, PartialEq, Clone)]
pub struct MethodParameter {
    pub name: String,
    pub param_type: String,
}

/// Enum definition
/// enum Name:
///     Variant1
///     Variant2(type1, type2)
///     Variant3(field:type)
#[derive(Debug, PartialEq, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

/// Enum variant
#[derive(Debug, PartialEq, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<VariantField>,
}

/// Variant field
#[derive(Debug, PartialEq, Clone)]
pub enum VariantField {
    /// Positional type: Variant2(type1, type2)
    Type(String),
    /// Named field: Variant3(field:type)
    Named { name: String, field_type: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn associated_method_without_self_has_no_injected_receiver() {
        let method = MethodDef {
            name: "custom".to_string(),
            parameters: vec![
                MethodParameter { name: "timeout".into(), param_type: "int".into() },
                MethodParameter { name: "retries".into(), param_type: "int".into() },
            ],
            return_type: "Config".to_string(),
            body: vec![],
        };
        let f = method.to_standalone_function("Config");
        assert_eq!(f.name, "Config_custom");
        assert_eq!(
            f.parameters.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["timeout", "retries"]
        );
    }

    #[test]
    fn instance_method_keeps_single_self() {
        let method = MethodDef {
            name: "move".to_string(),
            parameters: vec![
                MethodParameter { name: "self".into(), param_type: String::new() },
                MethodParameter { name: "dx".into(), param_type: "int".into() },
            ],
            return_type: "Point".to_string(),
            body: vec![],
        };
        let f = method.to_standalone_function("Point");
        assert_eq!(
            f.parameters.iter().map(|p| (p.name.as_str(), p.param_type.as_str())).collect::<Vec<_>>(),
            vec![("self", "Point"), ("dx", "int")]
        );
    }
}
