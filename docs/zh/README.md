# Coffee 编译器文档

## 概述

Coffee 是一个用 Rust 编写的现代编程语言编译器，使用 LLVM 作为后端。它是一个实验性的静态类型语言，具有类似 Python 的缩进语法，支持函数式和面向对象编程特性，以及与 C 语言的完美双向互操作性。

- **项目名称**: Coffee
- **版本**: 0.3.9（**不是 1.0**；每个中版本满 10 个 patch 再升。1.0 需要产品门槛，见仓库根 [README](../../README.md)。）
- **开发环境**: Rust 2024 edition (Termux on Android)
- **编译器后端**: LLVM (通过 inkwell crate)
- **解析器**: 使用 nom 库的自定义解析器

仓库根目录 [README](../../README.md) 写了构建依赖和当前管线（函数体必须走 MIR、`stmt_spans`、`NestedDecl`）。

## 文档结构

本文档提供了关于 Coffee 编译器架构、模块和实现细节的全面信息。

### 核心模块

1. **[解析器模块](parser.md)** - 源代码解析和 AST 生成
2. **[语义分析模块](semantic.md)** - 作用域管理和符号解析
3. **[类型系统模块](types.md)** - 类型检查和推断
4. **[后端模块](backend.md)** - LLVM 代码生成
5. **[C 语言集成模块](c_integration.md)** - C 语言 FFI 支持
6. **[编译器前端模块](compiler.md)** - 项目编译编排
7. **[运行时](runtime.md)** — 无 runtime crate；`raise` 走 libc / `#监听器`

## 主要特性

### 语言特性
- 静态类型系统（`int(N)+` / `int(N)-` / `float(N)`：**N 是字节数**，如 `int(4)+` 对应 C `int`）
- 类似 Python 的缩进语法
- 函数式编程支持
- 面向对象编程（类和继承）
- 模式匹配
- 先进的内存管理（值类型作用域结束自动清理；资源用 `mv`/`clone`/`rm`；简单变量的 **last-use** 视为隐式搬走）。过程内 `&`/`&mut`，地点含变量、字段 `p.x`、下标 `a[i]`。类 `clone` 对嵌套 `str` / class / 资源数组 / 元组是深拷贝；`object`、引用、切片字段仍浅拷贝。`[T; N]` 与元组资源字段随容器 drop；class 的 `[T]` 按胖指针长度 drop **元素**、不 `free` 缓冲区；`object` 不释放载荷。**`buf`** 是 Coffee 拥有的 `malloc` 指针（drop 会 `free`）；`clone buf` 是类型错误。
- 完美的 C 语言双向互操作
- 项目管理（coffee.toml；包依赖是 `path` 与 `url`+`hash` 二选一，不是 semver 字符串）。官方 **std** 嵌在编译器里（`coffee std install`）；除非 toml 写了 `[dependencies.packages.std]`，编译会自动前置导入根。用户程序写 `use print in std`（或 `use * in std`）。`of c` 约定写在 `library/std/src/sys.cf`，不要写在普通用户代码里。可增长整数缓冲：`use mem` 后 `IntBuf::new` / `push` / `get` / `length`。
- `coffee fetch` 拉取 url+hash 包到缓存（`$COFFEE_CACHE` / `~/.cache/coffee`）
- 泛型 v1：`class List<T>:` / `List<int>` → `List__int`（无 `T: Trait`，无标准库泛型 `List`；std 提供 **`IntBuf`**）
- `raise` 默认中止（fprintf + exit）；函数带 `#name` 时由监听器接收；没有 `try`/`catch`
- 丰富的表达式支持
- 数组索引访问（`[T; N]`）；Coffee 函数上的 `[T]` 是胖指针 `{ptr,len}`（`c fn` 仍拒绝切片）
- `coffee run` / `coffee test`（也可用 `--bin` / `--jit`）

### 编译功能
- LLVM IR 生成
- 多种输出格式（目标文件、汇编、位码、可执行文件）
- JIT 执行
- 跨平台编译支持
- 优化级别（O0-O3）
- 静态/动态链接
- 交叉编译支持

### C 语言互操作性（核心特性）
- **双向互操作**: Coffee 函数可以被 C 代码调用，C 函数也可以被 Coffee 调用
- **.cfc 文件**: Coffee 的 C 函数描述文件，用于描述 C 函数签名
- **c fn 语法**: 用于从 C 头文件生成 .cfc 文件，即使没有 .h 文件只要有库也能链接
- **无头文件依赖**: 通过 .cfc 文件实现对 C 库的链接，无需原始头文件
- **自动类型映射**: Coffee类型与C类型自动转换
- **C标准库支持**: 内置libc常见函数签名
- **C头文件生成**: 从Coffee函数生成C头文件
- **clang集成**: 使用clang解析C头文件生成.cfc文件

## 快速开始

### 安装

```bash
# 构建编译器
cargo build --release

# 添加到 PATH（可选）
export PATH=$PATH:/path/to/coffee/target/release
```

### 基本使用

