use std::fmt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 类型定义
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// 基础类型
    /// int(4)+ = i32, int(1)- = u8, int = i32 (默认)
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
    /// 可变参数类型 (object, 用于 C ABI 可变参数函数)
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

    /// 创建引用类型 &T
    pub fn make_ref(elem: Type) -> Self {
        Type::Ref {
            elem: Box::new(elem),
            mutable: false,
        }
    }

    /// 创建可变引用类型 &mut T
    pub fn make_ref_mut(elem: Type) -> Self {
        Type::Ref {
            elem: Box::new(elem),
            mutable: true,
        }
    }

    /// 创建切片类型 [T]
    pub fn slice(elem: Type) -> Self {
        Type::Slice(Box::new(elem))
    }

    /// 创建数组类型 [T; N]
    pub fn array(elem: Type, size: usize) -> Self {
        Type::Array {
            elem: Box::new(elem),
            size,
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

    /// 检查是否是引用类型
    pub fn is_ref(&self) -> bool {
        matches!(self, Type::Ref { .. })
    }

    /// 获取类型的字节大小
    pub fn size_of(&self) -> usize {
        match self {
            Type::Int { bits, .. } | Type::Float { bits } => (*bits as usize) / 8,
            Type::Bool => 1,
            Type::String => 8, // 指针大小
            Type::Unit => 0,
            Type::Void => 0,
            Type::Tuple(types) => types.iter().map(|t| t.size_of()).sum(),
            Type::Array { elem, size } => elem.size_of() * size,
            Type::Slice(_) => 16, // fat pointer: ptr + len
            Type::Ref { .. } => 8, // 指针大小
            Type::Function { .. } => 8, // 函数指针
            Type::NamedType { .. } => 8, // 占位
            Type::Variadic => 8, // 可变参数，按指针大小处理
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
            // &T 可以是 &mut T
            (Type::Ref { elem: e1, mutable: false }, Type::Ref { elem: e2, mutable: true }) => {
                e1.can_coerce_from(e2)
            }
            // 单元类型可以是 void
            (Type::Unit, Type::Void) => true,
            _ => false,
        }
    }

    /// Parse type from string representation
    /// Supports: int, float, bool, string, void, [T; N], [T], etc.
    pub fn from_str(type_str: &str) -> Result<Self, String> {
        let type_str = type_str.trim();

        // Array type: [elem; size] or [elem]
        if type_str.starts_with('[') {
            if type_str.ends_with(']') {
                let inner = &type_str[1..type_str.len()-1].trim();

                // Check for [elem; size] syntax
                if let Some(semi_pos) = inner.find(';') {
                    let elem_str = inner[..semi_pos].trim();
                    let size_str = inner[semi_pos+1..].trim();

                    let elem_type = Type::from_str(elem_str)?;
                    let size: usize = size_str.parse()
                        .map_err(|_| format!("Invalid array size: '{}'", size_str))?;

                    // Security: Validate array size to prevent DoS attacks
                    const MAX_ARRAY_SIZE: usize = 1024 * 1024; // 1M elements
                    if size == 0 {
                        return Err(format!("Array size cannot be zero"));
                    }
                    if size > MAX_ARRAY_SIZE {
                        return Err(format!(
                            "Array size {} exceeds maximum allowed size of {}\n  = note: Large arrays can cause memory exhaustion",
                            size, MAX_ARRAY_SIZE
                        ));
                    }

                    return Ok(Type::Array {
                        elem: Box::new(elem_type),
                        size,
                    });
                }

                // Slice type: [elem]
                let elem_type = Type::from_str(inner)?;
                return Ok(Type::Slice(Box::new(elem_type)));
            }
        }

        // Basic types
        match type_str {
            "int" => return Ok(Type::int()),  // int = i64 (64-bit, default)
            "i32" => return Ok(Type::Int { bits: 32, signed: true }),
            "i64" => return Ok(Type::Int { bits: 64, signed: true }),
            "i8" => return Ok(Type::Int { bits: 8, signed: true }),
            "i16" => return Ok(Type::Int { bits: 16, signed: true }),
            "u8" => return Ok(Type::Int { bits: 8, signed: false }),
            "u16" => return Ok(Type::Int { bits: 16, signed: false }),
            "u32" => return Ok(Type::Int { bits: 32, signed: false }),
            "u64" => return Ok(Type::Int { bits: 64, signed: false }),
            "float" | "f64" => return Ok(Type::Float { bits: 64 }),
            "f32" => return Ok(Type::Float { bits: 32 }),
            "bool" => return Ok(Type::Bool),
            "string" | "str" => return Ok(Type::String),
            "void" | "()" | "nil" => return Ok(Type::Void),
            _ => {}
        }

        // Parameterized int types: int(N)+ or int(N)-
        if let Some(rest) = type_str.strip_prefix("int(") {
            if let Some(end_pos) = rest.find(')') {
                let bits_str = &rest[..end_pos];
                // int(N)+ 表示 N 字节 = N*8 位
                let bytes = bits_str.parse::<u8>().map_err(|_| format!("Invalid int bytes: {}", bits_str))?;
                let bits = bytes * 8;

                // Check for + or - after the closing parenthesis
                let rest_after_paren = &rest[end_pos + 1..];
                let signed = !rest_after_paren.starts_with('-');

                return Ok(Type::Int { bits, signed });
            }
        }

        // Parameterized float types: float(N)
        if let Some(rest) = type_str.strip_prefix("float(") {
            if let Some(end_pos) = rest.find(')') {
                let bits_str = &rest[..end_pos];
                // float(N) 表示 N 字节 = N*8 位
                let bytes = bits_str.parse::<u8>().map_err(|_| format!("Invalid float bytes: {}", bits_str))?;
                let bits = bytes * 8;
                return Ok(Type::Float { bits });
            }
        }

        // Named type
        Ok(Type::NamedType {
            name: type_str.to_string(),
        })
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int { bits: 64, signed: true } => write!(f, "int"),
            Type::Int { bits, signed: true } => write!(f, "int({})", *bits),
            Type::Int { bits: 8, signed: false } => write!(f, "u8"),
            Type::Int { bits, signed: false } => write!(f, "int({})-", *bits),
            Type::Float { bits: 64 } => write!(f, "float"),
            Type::Float { bits } => write!(f, "float({})", *bits),
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
            Type::Variadic => write!(f, "object"),
        }
    }
}

