use super::definition::*;
use super::errors::TypeSystemError;
use super::mono;
use crate::coffee_debug;
use crate::parser::class::ClassDef;
use crate::parser::function::Function;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

/// Kind of a C record registered from `.cfc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CLayoutKind {
    Class,
    Union,
    Enum,
}

/// Clang size/align for a `.cfc` value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CLayout {
    pub size: u64,
    pub align: u64,
    pub kind: CLayoutKind,
}

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
    /// 类型定义
    types: Arc<RwLock<HashMap<String, TypeDef>>>,
    /// 类型别名
    aliases: Arc<RwLock<HashMap<String, Type>>>,
    class_params: Arc<RwLock<HashMap<String, Vec<String>>>>,
    fn_params: Arc<RwLock<HashMap<String, Vec<String>>>>,
    class_templates: Arc<RwLock<HashMap<String, ClassDef>>>,
    fn_templates: Arc<RwLock<HashMap<String, Function>>>,
    instantiated_classes: Arc<RwLock<Vec<ClassDef>>>,
    instantiated_fns: Arc<RwLock<Vec<Function>>>,
    /// concrete class name → template name
    instance_of: Arc<RwLock<HashMap<String, String>>>,
    /// Nominal `type Name: T` brands (not aliases).
    pub(crate) newtypes: Arc<RwLock<HashMap<String, Type>>>,
    /// `.cfc` `c class` / `c union` / `c enum` layouts (memcpy value types).
    pub(crate) c_layouts: Arc<RwLock<HashMap<String, CLayout>>>,
}

