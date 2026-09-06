use std::fmt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 类型定义
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// 基础类型
    /// int(4)+ = i32, int(1)- = u8, int = i64 (8-byte default)
    Int {
        bits: u8,
        signed: bool,
    },
    /// 浮点类型
    /// float(8) = f64, float = f64 (默认)
    Float {
        bits: u8,
    },
    /// 布尔类型
    Bool,
    /// 字符串类型
    String,
    /// 单元类型 (空元组)
    Unit,
    /// void 类型 (仅用于返回值)
    Void,
    /// 元组类型 (T1, T2, ...)
    Tuple(Vec<Type>),
    /// 数组类型 [T; N]
    Array {
        elem: Box<Type>,
        size: usize,
    },
    /// 切片/向量类型 [T]
    Slice(Box<Type>),
    /// 指针类型 &T 或 &mut T
    Ref {
        elem: Box<Type>,
        mutable: bool,
    },
    /// 函数类型 (params) -> return_type
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    /// 用户定义类型 (class/enum)
    NamedType {
        name: String,
    },
    /// Generic application `List<int>` / `Map<str, int>` (checker monomorphizes).
    App {
        name: String,
        args: Vec<Type>,
    },
    /// Opaque C pointer (`object`). Not C varargs (`...` / trailing `args: object`).
    Variadic,
}

impl Type {
    /// 创建默认 int 类型 (i64)
    pub fn int() -> Self {
        Type::Int { bits: 64, signed: true }
    }

    /// 创建 u8 类型
    pub fn u8() -> Self {
        Type::Int { bits: 8, signed: false }
    }

    /// 创建 i32 类型
    pub fn i32() -> Self {
        Type::Int { bits: 32, signed: true }
    }

    /// 创建默认 float 类型 (f64)
    pub fn float() -> Self {
        Type::Float { bits: 64 }
    }

    /// 创建 f64 类型
    pub fn f64() -> Self {
        Type::Float { bits: 64 }
    }

    /// 创建 bool 类型
    pub fn bool() -> Self {
        Type::Bool
    }

    /// 创建 string 类型
    pub fn string() -> Self {
        Type::String
    }

    /// 创建 void 类型
    pub fn void() -> Self {
        Type::Void
    }

    /// 创建单元类型
    pub fn unit() -> Self {
        Type::Unit
    }

    /// Builtin owned malloc pointer (`buf`). LLVM opaque ptr; drop calls `free`.
    pub fn buf() -> Self {
        Type::NamedType {
            name: "buf".to_string(),
        }
    }

    /// Coffee-owned malloc buffer, not a C handle (`object`).
    pub fn is_buf(&self) -> bool {
        matches!(self, Type::NamedType { name } if name == "buf")
    }

    /// `object` / `buf` byte offset via `p + n` (LLVM GEP, not ptrtoint).
    pub fn is_ptr_offset_base(&self) -> bool {
        self.is_buf() || matches!(self, Type::Variadic)
    }

    /// 创建可变引用类型 &mut T
    pub fn make_ref_mut(elem: Type) -> Self {
        Type::Ref {
            elem: Box::new(elem),
            mutable: true,
        }
    }

    /// 检查是否是整数类型
    pub fn is_int(&self) -> bool {
        matches!(self, Type::Int { .. })
    }

    /// 检查是否是浮点类型
    pub fn is_float(&self) -> bool {
        matches!(self, Type::Float { .. })
    }

    /// 检查是否是数值类型
    pub fn is_numeric(&self) -> bool {
        self.is_int() || self.is_float()
    }

    /// Pointer-sized integer (`int` / `int(8)±`), used as a C handle alongside `object`.
    pub fn is_pointer_sized_int(&self) -> bool {
        matches!(self, Type::Int { bits, .. } if *bits >= 64)
    }

    /// `object` (`Type::Variadic`) as a C pointer: ints, strings, `buf`, named types, refs.
    /// Does not include `bool` (object must not coerce to/from bool).
    pub fn is_c_handle_value(&self) -> bool {
        match self {
            Type::Int { .. } | Type::String | Type::NamedType { .. } | Type::App { .. }
            | Type::Ref { .. } | Type::Variadic => true,
            _ => false,
        }
    }

    /// Heap/resource types: assignment copies are forbidden (`mv` / `clone` required).
    /// Includes builtin `buf` (`NamedType { name: "buf" }`).
    pub fn is_resource(&self) -> bool {
        match self {
            Type::String | Type::Slice(_) | Type::Variadic | Type::Ref { .. }
            | Type::NamedType { .. } | Type::App { .. } => {
                true
            }
            Type::Array { elem, .. } => elem.is_resource(),
            Type::Tuple(elems) => elems.iter().any(Type::is_resource),
            _ => false,
        }
    }

