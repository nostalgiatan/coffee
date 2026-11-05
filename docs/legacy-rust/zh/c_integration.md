# C 语言集成模块

## 概述

C 语言集成模块（`src/c/`）提供 Coffee 和 C 库之间的全面外部函数接口（FFI）支持。它实现双向互操作性，允许 Coffee 函数调用 C 函数，C 代码也可以调用 Coffee 函数。

## 模块结构

```
src/c/
├── mod.rs        # 主 C 集成模块
├── parser.rs     # .cfc 文件解析器
├── signature.rs  # C 函数签名管理
├── generator.rs  # 从 C 头文件生成 .cfc 文件
└── header_gen.rs # 从 Coffee 函数生成 C 头文件
```

## 核心组件

### 1. .cfc 文件格式

`.cfc`（C Function Declaration）文件是 Coffee 独特的 C 集成方法。它用 Coffee 语法声明 C 函数签名，允许 Coffee 程序与 C 库链接而无需原始 C 头文件。

**示例 .cfc 文件：**
```coffee
# libc.cfc - C 标准库函数
c fn printf(format: string, args: object) => int:
c fn puts(s: string) => int:
c fn putchar(c: int) => int:
c fn exit(status: int) => ():
c fn malloc(size: int) => int:
c fn free(ptr: int) => ():
c fn strlen(s: string) => int:
c fn strcmp(s1: string, s2: string) => int:
```

**优势：**
- 无需原始 C 头文件
- Coffee 语法提高可读性
- 自动类型映射
- 易于维护和更新

### 2. C 符号表（`signature.rs`）

管理从 .cfc 文件解析的 C 函数签名。

```rust
pub struct CSymbol {
    pub name: String,
    pub return_type: Type,
    pub params: Vec<(String, Type)>,
    pub is_variadic: bool,
}

pub struct CSymbolTable {
    pub functions: HashMap<String, CSymbol>,
}

impl CSymbolTable {
    pub fn from_cfc(content: &str) -> Result<Self, ParseError>;
    pub fn get_function(&self, name: &str) -> Option<&CSymbol>;
    pub fn add_function(&mut self, symbol: CSymbol);
}
```

### 3. .cfc 解析器（`parser.rs`）

解析 .cfc 文件以提取 C 函数签名。

**解析规则：**
- 函数声明以 `c fn` 开头
- 参数格式为 `name: type`
- 返回类型跟随 `=>`
- 可变参数函数在参数中使用 `...`

**解析示例：**
```coffee
c fn printf(format: string, ...) => int:
```

**解析结果：**
```rust
CSymbol {
    name: "printf",
    return_type: Type::Int { signed: true, bytes: 4 },
    params: vec![
        ("format".to_string(), Type::String),
    ],
    is_variadic: true,
}
```

### 4. .cfc 生成器（`generator.rs`）

使用 clang 从 C 头文件生成 .cfc 文件。

**生成过程：**
1. 使用 clang-sys 解析 C 头文件
2. 提取函数声明
3. 将 C 类型映射到 Coffee 类型
4. 生成 .cfc 语法

**使用方法：**
```bash
coffee --gen-cfc /usr/include/math.h > libm.cfc
```

**生成输出：**
```coffee
# libm.cfc
c fn sin(x: float(8)) => float(8):
c fn cos(x: float(8)) => float(8):
c fn sqrt(x: float(8)) => float(8):
c fn pow(x: float(8), y: float(8)) => float(8):
```

### 5. C 头文件生成器（`header_gen.rs`）

从 Coffee 函数生成 C 头文件，使 C 代码能够调用 Coffee 函数。

**生成过程：**
1. 解析 Coffee 源代码
2. 提取函数定义
3. 将 Coffee 类型映射到 C 类型
4. 生成 C 头文件语法

**示例 Coffee 代码：**
```coffee
fn coffee_add(x: int(4)+, y: int(4)+) => int(4)+:
    return x + y

fn coffee_greet(name: string) => string:
    return name
```

**生成的 C 头文件：**
```c
// coffee_functions.h
#ifndef COFFEE_FUNCTIONS_H
#define COFFEE_FUNCTIONS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int32_t coffee_add(int32_t x, int32_t y);
const char* coffee_greet(const char* name);

#ifdef __cplusplus
}
#endif

#endif // COFFEE_FUNCTIONS_H
```

## 类型映射

### Coffee 到 C 类型

| Coffee 类型 | C 类型 | 描述 |
|-------------|--------|------|
| `int(8)+` | `int64_t` | 64 位有符号整数 |
| `int(4)+` | `int32_t` | 32 位有符号整数 |
| `int(2)+` | `int16_t` | 16 位有符号整数 |
| `int(1)+` | `int8_t` | 8 位有符号整数 |
| `int(8)-` | `uint64_t` | 64 位无符号整数 |
| `int(4)-` | `uint32_t` | 32 位无符号整数 |
| `int(2)-` | `uint16_t` | 16 位无符号整数 |
| `int(1)-` | `uint8_t` | 8 位无符号整数 |
| `float(8)` | `double` | 64 位浮点数 |
| `float(4)` | `float` | 32 位浮点数 |
| `bool` | `_Bool` | 布尔类型 |
| `string` | `const char*` | C 字符串 |
| `void` | `void` | 无返回值 |
| `(T1, T2)` | `struct Tuple_2` | 元组 |
| `[T]` | `struct Slice_T` | 切片 |
| `&T` | `const T*` | 不可变引用 |
| `&mut T` | `T*` | 可变引用 |

