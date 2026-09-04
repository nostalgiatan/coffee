# Coffee 编译器文档

## 概述

Coffee 是一个用 Rust 编写的现代编程语言编译器，使用 LLVM 作为后端。它是一个实验性的静态类型语言，具有类似 Python 的缩进语法，支持函数式和面向对象编程特性，以及与 C 语言的完美双向互操作性。

- **项目名称**: Coffee
- **版本**: 0.2.1
- **开发环境**: Rust 2024 edition (Termux on Android)
- **编译器后端**: LLVM (通过 inkwell crate)
- **解析器**: 使用 nom 库的自定义解析器

## 文档结构

本文档提供了关于 Coffee 编译器架构、模块和实现细节的全面信息。

### 核心模块

1. **[解析器模块](parser.md)** - 源代码解析和 AST 生成
2. **[语义分析模块](semantic.md)** - 作用域管理和符号解析
3. **[类型系统模块](types.md)** - 类型检查和推断
4. **[后端模块](backend.md)** - LLVM 代码生成
5. **[C 语言集成模块](c_integration.md)** - C 语言 FFI 支持
6. **[编译器前端模块](compiler.md)** - 项目编译编排
7. **[运行时模块](runtime.md)** - 标准运行时库

## 主要特性

### 语言特性
- 静态类型系统
- 类似 Python 的缩进语法
- 函数式编程支持
- 面向对象编程（类和继承）
- 模式匹配
- 先进的内存管理（mv/clone/copy/rm/alloc/free/load/store/clean 操作）
- 完美的 C 语言双向互操作
- 项目管理（coffee.toml）
- 异常处理（raise 语句）
- 丰富的表达式支持
- 数组索引访问

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

# 从 C 头文件生成 .cfc 文件
coffee --gen-cfc input.h

# 静态链接
coffee --static input.cf

# 指定目标三元组进行交叉编译
coffee --target x86_64-unknown-linux-gnu input.cf
```

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
# 标准库依赖
std = "0.2.0"

# Coffee包依赖
[dependencies.packages]
# coffee_package = "0.1.0"

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