/// 从解析器的类型字符串创建 Type
/// 例如: "int", "int(4)+", "float(8)", "Vec<T>", "Option<T>"
pub fn type_from_str(s: &str) -> Result<Type, String> {
    let s = s.trim();

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

    // 处理基础类型
    match s {
        "bool" => return Ok(Type::Bool),
        "str" | "string" => return Ok(Type::String),
        "()" => return Ok(Type::Unit),
        "void" => return Ok(Type::Void),
        "object" | "..." => return Ok(Type::Variadic),  // 可变参数类型
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

    // 其他命名类型（class类型）- 视为命名类型
    // 这样可以支持用户定义的class类型
    return Ok(Type::NamedType { name: s.to_string() });
}

/// 解析类型列表 "T1, T2, ..."

/// 为未知类型名称提供建议
fn suggest_type_name(input: &str) -> &'static str {
    let input_lower = input.to_lowercase();

    // Coffee类型系统的常见类型别名映射
    // Coffee实际支持的类型: int, int(N)+, int(N)-, float, float(N), bool, str/string, (), void/nil, object/...
    // 以及复合类型: [T], &T, &mut T, (T1, T2, ...), Name<T1, T2>
    let type_aliases = [
        // 字符串类型
        ("text", "str"),
        ("char", "str"),
        ("cstring", "str"),
        // 整数类型（Coffee使用int(N)+和int(N)-语法）
        ("integer", "int"),
        ("number", "int"),
        ("int32", "int(4)+"),
        ("int64", "int(8)+"),
        ("uint", "int(4)-"),
        ("uint32", "int(4)-"),
        ("uint64", "int(8)-"),
        ("long", "int(8)+"),
        ("short", "int(2)+"),
        ("byte", "int(1)-"),
        ("ubyte", "int(1)-"),
        ("i8", "int(1)+"),
        ("i16", "int(2)+"),
        ("i32", "int(4)+"),
        ("i64", "int(8)+"),
        ("u8", "int(1)-"),
        ("u16", "int(2)-"),
        ("u32", "int(4)-"),
        ("u64", "int(8)-"),
        // 浮点类型（Coffee使用float(N)语法）
        ("double", "float(8)"),
        ("real", "float(8)"),
        ("decimal", "float(8)"),
        ("f32", "float(4)"),
        ("f64", "float(8)"),
        // 布尔类型
        ("boolean", "bool"),
        // 空类型
        ("null", "void"),
        ("none", "void"),
        ("unit", "()"),
        ("empty", "()"),
        // 可变参数
        ("varargs", "..."),
        ("variadic", "..."),
        // 其他
        ("auto", "int"),
        ("any", "int"),
    ];

    // 检查精确匹配（忽略大小写）
    for (alias, suggested) in &type_aliases {
        if input_lower == *alias {
            return suggested;
        }
    }

    // 检查模糊匹配（编辑距离）
    // Coffee的基本类型
    let valid_types = [
        "int", "float", "bool", "str", "void",
        "string", "()", "...", "object",
        "[int]", "[str]", "&int", "&str",
        "(int, str)", "(int, float)",
    ];

    let mut best_match = "int";
    let mut best_distance = usize::MAX;

    for valid_type in &valid_types {
        let distance = edit_distance(&input_lower, valid_type);
        if distance < best_distance {
            best_distance = distance;
            best_match = valid_type;
        }
    }

    // 只在编辑距离足够小时才建议
    if best_distance <= 2 {
        best_match
    } else {
        "int" // 默认建议
    }
}

