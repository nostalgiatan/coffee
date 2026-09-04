# 类型系统模块

## 概述

类型系统模块（`src/types/`）为 Coffee 语言提供全面的类型检查、类型推断和类型管理。它确保所有语言构造的类型安全，并在省略类型注解时启用自动类型推断。

## 模块结构

```
src/types/
├── mod.rs         # 主类型系统模块
├── definition.rs  # 类型定义
├── registry.rs    # 类型注册表和管理
├── checker.rs     # 类型检查器
├── inference.rs   # 类型推断引擎
└── errors.rs      # 类型系统错误
```

## 核心组件

### 1. 类型定义（`definition.rs`）

定义 Coffee 语言中的所有类型。

**内置类型：**
```rust
pub enum Type {
    // 基本类型
    Int { signed: bool, bytes: usize },
    Float { bytes: usize },
    Bool,
    String,
    Void,

    // 复合类型
    Array(Box<Type>, Option<usize>),      // [T; N] 或 [T]
    Slice(Box<Type>),                      // [T]
    Tuple(Vec<Type>),                      // (T1, T2, ...)
    Function(Vec<Type>, Box<Type>),        // fn(params) -> return
    Reference(Box<Type>, bool),            // &T 或 &mut T

    // 用户定义类型
    Struct(String, Vec<(String, Type)>),
    Enum(String, Vec<EnumVariant>),
    Class(String),

    // 类型变量（用于泛型）
    TypeVar(String),
}
```

**类型属性：**
- 大小和对齐信息
- 方法分发能力
- 特质实现
- 子类型关系

### 2. 类型注册表（`registry.rs`）

管理程序中的所有类型。

**注册表操作：**
```rust
pub struct TypeRegistry {
    types: HashMap<String, Type>,
    next_type_id: usize,
}

impl TypeRegistry {
    pub fn root() -> Self;  // 使用内置类型创建

    pub fn register_type(&mut self, name: String, type_: Type) -> Result<(), TypeSystemError>;
    pub fn lookup_type(&self, name: &str) -> Option<&Type>;
    pub fn get_type_id(&self, type_: &Type) -> Option<usize>;

    pub fn is_subtype(&self, sub: &Type, sup: &Type) -> bool;
    pub fn unify(&mut self, t1: &Type, t2: &Type) -> Result<Type, TypeSystemError>;
}
```

**内置类型：**
- `int(8)+` (i64), `int(4)+` (i32), `int(2)+` (i16), `int(1)+` (i8)
- `int(8)-` (u64), `int(4)-` (u32), `int(2)-` (u16), `int(1)-` (u8)
- `float(8)` (f64), `float(4)` (f32)
- `bool`, `string`, `void`

### 3. 类型检查器（`checker.rs`）

验证整个程序的类型正确性。

**检查模式：**
```rust
pub enum CheckingMode {
    Strict,        // 需要显式类型注解
    Inference,     // 尽可能推断类型
    Comprehensive, // 带推断的完整类型检查
}
```

**类型检查方法：**
```rust
pub struct TypeChecker {
    registry: Arc<RwLock<TypeRegistry>>,
    mode: CheckingMode,
    diagnostics: Vec<Diagnostic>,
}

impl TypeChecker {
    pub fn check_variable_decl(&mut self, decl: &VariableDecl) -> Result<(), Diagnostic>;
    pub fn check_function_decl(&mut self, func: &Function) -> Result<(), Diagnostic>;
    pub fn check_expression(&mut self, expr: &Expr) -> Result<Type, Diagnostic>;
    pub fn check_memory_op(&mut self, op: &MemoryOp) -> Result<(), Diagnostic>;

    pub fn check_types_compatible(&self, t1: &Type, t2: &Type) -> bool;
    pub fn coerce_type(&self, from: &Type, to: &Type) -> Result<Type, Diagnostic>;
}
```

**类型兼容性规则：**
- 大多数类型需要精确匹配
- 数值类型提升（int 到 float，小 int 到大 int）
- 引用兼容性
- 子类型多态

### 4. 类型推断（`inference.rs`）

为没有显式注解的表达式推断类型。

**推断算法（Hindley-Milner 变体）：**
1. 为未注解的表达式生成类型变量
2. 从表达式结构生成约束
3. 统一约束以求解类型变量
4. 用具体类型替换类型变量

**推断示例：**
```rust
// 推断: int
let x = 42;

// 推断: float
let y = 3.14;

// 推断: int（从函数签名）
fn add(a: int, b: int) => int:
    return a + b  // a + b 推断为 int

// 推断: [int]
let arr = [1, 2, 3];
```

### 5. 类型系统错误（`errors.rs`）

全面的类型错误报告。

**错误类型：**
```rust
pub enum TypeSystemError {
    UndefinedType { name: String },
    TypeMismatch { expected: Type, found: Type },
    AmbiguousType { expr: String },
    InferenceFailed { reason: String },
    InvalidCoercion { from: Type, to: Type },
    RecursiveType { name: String },
}
```

## 类型检查过程

### 1. 声明检查
- 验证类型注解
- 注册用户定义类型
- 检查递归类型定义

### 2. 表达式检查
- 推断表达式类型
- 验证操作符使用
- 检查函数调用兼容性
- 验证数组索引

### 3. 语句检查
- 验证变量声明
- 检查 return 语句
- 验证控制流类型
- 检查内存操作

### 4. 函数检查
- 验证参数类型
- 检查返回类型一致性
- 验证函数调用
- 检查 C ABI 兼容性

## 类型推断

**支持的推断：**
- 变量初始化
- 函数返回表达式
- 数组字面量
- 元组字面量
- 条件表达式
- Lambda 表达式（如果支持）

**推断限制：**
- 函数参数需要类型注解
- 用户定义类型需要显式定义
- C 函数签名必须声明

## 集成

### 与语义分析
- 为符号表提供类型信息
- 验证声明中的类型注解
- 执行基于类型的语义检查

### 与后端
- 为 LLVM IR 生成提供类型信息
- 生成类型元数据
- 处理代码生成中的类型转换

### 与 C 集成
- 将 Coffee 类型映射到 C 类型
- 验证 C 函数签名
- 处理 FFI 的类型转换

## 使用示例

```rust
use coffee::types::{TypeChecker, TypeRegistry, CheckingMode};

let type_registry = Arc::new(RwLock::new(TypeRegistry::root()));
let mut checker = TypeChecker::new(type_registry.clone(), CheckingMode::Comprehensive);

// 检查变量声明
let decl = VariableDecl {
    name: "x".to_string(),
    type_: Type::Int { signed: true, bytes: 4 },
    init: Some(Box::new(Expr::Literal(Literal::Int(42)))),
};
checker.check_variable_decl(&decl)?;

// 检查表达式
let expr = Expr::BinaryOp {
    left: Box::new(Expr::Variable("x".to_string())),
    op: BinaryOperator::Add,
    right: Box::new(Expr::Literal(Literal::Int(10))),
};
let result_type = checker.check_expression(&expr)?;
```

## 设计考虑

### 1. 类型安全
- 所有表达式都有明确定义的类型
- 类型转换是显式的（除了安全提升）
- 没有丢失信息的隐式转换

### 2. 性能
- 类型注册表使用高效查找
- 大多数操作的类型检查是 O(n)
- 推断使用高效的统一算法

### 3. 可扩展性
- 易于添加新类型
- 特质系统支持（未来）
- 泛型类型参数（未来）

## 未来增强

- 泛型类型和类型参数
- 特质系统
- 关联类型
- 类型级编程
- 依赖类型
- 更好的错误消息和类型建议