# 诊断模块

## 概述

诊断模块（`src/diagnostics.rs`）为 Coffee 编译器提供全面的错误报告和诊断系统。它提供专业、彩色编码的错误消息，包含丰富的上下文、建议和相关信息，帮助开发者快速理解和解决问题。

## 模块目的

诊断系统服务于几个关键目的：

1. **错误报告**：清晰、结构化的错误消息
2. **上下文感知**：源代码位置和代码片段
3. **建议**：修复问题的有用提示
4. **错误分类**：带有代码的分类错误
5. **错误链**：因果关系
6. **相似名称检测**："你是想说吗？"建议

## 核心组件

### 1. 严重性级别（`Severity`）

严重性级别指示诊断消息的重要性：

```rust
pub enum Severity {
    Error,   // 阻止编译
    Warning, // 不阻止编译但应处理
    Hint,    // 改进建议
    Note,    // 附加信息
}
```

**颜色编码：**
- `Error`：红色（`\x1b[31m`）
- `Warning`：黄色（`\x1b[33m`）
- `Hint`：青色（`\x1b[36m`）
- `Note`：灰色（`\x1b[90m`）

### 2. 源代码位置（`SourceLocation`）

跟踪精确的源代码位置：

```rust
pub struct SourceLocation {
    pub file: Option<String>,      // 文件路径
    pub line_start: usize,         // 起始行（从1开始）
    pub line_end: usize,           // 结束行（从1开始）
    pub column_start: usize,       // 起始列（从1开始）
    pub column_end: usize,         // 结束列（从1开始）
    pub byte_offset: usize,        // 源代码中的字节偏移
    pub byte_length: usize,        // 字节长度
}
```

**使用示例：**
```rust
let loc = SourceLocation::with_file("example.cf", 10, 5, 15);
println!("{}", loc.format()); // "example.cf:10:5"
```

### 3. 错误分类（`ErrorKind`）

使用唯一错误代码分类错误：

```rust
pub enum ErrorKind {
    // 解析错误 (E001-E099)
    InvalidSyntax { context: String },
    UnexpectedToken { token: String, expected: Vec<String> },
    IncompleteInput { expected: String },

    // 类型错误 (E100-E199)
    TypeMismatch { expected: String, found: String },
    UnknownType { name: String },
    IncompatibleTypes { ty1: String, ty2: String },

    // 符号错误 (E200-E299)
    UndefinedSymbol { name: String, symbol_type: SymbolType },
    DuplicateDefinition { name: String, previous: SourceLocation },
    InvalidSymbolAccess { name: String, reason: String },

    // 内存错误 (E300-E399)
    UseAfterMove { name: String },
    UseAfterDrop { name: String },
    InvalidMemoryOperation { operation: String, reason: String },
    BorrowViolation { variable: String, reason: String },

    // 生命周期错误 (E400-E499)
    LifetimeError { variable: String, reason: String },
    BorrowConflict { variable: String },

    // 导入错误 (E500-E599)
    ModuleNotFound { module: String },
    CircularImport { path: Vec<String> },
    InvalidImport { import: String, reason: String },

    // 约束错误 (E600-E699)
    ConstraintViolation { constraint: String, reason: String },

    // 代码生成错误 (E700-E799)
    CodeGeneration { stage: String, details: String },

    // 验证错误 (E800-E899)
    Verification { phase: String, details: String },

    // 链接错误 (E900-E999)
    LinkError { details: String },
}
```

**错误代码范围：**
- `E001-E099`：解析错误
- `E100-E199`：类型错误
- `E200-E299`：符号错误
- `E300-E399`：内存错误
- `E400-E499`：生命周期错误
- `E500-E599`：导入错误
- `E600-E699`：约束错误
- `E700-E799`：代码生成错误
- `E800-E899`：验证错误
- `E900-E999`：链接错误

### 4. 符号类型（`SymbolType`）

分类符号类别：

```rust
pub enum SymbolType {
    Variable,  // 变量
    Function,  // 函数
    Type,      // 类型
    Module,    // 模块
    Lifetime,  // 生命周期
    Constant,  // 常量
    Method,    // 方法
    Field,     // 字段
}
```