/// 计算两个字符串的编辑距离（Levenshtein距离）
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    let mut dp = vec![vec![0; b_len + 1]; a_len + 1];

    // 初始化第一行和第一列
    for i in 0..=a_len {
        dp[i][0] = i;
    }
    for j in 0..=b_len {
        dp[0][j] = j;
    }

    // 动态规划计算编辑距离
    for i in 1..=a_len {
        for j in 1..=b_len {
            if a_chars[i - 1] == b_chars[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            } else {
                dp[i][j] = 1 + std::cmp::min(
                    dp[i - 1][j],        // 删除
                    std::cmp::min(
                        dp[i][j - 1],        // 插入
                        dp[i - 1][j - 1]     // 替换
                    )
                );
            }
        }
    }

    dp[a_len][b_len]
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

    /// 获取原始ID值
    pub fn as_u64(&self) -> u64 {
        self.0
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
    /// 受保护（在继承中使用）
    Protected,
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
        generics: Vec<String>,
        parent: Option<String>,
    },
    /// 枚举
    Enum {
        name: String,
        variants: Vec<EnumVariant>,
        generics: Vec<String>,
    },
    /// 结构体（从 ty 模块继承）
    Struct {
        name: String,
        fields: Vec<ClassField>,
        methods: Vec<MethodSignature>,
        generics: Vec<String>,
    },
    /// 类型别名
    Alias {
        name: String,
        target: Box<Type>,
    },
}

impl TypeDef {
    /// 获取类型定义的名称
    pub fn name(&self) -> &str {
        match self {
            TypeDef::Class { name, .. } => name,
            TypeDef::Enum { name, .. } => name,
            TypeDef::Struct { name, .. } => name,
            TypeDef::Alias { name, .. } => name,
        }
    }

    /// 获取泛型参数
    pub fn generics(&self) -> &[String] {
        match self {
            TypeDef::Class { generics, .. } => generics,
            TypeDef::Enum { generics, .. } => generics,
            TypeDef::Struct { generics, .. } => generics,
            TypeDef::Alias { .. } => &[],
        }
    }

    /// 获取字段定义
    pub fn fields(&self) -> Option<&[ClassField]> {
        match self {
            TypeDef::Class { fields, .. } => Some(fields),
            TypeDef::Struct { fields, .. } => Some(fields),
            _ => None,
        }
    }

    /// 获取方法定义
    pub fn methods(&self) -> Option<&[MethodSignature]> {
        match self {
            TypeDef::Class { methods, .. } => Some(methods),
            TypeDef::Struct { methods, .. } => Some(methods),
            _ => None,
        }
    }

    /// 获取枚举变体
    pub fn variants(&self) -> Option<&[EnumVariant]> {
        match self {
            TypeDef::Enum { variants, .. } => Some(variants),
            _ => None,
        }
    }

    /// 检查是否为泛型类型
    pub fn is_generic(&self) -> bool {
        !self.generics().is_empty()
    }

    // Legacy conversion function - deprecated after module consolidation
}

/// 类字段
#[derive(Debug, Clone)]
pub struct ClassField {
    pub name: String,
    pub ty: String,
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
    Named { name: String, ty: String },
}