### 复杂类型

**元组：**
```coffee
# Coffee
fn process(data: (int(4)+, string, float(8))) => (bool, int(4)+):
```

```c
// C
struct Tuple_3_i32_str_f64 {
    int32_t field0;
    const char* field1;
    double field2;
};

struct Tuple_2_bool_i32 {
    _Bool field0;
    int32_t field1;
};

struct Tuple_2_bool_i32 process(struct Tuple_3_i32_str_f64 data);
```

**切片：**
```coffee
# Coffee
fn sum(arr: [int(4)+]) => int(4)+:
```

```c
// C
struct Slice_i32 {
    int32_t* data;
    size_t len;
};

int32_t sum(struct Slice_i32 arr);
```

## 双向互操作性

### Coffee 调用 C

```coffee
# 导入 C 函数
use printf, puts, exit in libc of c
use sin, cos, sqrt in libm of c

fn main() => ():
    printf("Hello from Coffee!\n")
    let angle: float(8) = 3.14159 / 2.0
    let result: float(8) = sin(angle)
    printf("sin(π/2) = %.2f\n", result)
    exit(0)
```

### C 调用 Coffee

**Coffee 代码：**
```coffee
c fn coffee_multiply(a: int(4)+, b: int(4)+) => int(4)+:
    return a * b
```

**生成的头文件：**
```c
// coffee_multiply.h
int32_t coffee_multiply(int32_t a, int32_t b);
```

**C 代码：**
```c
#include <stdio.h>
#include "coffee_multiply.h"

int main() {
    int32_t result = coffee_multiply(5, 7);
    printf("5 * 7 = %d\n", result);
    return 0;
}
```

## 导入语法

### 简单导入
```coffee
use printf in libc of c
```

### 多个函数
```coffee
use printf, fprintf, puts in libc of c
```

### 带别名的导入
```coffee
use print in libc of c as printf
```

## 库配置

### coffee.toml 配置

```toml
[dependencies.c_libraries.libm]
name = "m"
headers = ["math.h"]
include_paths = ["/usr/include"]
link_flags = ["-L/usr/lib"]
static_link = false
static_lib_path = ""
```

**配置选项：**
- `name`: 库名称（例如，libm 用 "m"）
- `headers`: 用于函数发现的 C 头文件
- `include_paths`: 额外的包含目录
- `link_flags`: 额外的链接器标志
- `static_link`: 强制静态链接
- `static_lib_path`: 静态库文件路径

## 内置 C 函数

Coffee 包含常见 C 库函数的内置签名：

**I/O 函数：**
- `printf`, `fprintf`, `puts`, `putchar`, `fwrite`

**字符串函数：**
- `strlen`, `strcmp`, `strcpy`

**内存函数：**
- `malloc`, `free`

**进程控制：**
- `exit`, `abort`

**数学函数：**
- `abs`, `sin`, `cos`, `sqrt`, `pow`

## 错误处理

**C 集成错误：**
```rust
pub enum CIntegrationError {
    ParseError { message: String },
    TypeMappingError { c_type: String },
    SymbolNotFound { name: String },
    HeaderGenerationError { message: String },
}
```

## 使用示例

### 在 Coffee 中使用 C 函数

```coffee
# 导入 C 函数
use printf, scanf, malloc, free in libc of c

fn read_and_print() => ():
    # 分配内存
    let buffer: int = malloc(1024)
    
    # 读取输入
    printf("Enter your name: ")
    scanf("%s", buffer)
    
    # 打印问候
    printf("Hello, %s!\n", buffer)
    
    # 释放内存
    free(buffer)

main(read_and_print())
```

### 生成 .cfc 文件

```bash
# 从 C 头文件生成 .cfc
coffee --gen-cfc /usr/include/curl/curl.h > libcurl.cfc

# 使用生成的 .cfc
use curl_easy_init, curl_easy_perform in libcurl of c
```

## 设计考虑

### 1. 无头文件依赖
- .cfc 文件是自包含的
- 链接时无需原始 C 头文件
- 简化分发和部署

### 2. 类型安全
- Coffee 级别的强类型
- 自动类型检查
- 防止常见 C 错误

### 3. 性能
- 零成本抽象
- 直接函数调用
- 最小运行时开销

### 4. 可移植性
- 跨平台工作
- 处理不同 ABI
- 交叉编译支持

## 未来增强

- 从共享库自动生成 .cfc
- 回调函数支持
- 结构体和枚举映射
- 宏支持
- 更好的错误消息
- C++ 集成