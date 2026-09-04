use super::definition::*;
use super::errors::TypeSystemError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 统一类型注册器：合并 TypeSpace 和 BuiltinTypes 的功能
///
/// 特性：
/// - 类型层次：支持继承和trait约束
/// - 泛型实例化：跟踪泛型类型的实例化
/// - 类型等价：判断两个类型在语义上是否等价
/// - 内置类型管理：管理基础类型和标准库类型
pub struct TypeRegistry {
    /// 空间ID
    id: SpaceId,
    /// 空间名称
    name: String,
    /// 父类型空间
    parent: Option<Arc<RwLock<TypeRegistry>>>,
    /// 类型定义
    types: Arc<RwLock<HashMap<String, TypeDef>>>,
    /// 类型别名
    aliases: Arc<RwLock<HashMap<String, Type>>>,
    /// 泛型实例缓存
    instances: Arc<RwLock<HashMap<String, Vec<Vec<Type>>>>>,
    /// 类型约束
    constraints: Arc<RwLock<HashMap<String, Vec<TypeConstraint>>>>,
}

impl TypeRegistry {
    /// 创建新的类型注册器
    pub fn new(name: impl Into<String>) -> Self {
        TypeRegistry {
            id: SpaceId::new(),
            name: name.into(),
            parent: None,
            types: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            constraints: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建根类型注册器（带内置类型）
    pub fn root() -> Self {
        let mut registry = TypeRegistry {
            id: SpaceId::new(),
            name: "builtin".to_string(),
            parent: None,
            types: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            constraints: Arc::new(RwLock::new(HashMap::new())),
        };

        // 注册内置类型
        registry.register_builtin_types();
        registry
    }

    /// 创建带有父注册器的注册器（用于作用域）
    pub fn with_parent(parent: Arc<RwLock<TypeRegistry>>) -> Self {
        TypeRegistry {
            id: SpaceId::new(),
            name: format!("child-{}", parent.read().unwrap().name),
            parent: Some(parent),
            types: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            constraints: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册内置类型（合并 BuiltinTypes 和 TypeSpace 的内置类型）
    fn register_builtin_types(&mut self) {
        let mut types = self.types.write().unwrap();
        let mut aliases = self.aliases.write().unwrap();

        // 基础类型作为别名（来自 BuiltinTypes）
        aliases.insert("int".to_string(), Type::int());
        aliases.insert("i32".to_string(), Type::i32());
        aliases.insert("u8".to_string(), Type::u8());
        aliases.insert("float".to_string(), Type::float());
        aliases.insert("f64".to_string(), Type::f64());
        aliases.insert("bool".to_string(), Type::bool());
        aliases.insert("str".to_string(), Type::String);
        aliases.insert("string".to_string(), Type::String);
        aliases.insert("void".to_string(), Type::void());
        aliases.insert("()".to_string(), Type::unit());

        // Error 基类
        types.insert("Error".to_string(), TypeDef::Class {
            name: "Error".to_string(),
            fields: vec![
                ClassField {
                    name: "code".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "message".to_string(),
                    ty: "str".to_string(),
                    visibility: Visibility::Public,
                },
            ],
            methods: vec![
                MethodSignature {
                    name: "to_string".to_string(),
                    params: vec![],
                    return_type: Type::NamedType {
                        name: "str".to_string(),
                    },
                    receiver: None,
                    generics: vec![],
                },
                MethodSignature {
                    name: "get_code".to_string(),
                    params: vec![],
                    return_type: Type::NamedType {
                        name: "int".to_string(),
                    },
                    receiver: None,
                    generics: vec![],
                },
                MethodSignature {
                    name: "get_message".to_string(),
                    params: vec![],
                    return_type: Type::NamedType {
                        name: "str".to_string(),
                    },
                    receiver: None,
                    generics: vec![],
                },
            ],
            generics: vec![],
            parent: None,
        });

        // ErrorContext (Ctx) 结构
        types.insert("Ctx".to_string(), TypeDef::Struct {
            name: "Ctx".to_string(),
            fields: vec![
                ClassField {
                    name: "file".to_string(),
                    ty: "str".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "line".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "column".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "function".to_string(),
                    ty: "str".to_string(),
                    visibility: Visibility::Public,
                },
            ],
            methods: vec![],
            generics: vec![],
        });

        // ErrorContext 别名
        aliases.insert("ErrorContext".to_string(), Type::NamedType {
            name: "Ctx".to_string(),
        });

        // 标准库类型（来自 TypeSpace）
        types.insert("Vec".to_string(), TypeDef::Class {
            name: "Vec".to_string(),
            fields: vec![
                ClassField {
                    name: "ptr".to_string(),
                    ty: "*T".to_string(),
                    visibility: Visibility::Private,
                },
                ClassField {
                    name: "len".to_string(),
                    ty: "usize".to_string(),
                    visibility: Visibility::Private,
                },
                ClassField {
                    name: "cap".to_string(),
                    ty: "usize".to_string(),
                    visibility: Visibility::Private,
                },
            ],
            methods: vec![
                MethodSignature {
                    name: "new".to_string(),
                    params: vec![],
                    return_type: Type::NamedType { name: "Vec".to_string() },
                    receiver: None,
                    generics: vec![],
                },
                MethodSignature {
                    name: "push".to_string(),
                    params: vec![Type::int()],  // Vec<int> for now
                    return_type: Type::unit(),
                    receiver: Some(ReceiverKind::Mutable),
                    generics: vec![],
                },
                MethodSignature {
                    name: "pop".to_string(),
                    params: vec![],
                    return_type: Type::NamedType { name: "Vec".to_string() },  // Simplified
                    receiver: Some(ReceiverKind::Mutable),
                    generics: vec![],
                },
                MethodSignature {
                    name: "get".to_string(),
                    params: vec![Type::Int { bits: 64, signed: false }],
                    return_type: Type::NamedType { name: "Vec".to_string() },  // Simplified
                    receiver: Some(ReceiverKind::Shared),
                    generics: vec![],
                },
            ],
            generics: vec![],  // No generics anymore
            parent: None,
        });

        // Coffee 内置错误处理类型
        types.insert("Error".to_string(), TypeDef::Struct {
            name: "Error".to_string(),
            fields: vec![
                ClassField {
                    name: "code".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "message".to_string(),
                    ty: "str".to_string(),
                    visibility: Visibility::Public,
                },
            ],
            methods: vec![],
            generics: vec![],
        });

        types.insert("ErrorContext".to_string(), TypeDef::Struct {
            name: "ErrorContext".to_string(),
            fields: vec![
                ClassField {
                    name: "line".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "column".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "file".to_string(),
                    ty: "str".to_string(),
                    visibility: Visibility::Public,
                },
            ],
            methods: vec![],
            generics: vec![],
        });
    }

    /// 注册类型定义
    pub fn define_type(&self, name: impl Into<String>, def: TypeDef) -> Result<(), TypeSystemError> {
        let name = name.into();

        if let Ok(mut types) = self.types.write() {
            if types.contains_key(&name) {
                return Err(TypeSystemError::Duplicate {
                    name,
                    existing: Span::new(0, 0),
                    new: Span::new(0, 0),
                });
            }
            types.insert(name, def);
            Ok(())
        } else {
            Err(TypeSystemError::NotFound {
                name: format!("type '{}'", name),
                kind: SpaceKind::Type,
            })
        }
    }

    /// 定义类型别名
    pub fn define_alias(&self, name: impl Into<String>, ty: Type) -> Result<(), TypeSystemError> {
        let name = name.into();

        if let Ok(mut aliases) = self.aliases.write() {
            if aliases.contains_key(&name) {
                return Err(TypeSystemError::Duplicate {
                    name,
                    existing: Span::new(0, 0),
                    new: Span::new(0, 0),
                });
            }
            aliases.insert(name, ty);
            Ok(())
        } else {
            Err(TypeSystemError::NotFound {
                name: format!("alias '{}'", name),
                kind: SpaceKind::Type,
            })
        }
    }

    /// 查找类型定义
    pub fn get_type(&self, name: &str) -> Option<TypeDef> {
        if let Ok(types) = self.types.read() {
            if let Some(def) = types.get(name) {
                return Some(def.clone());
            }
        }

        // 检查父注册器
        if let Some(ref parent) = self.parent {
            if let Ok(parent_lock) = parent.read() {
                return parent_lock.get_type(name);
            }
        }

        None
    }

    /// 查找类型别名
    pub fn get_alias(&self, name: &str) -> Option<Type> {
        if let Ok(aliases) = self.aliases.read() {
            if let Some(ty) = aliases.get(name) {
                return Some(ty.clone());
            }
        }

        // 检查父注册器
        if let Some(ref parent) = self.parent {
            if let Ok(parent_lock) = parent.read() {
                return parent_lock.get_alias(name);
            }
        }

        None
    }

    /// 解析类型字符串，支持泛型参数替换
    pub fn resolve_type(&self, type_str: &str) -> Result<Type, TypeSystemError> {
        self.resolve_type_with_params(type_str, &[])
    }

    /// 解析类型字符串，支持泛型参数替换（合并 BuiltinTypes 的功能）
    pub fn resolve_type_with_params(&self, type_str: &str, type_params: &[String]) -> Result<Type, TypeSystemError> {
        // 先检查别名
        if let Some(ty) = self.get_alias(type_str) {
            return Ok(self.substitute_type_params(ty, type_params));
        }

        // 再检查类型定义
        if self.get_type(type_str).is_some() {
            return Ok(self.substitute_type_params(Type::NamedType {
                name: type_str.to_string(),
            }, type_params));
        }

        // 尝试从字符串解析基础类型
        match super::definition::type_from_str(type_str) {
            Ok(ty) => {
                // 如果是命名类型，检查它是否存在于注册器中
                if let Type::NamedType { name } = &ty {
                    if self.get_type(name).is_none() {
                        return Err(TypeSystemError::UndefinedType {
                            name: name.clone(),
                            span: super::definition::Span::new(0, type_str.len()),
                        });
                    }
                }
                Ok(self.substitute_type_params(ty, type_params))
            }
            Err(e) => Err(TypeSystemError::ParseError {
                type_str: type_str.to_string(),
                reason: e,
            }),
        }
    }

    /// 替换类型参数（不再支持泛型，直接返回原类型）
    fn substitute_type_params(&self, ty: Type, _type_params: &[String]) -> Type {
        // 不再支持泛型，直接返回原类型
        ty
    }

    /// 添加泛型实例（已废弃，保留API兼容性）
    pub fn add_instance(&self, _name: impl Into<String>, _args: Vec<Type>) -> Result<(), TypeSystemError> {
        // 不再支持泛型，此函数已废弃
        Ok(())
    }

    /// 获取泛型的所有实例
    pub fn get_instances(&self, name: &str) -> Vec<Vec<Type>> {
        if let Ok(instances) = self.instances.read() {
            instances.get(name).cloned().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// 添加类型约束
    pub fn add_constraint(&self, name: impl Into<String>, constraint: TypeConstraint) -> Result<(), TypeSystemError> {
        let name = name.into();

        if let Ok(mut constraints) = self.constraints.write() {
            constraints.entry(name).or_insert_with(Vec::new).push(constraint);
            Ok(())
        } else {
            Err(TypeSystemError::NotFound {
                name: format!("constraint '{}'", name),
                kind: SpaceKind::Type,
            })
        }
    }

    /// 获取类型约束
    pub fn get_constraints(&self, name: &str) -> Vec<TypeConstraint> {
        if let Ok(constraints) = self.constraints.read() {
            constraints.get(name).cloned().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// 检查类型是否满足约束
    pub fn check_constraints(&self, name: &str, ty: &Type) -> Result<(), TypeSystemError> {
        let constraints = self.get_constraints(name);

        for constraint in constraints {
            match constraint {
                TypeConstraint::Trait(trait_name) => {
                    // 简化实现：检查类型是否实现了指定的trait
                    // 实际实现需要trait系统
                    if !self.implements_trait(ty, &trait_name) {
                        return Err(TypeSystemError::ConstraintViolation {
                            constraint: trait_name.clone(),
                            reason: format!("type {} does not implement trait {}", ty, trait_name),
                            span: Span::new(0, 0),
                        });
                    }
                }
                TypeConstraint::Equals(expected_type) => {
                    let expected = self.resolve_type(&expected_type)
                        .map_err(|_| TypeSystemError::NotFound {
                            name: expected_type.clone(),
                            kind: SpaceKind::Type,
                        })?;

                    if ty != &expected {
                        return Err(TypeSystemError::ConstraintViolation {
                            constraint: expected_type,
                            reason: format!("type {} does not equal expected type {}", ty, expected),
                            span: Span::new(0, 0),
                        });
                    }
                }
                TypeConstraint::All(sub_constraints) => {
                    for _sub in sub_constraints {
                        // 递归检查（简化实现）
                    }
                }
            }
        }

        Ok(())
    }

    /// 检查类型是否实现了指定trait（简化实现）
    fn implements_trait(&self, _ty: &Type, _trait_name: &str) -> bool {
        // 简化实现：假设所有类型都实现了基本trait
        true
    }

    /// 检查类型兼容性
    pub fn is_compatible(&self, ty1: &Type, ty2: &Type) -> bool {
        // 相同类型
        if ty1 == ty2 {
            return true;
        }

        // 检查隐式转换
        if ty1.can_coerce_from(ty2) {
            return true;
        }

        // 对于命名类型，检查继承关系
        match (ty1, ty2) {
            (Type::NamedType { name: n1, .. }, Type::NamedType { name: n2, .. }) => {
                self.is_subtype_of(n1, n2)
            }
            _ => false,
        }
    }

    /// 检查类型继承关系
    fn is_subtype_of(&self, child: &str, parent: &str) -> bool {
        if child == parent {
            return true;
        }

        if let Some(def) = self.get_type(child) {
            if let TypeDef::Class { parent: Some(p), .. } = def {
                return self.is_subtype_of(&p, parent);
            }
        }

        false
    }

    /// 获取类型的所有字段
    pub fn get_fields(&self, type_name: &str) -> Vec<ClassField> {
        match self.get_type(type_name) {
            Some(TypeDef::Class { fields, .. }) | Some(TypeDef::Struct { fields, .. }) => fields,
            _ => vec![],
        }
    }

    /// 获取类型的所有方法
    pub fn get_methods(&self, type_name: &str) -> Vec<MethodSignature> {
        match self.get_type(type_name) {
            Some(TypeDef::Class { methods, .. }) | Some(TypeDef::Struct { methods, .. }) => methods,
            _ => vec![],
        }
    }

    /// 获取枚举的所有变体
    pub fn get_variants(&self, type_name: &str) -> Vec<EnumVariant> {
        match self.get_type(type_name) {
            Some(TypeDef::Enum { variants, .. }) => variants,
            _ => vec![],
        }
    }

    /// 获取所有类型名称
    pub fn type_names(&self) -> Vec<String> {
        let mut names = Vec::new();

        if let Ok(types) = self.types.read() {
            names.extend(types.keys().cloned());
        }

        if let Ok(aliases) = self.aliases.read() {
            names.extend(aliases.keys().cloned());
        }

        names
    }

    /// 合并另一个类型注册器
    pub fn merge(&self, other: &TypeRegistry) -> Result<(), TypeSystemError> {
        // 合并类型定义
        if let Ok(mut types) = self.types.write() {
            if let Ok(other_types) = other.types.read() {
                for (name, def) in other_types.iter() {
                    if types.contains_key(name) {
                        return Err(TypeSystemError::Duplicate {
                            name: name.clone(),
                            existing: Span::new(0, 0),
                            new: Span::new(0, 0),
                        });
                    }
                    types.insert(name.clone(), def.clone());
                }
            }
        }

        // 合并别名
        if let Ok(mut aliases) = self.aliases.write() {
            if let Ok(other_aliases) = other.aliases.read() {
                for (name, ty) in other_aliases.iter() {
                    if aliases.contains_key(name) {
                        return Err(TypeSystemError::Duplicate {
                            name: name.clone(),
                            existing: Span::new(0, 0),
                            new: Span::new(0, 0),
                        });
                    }
                    aliases.insert(name.clone(), ty.clone());
                }
            }
        }

        Ok(())
    }

    /// 从 AST 的 VariableDecl 创建变量绑定（来自 TypeEnv）
    pub fn bind_variable_decl(&self, decl: &crate::parser::var::VariableDecl) -> Result<Type, TypeSystemError> {
        self.resolve_type(&decl.var_type)
    }

    /// 从 AST 的 Function 创建函数类型（来自 TypeEnv）
    pub fn bind_function(&self, func: &crate::parser::function::Function) -> Result<Type, TypeSystemError> {
        // 解析参数类型
        let mut param_types = Vec::new();
        for param in &func.parameters {
            let ty = self.resolve_type(&param.param_type)?;
            param_types.push(ty);
        }

        // 解析返回类型
        let return_type = self.resolve_type(&func.return_type)?;

        // 创建函数类型
        Ok(Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        })
    }

    /// 从 AST 的 ClassDef 创建类型定义（来自 TypeEnv）
    pub fn bind_class(&self, class: &crate::parser::class::ClassDef) -> Result<TypeDef, TypeSystemError> {
        // 解析字段类型
        let mut fields = Vec::new();
        for field in &class.fields {
            fields.push(ClassField {
                name: field.name.clone(),
                ty: field.field_type.clone(),
                visibility: Visibility::Public,
            });
        }

        // 解析方法签名
        let mut methods = Vec::new();
        for method in &class.methods {
            let mut param_types = Vec::new();
            // 添加 self 参数
            let self_type = Type::NamedType {
                name: class.name.clone(),
            };
            param_types.push(self_type);

            for param in &method.parameters {
                // 如果参数类型是类本身，使用 NamedType 避免循环依赖
                let ty = if param.param_type == class.name {
                    Type::NamedType {
                        name: class.name.clone(),
                    }
                } else {
                    self.resolve_type(&param.param_type)?
                };
                param_types.push(ty);
            }

            // 解析返回类型，但如果返回类型是类本身，使用 NamedType 避免循环依赖
            let return_type = if method.return_type == class.name {
                Type::NamedType {
                    name: class.name.clone(),
                }
            } else {
                self.resolve_type(&method.return_type)?
            };

            methods.push(MethodSignature {
                name: method.name.clone(),
                params: param_types,
                return_type,
                receiver: Some(ReceiverKind::Shared), // 默认为共享引用
                generics: vec![],
            });
        }

        Ok(TypeDef::Class {
            name: class.name.clone(),
            fields,
            methods,
            generics: vec![],  // No generics anymore
            parent: None,
        })
    }

    /// 从 AST 的 EnumDef 创建类型定义（来自 TypeEnv）
    pub fn bind_enum(&self, enum_def: &crate::parser::class::EnumDef) -> Result<TypeDef, TypeSystemError> {
        // Enum 的变体类型
        let mut variants = Vec::new();
        for variant in &enum_def.variants {
            let fields = match &variant.fields[..] {
                [] => vec![],
                [field] => {
                    // 单个字段的变体
                    let ty_str = match field {
                        crate::parser::class::VariantField::Type(t) => t.clone(),
                        crate::parser::class::VariantField::Named { field_type, .. } => field_type.clone(),
                    };
                    vec![VariantField::Positional(ty_str)]
                }
                _ => {
                    // 多字段变体
                    variant.fields.iter().map(|field| {
                        match field {
                            crate::parser::class::VariantField::Type(t) => VariantField::Positional(t.clone()),
                            crate::parser::class::VariantField::Named { name, field_type } => {
                                VariantField::Named { name: name.clone(), ty: field_type.clone() }
                            },
                        }
                    }).collect()
                }
            };

            variants.push(EnumVariant {
                name: variant.name.clone(),
                fields,
            });
        }

        Ok(TypeDef::Enum {
            name: enum_def.name.clone(),
            variants,
            generics: vec![],  // No generics anymore
        })
    }

    /// 获取空间ID
    pub fn id(&self) -> SpaceId {
        self.id
    }

    /// 获取空间名称
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Clone for TypeRegistry {
    fn clone(&self) -> Self {
        TypeRegistry {
            id: self.id,
            name: self.name.clone(),
            parent: self.parent.clone(),
            types: Arc::clone(&self.types),
            aliases: Arc::clone(&self.aliases),
            instances: Arc::clone(&self.instances),
            constraints: Arc::clone(&self.constraints),
        }
    }
}

impl Space for TypeRegistry {
    fn id(&self) -> SpaceId {
        self.id
    }

    fn kind(&self) -> SpaceKind {
        SpaceKind::Type
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn parent(&self) -> Option<&dyn Space> {
        None
    }

    fn lookup(&self, _name: &str) -> Option<&Binding> {
        None
    }

    fn bindings(&self) -> Vec<String> {
        self.type_names()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_types() {
        let registry = TypeRegistry::root();

        // 检查内置类型别名
        assert!(registry.get_alias("int").is_some());
        assert!(registry.get_alias("bool").is_some());
        assert!(registry.get_alias("str").is_some());

        // 检查标准库类型
        assert!(registry.get_type("Vec").is_some());
        // Note: Option and Result are not built-in types, they are user-defined enums
    }

    #[test]
    fn test_define_type() {
        let registry = TypeRegistry::new("test");

        let def = TypeDef::Class {
            name: "Point".to_string(),
            fields: vec![
                ClassField {
                    name: "x".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                }
            ],
            methods: vec![],
            generics: vec![],
            parent: None,
        };

        registry.define_type("Point", def).unwrap();
        assert!(registry.get_type("Point").is_some());
    }

    #[test]
    fn test_type_resolution() {
        let registry = TypeRegistry::root();

        // 内置类型应该可以解析
        assert!(registry.resolve_type("int").is_ok());
        assert!(registry.resolve_type("bool").is_ok());

        // 未知类型应该返回错误
        assert!(registry.resolve_type("NonExistent").is_err());
    }

    #[test]
    fn test_type_compatibility() {
        let registry = TypeRegistry::root();

        let i32 = Type::i32();
        let i32_copy = Type::i32();

        // 相同类型兼容
        assert!(registry.is_compatible(&i32, &i32_copy));

        // 可以隐式转换的类型（安全转换）
        let f64 = Type::f64();
        let f32 = Type::from_str("f32").unwrap();
        assert!(registry.is_compatible(&f64, &f32)); // f64可以接受f32（安全提升）

        // 注意：int 到 float 的隐式转换被禁用以防止精度损失
        // 这需要显式类型转换
    }
}