    /// Mangled concrete name: `List<int>` → `List__int`.
    pub fn mono_name(&self) -> String {
        match self {
            Type::App { name, args } => {
                let mut out = sanitize_mono_segment(name);
                for arg in args {
                    out.push_str("__");
                    out.push_str(&arg.mono_name());
                }
                out
            }
            Type::NamedType { name } => sanitize_mono_segment(name),
            other => sanitize_mono_segment(&other.to_string()),
        }
    }

    /// Substitute type-parameter names (`T`) with concrete types. Names match as
    /// whole `NamedType` / `App` heads, never as a substring of `Other`.
    pub fn subst_params(&self, map: &HashMap<String, Type>) -> Type {
        match self {
            Type::NamedType { name } => map.get(name).cloned().unwrap_or_else(|| self.clone()),
            Type::App { name, args } => {
                let args = args.iter().map(|a| a.subst_params(map)).collect();
                if let Some(replaced) = map.get(name) {
                    match replaced {
                        Type::NamedType { name: n } => Type::App {
                            name: n.clone(),
                            args,
                        },
                        other if args.is_empty() => other.clone(),
                        other => other.clone(),
                    }
                } else {
                    Type::App {
                        name: name.clone(),
                        args,
                    }
                }
            }
            Type::Ref { elem, mutable } => Type::Ref {
                elem: Box::new(elem.subst_params(map)),
                mutable: *mutable,
            },
            Type::Array { elem, size } => Type::Array {
                elem: Box::new(elem.subst_params(map)),
                size: *size,
            },
            Type::Slice(elem) => Type::Slice(Box::new(elem.subst_params(map))),
            Type::Tuple(elems) => Type::Tuple(elems.iter().map(|e| e.subst_params(map)).collect()),
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|p| p.subst_params(map)).collect(),
                return_type: Box::new(return_type.subst_params(map)),
            },
            other => other.clone(),
        }
    }

    pub fn mentions_type_param(&self, params: &[String]) -> bool {
        match self {
            Type::NamedType { name } => params.iter().any(|p| p == name),
            Type::App { name, args } => {
                params.iter().any(|p| p == name) || args.iter().any(|a| a.mentions_type_param(params))
            }
            Type::Ref { elem, .. } | Type::Array { elem, .. } | Type::Slice(elem) => {
                elem.mentions_type_param(params)
            }
            Type::Tuple(elems) => elems.iter().any(|e| e.mentions_type_param(params)),
            Type::Function {
                params: fps,
                return_type,
            } => {
                fps.iter().any(|p| p.mentions_type_param(params))
                    || return_type.mentions_type_param(params)
            }
            _ => false,
        }
    }

    /// 检查类型是否可以被隐式转换
    pub fn can_coerce_from(&self, from: &Type) -> bool {
        match (self, from) {
            // 相同类型
            (a, b) if a == b => true,
            // Security: Only allow safe numeric type coercion (CRITICAL-4 fix)
            // Int to Int: same signedness, to larger or equal bits
            (Type::Int { bits: b1, signed: s1 }, Type::Int { bits: b2, signed: s2 }) => {
                s1 == s2 && b2 <= b1
            }
            // Float32 to Float64: safe promotion
            (Type::Float { bits: 64 }, Type::Float { bits: 32 }) => true,
            // DANGEROUS: Disallow implicit int->float conversion (CRITICAL-4 fix)
            // This prevents precision loss and security bypasses
            // Explicit casts should be required instead
            // (Type::Float { .. }, Type::Int { .. }) => true,  // REMOVED
            // C `object` parameter: accept ints, strings, named, refs, objects (not bool)
            (Type::Variadic, from) if from.is_c_handle_value() => true,
            // C `object` result: assign to pointer-sized int (`let p: int = malloc(...)`)
            (to, Type::Variadic) if to.is_pointer_sized_int() => true,
            // Annotated owned buffer: `let p: buf = malloc(...)` (object → buf). Not buf → bool.
            (to, Type::Variadic) if to.is_buf() => true,
            // C `char *` / `object`: string ↔ object
            (Type::String, Type::Variadic) => true,
            // &T 可以是 &mut T
            (Type::Ref { elem: e1, mutable: false }, Type::Ref { elem: e2, mutable: true }) => {
                e1.can_coerce_from(e2)
            }
            // 单元类型可以是 void
            (Type::Unit, Type::Void) => true,
            _ => false,
        }
    }

    /// Parse a type string. Same table as [`type_from_str`] (`object` is C pointer,
    /// not a second `NamedType { "object" }` spelling).
    pub fn from_str(type_str: &str) -> Result<Self, String> {
        type_from_str(type_str)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int { bits: 64, signed: true } => write!(f, "int"),
            Type::Int { bits, signed: true } => write!(f, "int({})+", *bits / 8),
            Type::Int { bits, signed: false } => write!(f, "int({})-", *bits / 8),
            Type::Float { bits: 64 } => write!(f, "float"),
            Type::Float { bits } => write!(f, "float({})", *bits / 8),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "str"),
            Type::Unit => write!(f, "()"),
            Type::Void => write!(f, "void"),
            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            Type::Array { elem, size } => write!(f, "[{}; {}]", elem, size),
            Type::Slice(elem) => write!(f, "[{}]", elem),
            Type::Ref { elem, mutable: false } => write!(f, "&{}", elem),
            Type::Ref { elem, mutable: true } => write!(f, "&mut {}", elem),
            Type::Function { params, return_type } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") => {}", return_type)
            }
            Type::NamedType { name } => {
                write!(f, "{}", name)
            }
            Type::App { name, args } => {
                write!(f, "{}<", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
            Type::Variadic => write!(f, "object"),
        }
    }
}

