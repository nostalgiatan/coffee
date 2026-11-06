# Coffee 编译器项目文档

## 项目概述

Coffee 是一个用 Rust 编写的现代编程语言编译器，使用 LLVM 作为后端。它是一个实验性的静态类型语言，具有类似 Python 的缩进语法，支持函数式和面向对象编程特性，以及与 C 语言的完美双向互操作性。

- **项目名称**: Coffee
- **版本**: 0.3.9（从 0.2.2 起满 10 次落地到 0.3.0，再 patch；之后每 10 个 patch 升中版本）。`coffee --version` 为 `0.3.9+` 加编译器源码哈希前 12 位；同 patch 重装仍会使工程 `.o` 缓存失效。
- **开发环境**: Rust 2024 edition (Termux on Android)
- **编译器后端**: LLVM (通过 inkwell crate) 
- **解析器**: 使用 nom 库的自定义解析器

## 主要特性

### 语言特性
- 静态类型系统
- 类似 Python 的缩进语法
- 函数式编程支持
- 面向对象编程 (类和继承)
- 模式匹配
- 先进的内存管理（值类型作用域结束自动清理；资源用 `mv`/`clone`/`rm`；过程内 `&`/`&mut` 借用检查）
- 完美的 C 语言双向互操作
- 项目管理 (coffee.toml)
- 异常处理 (raise 语句)
- 丰富的表达式支持
- 数组索引访问

### 编译功能
- LLVM IR 生成
- 多种输出格式 (object file, assembly, bitcode, executable)
- JIT 执行
- 跨平台编译支持
- 优化级别 (O0-O3)
- 静态/动态链接
- 交叉编译支持

### C 语言互操作性 (核心特性)
- **双向互操作**: Coffee 函数可以被 C 代码调用，C 函数也可以被 Coffee 调用
- **.cfc 文件**: Coffee 的 C 函数描述文件，用于描述 C 函数签名
- **c fn 语法**: 用于从 C 头文件生成 .cfc 文件，即使没有 .h 文件只要有库也能链接
- **无头文件依赖**: 通过 .cfc 文件实现对 C 库的链接，无需原始头文件
- **自动类型映射**: Coffee类型与C类型自动转换
- **C标准库支持**: 内置libc常见函数签名
- **C头文件生成**: 从Coffee函数生成C头文件
- **clang集成**: 使用clang解析C头文件生成.cfc文件

### 安全功能
- 路径验证 (防止目录遍历攻击)
- 文件锁 (防止并发编译冲突)
- 符号链接验证
- C 函数类型检查

## 项目架构

### 核心模块

1. **Parser (src/parser/)**: 解析 Coffee 源代码
   - 表达式在 `expr/`（不是 `expr.rs`）
   - 支持函数、类、枚举、控制流语句
   - 缩进和块结构跟踪
   - 错误恢复机制

2. **Semantic Analysis (src/semantic/)**: 语义分析
   - 主分析器在 `analyzer/`（`mod.rs` + decls/expr/const_eval/report/memory）
   - 作用域管理、符号表、生命周期（`scope.rs` / `symbols.rs` / `lifetime.rs`）

3. **Type System (src/types/)**: 类型系统
   - `definition/` 类型定义，`checker/` 类型检查
   - `borrow.rs` 过程内借用；`registry.rs` / `errors.rs`

4. **Backend (src/backend/)**: LLVM 代码生成
   - `CodeGenerator::compile_program_with_hir`；完整 MIR 在 `mir_gen.rs` 编译
   - `if`/`while`、范围与集合 `for`（`ForRange`）、`match`（降为 `If`）、`raise` 为 MIR 原生；到达 codegen 的 `MirFn` 为 `complete: true` 且不含残留 `Match`/`ForIn`；codegen 不报 `internal leftover`
   - 函数体里的嵌套 class/fn 是 `MirStmt::Nested(NestedDecl)`（源名 + hir/LLVM key），不是整份 `Statement` 克隆。无法降的 `match`/`for`（非 simple match、非 array/slice/tuple/literal 集合 for）由 `lower_function` `Err`（Error 诊断），该函数不进 `hir_fns`；缺 MIR 是硬错误（`missing MIR for function`），不是 AST 回退。`compile_program` 是死接口（禁止空 MIR）。`compile_statement` 的 Match/For/Raise 若被走到会报错。`p.x` 再 `rm` 仍产出完整 MIR
   - 语句级 MIR，不是 SSA

5. **C Integration (src/c/)**: C 语言集成
   - C 库函数解析
   - C 函数签名生成
   - .cfc 文件生成和处理
   - C头文件生成
   - 双向互操作实现

