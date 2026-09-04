# 语义分析模块

## 概述

语义分析模块（`src/semantic/`）负责分析解析后的 AST，确保其符合 Coffee 语言的语义规则。它执行作用域管理、符号解析、生命周期跟踪和全面的语义验证。

## 模块结构

```
src/semantic/
├── mod.rs       # 主语义分析模块
├── analyzer.rs  # 主语义分析器
├── scope.rs     # 作用域管理
├── symbols.rs   # 符号表管理
└── lifetime.rs  # 生命周期跟踪和引用验证
```

## 核心组件

### 1. 语义分析器（`analyzer.rs`）

`SemanticAnalyzer` 是协调整个语义分析过程的主要组件。

**主要职责：**
- 管理作用域和符号表
- 分析声明（变量、函数、类、枚举）
- 验证符号使用
- 检查生命周期和借用规则
- 收集语义错误

**主要方法：**

```rust
pub struct SemanticAnalyzer {
    type_registry: Arc<RwLock<TypeRegistry>>,
    scopes: Vec<Scope>,
    symbols: SymbolTable,
    errors: Vec<SemanticError>,
    c_imports: Vec<String>,
    cfc_symbols: HashMap<String, CSymbolTable>,
}
```

**分析方法：**
- `analyze_statement(&Statement) -> Result<(), SemanticError>`
- `analyze_function_decl(&Function) -> Result<(), SemanticError>`
- `analyze_class_def(&ClassDef) -> Result<(), SemanticError>`
- `analyze_variable_decl(&VariableDecl) -> Result<(), SemanticError>`
- `analyze_expression(&Expr) -> Result<(), SemanticError>`

### 2. 作用域管理（`scope.rs`）

作用域系统管理变量可见性和生命周期。

**作用域层次：**
- 全局作用域（模块级）
- 函数作用域
- 块作用域（if、while、for、match）
- 类作用域

**作用域操作：**
```rust
pub struct Scope {
    level: usize,
    parent: Option<usize>,
    variables: HashMap<String, Symbol>,
    functions: HashMap<String, FunctionSymbol>,
    classes: HashMap<String, ClassSymbol>,
}

impl Scope {
    pub fn new(level: usize, parent: Option<usize>) -> Self;
    pub fn enter(&mut self) -> usize;
    pub fn exit(&mut self) -> Option<Scope>;
    pub fn lookup(&self, name: &str) -> Option<&Symbol>;
    pub fn define(&mut self, name: String, symbol: Symbol) -> Result<(), SemanticError>;
}
```

**作用域特性：**
- 嵌套作用域支持
- 遮蔽检测
- 变量生命周期跟踪
- 基于作用域的符号解析

### 3. 符号表（`symbols.rs`）

管理程序中声明的所有符号。

**符号类型：**
```rust
pub enum Symbol {
    Variable(VariableSymbol),
    Function(FunctionSymbol),
    Class(ClassSymbol),
    Enum(EnumSymbol),
    Method(MethodSymbol),
}

pub struct VariableSymbol {
    pub name: String,
    pub type_: Type,
    pub is_mutable: bool,
    pub scope_level: usize,
    pub is_extern: bool,
}

pub struct FunctionSymbol {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub is_c_abi: bool,
    pub scope_level: usize,
}
```

**符号表操作：**
- 插入符号
- 查找符号（带作用域解析）
- 检查重复声明
- 导出符号供外部使用

### 4. 生命周期分析（`lifetime.rs`）

跟踪变量生命周期并验证借用规则。

**生命周期特性：**
- 变量生命周期跟踪
- 借用检查器（借用/借贷规则）
- 移动语义验证
- 引用安全检查

**生命周期规则：**
- 变量存活到其作用域结束
- 引用不能超过其引用对象的生命周期
- 可变引用是独占的
- 允许多个不可变引用

**分析检查：**
```rust
pub fn analyze_lifetime(&mut self, expr: &Expr) -> Result<(), SemanticError> {
    match expr {
        Expr::Variable(name) => {
            self.check_variable_lifetime(name)?;
        }
        Expr::BinaryOp { left, op, right } => {
            self.analyze_lifetime(left)?;
            self.analyze_lifetime(right)?;
        }
        // ... 更多情况
    }
    Ok(())
}
```