/// 从解析器的类型字符串创建 Type
/// 例如: "int", "int(4)+", "float(8)"
pub fn type_from_str(s: &str) -> Result<Type, String> {
    let s = s.trim();

    if let Some(fn_ty) = parse_fn_type_str(s) {
        return Ok(fn_ty);
    }

    // 处理 int 类型
    if s == "int" {
        return Ok(Type::int());
    }
    if let Some(rest) = s.strip_prefix("int(") {
        if let Some(end_pos) = rest.find(')') {
            let bits_str = &rest[..end_pos];
            // int(N)+ 表示 N 字节 = N*8 位
            let bytes = bits_str.parse::<u8>().map_err(|_| format!("Invalid int bytes: {}", bits_str))?;
            let bits = bytes * 8;
            let signed = !rest[end_pos + 1..].starts_with('-');
            return Ok(Type::Int { bits, signed });
        }
    }

    // 处理 float 类型
    if s == "float" {
        return Ok(Type::float());
    }
    if let Some(rest) = s.strip_prefix("float(") {
        if let Some(end_pos) = rest.find(')') {
            let bits_str = &rest[..end_pos];
            // float(N) 表示 N 字节 = N*8 位
            let bytes = bits_str.parse::<u8>().map_err(|_| format!("Invalid float bytes: {}", bits_str))?;
            let bits = bytes * 8;
            return Ok(Type::Float { bits });
        }
    }

    // 处理基础类型（含 C 风格别名；`object` 是 C 指针，不是 varargs）
    match s {
        "bool" => return Ok(Type::Bool),
        "str" | "string" => return Ok(Type::String),
        "()" => return Ok(Type::Unit),
        "void" | "nil" => return Ok(Type::Void),
        "object" | "..." => return Ok(Type::Variadic),
        "buf" => return Ok(Type::buf()),
        "i8" => return Ok(Type::Int { bits: 8, signed: true }),
        "i16" => return Ok(Type::Int { bits: 16, signed: true }),
        "i32" => return Ok(Type::Int { bits: 32, signed: true }),
        "i64" => return Ok(Type::Int { bits: 64, signed: true }),
        "u8" => return Ok(Type::Int { bits: 8, signed: false }),
        "u16" => return Ok(Type::Int { bits: 16, signed: false }),
        "u32" => return Ok(Type::Int { bits: 32, signed: false }),
        "u64" => return Ok(Type::Int { bits: 64, signed: false }),
        "f32" => return Ok(Type::Float { bits: 32 }),
        "f64" => return Ok(Type::Float { bits: 64 }),
        _ => {}
    }

    // 处理引用类型 &T 和 &mut T
    if let Some(rest) = s.strip_prefix("&mut ") {
        let elem = type_from_str(rest)?;
        return Ok(Type::make_ref_mut(elem));
    }
    if let Some(rest) = s.strip_prefix('&') {
        let elem = type_from_str(rest)?;
        return Ok(Type::Ref {
            elem: Box::new(elem),
            mutable: false,
        });
    }

    // 处理数组 [T; N] 与切片 [T]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1].trim();
        if let Some(semi_pos) = inner.find(';') {
            let elem = type_from_str(inner[..semi_pos].trim())?;
            let size: usize = inner[semi_pos + 1..].trim()
                .parse()
                .map_err(|_| format!("Invalid array size: '{}'", inner))?;
            const MAX_ARRAY_SIZE: usize = 1024 * 1024;
            if size == 0 {
                return Err("Array size cannot be zero".to_string());
            }
            if size > MAX_ARRAY_SIZE {
                return Err(format!(
                    "Array size {} exceeds maximum allowed size of {}\n  = note: Large arrays can cause memory exhaustion",
                    size, MAX_ARRAY_SIZE
                ));
            }
            return Ok(Type::Array {
                elem: Box::new(elem),
                size,
            });
        }
        let elem = type_from_str(inner)?;
        return Ok(Type::Slice(Box::new(elem)));
    }

    // 处理元组类型 (T1, T2, ...)
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        if !inner.is_empty() {
            let types = parse_type_list(inner)?;
            return Ok(Type::Tuple(types));
        } else {
            return Ok(Type::Unit);
        }
    }

    if let Some(app) = parse_app_type(s) {
        return app;
    }

    // 其他命名类型（class类型）- 视为命名类型
    // 这样可以支持用户定义的class类型
    return Ok(Type::NamedType { name: s.to_string() });
}