6. **Compiler Frontend (`src/compiler.rs` + `src/compiler/`)**: 编译器前端
   - 流水线在 `pipeline/`（`mod.rs`、`parse.rs`、`imports.rs`、`collect.rs`），不是 `pipeline.rs`
   - `CompilationResult` 含 `program` / `c_imports` / `cfc_symbols` / `hir_fns`
   - 项目配置、编译单元、构建调度

### 语言语法详述

#### 1. 注释语法
- **单行注释**: `/#/ 这是注释`
- **多行注释**: `/#* 这是多行注释 *#/`

#### 2. 变量声明
```coffee
let name: type = value
```

#### 3. 函数定义
```coffee
# 普通Coffee函数
fn function_name(param1: type1, param2: type2) => return_type:
    body

# C ABI函数（可被C调用）
c fn function_name(param1: type1, param2: type2) => return_type:
    body

# 带错误监听器的函数
fn function_name(param: type) #on_err => return_type:
    body
```

#### 4. 类定义
```coffee
# 普通类
class ClassName:
    field1: type1
    field2: type2
    
    fn method(self, param: type) => return_type:
        body

# 继承类
class DerivedClass of ParentClass:
    field: type

# 紧凑类（无填充）
packed class DataStruct:
    field1: type1:1  # 1位字段
    field2: type2:7  # 7位字段
```

#### 5. 枚举定义
```coffee
enum Color:
    Red
    Green
    Blue(arg_type)
    Rgb(r: int(1)-, g: int(1)-, b: int(1)-)
```

#### 6. 控制流语句
```coffee
# if/elif/else语句
if condition:
    body
elif condition2:
    body2
else:
    body3

# while循环
while condition:
    body

# for循环
for variable in collection:
    body

for variable in range(start, end):
    body

for variable in start..end:
    body

# match模式匹配
match value:
    pattern1 => result1
    pattern2 => result2
    _ => default_result
```

#### 7. 内存管理与借用
```coffee
mv source target
clone source target
let b: T = clone a
rm variable
let y: &int = &x
return *y
```

值类型离开作用域自动结束；资源必须 `mv`/`clone`，禁止 `copy`/`clean out`。`&`/`&mut` 由类型检查器做过程内借用检查。

#### 8. 异常处理
```coffee
# 抛出异常
raise ErrorType(arguments)
```

#### 9. 模块导入
```coffee
# 导入Coffee模块
use module_name

# 导入Coffee模块并重命名
use module_name as alias

# 导入模块中的特定功能
use function_name in module_name
use function_name in module_name as alias

# 导入C函数（多符号支持）
use printf, fprintf, exit in libc of c
use sin, cos in libm of c

# 导入自定义C库函数
use my_function in mylib of c
```

#### 10. 主函数入口
```coffee
main(function_name())
main(function_name(arg1, arg2))
```

#### 11. 表达式语法
Coffee支持丰富的表达式：

```coffee
# 字面量
42
3.14
"hello"
true
false

# 变量
variable_name

# 二元操作
a + b
x - y
value * factor
dividend / divisor
a % b
x == y
x != y
x < y
x > y
x <= y
x >= y
x && y
x || y

# 一元操作
!boolean_value
-x

# 函数调用
function_name()
function_name(arg1, arg2)
function_name(arg1, arg2, arg3)

# 成员访问
object.field

# 数组索引
array[index]
array[expression]

# 格式化字符串（f-string）
f"Hello {name}, you are {age} years old!"
```

#### 12. 类型系统详解
- **基础类型**:
  - `int`: 默认64位有符号整数（对应 `int(8)+`）
  - `int(N)+`: N字节有符号整数（如 `int(4)+` 对应C的int）
  - `int(N)-`: N字节无符号整数（如 `int(4)-` 为32位无符号整数）
  - `float`: 默认64位浮点数（对应 `float(8)`）
  - `float(N)`: N字节浮点数（如 `float(4)` 为32位浮点数）
  - `str`/`string`: 字符串类型
  - `bool`: 布尔类型

- **复合类型**:
  - 数组: `[T; N]`
  - 切片: `[T]`
  - 元组: `(T1, T2, T3)`
  - 函数类型: `fn(T) -> R`
  - 引用类型: `&T`（共享引用）、`&mut T`（可变引用）

## C语言互操作性

### C函数导入 (FFI)

Coffee提供了强大的C语言互操作功能，允许在Coffee代码中调用C函数：