```bash
# 编译单个文件
coffee input.cf

# 生成 LLVM IR
coffee --emit-llvm input.cf

# 生成汇编代码
coffee --emit-asm input.cf

# 生成位码文件
coffee --emit-bc input.cf

# 优化编译
coffee -O2 input.cf

# 生成可执行文件
coffee --bin input.cf

# JIT 执行
coffee --jit input.cf

# 初始化新项目
coffee init

# 用户程序：`use print in std`（`of c` 写在 library/std/src/sys.cf）

# 安装随编译器捆绑的标准库
coffee std install

# 拉取 coffee.toml 里的 url+hash 包依赖到缓存
coffee fetch

# 下载 .tar.gz、打印树哈希并缓存
coffee fetch https://example.com/foo.tar.gz

# 同上，并写入 [dependencies.packages.<name>]
coffee fetch https://example.com/foo.tar.gz --save foo

# 从 C 头文件生成 .cfc 文件
coffee --gen-cfc input.h

# 静态链接
coffee --static input.cf

# 指定目标三元组进行交叉编译
coffee --target x86_64-unknown-linux-gnu input.cf
```

典型程序（若查找不到 `library/std` 则先 `coffee std install`）：

```coffee
use print in std

fn main() => int:
    print("Hello, Coffee!\n")
    return 0
```

`of c` 写在 `library/std/src/sys.cf`。用 `coffee run`（工程或单文件）或 `--bin` / `--jit`。

### 项目结构

标准的 Coffee 项目结构：

```
project_name/
├── coffee.toml          # 项目配置文件
├── src/                 # 源码目录
│   ├── main.cf          # 主入口文件
│   ├── lib/             # 库模块
│   │   ├── module1.cf
│   │   └── module2.cf
│   └── utils/           # 工具模块
│       └── helper.cf
├── lib/                 # C函数描述文件(.cfc)
│   ├── libc.cfc
│   └── libm.cfc
├── target/              # 输出目录
│   ├── debug/           # 调试版本
│   ├── release/         # 发布版本
│   └── include/         # 生成的C头文件
└── tests/               # 测试目录
    ├── test_module1.cf
    └── test_module2.cf
```

## 项目配置（coffee.toml）

```toml
[package]
name = "project_name"
version = "0.1.0"
authors = ["Author Name <email@example.com>"]
description = "Project description"

[dependencies]
# std 随编译器捆绑；缺失时运行 coffee std install。不要写本机 path= 指向 std。

# Coffee 包依赖：path XOR url+hash（不能写成 foo = "1.0"）
# hash 是解包后整棵树的 SHA-256
# [dependencies.packages.foo]
# url = "https://example.com/foo.tar.gz"
# hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
# [dependencies.packages.local_bar]
# path = "../bar"

# C库依赖
[dependencies.c_libraries.libm]
name = "m"
headers = ["math.h"]
include_paths = ["/usr/include"]
link_flags = ["-L/usr/lib"]
static_link = false
static_lib_path = ""

[build]
main = "src/main"
target_dir = "target"
src_dir = "src"
have_c = true

[target]
opt_level = 2
triple = "x86_64-unknown-linux-gnu"
```

## 架构概述

Coffee 编译器遵循传统的多阶段编译架构：

1. **解析阶段**: 源代码 → 抽象语法树（AST）
2. **语义分析阶段**: AST → 带有符号信息的增强 AST
3. **类型检查阶段**: 增强 AST → 类型验证的 AST
4. **代码生成阶段**: 类型验证的 AST → LLVM IR
5. **优化阶段**: LLVM IR → 优化的 LLVM IR
6. **代码输出阶段**: 优化的 LLVM IR → 目标文件/汇编/可执行文件

## 开发规范

### 编码风格
- 使用 Rust 编码规范
- 遵循 LLVM 代码生成最佳实践
- 使用有意义的变量和函数名
- 添加适当的注释和文档

### 测试实践
- 单元测试使用 Rust 的测试框架
- 集成测试验证编译器功能
- 性能测试评估编译器优化效果

### 贡献指南
- 遵循 Rust 最佳实践
- 保持代码风格一致性
- 添加适当的测试用例
- 更新相关文档

## 依赖项

### 主要依赖
- `inkwell`: LLVM 绑定（v0.7.1）
- `nom`: 解析器组合器（v8.0.0）
- `toml`: TOML 配置解析
- `serde`: 序列化/反序列化
- `rayon`: 并发处理
- `fs2`: 跨平台文件锁
- `libc`: C 库接口
- `clang-sys`: Clang 系统接口

## 项目状态

Coffee 编译器是一个实验性的现代编译器项目，专注于探索新型编程语言设计和实现技术。项目支持完整的编译器前端、类型检查、LLVM 代码生成和优化，并实现了与 C 语言的完美双向互操作性。其最大的特点是与 C 语言的无缝集成能力，通过 .cfc 文件实现 C 库的链接，而无需原始头文件。它特别适合用于学习编译器设计和实现。

## 许可证

请参考项目的 LICENSE 文件了解许可信息。

## 联系方式

如有问题、建议或贡献，请参考项目仓库。