## 分析过程

语义分析遵循以下阶段：

### 阶段 1：声明收集
1. 创建全局作用域
2. 收集所有顶级声明
3. 构建符号表
4. 检测重复声明

### 阶段 2：语句分析
1. 为每个语句进入适当的作用域
2. 分析作用域内的声明
3. 验证符号使用
4. 检查语义规则

### 阶段 3：表达式分析
1. 验证变量引用
2. 检查函数调用
3. 验证操作符使用
4. 类型推断集成

### 阶段 4：生命周期验证
1. 跟踪变量生命周期
2. 验证借用规则
3. 检查移动语义
4. 确保引用安全

## 语义检查

### 变量声明检查
- 类型注解有效性
- 初始化表达式类型兼容性
- 重复变量检测
- 遮蔽验证

### 函数声明检查
- 参数名称唯一性
- 返回类型有效性
- C ABI 验证
- 函数签名唯一性

### 类声明检查
- 字段名称唯一性
- 字段类型有效性
- 继承有效性
- 方法签名唯一性

### 表达式检查
- 变量存在性
- 函数调用有效性
- 操作符类型兼容性
- 数组索引边界（尽可能）

### 内存操作检查
- `mv`、`clone`、`copy`、`rm` 的变量存在性
- 内存操作的类型兼容性
- 所有权转移验证
- 借用规则合规性

## 错误处理

**语义错误类型：**
```rust
pub enum SemanticError {
    UndefinedVariable { name: String, location: Location },
    UndefinedFunction { name: String, location: Location },
    DuplicateDeclaration { name: String, location: Location },
    TypeMismatch { expected: Type, found: Type, location: Location },
    InvalidBorrow { reason: String, location: Location },
    InvalidMove { reason: String, location: Location },
    LifetimeError { reason: String, location: Location },
    // ... 更多错误类型
}
```

**错误报告：**
- 分析期间收集所有错误
- 提供详细的错误消息
- 包含源位置信息
- 尽可能建议修复

## 与其他模块的集成

### 与解析器
- 从解析器接收 AST
- 分析解析的语句和表达式
- 报告语义错误

### 与类型系统
- 使用类型注册表进行类型验证
- 对表达式执行类型检查
- 与类型推断集成

### 与后端
- 为代码生成提供符号信息
- 为 LLVM IR 生成提供类型信息
- 验证 C 函数签名

## 使用示例

```rust
use coffee::semantic::SemanticAnalyzer;
use coffee::parser;

let source = r#"
fn add(x: int, y: int) => int:
    return x + y

let result: int = add(1, 2)
"#;

// 解析源代码
let program = parser::parse_program(source)?;

// 创建语义分析器
let type_registry = Arc::new(RwLock::new(TypeRegistry::root()));
let mut analyzer = SemanticAnalyzer::new(type_registry);

// 分析程序
for statement in program.statements {
    analyzer.analyze_statement(&statement)?;
}

// 检查错误
if analyzer.has_errors() {
    for error in analyzer.errors() {
        eprintln!("语义错误: {:?}", error);
    }
} else {
    println!("语义分析通过");
}
```

## 设计考虑

### 1. 多遍分析
分析器执行多次遍历：
- 声明收集遍
- 定义分析遍
- 使用验证遍

### 2. 增量分析
支持增量分析代码：
- 分析单个语句
- 在分析之间维护状态
- 重用符号表

### 3. 错误恢复
错误后继续分析：
- 收集所有错误
- 提供全面报告
- 不在第一个错误时停止

### 4. C 集成
C 函数的特殊处理：
- C 函数符号表
- Coffee 和 C 之间的类型映射
- ABI 验证

## 测试

测试覆盖应包括：
- 正确的符号解析
- 正确的作用域管理
- 生命周期验证
- 错误检测和报告
- 边缘情况（遮蔽、循环依赖）
- C 函数集成

## 未来增强

潜在改进：
- 更精确的生命周期分析
- 高级借用检查
- 泛型类型参数分析
- 特质系统支持
- 模块系统增强
- 更好的错误消息和建议