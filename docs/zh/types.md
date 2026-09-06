# 类型系统模块

## 概述

类型系统模块（`src/types/`）为 Coffee 语言提供全面的类型检查、类型推断和类型管理。它确保所有语言构造的类型安全，并在省略类型注解时启用自动类型推断。

## 模块结构

```
src/types/
├── mod.rs           # 类型系统模块
├── definition/      # 类型定义（不是 definition.rs）
│   ├── mod.rs
│   └── tests.rs
├── registry.rs      # 类型注册表
├── checker/         # TypeChecker（不是 checker.rs）
│   ├── mod.rs
│   ├── values.rs
│   ├── stmt.rs
│   ├── expr/        # 表达式检查（不是单独的 expr.rs）
│   ├── call.rs
│   ├── match_check.rs
│   ├── memory.rs
│   ├── import_main.rs
│   ├── compat.rs
│   └── tests.rs
├── borrow.rs        # 过程内借用检查
├── last_use.rs      # last-use 隐式搬走
├── mono.rs          # 泛型单态化（`List<int>` → `List__int`）
└── errors.rs        # 类型系统错误
```

## 核心组件

### 1. 类型定义（`definition/`）

定义 Coffee 语言中的所有类型。

表面语法 `int(N)+` / `int(N)-` / `float(N)` 的 **N 是字节数**（`int(4)+` 对应 C `int`）。枚举存的是 **位数**：`bits = 8 * N`。

**内置类型：**
```rust
pub enum Type {
    Int { bits: u8, signed: bool },
    Float { bits: u8 },
    Bool,
    String,
    Void,
    Unit,
    Array { elem: Box<Type>, size: usize },
    Slice(Box<Type>),
    Tuple(Vec<Type>),
    Function { params: Vec<Type>, return_type: Box<Type> },
    Ref { elem: Box<Type>, mutable: bool },
    NamedType { name: String },
    App { name: String, args: Vec<Type> }, // `List<int>`；单态化为 `List__int`
    Variadic,
}
```

**类型属性：**
- 大小和对齐信息
- 在单态化后的具体名字上做方法分发
- 子类型关系

**泛型 v1（`mono.rs`）：** `Type::App` 做 token 替换。`List<int>` 变成具体类 `List__int`；方法是 `List__int_push`。语法：`class List<T>:` / `fn id<T>(x: T) => T`。嵌套 `List<List<int>>` 是一个类型。没有泛型 std `List`（std 提供 `IntBuf`），也没有 `T: Trait`。

**`buf`：** 内建资源类型（`NamedType "buf"`）：拥有的 malloc 指针；drop 会 `free`；`clone buf` 是类型错误。`object` 是不释放的 C 句柄。

**切片：** Coffee 函数上的 `[T]` 是胖指针。`c fn` 仍拒绝切片。Last-use 隐式搬走：`src/types/last_use.rs`。

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
- `int(8)+` (i64，8 字节), `int(4)+` (i32，4 字节), `int(2)+` (i16), `int(1)+` (i8)
- `int(8)-` (u64), `int(4)-` (u32), `int(2)-` (u16), `int(1)-` (u8)
- `float(8)` (f64), `float(4)` (f32)
- `bool`, `string`, `void`
- 内建类 `Error` `{ code: int, note: str, e: object }`（源码不能再声明 `class Error`）

**异常（`raise`）：** 仅当类型是 `Error`，或继承链到达 `Error` 的类（`class C of Error`，及其后代）时允许。异常类可以有额外字段和方法（不要重声明 `code` / `note` / `e`）。名为 `*Error` 但没有 `of Error` 的类是普通类，不能 `raise`。`#name` 监听器参数仍是 `err: Error`，只依赖该前缀。

### 3. 类型检查器（`checker/`）

验证整个程序的类型正确性。

**检查模式：**
```rust
pub enum CheckingMode {
    Simple,
    Comprehensive, // 所有权、内存操作、借用检查
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
- 整数/浮点**字面量**可在范围内落入更窄的注解宽度
- `object` C 句柄与指针宽度整数的规则见类型检查器
- `&T` / `&mut T` 由借用检查器约束（共享 XOR 可变）

### 3b. 借用检查（`borrow.rs`）

过程内 loan，地点为**变量、字段 `p.x`、下标 `a[i]`**。由 `TypeChecker` 接入（`&x`、`&mut x`、`*r`、move/赋值/`rm`、返回 `&local`）。若被调函数返回 `&T`，实参 `&`/`&mut` 的 loan 可以活过该语句（同模块；无 `'a`）。见 `docs/superpowers/specs/2026-09-05-borrow-checker.md`。

表达式类型在 `checker/` 中检查，没有单独的 `inference.rs`。

### 3c. 泛型 v1（`mono.rs` + `Type::App`）

检查器把 `List<int>` 当成 `Type::App { name: "List", args: [int] }`，按 token 替换实例化成具体类型名 `List__int`（`Type::mono_name`）。方法变成 `List__int_push` 这类 LLVM/符号名。`class List<T>:` / `fn id<T>(x: T) => T` 携带 `type_params`。

这不是带约束的完整泛型：没有 `T: Trait`，也没有随编译器发布的标准库 `List`。

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
    type_: Type::Int { bits: 32, signed: true },
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
- 泛型 v1：`Type::App` 单态化（无 trait bound）
- 特质系统支持（未来）

## 未来增强

- 特质系统与 `T: Trait`；泛型 std `List`（std 现有 `IntBuf`，不是 `List<T>`）
- 关联类型
- 类型级编程
- 依赖类型
- 更好的错误消息和类型建议