### 5. 诊断消息（`Diagnostic`）

完整的诊断消息结构：

```rust
pub struct Diagnostic {
    pub severity: Severity,                    // 严重性级别
    pub kind: ErrorKind,                       // 错误类型
    pub message: String,                       // 主要消息
    pub location: SourceLocation,              // 源代码位置
    pub code: String,                          // 错误代码（如 "E200"）
    pub snippets: Vec<CodeSnippet>,            // 显示错误的代码片段
    pub suggestions: Vec<Suggestion>,          // 修复建议
    pub related: Vec<RelatedDiagnostic>,       // 相关诊断
    pub cause: Option<Box<Diagnostic>>,        // 错误链
    pub similar_names: Vec<String>,            // "你是想说吗？"建议
}
```

## 诊断功能

### 1. 代码片段

显示带有高亮的精确源代码：

```rust
pub struct CodeSnippet {
    pub lines: Vec<String>,                    // 源代码行
    pub line_start: usize,                     // 第一行的行号
    pub highlights: Vec<(usize, usize, usize)>, // 高亮范围
}
```

**输出示例：**
```
    10 | fn add(a: int, b: int) => int:
    11 |     return a + b
    12 | }
       |     ^--- 错误：意外的右大括号
```

### 2. 建议

提供修复问题的有用建议：

```rust
pub struct Suggestion {
    pub message: String,
}
```

**输出示例：**
```
    = 帮助：删除右大括号
    = 帮助：使用 'end' 关键字代替 '}'
```

### 3. 相关诊断

显示相关错误或注释：

```rust
pub struct RelatedDiagnostic {
    pub relation: RelationType,
    pub location: SourceLocation,
    pub message: String,
}

pub enum RelationType {
    CausedBy,  // 由...引起
}
```

**输出示例：**
```
error[E200]: 未定义的变量：'x'
   --> example.cf:5:10
    |
  5 |     return x + y
    |            ^ 此变量未定义
    |
note[E200]: 变量 'x' 在此处定义
   --> example.cf:3:5
    |
  3 | let x: int = 10
    |     ^--- 但它已超出作用域
```

### 4. 错误链

显示因果关系：

```rust
pub cause: Option<Box<Diagnostic>>,
```

**输出示例：**
```
error[E900]: 链接错误：找不到库 'mylib'
   --> main.cf:1:1
    |
  1 | use myfunction in mylib of c
    | ^--- 找不到库

caused by:
    error[E500]: 未找到模块 'mylib'
       --> 搜索路径：./, ./lib, /usr/lib
```

### 5. 相似名称检测

为拼写错误建议相似名称：

```rust
pub fn levenshtein_distance(a: &str, b: &str) -> usize;
pub fn find_similar_names(target: &str, candidates: &[String], max_distance: usize, max_results: usize) -> Vec<String>;
```

**输出示例：**
```
error[E200]: 未定义的变量：'lenght'
   --> example.cf:3:10
    |
  3 | let lenght: int = 10
    |     ^------ 你是想说：`length`
```

## 诊断发射器

`DiagnosticEmitter` 收集和管理诊断：

```rust
pub struct DiagnosticEmitter {
    diagnostics: Arc<RwLock<Vec<Diagnostic>>>,
    config: EmitterConfig,
}

impl DiagnosticEmitter {
    pub fn new() -> Self;
    pub fn emit(&self, diagnostic: Diagnostic);
    pub fn diagnostics(&self) -> Vec<Diagnostic>;
    pub fn clear(&self);
}
```

## 诊断格式化

### 彩色输出

```rust
pub fn format(&self) -> String;
```

**示例：**
```
error[E200]: 未定义的变量：'x'
   --> example.cf:5:10
    |
  5 |     return x + y
    |            ^ 此变量未定义
    |
    | 这是一个符号错误
    = 帮助：在使用前声明变量
```

### 纯文本输出

```rust
pub fn format_plain(&self) -> String;
```

**示例：**
```
error[E200]: 未定义的变量：'x'
   --> example.cf:5:10
  5 |     return x + y
    |            ^ 此变量未定义
help: 在使用前声明变量
```

## 错误类别