/// 方法签名
#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub name: String,
    pub params: Vec<Type>,
    pub return_type: Type,
    pub receiver: Option<ReceiverKind>,
    pub generics: Vec<String>,
}

/// 接收者类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    /// 共享引用 &self
    Shared,
    /// 可变引用 &mut self
    Mutable,
    /// 自有值 self
    Owned,
}

/// 语义实体类型
#[derive(Debug, Clone)]
pub enum Entity {
    /// 变量
    Variable {
        ty: Type,
        initialized: bool,
    },
    /// 函数
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
        generics: Vec<String>,
    },
    /// 类型定义
    TypeDef {
        def: TypeDef,
    },
    /// 类型参数（泛型）
    TypeParam {
        constraint: Option<TypeConstraint>,
    },
    /// 生命周期参数
    LifetimeParam {
        bounds: Vec<String>,
    },
    /// 方法
    Method {
        receiver: Option<ReceiverKind>,
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    /// 字段
    Field {
        ty: Type,
    },
    /// 枚举变体
    Variant {
        fields: Vec<VariantField>,
    },
}

/// 类型约束
#[derive(Debug, Clone)]
pub enum TypeConstraint {
    /// Trait 约束
    Trait(String),
    /// 相等约束
    Equals(String),
    /// 多个约束
    All(Vec<TypeConstraint>),
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
    /// 绑定是否可变
    pub mutable: bool,
    /// 绑定是否公开（public/private）
    pub visibility: Visibility,
}

/// 路径：在空间中导航的路径
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Path {
    /// 根路径
    Root,
    /// 当前路径
    This,
    /// 父路径
    Parent,
    /// 标识符
    Ident(String),
    /// 链式路径 path.sub
    Chain {
        base: Box<Path>,
        segment: String,
    },
    /// 泛型实例 path<T>
    Generic {
        base: Box<Path>,
        args: Vec<String>,
    },
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Path::Root => write!(f, "/"),
            Path::This => write!(f, "this"),
            Path::Parent => write!(f, ".."),
            Path::Ident(name) => write!(f, "{}", name),
            Path::Chain { base, segment } => write!(f, "{}.{}", base, segment),
            Path::Generic { base, args } => {
                write!(f, "{}<", base)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
        }
    }
}

/// 空间查询结果
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// 找到的实体
    pub entity: Entity,
    /// 定义位置
    pub span: Span,
    /// 查询路径
    pub path: Path,
}

/// 区域：空间中的命名子区域
#[derive(Debug, Clone)]
pub struct Region {
    /// 区域名称
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
    pub fn bind(&mut self, name: impl Into<String>, binding: Binding) {
        self.bindings.insert(name.into(), binding);
    }

    /// 添加子区域
    pub fn add_child(&mut self, child: Region) {
        self.children.push(child);
    }

    /// 查找绑定
    pub fn lookup(&self, name: &str) -> Option<&Binding> {
        self.bindings.get(name)
    }
}

/// 空间核心抽象
///
/// 空间是声明式的、不可变的语义容器，可以并发访问
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
mod tests {
    use super::*;

    #[test]
    fn test_space_id_unique() {
        let id1 = SpaceId::new();
        let id2 = SpaceId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_path_display() {
        assert_eq!(Path::Root.to_string(), "/");
        assert_eq!(Path::Parent.to_string(), "..");
        assert_eq!(Path::Ident("x".to_string()).to_string(), "x");
        assert_eq!(
            Path::Chain {
                base: Box::new(Path::Ident("a".to_string())),
                segment: "b".to_string()
            }.to_string(),
            "a.b"
        );
    }

    #[test]
    fn test_type_def_name() {
        let class_def = TypeDef::Class {
            name: "TestClass".to_string(),
            fields: vec![],
            methods: vec![],
            generics: vec![],
            parent: None,
        };
        assert_eq!(class_def.name(), "TestClass");
    }

    #[test]
    fn test_region() {
        let mut region = Region::new("test");
        region.bind("x", Binding {
            name: "x".to_string(),
            entity: Entity::Variable {
                ty: Type::i32(),
                initialized: true,
            },
            span: Span::new(0, 1),
            mutable: false,
            visibility: Visibility::Private,
        });

        assert!(region.lookup("x").is_some());
        assert!(region.lookup("y").is_none());
    }
}