impl TypeRegistry {
    /// 创建新的类型注册器
    #[cfg(test)]
    pub fn new(name: impl Into<String>) -> Self {
        TypeRegistry {
            id: SpaceId::new(),
            name: name.into(),
            types: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            class_params: Arc::new(RwLock::new(HashMap::new())),
            fn_params: Arc::new(RwLock::new(HashMap::new())),
            class_templates: Arc::new(RwLock::new(HashMap::new())),
            fn_templates: Arc::new(RwLock::new(HashMap::new())),
            instantiated_classes: Arc::new(RwLock::new(Vec::new())),
            instantiated_fns: Arc::new(RwLock::new(Vec::new())),
            instance_of: Arc::new(RwLock::new(HashMap::new())),
            newtypes: Arc::new(RwLock::new(HashMap::new())),
            c_layouts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建根类型注册器（带内置类型）
    pub fn root() -> Self {
        let mut registry = TypeRegistry {
            id: SpaceId::new(),
            name: "builtin".to_string(),
            types: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            class_params: Arc::new(RwLock::new(HashMap::new())),
            fn_params: Arc::new(RwLock::new(HashMap::new())),
            class_templates: Arc::new(RwLock::new(HashMap::new())),
            fn_templates: Arc::new(RwLock::new(HashMap::new())),
            instantiated_classes: Arc::new(RwLock::new(Vec::new())),
            instantiated_fns: Arc::new(RwLock::new(Vec::new())),
            instance_of: Arc::new(RwLock::new(HashMap::new())),
            newtypes: Arc::new(RwLock::new(HashMap::new())),
            c_layouts: Arc::new(RwLock::new(HashMap::new())),
        };

        // 注册内置类型
        registry.register_builtin_types();
        registry
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
        aliases.insert("buf".to_string(), Type::buf());
        aliases.insert("void".to_string(), Type::void());
        aliases.insert("()".to_string(), Type::unit());

        types.insert("Error".to_string(), Self::builtin_error_def());
    }

    /// 注册类型定义
    pub fn define_type(&self, name: impl Into<String>, def: TypeDef) -> Result<(), TypeSystemError> {
        let name = name.into();

        coffee_debug!("define_type: {} ({})", name, def.name());
        if let Ok(mut types) = self.types.write() {
            if types.contains_key(&name) {
                let span = TypeSystemError::span_for_name(&name);
                return Err(TypeSystemError::Duplicate {
                    name,
                    existing: span,
                    new: span,
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
                let span = TypeSystemError::span_for_name(&name);
                return Err(TypeSystemError::Duplicate {
                    name,
                    existing: span,
                    new: span,
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

        None
    }

    /// 查找类型别名
    pub fn get_alias(&self, name: &str) -> Option<Type> {
        if let Ok(aliases) = self.aliases.read() {
            if let Some(ty) = aliases.get(name) {
                return Some(ty.clone());
            }
        }

        None
    }

    /// 解析类型字符串
    pub fn resolve_type(&self, type_str: &str) -> Result<Type, TypeSystemError> {
        if type_str == "Error" {
            self.ensure_error_type();
        }
        if let Some(ty) = self.get_alias(type_str) {
            return Ok(ty);
        }

        if self.get_type(type_str).is_some() {
            if self.is_generic_class(type_str) {
                return Err(TypeSystemError::InstantiationError {
                    type_name: type_str.to_string(),
                    args: vec![],
                    reason: format!("type '{type_str}' used without type arguments"),
                    span: TypeSystemError::span_for_name(type_str),
                });
            }
            return Ok(Type::NamedType {
                name: type_str.to_string(),
            });
        }

        if self.is_newtype(type_str) {
            return Ok(Type::NamedType {
                name: type_str.to_string(),
            });
        }

        match super::definition::type_from_str(type_str) {
            Ok(ty) => self.normalize_type(ty, type_str),
            Err(e) => Err(TypeSystemError::ParseError {
                type_str: type_str.to_string(),
                reason: e,
            }),
        }
    }

    fn normalize_type(&self, ty: Type, origin: &str) -> Result<Type, TypeSystemError> {
        match ty {
            Type::App { name, args } => {
                let mut norm_args = Vec::with_capacity(args.len());
                for a in args {
                    norm_args.push(self.normalize_type(a, origin)?);
                }
                self.instantiate_class_app(&name, &norm_args)
            }
            Type::NamedType { name } => {
                if self.is_generic_class(&name) {
                    return Err(TypeSystemError::InstantiationError {
                        type_name: name.clone(),
                        args: vec![],
                        reason: format!("type '{name}' used without type arguments"),
                        span: TypeSystemError::span_for_name(&name),
                    });
                }
                if self.get_type(&name).is_none()
                    && self.get_alias(&name).is_none()
                    && !self.is_newtype(&name)
                {
                    return Err(TypeSystemError::undefined_type(
                        name,
                        super::definition::Span::new(0, origin.len()),
                    ));
                }
                Ok(Type::NamedType { name })
            }
            Type::Ref { elem, mutable } => Ok(Type::Ref {
                elem: Box::new(self.normalize_type(*elem, origin)?),
                mutable,
            }),
            Type::Array { elem, size } => Ok(Type::Array {
                elem: Box::new(self.normalize_type(*elem, origin)?),
                size,
            }),
            Type::Slice(elem) => Ok(Type::Slice(Box::new(self.normalize_type(*elem, origin)?))),
            Type::Tuple(elems) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems {
                    out.push(self.normalize_type(e, origin)?);
                }
                Ok(Type::Tuple(out))
            }
            Type::Function {
                params,
                return_type,
            } => {
                let mut ps = Vec::with_capacity(params.len());
                for p in params {
                    ps.push(self.normalize_type(p, origin)?);
                }
                Ok(Type::Function {
                    params: ps,
                    return_type: Box::new(self.normalize_type(*return_type, origin)?),
                })
            }
            other => Ok(other),
        }
    }

    pub fn install_header_generics(&self, headers: &mono::HeaderGenerics) {
        if let Ok(mut m) = self.class_params.write() {
            for (k, v) in &headers.classes {
                m.insert(k.clone(), v.clone());
            }
        }
        if let Ok(mut m) = self.fn_params.write() {
            for (k, v) in &headers.functions {
                m.insert(k.clone(), v.clone());
            }
        }
    }

    pub fn register_class_template(&self, class: &ClassDef) {
        let mut params = mono::ast_class_type_params(class);
        if params.is_empty() {
            if let Ok(m) = self.class_params.read() {
                if let Some(p) = m.get(&class.name) {
                    params = p.clone();
                }
            }
        } else if let Ok(mut m) = self.class_params.write() {
            m.insert(class.name.clone(), params.clone());
        }
        if !params.is_empty() {
            if let Ok(mut t) = self.class_templates.write() {
                t.insert(class.name.clone(), class.clone());
            }
        }
    }

    pub fn register_fn_template(&self, func: &Function) {
        let mut params = mono::ast_fn_type_params(func);
        if params.is_empty() {
            if let Ok(m) = self.fn_params.read() {
                if let Some(p) = m.get(&func.name) {
                    params = p.clone();
                }
            }
        } else if let Ok(mut m) = self.fn_params.write() {
            m.insert(func.name.clone(), params.clone());
        }
        if !params.is_empty() {
            if let Ok(mut t) = self.fn_templates.write() {
                t.insert(func.name.clone(), func.clone());
            }
        }
    }

    pub fn class_type_params(&self, name: &str) -> Vec<String> {
        self.class_params
            .read()
            .ok()
            .and_then(|m| m.get(name).cloned())
            .unwrap_or_default()
    }

    pub fn fn_type_params(&self, name: &str) -> Vec<String> {
        self.fn_params
            .read()
            .ok()
            .and_then(|m| m.get(name).cloned())
            .unwrap_or_default()
    }

    pub fn is_generic_class(&self, name: &str) -> bool {
        !self.class_type_params(name).is_empty()
    }

    pub fn is_generic_fn(&self, name: &str) -> bool {
        !self.fn_type_params(name).is_empty()
    }

    pub fn template_name_of(&self, concrete: &str) -> Option<String> {
        self.instance_of
            .read()
            .ok()
            .and_then(|m| m.get(concrete).cloned())
    }

    pub fn take_instantiated_classes(&self) -> Vec<ClassDef> {
        self.instantiated_classes
            .write()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default()
    }

    pub fn cloned_instantiated_classes(&self) -> Vec<ClassDef> {
        self.instantiated_classes
            .read()
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    pub fn take_instantiated_fns(&self) -> Vec<Function> {
        self.instantiated_fns
            .write()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default()
    }

    pub fn cloned_instantiated_fns(&self) -> Vec<Function> {
        self.instantiated_fns
            .read()
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    pub fn generic_class_names(&self) -> Vec<String> {
        self.class_params
            .read()
            .ok()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn generic_fn_names(&self) -> Vec<String> {
        self.fn_params
            .read()
            .ok()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn instantiate_class_app(&self, name: &str, args: &[Type]) -> Result<Type, TypeSystemError> {
        let params = self.class_type_params(name);
        if params.is_empty() {
            if self.get_type(name).is_some() {
                return Err(TypeSystemError::InstantiationError {
                    type_name: name.to_string(),
                    args: args.to_vec(),
                    reason: format!("type '{name}' is not generic"),
                    span: TypeSystemError::span_for_name(name),
                });
            }
            return Err(TypeSystemError::undefined_type(
                name.to_string(),
                TypeSystemError::span_for_name(name),
            ));
        }
        if params.len() != args.len() {
            return Err(TypeSystemError::GenericArgCountMismatch {
                type_name: name.to_string(),
                expected: params.len(),
                found: args.len(),
                span: TypeSystemError::span_for_name(name),
            });
        }
        let concrete = Type::App {
            name: name.to_string(),
            args: args.to_vec(),
        }
        .mono_name();
        if self.get_type(&concrete).is_some() {
            return Ok(Type::NamedType { name: concrete });
        }
        let template = self
            .class_templates
            .read()
            .ok()
            .and_then(|m| m.get(name).cloned())
            .ok_or_else(|| TypeSystemError::undefined_type(
                name.to_string(),
                TypeSystemError::span_for_name(name),
            ))?;
        let inst = mono::instantiate_class(&template, &params, args).map_err(|reason| {
            TypeSystemError::InstantiationError {
                type_name: name.to_string(),
                args: args.to_vec(),
                reason,
                span: TypeSystemError::span_for_name(name),
            }
        })?;
        let type_def = self.bind_class(&inst)?;
        self.define_type(inst.name.clone(), type_def)?;
        if let Ok(mut m) = self.instance_of.write() {
            m.insert(inst.name.clone(), name.to_string());
        }
        if let Ok(mut v) = self.instantiated_classes.write() {
            if !v.iter().any(|c| c.name == inst.name) {
                v.push(inst);
            }
        }
        Ok(Type::NamedType { name: concrete })
    }

    pub fn instantiate_generic_fn(
        &self,
        name: &str,
        args: &[Type],
    ) -> Result<Type, TypeSystemError> {
        let params = self.fn_type_params(name);
        if params.is_empty() {
            return Err(TypeSystemError::undefined_function(
                name,
                TypeSystemError::span_for_name(name),
            ));
        }
        if params.len() != args.len() {
            return Err(TypeSystemError::GenericArgCountMismatch {
                type_name: name.to_string(),
                expected: params.len(),
                found: args.len(),
                span: TypeSystemError::span_for_name(name),
            });
        }
        let template = self
            .fn_templates
            .read()
            .ok()
            .and_then(|m| m.get(name).cloned())
            .ok_or_else(|| {
                TypeSystemError::undefined_function(name, TypeSystemError::span_for_name(name))
            })?;
        if let Some(existing) = self.get_alias(name) {
            return Ok(existing);
        }
        let inst = mono::instantiate_function(&template, &params, args).map_err(|reason| {
            TypeSystemError::InstantiationError {
                type_name: name.to_string(),
                args: args.to_vec(),
                reason,
                span: TypeSystemError::span_for_name(name),
            }
        })?;
        let ty = self.bind_function(&inst)?;
        if self.get_alias(name).is_none() {
            self.define_alias(name.to_string(), ty.clone())?;
        }
        if let Ok(mut v) = self.instantiated_fns.write() {
            if !v.iter().any(|f| f.name == inst.name) {
                v.push(inst);
            }
        }
        Ok(ty)
    }

    pub fn generic_fn_value_arity(&self, name: &str) -> Option<usize> {
        self.fn_templates
            .read()
            .ok()
            .and_then(|m| m.get(name).map(|f| f.parameters.len()))
    }

    /// Rewrite `Box { }` → `Box__int` and splice concrete class/fn ASTs into `program`.
    pub fn inject_monomorphized(&self, program: &mut crate::parser::Program) {
        mono::rewrite_program(program, self);
        let classes = self.take_instantiated_classes();
        let fns = self.take_instantiated_fns();
        let mut statements = Vec::with_capacity(program.statements.len() + classes.len() + fns.len());
        let mut spans = Vec::with_capacity(program.stmt_spans.len() + classes.len() + fns.len());
        for (stmt, span) in program
            .statements
            .drain(..)
            .zip(program.stmt_spans.drain(..))
        {
            match &stmt {
                crate::parser::Statement::Class(c) if !c.type_params.is_empty() => continue,
                crate::parser::Statement::Function(f) if !f.type_params.is_empty() => continue,
                _ => {
                    statements.push(stmt);
                    spans.push(span);
                }
            }
        }
        for class in classes {
            statements.push(crate::parser::Statement::Class(class));
            spans.push(crate::types::definition::Span::new(0, 0));
        }
        for func in fns {
            statements.push(crate::parser::Statement::Function(func));
            spans.push(crate::types::definition::Span::new(0, 0));
        }
        program.statements = statements;
        program.stmt_spans = spans;
    }

    pub fn infer_fn_instance(&self, name: &str, arg_types: &[Type]) -> Result<Type, TypeSystemError> {
        let params = self.fn_type_params(name);
        let template = self
            .fn_templates
            .read()
            .ok()
            .and_then(|m| m.get(name).cloned());
        let Some(func) = template else {
            return Err(TypeSystemError::undefined_function(
                name,
                TypeSystemError::span_for_name(name),
            ));
        };
        let param_strs: Vec<String> = func.parameters.iter().map(|p| p.param_type.clone()).collect();
        let args = mono::infer_generic_args(&params, &param_strs, arg_types).map_err(|reason| {
            TypeSystemError::InstantiationError {
                type_name: name.to_string(),
                args: vec![],
                reason,
                span: TypeSystemError::span_for_name(name),
            }
        })?;
        self.instantiate_generic_fn(name, &args)
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

    /// Builtin `Error` or a class whose `of` chain reaches it.
    pub fn is_exception_type(&self, name: &str) -> bool {
        self.is_subtype_of(name, "Error")
    }

    /// Layout order: ancestor fields (root first), then this class's own fields.
    /// Missing or non-class parents, and parent cycles, are errors (not a partial name list).
    pub fn class_field_names_including_ancestors(
        &self,
        class_name: &str,
    ) -> Result<Vec<String>, TypeSystemError> {
        match self.get_type(class_name) {
            Some(TypeDef::Class { .. }) => {}
            None => {
                return Err(TypeSystemError::undefined_type(
                    class_name.to_string(),
                    TypeSystemError::span_for_name(class_name),
                ));
            }
            Some(_) => {
                return Err(TypeSystemError::internal(format!(
                    "expected class `{class_name}` for ancestor field walk"
                )));
            }
        }

        let mut layers: Vec<Vec<String>> = Vec::new();
        let mut current = Some(class_name.to_string());
        let mut seen = HashSet::new();
        while let Some(name) = current {
            if !seen.insert(name.clone()) {
                return Err(TypeSystemError::internal(format!(
                    "cyclic class parent chain at `{name}`"
                )));
            }
            match self.get_type(&name) {
                Some(TypeDef::Class { fields, parent, .. }) => {
                    layers.push(fields.iter().map(|f| f.name.clone()).collect());
                    current = parent;
                }
                None => {
                    return Err(TypeSystemError::undefined_type(
                        name.clone(),
                        TypeSystemError::span_for_name(&name),
                    ));
                }
                Some(_) => {
                    return Err(TypeSystemError::internal(format!(
                        "parent `{name}` is not a class"
                    )));
                }
            }
        }
        layers.reverse();
        Ok(layers.into_iter().flatten().collect())
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

    /// Builtin `Error { code: int, note: str, e: object }` for `#on_err` listeners.
    pub fn builtin_error_def() -> TypeDef {
        TypeDef::Class {
            name: "Error".to_string(),
            fields: vec![
                ClassField {
                    name: "code".to_string(),
                    ty: "int".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "note".to_string(),
                    ty: "str".to_string(),
                    visibility: Visibility::Public,
                },
                ClassField {
                    name: "e".to_string(),
                    ty: "object".to_string(),
                    visibility: Visibility::Public,
                },
            ],
            methods: vec![],
            generics: vec![],
            parent: None,
        }
    }

    /// Register builtin `Error` when it is missing (empty / test registries).
    pub fn ensure_error_type(&self) {
        if self.get_type("Error").is_some() {
            return;
        }
        if let Ok(mut types) = self.types.write() {
            types
                .entry("Error".to_string())
                .or_insert_with(Self::builtin_error_def);
        }
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
        if class.name == "Error" {
            return Err(TypeSystemError::duplicate(
                "Error",
                Span::new(0, 5),
                Span::new(0, 5),
            ));
        }
        if class.name == "buf" {
            return Err(TypeSystemError::duplicate(
                "buf",
                Span::new(0, 3),
                Span::new(0, 3),
            ));
        }

        let type_params = {
            let mut p = mono::ast_class_type_params(class);
            if p.is_empty() {
                p = self.class_type_params(&class.name);
            }
            p
        };

        // 解析字段类型
        let mut fields = Vec::new();
        for field in &class.fields {
            if !type_params.is_empty() {
                if let Ok(ty) = super::definition::type_from_str(&field.field_type) {
                    if ty.mentions_type_param(&type_params) {
                        fields.push(ClassField {
                            name: field.name.clone(),
                            ty: field.field_type.clone(),
                            visibility: Visibility::Public,
                        });
                        continue;
                    }
                }
            }
            fields.push(ClassField {
                name: field.name.clone(),
                ty: field.field_type.clone(),
                visibility: Visibility::Public,
            });
        }

        // 解析方法签名
        let mut methods = Vec::new();
        if type_params.is_empty() {
        for method in &class.methods {
            let mut param_types = Vec::new();
            let has_self = method.parameters.iter().any(|p| p.name == "self");
            if has_self {
                param_types.push(Type::NamedType {
                    name: class.name.clone(),
                });
            }

            for param in &method.parameters {
                if param.name == "self" {
                    continue;
                }
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
        }

        Ok(TypeDef::Class {
            name: class.name.clone(),
            fields,
            methods,
            generics: type_params,
            parent: class.parent.clone(),
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
}

impl Clone for TypeRegistry {
    fn clone(&self) -> Self {
        TypeRegistry {
            id: self.id,
            name: self.name.clone(),
            types: Arc::clone(&self.types),
            aliases: Arc::clone(&self.aliases),
            class_params: Arc::clone(&self.class_params),
            fn_params: Arc::clone(&self.fn_params),
            class_templates: Arc::clone(&self.class_templates),
            fn_templates: Arc::clone(&self.fn_templates),
            instantiated_classes: Arc::clone(&self.instantiated_classes),
            instantiated_fns: Arc::clone(&self.instantiated_fns),
            instance_of: Arc::clone(&self.instance_of),
            newtypes: Arc::clone(&self.newtypes),
            c_layouts: Arc::clone(&self.c_layouts),
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

        let error_ty = registry.get_type("Error").expect("builtin Error");
        match error_ty {
            TypeDef::Class {
                parent,
                fields,
                methods,
                generics,
                ..
            } => {
                assert!(parent.is_none());
                assert!(methods.is_empty());
                assert!(generics.is_empty());
                let names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
                assert_eq!(names, vec!["code", "note", "e"]);
                assert_eq!(fields[0].ty, "int");
                assert_eq!(fields[1].ty, "str");
                assert_eq!(fields[2].ty, "object");
            }
            other => panic!("Error should be a class, got {:?}", other),
        }
        assert!(registry.resolve_type("Error").is_ok());
    }

    fn class_named(name: &str, parent: Option<&str>) -> crate::parser::class::ClassDef {
        crate::parser::class::ClassDef {
            type_params: vec![],
            name: name.to_string(),
            parent: parent.map(str::to_string),
            fields: vec![],
            methods: vec![],
            packed: false,
            has_constructor: false,
        }
    }

    #[test]
    fn test_bind_class_rejects_user_error() {
        let registry = TypeRegistry::root();
        let err = registry
            .bind_class(&class_named("Error", None))
            .expect_err("user class Error must not bind");
        assert!(
            err.to_string().contains("Error"),
            "unexpected error: {err}"
        );
        match registry.get_type("Error").expect("builtin remains") {
            TypeDef::Class { name, parent: None, .. } => assert_eq!(name, "Error"),
            other => panic!("builtin Error overwritten: {:?}", other),
        }
    }

    #[test]
    fn test_is_exception_type_walks_class_parents() {
        let registry = TypeRegistry::root();
        assert!(registry.is_exception_type("Error"));
        assert!(!registry.is_exception_type("Point"));
        assert!(!registry.is_exception_type("FooError"));

        registry
            .define_type(
                "E",
                TypeDef::Class {
                    name: "E".to_string(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: Some("Error".to_string()),
                },
            )
            .unwrap();
        registry
            .define_type(
                "F",
                TypeDef::Class {
                    name: "F".to_string(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: Some("E".to_string()),
                },
            )
            .unwrap();
        registry
            .define_type(
                "FooError",
                TypeDef::Class {
                    name: "FooError".to_string(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: None,
                },
            )
            .unwrap();
        assert!(registry.is_exception_type("E"));
        assert!(registry.is_exception_type("F"));
        assert!(!registry.is_exception_type("FooError"));
    }

    #[test]
    fn test_ensure_error_type_on_empty_registry() {
        let registry = TypeRegistry::new("test");
        assert!(registry.get_type("Error").is_none());
        assert!(registry.resolve_type("Error").is_ok());
        assert!(registry.get_type("Error").is_some());
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

    #[test]
    fn duplicate_type_span_covers_name() {
        let registry = TypeRegistry::root();
        let err = registry
            .define_type("Error", TypeRegistry::builtin_error_def())
            .expect_err("Error is already registered");
        match err {
            TypeSystemError::Duplicate { name, existing, new } => {
                assert_eq!(name, "Error");
                assert_eq!(existing, TypeSystemError::span_for_name("Error"));
                assert_eq!(new, TypeSystemError::span_for_name("Error"));
            }
            other => panic!("expected Duplicate, got {other:?}"),
        }
    }
}