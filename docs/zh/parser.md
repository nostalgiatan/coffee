# 解析器模块

## 概述

解析器模块（`src/parser/`）负责将 Coffee 源代码解析为抽象语法树（AST）。它使用 `nom` 解析器组合器库实现自定义解析器，支持 Coffee 独特的语法特性，包括基于缩进的块结构、内存管理操作、模式匹配和 C 语言集成。

## 模块结构

```
src/parser/
├── mod.rs           # 主解析器模块和协调
├── import.rs        # 导入语句解析
├── function.rs      # 函数定义解析
├── main.rs          # 主入口点解析
├── if.rs            # if 表达式解析
├── while.rs         # while 循环解析
├── match.rs         # match 表达式解析
├── for.rs           # for 循环解析
├── var.rs           # 变量声明和控制流解析
├── comment.rs       # 注释解析
├── class.rs         # 类和枚举定义解析
├── memory.rs        # 内存操作解析
├── expr.rs          # 表达式解析
├── error.rs         # 错误类型和处理
├── tracker.rs       # 块跟踪（用于缩进）
├── indent.rs        # 缩进处理
└── raise.rs         # raise 语句解析
```

## 核心组件

### 1. 主解析器（`mod.rs`）

主解析器通过 `parse_program` 函数协调整个解析过程。

#### 关键函数

**`parse_program(input: &str) -> Result<Program, Vec<ParseError>>`**

将整个 Coffee 源文件解析为 AST。该函数逐行处理输入，根据结构和内容识别每条语句的适当解析策略。

**解析策略：**
1. 跳过空行和整行注释
2. 检查多行注释（`/#* ... *#/`）
3. 尝试解析多行结构（函数、类、枚举、控制流）
4. 尝试解析单行语句
5. 智能错误检测和恢复

**错误恢复：**
解析器在遇到语法错误时继续解析，收集所有错误以进行全面报告。

**`parse_multiline_statement(lines: &[&str]) -> Option<(Statement, usize)>`**

尝试解析复杂的多行构造，包括：
- 函数（`fn` 和 `c fn`）
- 类（普通类和 `packed` 类）
- 枚举
- 控制流语句（if、while、for、match）

**`parse_single_line_statement(line: &str) -> Option<Statement>`**

使用基于第一个词的快速分发解析单行语句：
- 主入口点（`main(...)`）
- 导入语句（`use ...`）
- 变量声明（`let ...`）
- return/break/continue 语句
- 注释
- 内存操作
- 表达式语句

**`collect_multiline_content(lines: &[&str], keyword: &str) -> Option<(String, usize)>`**

通过使用 `BlockTracker` 跟踪缩进级别和块结构来收集多行构造的内容。

### 2. 块跟踪（`tracker.rs`）

`BlockTracker` 负责正确识别嵌套结构并确定块何时结束。

**关键特性：**
- 跟踪缩进级别
- 管理嵌套块类型（函数、类、控制流）
- 处理特殊情况如 `elif`/`else` 子句
- 验证块闭合

**块类型：**
```rust
pub enum BlockType {
    Function,
    Class,
    Enum,
    IfStatement,
    WhileLoop,
    ForLoop,
    MatchExpr,
}
```

### 3. 语句类型

解析器通过 `Statement` 枚举支持所有 Coffee 语言构造：

```rust
pub enum Statement {
    Import(Import),
    Function(Function),
    Main(MainEntry),
    If(IfExpr),
    While(WhileLoop),
    Match(MatchExpr),
    For(ForLoop),
    VariableDecl(VariableDecl),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    SingleLineComment(SingleLineComment),
    MultiLineComment(MultiLineComment),
    Class(ClassDef),
    Enum(EnumDef),
    MemoryOp(MemoryOp),
    Raise(RaiseStmt),
    Expr(Box<Expr>),
}
```

### 4. 导入解析（`import.rs`）

处理各种导入语句格式：

**导入类型：**
- 简单导入：`use module_name`
- 别名导入：`use module_name as alias`
- 模块内导入：`use function_name in module_name`
- 带别名的模块内导入：`use function_name in module_name as alias`
- 多符号导入：`use func1, func2 in module_name`
- C 库导入：`use printf in libc of c`

### 5. 函数解析（`function.rs`）

解析函数定义，支持：

**函数类型：**
- 普通 Coffee 函数：`fn name(params) => return_type:`
- C ABI 函数：`c fn name(params) => return_type:`
- 带错误处理器的函数：`fn name(params) #error_handler => return_type:`

**组件：**
- 函数名
- 参数（名称、类型、默认值）
- 返回类型
- 函数体（语句）
- C ABI 标志

### 6. 类解析（`class.rs`）

处理类和枚举定义：

**类类型：**
- 普通类：`class ClassName:`
- 紧凑类：`packed class ClassName:`（字段间无填充）
- 枚举：`enum EnumName:`

**类特性：**
- 带类型的字段定义
- 方法定义
- 继承：`class Derived of Parent:`
- 位域支持：`field: type:bit_width`