fn sanitize_mono_segment(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '(' | ')' | '+' | '-' | '[' | ';' | ']' | ',' | ' ' => '_',
            _ => c,
        })
        .collect()
}

fn is_type_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `Name<...>` with nested `<>` (same depth rules as `parse_type_list`).
fn parse_app_type(s: &str) -> Option<Result<Type, String>> {
    let lt = s.find('<')?;
    if !s.ends_with('>') {
        return None;
    }
    let name = s[..lt].trim();
    if !is_type_ident(name) {
        return None;
    }
    let inner = &s[lt + 1..s.len() - 1];
    let mut depth = 0i32;
    for (i, c) in s[lt..].char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    if lt + i + 1 != s.len() {
                        return None;
                    }
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Some(Err(format!("unbalanced angle brackets in type '{s}'")));
    }
    match parse_type_list(inner) {
        Ok(args) if args.is_empty() => Some(Err(format!(
            "type '{name}' has empty generic argument list"
        ))),
        Ok(args) => Some(Ok(Type::App {
            name: name.to_string(),
            args,
        })),
        Err(e) => Some(Err(e)),
    }
}

fn parse_type_list(s: &str) -> Result<Vec<Type>, String> {
    let mut types = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            ',' if depth == 0 => {
                let type_str = &s[start..i].trim();
                if !type_str.is_empty() {
                    types.push(type_from_str(type_str)?);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    // 添加最后一个类型
    let type_str = &s[start..].trim();
    if !type_str.is_empty() {
        types.push(type_from_str(type_str)?);
    }

    Ok(types)
}

fn parse_fn_type_str(s: &str) -> Option<Type> {
    let rest = s.strip_prefix("fn")?;
    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    let mut end_paren = None;
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end_paren = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end_paren = end_paren?;
    let inner = rest[1..end_paren].trim();
    let after = rest[end_paren + 1..].trim_start();
    let after = after.strip_prefix("=>")?.trim_start();
    let params = if inner.is_empty() {
        Vec::new()
    } else {
        parse_type_list(inner).ok()?
    };
    let return_type = parse_fn_type_str(after).or_else(|| type_from_str(after).ok())?;
    Some(Type::Function {
        params,
        return_type: Box::new(return_type),
    })
}

/// 全局空间ID计数器
static NEXT_SPACE_ID: AtomicU64 = AtomicU64::new(1);

/// 空间唯一标识符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpaceId(u64);

impl SpaceId {
    /// 创建新的空间ID
    pub fn new() -> Self {
        SpaceId(NEXT_SPACE_ID.fetch_add(1, Ordering::SeqCst))
    }

}

impl fmt::Display for SpaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Space{}", self.0)
    }
}

/// 空间类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpaceKind {
    /// 作用域空间：管理可见性和作用域层级
    Scope,
    /// 类型空间：管理类型定义和约束
    Type,
    /// 符号空间：管理符号声明和引用
    Symbol,
    /// 生命周期空间：管理引用的有效性
    Lifetime,
}

impl fmt::Display for SpaceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpaceKind::Scope => write!(f, "Scope"),
            SpaceKind::Type => write!(f, "Type"),
            SpaceKind::Symbol => write!(f, "Symbol"),
            SpaceKind::Lifetime => write!(f, "Lifetime"),
        }
    }
}

/// 可见性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// 公开
    Public,
    /// 私有
    Private,
}

/// 源代码位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