### 解析错误 (E001-E099)

**E001: 无效语法**
```coffee
# 错误
fn add(a: int, b: int => int:
```

**E002: 意外的令牌**
```coffee
# 错误
let x: int = 10 20
```

**E003: 不完整的输入**
```coffee
# 错误
fn add(a: int, b: int):
```

### 类型错误 (E100-E199)

**E100: 类型不匹配**
```coffee
# 错误
let x: int = "hello"
```

**E101: 未知类型**
```coffee
# 错误
let x: mytype = 10
```

**E102: 不兼容的类型**
```coffee
# 错误
fn add(a: int, b: string) => int:
```

### 符号错误 (E200-E299)

**E200: 未定义的符号**
```coffee
# 错误
fn example() => int:
    return x  # x 未定义
```

**E201: 重复定义**
```coffee
# 错误
let x: int = 10
let x: int = 20  # 重复
```

**E202: 无效的符号访问**
```coffee
# 错误
fn example() => int:
    let x: int = 10
    rm x
    return x  # 删除后使用
```

### 内存错误 (E300-E399)

**E300: 移动后使用**
```coffee
# 错误
let x: int = 10
mv x y
return x  # 移动后使用
```

**E301: 删除后使用**
```coffee
# 错误
let x: int = 10
rm x
return x  # 删除后使用
```

**E302: 无效的内存操作**
```coffee
# 错误
mv x x  # 无效：移动到自身
```

**E303: 借用违规**
```coffee
# 错误
let x: int = 10
let y: &int = &x
mv x z  # 借用时不能移动
```

### 生命周期错误 (E400-E499)

**E400: 生命周期错误**
```coffee
# 错误
fn get_ref() => &int:
    let x: int = 10
    return &x  # 返回局部变量的引用
```

**E401: 借用冲突**
```coffee
# 错误
let x: int = 10
let y: &mut int = &mut x
let z: &int = &x  # 可变借用时不能借用
```

### 导入错误 (E500-E599)

**E500: 未找到模块**
```coffee
# 错误
use nonexistent_module
```

**E501: 循环导入**
```coffee
# 错误
# module_a.cf
use module_b

# module_b.cf
use module_a
```

**E502: 无效的导入**
```coffee
# 错误
use symbol in nonexistent_module
```

### 代码生成错误 (E700-E799)

**E700: 代码生成错误**
```rust
// LLVM IR 生成期间的内部错误
```

### 验证错误 (E800-E899)

**E800: 验证错误**
```rust
// LLVM IR 验证失败
```

### 链接错误 (E900-E999)

**E900: 链接错误**
```coffee
# 错误
use myfunction in nonexistent_lib of c
```

## 与其他模块的集成

### 解析器集成

解析器为语法错误创建诊断：

```rust
impl From<parser::ParseError> for Diagnostic {
    fn from(error: parser::ParseError) -> Self {
        Diagnostic::new(
            Severity::Error,
            ErrorKind::InvalidSyntax { context: error.message },
            error.message
        ).with_location(error.location)
    }
}
```

### 类型系统集成

类型系统为类型错误创建诊断：

```rust
impl From<types::TypeSystemError> for Diagnostic {
    fn from(error: types::TypeSystemError) -> Self {
        match error {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                Diagnostic::new(
                    Severity::Error,
                    ErrorKind::TypeMismatch {
                        expected: format!("{:?}", expected),
                        found: format!("{:?}", found),
                    },
                    format!("类型不匹配：期望 {}，找到 {:?}", expected, found)
                )
            }
            // ... 其他情况
        }
    }
}
```

### 语义分析集成

语义分析器为语义错误创建诊断：

```rust
impl From<semantic::SpaceError> for Diagnostic {
    fn from(error: semantic::SpaceError) -> Self {
        Diagnostic::new(
            Severity::Error,
            ErrorKind::UndefinedSymbol {
                name: error.name,
                symbol_type: SymbolType::Variable,
            },
            format!("未定义的变量：'{}'", error.name)
        ).with_similar_names(error.suggestions)
    }
}
```

## 使用示例

### 创建简单诊断