```coffee
# 导入单个C函数
use printf in libc of c

# 导入多个C函数
use printf, fprintf, puts in libc of c

# 导入数学库函数
use sin, cos, sqrt in libm of c

# 使用导入的C函数
fn test_c_functions() => int:
    printf("Hello from C via Coffee!\n")
    let result: float(8) = sin(3.14159 / 2.0)
    printf("sin(π/2) = %.2f\n", result)
    return 0

main(test_c_functions())
```

### .cfc 文件 (C Function Declarations)

Coffee使用`.cfc`文件来描述C函数签名，无需C头文件即可链接C库：

```coffee
# libprintf.cfc - 示例C函数声明文件
c fn printf(format: string, args: object) => int:
c fn puts(s: string) => int:
c fn putchar(c: int) => int:
c fn fprintf(stream: int, format: string, args: object) => int:
c fn fwrite(ptr: int, size: int, n: int, stream: int) => int:
c fn exit(status: int) => ():
c fn abort() => ():
c fn malloc(size: int) => int:
c fn free(ptr: int) => ():
c fn strlen(s: string) => int:
c fn strcmp(s1: string, s2: string) => int:
c fn strcpy(dest: int, src: string) => int:
c fn abs(x: int) => int:
```

### 从C头文件生成.cfc文件

Coffee编译器可以使用clang解析C头文件并生成.cfc文件：

```bash
# 从C头文件生成.cfc文件 (使用编译器内部的clang集成)
# 这是由编译器内部完成的，无需手动执行
```

### C标准库内置支持

Coffee内置libc的常用函数签名，无需提供.cfc文件：

- **I/O函数**: `printf`, `puts`, `fprintf`, `fwrite`, `putchar`
- **字符串函数**: `strlen`, `strcmp`, `strcpy`
- **数学函数**: `abs`
- **进程控制**: `exit`, `abort`
- **内存管理**: 语言侧 `mv`/`clone`/`rm` 与借用；C 的 `malloc`/`free` 仅能通过 `use … in libc of c` 得到 `object` 句柄

### Coffee函数导出到C

Coffee函数可以被C代码调用，并且编译器可以生成C头文件：

```coffee
# example.cf - Coffee函数，可供C调用
fn coffee_add(x: int(4)+, y: int(4)+) => int(4)+:
    return x + y

fn coffee_greet(name: string) => string:
    return name

fn coffee_process_array(arr: [int(4)+], len: int(4)+) => int(4)+:
    let sum: int(4)+ = 0
    for i in 0..len:
        sum = sum + arr[i]
    return sum
```

编译器可以为Coffee函数生成C头文件：

```c
// example.h - 自动生成的C头文件
#ifndef EXAMPLE_H
#define EXAMPLE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Function declarations
int coffee_add(int x, int y);
const char* coffee_greet(const char* name);
int coffee_process_array(struct Slice_i32_ref_ * arr, int len);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // EXAMPLE_H
```

### 类型映射

Coffee和C之间的类型自动映射：

| Coffee类型 | C类型 | 说明 |
|-----------|-------|------|
| `int(8)+` | `long long` | 64位有符号整数 |
| `int(4)+` | `int` | 32位有符号整数 |
| `int(2)+` | `short` | 16位有符号整数 |
| `int(1)+` | `char` | 8位有符号整数 |
| `int(8)-` | `unsigned long long` | 64位无符号整数 |
| `int(4)-` | `unsigned int` | 32位无符号整数 |
| `int(2)-` | `unsigned short` | 16位无符号整数 |
| `int(1)-` | `unsigned char` | 8位无符号整数 |
| `float(8)` | `double` | 64位浮点数 |
| `float(4)` | `float` | 32位浮点数 |
| `bool` | `_Bool` | 布尔类型 |
| `string` | `const char*` | 字符串 |
| `void` | `void` | 无类型 |
| `(T1, T2, ...)` | `struct Tuple_N` | 元组 |
| `[T]` | `struct Slice_T` | 切片 |
| `&T` | `const T*` | 不可变引用 |
| `&mut T` | `T*` | 可变引用 |

### 复杂类型处理

Coffee的复合类型在C中映射为结构体：

```coffee
# Coffee中的复杂类型
fn process_data(data: (int(4)+, string, float(8))) => (bool, int(4)+):
    let value: int(4)+ = data.field0  # 访问元组元素
    let text: string = data.field1
    let f_val: float(8) = data.field2
    return (true, value)
```

在C中对应为：