/// 统一的类型定义，合并了原 ty 和 space 模块的 TypeDef
#[derive(Debug, Clone)]
pub enum TypeDef {
    /// 类/结构体
    Class {
        name: String,
        fields: Vec<ClassField>,
        methods: Vec<MethodSignature>,
        #[allow(dead_code)]
        generics: Vec<String>,
        parent: Option<String>,
    },
    /// 枚举
    Enum {
        name: String,
        variants: Vec<EnumVariant>,
        #[allow(dead_code)]
        generics: Vec<String>,
    },
}

impl TypeDef {
    /// 获取类型定义的名称
    pub fn name(&self) -> &str {
        match self {
            TypeDef::Class { name, .. } => name,
            TypeDef::Enum { name, .. } => name,
        }
    }
}

/// 类字段
#[derive(Debug, Clone)]
pub struct ClassField {
    pub name: String,
    pub ty: String,
    #[allow(dead_code)]
    pub visibility: Visibility,
}

/// 枚举变体
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<VariantField>,
}

/// 变体字段
#[derive(Debug, Clone)]
pub enum VariantField {
    /// 位置字段（元组风格）
    Positional(String),
    /// 命名字段（结构体风格）
    Named {
        #[allow(dead_code)]
        name: String,
        ty: String,
    },
}

/// 方法签名
#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub name: String,
    pub params: Vec<Type>,
    pub return_type: Type,
    #[allow(dead_code)]
    pub receiver: Option<ReceiverKind>,
    #[allow(dead_code)]
    pub generics: Vec<String>,
}

/// 接收者类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    /// 共享引用 &self
    Shared,
}

/// 语义实体类型
#[derive(Debug, Clone)]
pub enum Entity {
    /// 变量
    Variable {
        ty: Type,
        #[allow(dead_code)]
        initialized: bool,
    },
    /// 函数
    Function {
        params: Vec<Type>,
        #[allow(dead_code)]
        return_type: Box<Type>,
        #[allow(dead_code)]
        generics: Vec<String>,
    },
    /// 类型定义
    TypeDef {
        #[allow(dead_code)]
        def: TypeDef,
    },
}

/// 绑定：名称到语义实体的映射
#[derive(Debug, Clone)]
pub struct Binding {
    /// 绑定名称
    pub name: String,
    /// 绑定的语义实体
    pub entity: Entity,
    /// 绑定的源位置
    pub span: Span,
    #[allow(dead_code)]
    pub mutable: bool,
    /// 绑定是否公开（public/private）
    pub visibility: Visibility,
}

/// 路径：在空间中导航的路径
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Path {
    /// 根路径
    #[allow(dead_code)]
    Root,
    /// 父路径
    #[allow(dead_code)]
    Parent,
    /// 标识符
    #[allow(dead_code)]
    Ident(String),
    /// 链式路径 path.sub
    #[allow(dead_code)]
    Chain {
        base: Box<Path>,
        segment: String,
    },
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Path::Root => write!(f, "/"),
            Path::Parent => write!(f, ".."),
            Path::Ident(name) => write!(f, "{}", name),
            Path::Chain { base, segment } => write!(f, "{}.{}", base, segment),
        }
    }
}

/// 区域：空间中的命名子区域
#[derive(Debug, Clone)]
pub struct Region {
    /// 区域名称
    #[allow(dead_code)]
    pub name: String,
    /// 区域内的绑定
    pub bindings: HashMap<String, Binding>,
    /// 子区域
    pub children: Vec<Region>,
}

impl Region {
    /// 创建新区域
    pub fn new(name: impl Into<String>) -> Self {
        Region {
            name: name.into(),
            bindings: HashMap::new(),
            children: Vec::new(),
        }
    }

    /// 添加绑定
    #[cfg(test)]
    pub fn bind(&mut self, name: impl Into<String>, binding: Binding) {
        self.bindings.insert(name.into(), binding);
    }

    /// 查找绑定
    #[cfg(test)]
    pub fn lookup(&self, name: &str) -> Option<&Binding> {
        self.bindings.get(name)
    }
}

/// 空间核心抽象
///
/// 空间是声明式的、不可变的语义容器，可以并发访问
#[allow(dead_code)]
pub trait Space: Send + Sync {
    /// 获取空间ID
    fn id(&self) -> SpaceId;

    /// 获取空间类型
    fn kind(&self) -> SpaceKind;

    /// 获取空间名称
    fn name(&self) -> &str;

    /// 获取父空间
    fn parent(&self) -> Option<&dyn Space>;

    /// 查找绑定
    fn lookup(&self, name: &str) -> Option<&Binding>;

    /// 获取所有绑定的名称
    fn bindings(&self) -> Vec<String>;

    /// 转换为Any以支持向下转型
    fn as_any(&self) -> &dyn std::any::Any;
}

#[cfg(test)]
mod tests;

