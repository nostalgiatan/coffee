# 运行时模块

## 概述

运行时模块（`src/runtime/`）为 Coffee 语言提供标准运行时库支持。它包含用 Coffee 本身编写的嵌入式运行时函数和实用程序，当用户代码使用运行时特性（如异常处理 `raise` 语句）时自动链接。

## 模块结构

```
src/runtime/
├── mod.rs          # 主运行时模块
└── embedded.rs     # 嵌入式运行时库
```

## 核心组件

### 1. 嵌入式运行时（`embedded.rs`）

嵌入式运行时提供用 Coffee 本身编写的基本运行时函数：

```rust
pub const RUNTIME_SOURCE: &str = r#"
/# Coffee 标准运行时库 #/

use puts, exit in libc of c

c fn coffee_panic(msg: string, len: int) => ():
    puts("PANIC: ")
    exit(1)
"#;
```

### 2. 运行时符号

运行时模块还声明应该自动可用的符号：

```rust
pub const RUNTIME_SYMBOLS: &[(&str, &str)] = &[
    ("coffee_panic", "c fn coffee_panic(msg: string, len: int) => ()"),
];
```

## 运行时函数

### coffee_panic

当程序遇到不可恢复的错误时调用 panic 函数：

```coffee
c fn coffee_panic(msg: string, len: int) => ():
    puts("PANIC: ")
    exit(1)
```

**参数：**
- `msg`：错误消息字符串
- `len`：消息的长度

**行为：**
- 打印 "PANIC: " 到标准输出
- 以状态码 1 退出程序

**使用：**
编译器在以下情况下自动调用 panic 函数：
- 发生未处理的异常
- 检测到关键的运行时错误
- 发生断言失败

## 与编译器的集成

### 自动链接

运行时在需要时自动链接：

```rust
// 在 compiler.rs 中
if uses_runtime_features {
    link_runtime();
}
```

### 运行时特性检测

编译器检测运行时特性的使用：

```rust
// 检查是否使用了 raise 语句
let uses_raise = program.statements.iter().any(|stmt| {
    matches!(stmt, Statement::Raise(_))
});

if uses_raise {
    // 链接运行时
}
```

### 符号注入

运行时符号自动注入到符号表中：

```rust
for (name, signature) in runtime::RUNTIME_SYMBOLS {
    symbol_table.declare(name, signature);
}
```

## 运行时特性

### 异常处理

运行时只支持用 `raise` **抛出**异常。`try` / `catch` / `except` 未实现；语言中没有捕获处理器。

```coffee
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("除以零")
    return a / b
```

当引发异常时：
1. 运行时记录异常
2. 栈被展开
3. 可能调用 `coffee_panic`（没有 `catch` 可恢复）

### Panic 处理

运行时优雅地处理 panic：

```coffee
fn assert(condition: bool, msg: string) => ():
    if not condition:
        raise AssertionError(msg)
```

### 错误报告

运行时提供错误报告功能：

```coffee
c fn coffee_panic(msg: string, len: int) => ():
    puts("PANIC: ")
    # 附加错误处理
    exit(1)
```

## 使用示例

### 基本异常处理

```coffee
fn safe_divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("不能除以零")
    return a / b

main(safe_divide(10, 2))  # 返回 5
```

### 自定义错误

```coffee
enum MyError:
    InvalidInput(string)
    OutOfRange(int, int)

fn validate(value: int, min: int, max: int) => ():
    if value < min or value > max:
        raise OutOfRange(value, max)
```

### 错误时 Panic

```coffee
fn require(condition: bool, msg: string) => ():
    if not condition:
        raise AssertionError(msg)
```

## 运行时架构

### 最小化设计

Coffee 运行时遵循最小化设计理念：

1. **嵌入式**：运行时嵌入在编译器中
2. **自动**：无需手动链接
3. **轻量级**：最小开销
4. **自包含**：所有运行时代码都是 Coffee

### 无垃圾收集器

Coffee 不使用垃圾收集器：

- 内存通过显式操作管理（`mv`、`clone`、`copy`、`rm`）
- 所有权系统防止内存泄漏
- 确定性内存清理

### 无标准库

Coffee 没有传统的标准库：

- 核心函数内置在语言中
- C 库函数通过 FFI 导入
- 可以创建和共享用户库

## 运行时与标准库

### 传统语言

大多数语言都有大型标准库：

```
Python:
  - os, sys, json, datetime, collections, itertools, ...

Java:
  - java.lang, java.util, java.io, java.net, ...

C++:
  - STL: vector, map, string, algorithm, ...
```

### Coffee 方法

Coffee 采用不同的方法：

```
Coffee:
  - 最小运行时（panic、基本错误处理）
  - C 库集成用于其他所有功能
  - 用户定义的库
```