**枚举特性：**
- 简单变体：`Red`、`Green`、`Blue`
- 带数据的变体：`SomeValue(arg_type)`
- 结构体式变体：`Rgb(r: int, g: int, b: int)`

### 7. 控制流解析

#### If 表达式（`if.rs`）
```coffee
if condition:
    body
elif condition2:
    body2
else:
    body3
```

#### While 循环（`while.rs`）
```coffee
while condition:
    body
```

#### For 循环（`for.rs`）
支持多种迭代模式：
```coffee
# 集合迭代
for item in collection:
    body

# 范围迭代
for i in range(start, end):
    body

# 范围语法
for i in start..end:
    body
```

#### Match 表达式（`match.rs`）
```coffee
match value:
    pattern1 => result1
    pattern2 => result2
    _ => default_result
```

### 8. 内存操作（`memory.rs`）

解析 Coffee 独特的内存管理操作：

**操作：**
- `mv source target` - 移动所有权
- `clone source target` - 创建副本
- `copy source target` - 共享引用
- `rm variable` - 删除变量
- `rm var1, var2, var3` - 批量删除
- `clean out` - 清理所有变量
- `clean out except var1, var2` - 除指定变量外清理
- `clean out var1, var2` - 清理指定变量

### 9. 表达式解析（`expr.rs`）

解析各种表达式类型：

**表达式类型：**
- 字面量（整数、浮点数、字符串、布尔值）
- 变量
- 二元操作（`+`、`-`、`*`、`/`、`%`、`==`、`!=`、`<`、`>`、`<=`、`>=`、`&&`、`||`）
- 一元操作（`!`、`-`）
- 函数调用
- 成员访问（`object.field`）
- 数组索引（`array[index]`）
- 格式化字符串（`f"Hello {name}!"`）
- 元组（`(value1, value2, value3)`）

### 10. 错误处理（`error.rs`）

提供全面的错误检测和报告：

**错误类型：**
```rust
pub enum ParseError {
    GenericSyntaxError { line: usize, context: String, hint: String },
    UnexpectedToken { line: usize, expected: String, found: String },
    MissingColon { line: usize },
    InvalidIndentation { line: usize, expected: usize, found: usize },
    UnterminatedBlock { line: usize },
    // ... 更多错误类型
}
```

**智能错误检测：**
解析器分析语法错误以提供具体、可操作的错误消息和修正提示。

### 11. 注释解析（`comment.rs`）

支持两种注释样式：
- 单行：`/#/ 这是注释`
- 多行：`/#* 这是多行注释 *#/`

### 12. Raise 语句（`raise.rs`）

解析异常抛出：
```coffee
raise ErrorType(arguments)
```

## 解析算法

解析器使用混合方法：

1. **基于行的解析**：逐行处理源代码
2. **缩进跟踪**：使用 `BlockTracker` 管理块结构
3. **关键字分发**：通过第一个词快速识别语句类型
4. **错误恢复**：错误后继续解析以收集所有问题

### 多行内容收集

`collect_multiline_content` 函数实现复杂的块跟踪：

1. 在第一行建立基础缩进级别
2. 使用栈跟踪嵌套块
3. 处理特殊情况（elif/else 继续）
4. 检测块结束条件：
   - 缩进超过基础级别
   - 相同级别的不同顶级关键字
   - 无嵌套时相同级别的相同关键字

### 语句类型检测

`StatementType` 枚举提供快速分类：

```rust
pub enum StatementType {
    Function,
    Class,
    Enum,
    Import,
    VariableDecl,
    If,
    While,
    For,
    Match,
    Return,
    Break,
    Continue,
    Comment,
    Main,
    MemoryOp,
    Unknown,
}
```

## 设计决策

### 1. Nom 解析器组合器

使用 `nom` 提供：
- 可组合的解析原语
- 良好的错误消息
- 高效的解析
- 易于测试单个解析器

### 2. 基于缩进的块

Coffee 类似 Python 的缩进需要：
- 显式的缩进跟踪
- 块验证
- 控制流继续的特殊处理

### 3. 错误恢复策略

解析器：
- 错误后继续
- 收集所有错误
- 提供上下文和提示
- 不在第一个错误时快速失败

### 4. 混合解析方法

结合：
- 单行解析用于简单语句
- 多行解析用于复杂结构
- 块跟踪用于正确嵌套

## 使用示例

```rust
use coffee::parser;

let source = r#"
fn add(x: int, y: int) => int:
    return x + y

main(add(1, 2))
"#;

match parser::parse_program(source) {
    Ok(program) => {
        println!("解析了 {} 条语句", program.statements.len());
        for stmt in program.statements {
            println!("{:?}", stmt);
        }
    }
    Err(errors) => {
        for error in errors {
            eprintln!("解析错误: {:?}", error);
        }
    }
}
```

## 测试

解析器模块应测试：
- 所有语言构造的正确解析
- 错误检测和报告
- 边缘情况（空输入、格式错误的代码）
- 嵌套结构
- 注释和空白处理

## 未来增强

潜在改进：
- 更好的错误恢复
- 更精确的错误位置
- 语法高亮支持
- IDE 集成（语言服务器）
- 宏系统支持
- 更好的错误消息和代码片段