```c
struct Tuple_3_i32_str_f64 {
    int field0;
    const char* field1;
    double field2;
};

struct Tuple_2_bool_i32 {
    _Bool field0;
    int field1;
};

struct Tuple_2_bool_i32 process_data(struct Tuple_3_i32_str_f64 data);
```

## 项目工程特性

### 项目配置文件 (coffee.toml)
Coffee项目支持使用`coffee.toml`进行工程配置：

```toml
[package]
name = "project_name"      # 项目名称
version = "0.1.0"        # 版本号
authors = ["Author Name <email@example.com>"]  # 作者信息
description = "Project description"            # 项目描述

[dependencies]
# std 随编译器捆绑（coffee std install）；不要写本机 path= 指向 std

# Coffee包依赖：path 或 url+hash（二选一）
# [dependencies.packages.foo]
# url = "https://example.com/foo.tar.gz"
# hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
# [dependencies.packages.local_bar]
# path = "../bar"

# C 库：有 headers 时项目编译生成 target/cfc；include_paths 只给生成器用（-I），不是链接 -L
[dependencies.c_libraries.libm]
name = "m"               # 可省略，默认等于表键
headers = ["math.h"]
include_paths = []       # 非标准头路径才需要
link_flags = []
static_link = false
static_lib_path = ""

[build]
main = "src/main"        # 主入口模块
target_dir = "target"    # 输出目录
src_dir = "src"          # 源码目录
have_c = true            # 是否生成C头文件（用于FFI）

[target]
opt_level = 2            # 优化等级 (0-3)
triple = "x86_64-unknown-linux-gnu"  # 目标三元组（交叉编译用，默认为系统平台）
```

### 工程管理功能
Coffee编译器提供完整的项目工程管理功能：

1. **项目初始化**：
   ```bash
   coffee init project_name
   ```

2. **源文件扫描**：
   - 递归扫描`src_dir`目录下的`.cf`文件
   - 支持多层目录结构（模块化组织）
   - 自动构建依赖图

3. **依赖解析**：
   - 解析`use`语句建立模块依赖关系
   - 支持循环依赖检测
   - C库依赖管理

4. **并行编译**：
   - 基于依赖图的编译调度
   - 支持并行编译多个模块
   - 智能缓存机制（基于源文件哈希）

5. **链接管理**：
   - 支持静态/动态链接
   - 支持C库链接（包括静态库路径指定）
   - 链接器标志配置
   - 交叉编译支持

6. **C语言集成**：
   - 生成C头文件（`--have-c`标志）
   - C库函数导入
   - 双向互操作支持

7. **测试框架**：
   - 内置单元测试支持
   - 集成测试框架
   - 编译时和运行时错误检测
   - 性能测试支持

### 项目目录结构
标准的Coffee项目目录结构：

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

### 编译输出选项
Coffee编译器支持多种编译输出选项：

1. **编译模式**：
   - `-O0`: 无优化（默认）
   - `-O1`: 基础优化
   - `-O2`: 中等优化
   - `-O3`: 高级优化

2. **输出格式**：
   - `--emit-llvm`: 生成LLVM IR
   - `--emit-asm`: 生成汇编代码
   - `--emit-bc`: 生成位码
   - `--bin`: 生成可执行文件（默认）

3. **链接选项**：
   - `--static`: 静态链接
   - `--dynamic`: 动态链接（默认）
   - `-L path`: 添加库搜索路径
   - `-l library`: 链接指定库

4. **交叉编译**：
   - `--target triple`: 指定目标平台

## 构建和运行

### 构建命令
```bash
# 构建项目
cargo build

# 构建发布版本
cargo build --release

# 运行测试
cargo test

# 格式化代码
cargo fmt

# 检查代码
cargo check
```

### Coffee 编译器使用

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

## 开发约定

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
- `inkwell`: LLVM 绑定 (v0.7.1)
- `nom`: 解析器组合器 (v8.0.0)
- `toml`: TOML 配置解析
- `serde`: 序列化/反序列化
- `rayon`: 并发处理
- `fs2`: 跨平台文件锁
- `libc`: C 库接口
- `clang-sys`: Clang 系统接口

## 项目状态

Coffee 编译器是一个实验性的现代编译器项目，专注于探索新型编程语言设计和实现技术。项目支持完整的编译器前端、类型检查、LLVM 代码生成和优化，并实现了与 C 语言的完美双向互操作性。

该项目展示了多种编译器技术，包括语法解析、语义分析、类型系统、代码生成和优化。其最大的特点是与 C 语言的无缝集成能力，通过 .cfc 文件实现 C 库的链接，而无需原始头文件。它特别适合用于学习编译器设计和实现。