**优势：**
- 更小的二进制大小
- 更快的编译
- 更多的灵活性
- 更好的 C 互操作性

## 未来增强

### 计划的运行时特性

1. **增强的错误处理**
   - 栈跟踪
   - 错误上下文
   - 自定义错误类型

2. **运行时反射**
   - 类型信息
   - 动态分派
   - 元数据访问

3. **并发支持**
   - 线程生成
   - 同步原语
   - 异步/等待

4. **标准库**
   - 集合类型
   - 字符串实用程序
   - 文件 I/O 助手

5. **内存管理**
   - 引用计数
   - 智能指针
   - 内存池

### 实验性特性

1. **JIT 编译**
   - 运行时代码生成
   - 动态优化
   - 热代码重载

2. **外部函数接口**
   - 动态库加载
   - 回调注册
   - 类型编组

3. **调试支持**
   - 断点
   - 变量检查
   - 单步执行

## 性能考虑

### 零成本抽象

运行时设计为零成本抽象：

- 未使用的特性无运行时开销
- 编译时优化
- 最少的运行时检查

### 内联函数

运行时函数在可能时内联：

```rust
// 编译器可能内联 panic 检查
if unlikely(error) {
    coffee_panic(msg, len);
}
```

### 静态链接

运行时静态链接：

- 无运行时依赖
- 可移植的可执行文件
- 快速启动

## 安全考虑

### Panic 安全

运行时确保 panic 安全：

- 干净的栈展开
- 资源清理
- 无内存泄漏

### 错误隔离

错误被正确隔离：

- 无未定义行为
- 无数据损坏
- 安全的错误传播

### 输入验证

运行时验证输入：

- 字符串长度检查
- 数组边界检查
- 空指针预防

## 测试

### 运行时测试

运行时函数经过广泛测试：

```rust
#[test]
fn test_panic() {
    // 测试 panic 行为
}

#[test]
fn test_exception_handling() {
    // 测试异常传播
}
```

### 集成测试

运行时在集成中测试：

```rust
#[test]
fn test_runtime_integration() {
    // 使用用户代码测试运行时
}
```

## 最佳实践

### 1. 谨慎使用异常

```coffee
# 好的：将异常用于异常情况
fn open_file(path: string) => File:
    if not file_exists(path):
        raise FileNotFoundError(path)
    return File(path)

# 不好的：将异常用于控制流
fn get_first(arr: [int]) => int:
    if arr.len() == 0:
        raise EmptyArrayError
    return arr[0]  # 更好使用 Option 类型
```

### 2. 提供清晰的错误消息

```coffee
# 好的
raise ValueError(f"期望正数，得到 {value}")

# 不好的
raise ValueError
```

### 3. 适当地处理错误

Coffee 只有 `raise`。`try` / `catch` / `except` 未实现，因此无法在 Coffee 中捕获已抛出的异常。在会 `raise` 的代码之前先检查条件，或在需要恢复时返回错误值而不是抛出。

```coffee
# 无 catch 的恢复：先校验再继续
fn process(input: string) => Result:
    if not is_valid(input):
        return Error("invalid input")
    return parse(input)
```

### 4. 清理资源

```coffee
# 好的：在 panic 之前清理
fn critical_operation() => Result:
    let resource = acquire_resource()
    if error_occurs():
        release_resource(resource)
        raise CriticalError
    return Success(resource)

# 不好的：在 panic 时泄漏资源
fn critical_operation() => Result:
    let resource = acquire_resource()
    if error_occurs():
        raise CriticalError  # 资源泄漏！
    return Success(resource)
```

## 与其他语言的比较

### Python

Python 可用 `try` / `except` 捕获异常。Coffee 不能：只有 `raise`；`try` / `catch` / `except` 未实现。

```python
# Python — catch 是语言语法
try:
    result = 10 / 0
except ZeroDivisionError as e:
    print(f"Error: {e}")
```

```coffee
# Coffee — 仅 raise；无 try/catch
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("除以零")
    return a / b
```

### Rust

```rust
// Rust
fn divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        Err("division by zero".to_string())
    } else {
        Ok(a / b)
    }
}
```

```coffee
# Coffee
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("除以零")
    return a / b
```

### C++

```cpp
// C++
int divide(int a, int b) {
    if (b == 0) {
        throw std::runtime_error("division by zero");
    }
    return a / b;
}
```

```coffee
# Coffee
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("除以零")
    return a / b
```

## 另请参阅

- [C 集成模块](c_integration.md) - C 语言 FFI
- [解析器模块](parser.md) - 解析 raise 语句
- [后端模块](backend.md) - 运行时的代码生成
- [编译器模块](compiler.md) - 运行时链接