```rust
use diagnostics::{Diagnostic, ErrorKind, Severity};

let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::UndefinedSymbol {
        name: "x".to_string(),
        symbol_type: SymbolType::Variable,
    },
    "未定义的变量：'x'"
);

println!("{}", diagnostic.format());
```

### 创建带位置的诊断

```rust
let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::TypeMismatch {
        expected: "int".to_string(),
        found: "string".to_string(),
    },
    "类型不匹配：期望 int，找到 string"
).with_location(SourceLocation::with_file("example.cf", 5, 10, 20));
```

### 添加建议

```rust
let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::InvalidSyntax {
        context: "缺少右大括号".to_string(),
    },
    "缺少右大括号"
).with_suggestion(Suggestion::new("在末尾添加右大括号 '}'"));
```

### 添加代码片段

```rust
let snippet = CodeSnippet::new(
    vec![
        "fn add(a: int, b: int) => int:".to_string(),
        "    return a + b".to_string(),
    ],
    1
);

let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::InvalidSyntax { context: "缺少返回类型".to_string() },
    "缺少返回类型"
).with_snippet(snippet);
```

### 错误链

```rust
let cause = Diagnostic::new(
    Severity::Error,
    ErrorKind::ModuleNotFound { module: "mylib".to_string() },
    "未找到模块 'mylib'"
);

let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::InvalidImport {
        import: "mylib".to_string(),
        reason: "未找到模块".to_string(),
    },
    "无法导入 'mylib'"
).with_cause(Box::new(cause));
```

### 使用诊断发射器

```rust
let emitter = DiagnosticEmitter::new();

// 发射诊断
emitter.emit(diagnostic1);
emitter.emit(diagnostic2);

// 获取所有诊断
let diagnostics = emitter.diagnostics();

// 清除诊断
emitter.clear();
```

## 最佳实践

### 1. 提供清晰的消息

```rust
// 好的
"未定义的变量：'x'"

// 不好的
"错误：找不到变量"
```

### 2. 包含源代码位置

```rust
// 始终包含位置
diagnostic.with_location(location)
```

### 3. 提供有用的建议

```rust
diagnostic.with_suggestion(Suggestion::new(
    "在使用前声明变量"
))
```

### 4. 使用适当的严重性

```rust
// 对关键问题使用 Error
Diagnostic::new(Severity::Error, ...)

// 对非关键问题使用 Warning
Diagnostic::new(Severity::Warning, ...)

// 对建议使用 Hint
Diagnostic::new(Severity::Hint, ...)
```

### 5. 添加代码片段

```rust
diagnostic.with_snippet(snippet)
```

### 6. 建议相似名称

```rust
diagnostic.with_similar_names(vec!["length".to_string()])
```

## 配置

### 发射器配置

```rust
pub struct EmitterConfig {
    pub show_colors: bool,
}
```

**使用：**
```rust
let config = EmitterConfig {
    show_colors: false, // 禁用颜色
};

let emitter = DiagnosticEmitter::with_config(config);
```

## 性能考虑

### 1. 字符串分配

通过尽可能使用 `&str` 来最小化字符串分配：

```rust
// 好的
Diagnostic::new(Severity::Error, ..., "message")

// 避免
Diagnostic::new(Severity::Error, ..., format!("message: {}", x))
```

### 2. 诊断收集

使用 `Arc<RwLock>` 进行高效共享：

```rust
diagnostics: Arc<RwLock<Vec<Diagnostic>>>
```

### 3. 相似名称搜索

限制搜索空间以提高性能：

```rust
let similar = find_similar_names(
    target,
    &candidates,
    max_distance: 3,  // 限制距离
    max_results: 5    // 限制结果
);
```

## 未来增强

- 多行错误范围
- 更好的错误恢复建议
- 错误代码文档
- 交互式错误修复
- 错误抑制指令
- 自定义错误处理程序
- 错误聚合
- 错误趋势分析
- 用于建议的机器学习
- IDE 集成支持

## 另请参阅

- [解析器模块](parser.md) - 解析和语法错误
- [语义分析模块](semantic.md) - 语义错误
- [类型系统模块](types.md) - 类型错误
- [编译器模块](compiler.md) - 编译编排