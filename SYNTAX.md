# Coffee 语言语法参考

## 目录

1. [概述](#概述)
2. [词法规则](#词法规则)
3. [类型系统](#类型系统)
4. [语句](#语句)
5. [表达式](#表达式)
6. [控制流](#控制流)
7. [函数](#函数)
8. [类和对象](#类和对象)
9. [枚举](#枚举)
10. [模块导入](#模块导入)
11. [包依赖](#包依赖)
12. [泛型](#泛型)
13. [内存管理](#内存管理)
14. [异常处理](#异常处理)
15. [C 语言互操作性](#c-语言互操作性)
16. [运算符优先级](#运算符优先级)
17. [示例](#示例)

---

## 概述

Coffee 是一个静态类型、编译型的编程语言，具有以下特点：

- 类似 Python 的缩进语法
- 静态类型系统，支持类型推断
- 函数式和面向对象编程支持
- 完整的 C 语言互操作性
- 模式匹配和内存管理操作

**文件扩展名**: `.cf`

**入口函数**: 声明 `fn main() => int:`（或 `=> void`）即可作为程序入口。`main(fn())` 是可选的显式入口声明，不是必需的。

---

## 词法规则

### 标识符

标识符必须以字母或下划线开头，后续可以是字母、数字或下划线。

```
[a-zA-Z_][a-zA-Z0-9_]*
```

**有效标识符示例**:
```
foo, _bar, my_var_123, ClassName, my_function
```

**关键字** (不能用作标识符):
```
fn, c fn, class, type, packed, enum, of
if, elif, else, while, for, in, match
let, mv, clone, copy, rm, clean, raise, return, break, continue
use, in, of, as
true, false
int, float, bool, string, str, void
```

`copy` 与 `clean` 仍可被解析，但类型检查为错误，不是受支持的操作。

### 注释

```coffee
/#/ 单行注释，持续到行尾

/#* 多行注释
可以跨越多行
*#/
```

### 字符串字面量

```coffee
"Hello, World!"      /#/ 普通字符串
"Line 1\nLine 2"     /#/ 支持转义字符
f"Hello {name}"      /#/ 格式化字符串（f-string）
```

---

## 类型系统

### 基础类型

`int(N)+` / `int(N)-` / `float(N)` 中的 **N 是字节数**，不是位数。`int(4)+` 对应 C `int`。编译器内部 `Type::Int { bits, signed }` / `Type::Float { bits }` 的 `bits = 8 × N`。

| 类型 | 描述 | 宽度 | 对应 C 类型 |
|------|------|------|-------------|
| `int` | 默认整数类型 | 8 字节（64 位） | `long long` |
| `int(1)+` | 1 字节有符号整数 | 8 位 | `int8_t` / `char` |
| `int(2)+` | 2 字节有符号整数 | 16 位 | `int16_t` / `short` |
| `int(4)+` | 4 字节有符号整数 | 32 位 | `int32_t` / `int` |
| `int(8)+` | 8 字节有符号整数 | 64 位 | `int64_t` / `long long` |
| `int(1)-` | 1 字节无符号整数 | 8 位 | `uint8_t` / `unsigned char` |
| `int(2)-` | 2 字节无符号整数 | 16 位 | `uint16_t` / `unsigned short` |
| `int(4)-` | 4 字节无符号整数 | 32 位 | `uint32_t` / `unsigned int` |
| `int(8)-` | 8 字节无符号整数 | 64 位 | `uint64_t` / `unsigned long long` |
| `float` | 默认浮点类型 | 8 字节（64 位） | `double` |
| `float(4)` | 4 字节浮点数 | 32 位 | `float` |
| `float(8)` | 8 字节浮点数 | 64 位 | `double` |
| `bool` | 布尔类型 | 1 字节 | `_Bool` / `bool` |
| `str` / `string` | 字符串类型 | 指针 | `const char*` |
| `void` | 空类型 | - | `void` |

### 复合类型

#### 数组

```coffee
[T; N]      /#/ 固定大小数组，N 为编译时常量
[T]         /#/ 切片类型（动态大小）
```

**示例**:
```coffee
let arr: [int; 5] = [1, 2, 3, 4, 5]
```

**注意**:
- 切片 `[T]` 是胖指针 `{ ptr, i64 len }`，可用于 Coffee `fn` 的参数和返回值；`c fn` 仍不能使用 `[T]`
- 数组索引访问使用 `array[index]` 语法
- 数组、切片、元组可用 `for x in …`（已绑定的名字、字面量、或 `foo()` 等表达式）。集合只求值一次；其它类型无法降到 MIR

#### 元组

```coffee
(T1, T2, T3, ...)      /#/ 任意数量的类型
```

**示例**:
```coffee
let pair: (int, str) = (42, "hello")
let triple: (int, float, bool) = (1, 3.14, true)
```

#### 函数类型

```coffee
fn(T1, T2, ...) => R     /#/ 函数类型，参数列表和返回类型
```

**示例**:
```coffee
let callback: fn(int, float) => bool
```

#### 引用类型

```coffee
&T        /#/ 不可变引用
&mut T    /#/ 可变引用
```

#### 泛型应用

```coffee
List<T>              /#/ 类型参数（类/函数头上的名字）
List<int>            /#/ 具体化；检查器实例化为 `List__int`
List<List<int>>      /#/ 嵌套尖括号是一个完整类型字符串
```

没有 `T: Trait` 约束，也没有随编译器附带的标准库 `List`。见 [泛型](#泛型)。

---

## 语句

### 变量声明

```coffee
let variable_name: type = value
```

**示例**:
```coffee
let x: int = 42
let y: float = 3.14
let name: str = "Coffee"
let enabled: bool = true
```

### 赋值语句

```coffee
variable = value
```

**示例**:
```coffee
x = 100
name = "Updated"
```

### return 语句

```coffee
return value
return          /#/ 对于 void 函数
```

**示例**:
```coffee
fn add(a: int, b: int) => int:
    return a + b

fn say_hello() => void:
    printf("Hello!\n")
    return
```

### break 和 continue

```coffee
break       /#/ 跳出循环
continue    /#/ 继续下一次迭代
```

---

## 表达式

### 字面量

```coffee
42          /#/ 整数字面量
3.14        /#/ 浮点数字面量
true        /#/ 布尔值 true
false       /#/ 布尔值 false
"hello"     /#/ 字符串字面量
```

### 变量引用

```coffee
variable_name
```

### 二元运算符

| 运算符 | 描述 | 优先级 |
|--------|------|--------|
| `*` `/` `%` | 乘、除、取模 | 高 |
| `+` `-` | 加、减、字符串拼接 | 中 |
| `<<` `>>` | 左移、右移 | 中 |
| `<` `<=` `>` `>=` | 比较 | 低 |
| `==` `!=` | 等于、不等于 | 低 |
| `&` | 位与 | 中 |
| `^` | 位异或 | 中 |
| `\|` | 位或 | 中 |
| `&&` | 逻辑与 | 低 |
| `\|\|` | 逻辑或 | 低 |

**注意**: `+` 运算符用于字符串时执行字符串拼接操作。

### 一元运算符

```coffee
-x           /#/ 负号
!x           /#/ 逻辑非
~x           /#/ 位非（按位取反）
&x           /#/ 不可变借用（变量、字段 p.x、下标 a[i]）
&mut x       /#/ 可变借用
*r           /#/ 解引用
```

### 函数调用

```coffee
function_name()
function_name(arg1, arg2)
function_name(arg1, arg2, arg3)
```

### 成员访问

```coffee
object.field
object.method(args)
```

### 数组索引

```coffee
array[index]
array[expression]
```

### 类型转换

```coffee
int(value)       /#/ 转换为整数
float(value)     /#/ 转换为浮点数
bool(value)      /#/ 转换为布尔值
```

### 元组字面量

```coffee
(value1, value2, value3)
(1, "hello", 3.14)
```

### 数组字面量

```coffee
[value1, value2, value3]
[1, 2, 3, 4, 5]
```

### 结构体字面量

```coffee
ClassName { field1: value1, field2: value2 }
Point { x: 10, y: 20 }
```

### 格式化字符串 (f-string)

```coffee
f"Hello {name}, you are {age} years old!"
```

---

## 控制流

### if 语句

```coffee
if condition:
    body

if condition:
    body
else:
    else_body

if condition:
    body
elif condition2:
    elif_body
else:
    else_body
```

**示例**:
```coffee
if x > 0:
    printf("x is positive\n")
elif x < 0:
    printf("x is negative\n")
else:
    printf("x is zero\n")
```

### while 循环

```coffee
while condition:
    body
```

**示例**:
```coffee
let i: int = 0
while i < 10:
    printf("i = %d\n", i)
    i = i + 1
```

### for 循环

```coffee
for variable in collection:
    body

for variable in range(start, end):
    body

for variable in start..end:
    body
```

**示例**:
```coffee
for i in 0..10:
    printf("i = %d\n", i)

for item in items:
    printf("item = %s\n", item)
```

`for` 支持范围（`0..10` / `range`）以及数组、切片、元组上的 for-in（变量、字面量、或 `foo()`；集合只求值一次）。其它集合类型无法降低。

### match 表达式

```coffee
match value:
    pattern1 => result1
    pattern2 => result2
    _ => default_result
```

**示例**:
```coffee
fn describe(x: int) => str:
    match x:
        0 => "zero"
        1 => "one"
        _ => "other"
```

---

## 函数

### 函数定义

```coffee
fn function_name(param1: type1, param2: type2) => return_type:
    body

fn id<T>(x: T) => T:
    return x
```

**示例**:
```coffee
fn add(a: int, b: int) => int:
    return a + b

fn greet(name: str) => str:
    return f"Hello, {name}!"
```

### C ABI 函数

```coffee
c fn function_name(param1: type1, param2: type2) => return_type:
    body
```

C ABI 函数可以被 C 代码调用。

**示例**:
```coffee
c fn coffee_add(a: int, b: int) => int:
    return a + b
```

### 错误监听器（`#name`）

`#name` 命名一个已有函数，不是关键字。`fn f(...) #name => R` 时，只有写在 `f` 函数体里的 `raise` 会调用 `name`，不会沿调用栈穿透。`name` 的签名是 `fn name(err: Error) => R`，返回值就是 `f` 的返回值。`name` 里的 `raise` 一律中止进程（打印后 `exit`），即使监听器自己也带 `#…`。`f` 与 `name` 必须是不同名字。内建 `Error` 为 `{ code: int, note: str, e: object }`；可 `raise` 的还有 `of Error` 的子类（见「异常处理」）。监听器参数仍是 `err: Error`，只依赖该前缀字段。没有 `try` / `catch`。

```coffee
fn name(err: Error) => return_type:
    return dummy

fn function_name(param: type) #name => return_type:
    body
```

### 匿名函数 / 闭包

```coffee
let callback: fn(int) => int = fn(x: int) => int:
    return x * 2
```

### 主函数入口

名为 `main` 的函数即可作为入口。`main(fn())` 是可选的显式入口。

```coffee
fn main() => int:
    return 0
```

显式入口示例：

```coffee
fn my_main() => int:
    printf("Hello, World!\n")
    return 0

main(my_main())
```

---

## 类和对象

### 类定义

```coffee
class ClassName:
    field1: type1
    field2: type2
    
    fn method(self, param: type) => return_type:
        body

class List<T>:
    len: int
```

**示例**:
```coffee
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        return Point { x: x, y: y }
    
    fn get_x(self) => int:
        return self.x
    
    fn set_x(self, x: int) => void:
        self.x = x
```

### 继承

```coffee
class DerivedClass of ParentClass:
    field: type
```

**示例**:
```coffee
class Animal:
    name: str
    
    fn speak(self) => str:
        return "Some sound"

class Dog of Animal:
    breed: str
    
    fn speak(self) => str:
        return "Woof!"
```

### 紧凑类（位字段）

```coffee
packed class DataStruct:
    field1: type1:1    /#/ 1 位字段
    field2: type2:7    /#/ 7 位字段
```

### 构造函数

```coffee
fn new(...) => ClassName:
    return ClassName { field1: value1, ... }
```

### 方法定义

```coffee
fn method(self, param: type) => return_type:
    body
```

---

## 枚举

### 枚举定义

```coffee
enum EnumName:
    Variant1
    Variant2
    Variant3(field_type)
    Variant4(field1: type1, field2: type2)
```

**示例**:
```coffee
enum Color:
    Red
    Green
    Blue
    Rgb(r: int, g: int, b: int)
```

### 枚举值与模式

规范写法使用点号：`Color.Red`。`Color::Red` 仍可能作为路径解析，但不作为规范语法。

```coffee
let color: Color = Color.Red

match color:
    Color.Red => ...
    Color.Rgb(r, g, b) => ...
    _ => ...
```

### 枚举变体字段

枚举变体可以包含字段：

```coffee
/#/ 无字段变体（单元变体）
Red

/#/ 单字段变体（元组风格）
Green(int)

/#/ 多字段变体（命名风格）
Rgb(r: int, g: int, b: int)
```

---

## 模块导入

### 导入 Coffee 模块

```coffee
use module_name
use module_name as alias
```

### 导入模块中的特定功能

```coffee
use function_name in module_name
use function_name in module_name as alias
use fn_a, fn_b in module_name
use * in module_name
```

逗号列表把多个**函数**拉进当前文件（等价于多行 `use a in m`）。`use * in m` 导入该模块全部导出函数，仍**不含**类；类要用整模块 `use mem`。`use *` 不能和其它名字写在同一行。C 的多符号仍写 `use a, b in libc of c`。

`object` / `buf` 可以写 `p + n`（按字节偏移，LLVM GEP），给 `memcpy` 这类 C 调用用；不要把指针先转成 `int` 再加（Android 上会打坏指针标签）。

### 导入 C 函数

```coffee
use function_name in libc of c
use function1, function2 in libm of c
```

**示例**:
```coffee
use printf, fprintf, exit in libc of c
use sin, cos, sqrt in libm of c
use my_function in mylib of c
```

项目模式在 `coffee.toml` 里**声明** C 库后，编译会从系统头生成 `target/cfc/lib<键>.cfc`（已安装的传递依赖也会跟着生成）：

```toml
[dependencies.c_libraries.z]
headers = ["zlib.h"]
```

然后 `use zlibVersion in z of c`。没有声明、磁盘上也没有 `.cfc` 时不会猜头文件，需要 `coffee -c header.h` 或补上 `c_libraries`。`libc` / `libm` 仍用捆绑表，不必声明。

**注意**: `use` 必须写在所有函数定义之前（全局作用域），不能写在函数内部。

---

## 包依赖

没有中央包注册表，也没有 `foo = "1.0"` 这种 semver 字符串依赖。在 `coffee.toml` 里用 `[dependencies.packages.<名字>]`，**二选一**：

- `path = "..."`（本地树），或
- `url` + `hash`（解包后整棵树的 SHA-256，不是 tarball 字节的哈希）

```toml
[dependencies.packages.foo]
url = "https://example.com/foo.tar.gz"
hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[dependencies.packages.local_bar]
path = "../bar"
```

`coffee fetch` 把 url+hash 依赖拉进缓存（`$COFFEE_CACHE`，否则 `~/.cache/coffee`）。`coffee fetch <url>` 下载 `.tar.gz` 并打印树哈希；`--save [name]` 写入 `coffee.toml`。项目编译会解析依赖并把各包的导入根（有 `src/` 则用 `src/`，否则用包根）**前置**到导入搜索路径。细节见 [编译器文档](docs/zh/compiler.md)。

### 标准库 `std`

编译器把仓库里的 `library/std` **嵌入** `coffee` 二进制。`coffee std install [dir]` 解包到 `$COFFEE_STD`，否则 `$XDG_DATA_HOME/coffee/std`，否则 `~/.local/share/coffee/std`。编译时若 `coffee.toml` 没有 `[dependencies.packages.std]`，会自动前置该包的导入根（单文件模式同样）。查找顺序：`COFFEE_STD`（内含 `coffee.toml`）→ 上述安装目录 → 从 cwd / 可执行文件向上找 `library/std`（给仓库内 `cargo test` 用）→ 否则报错 `standard library not installed; run coffee std install`。不要在 toml 里写本机 `path=` 指向 std。

```coffee
use print in std

fn main() => int:
    print("Hello, Coffee!\n")
    return 0
```

用户代码里不要写 `of c`；C 边界只在 std 的 `sys` 模块。可增长整数缓冲：`use mem` 后 `IntBuf::new` / `push` / `get` / `length`（堆上 `buf`，`get` 越界返回 0；move-only，不能 `clone`）。

---

## 泛型

v1 是**单态化**（token 替换），不是带约束的完整泛型系统。

```coffee
class List<T>:
    len: int

    fn push(self, x: T) => void:
        return

fn id<T>(x: T) => T:
    return x

fn main() => int:
    let xs: List<int> = List { len: 0 }
    return 0
```

- `class List<T>:` / `fn id<T>(x: T) => T` 解析 `type_params`。
- 类型 `List<int>` 在检查器里是 `Type::App`，实例化后的具体名字是 `List__int`；方法变成 `List__int_push` 这类名字。
- 嵌套 `List<List<int>>` 是一个类型字符串（解析在统一的 `src/parser/ty.rs`）。
- 不支持 `T: Trait`。语言不附带标准库 `List` 实现；上面的 `List` 只是语法示例。

---

## 内存管理

值类型（`int` / `float` / `bool`，以及只含值类型的元组和定长数组）在作用域结束时由编译器清理，不必写 `rm`。资源类型（`class` 实例、`str`、切片、`object`、`buf`）不能用 `=` 或 `let b = a` **复制**；若该语句是简单变量 `a` 在函数剩余路径上的最后一次使用，则 `let b: T = a`、`b = a`、按值实参 `foo(a)`、`return a` 与 `mv a …` 相同（搬走，不是 clone）。否则仍须写 `mv` 或 `clone`。`p.x` / `a[i]`、循环体内（`return` 除外）、以及仍被借用的名字不做隐式搬走。`rm` 只用于提前释放。`copy` 与 `clean out` 仍是可解析语法，类型检查为错误。

`object` 是 C 句柄：Coffee **不会** `free` 其所指对象（class 字段也不例外）。`buf` 是 Coffee 拥有的 `malloc` 指针（LLVM 不透明 ptr，与 `str` 一样）：drop / `rm` / 作用域结束会 `free` 该指针。`let p: buf = malloc(n)` 通过 `object` → `buf` 强制转换。`clone` 一个 `buf` 是类型错误（没有长度；用切片）。`fopen` 存成 `object` 不会被释放；写成 `let f: buf = fopen(...)` 则用户选择了 `free`。

容器 drop：定长资源数组 `[T; N]`（字段或局部）随容器 drop 每个元素（如 `[str; 2]` 会 `free` 两次）。元组里的资源字段按结构体下标 drop。class 的切片字段 `[T]` 按长度循环 drop 每个元素，**不** `free` 缓冲区指针（分配器未知）。`object` 仍不释放载荷。`&T` / `&mut T` 不 drop 所指对象。

### 移动操作（转移所有权）

```coffee
mv source target
```

将 `source` 的所有权转移给 `target`，`source` 变为无效。被借用的变量不能 `mv`。

### 克隆操作（创建副本）

```coffee
clone source target
let b: T = clone a
```

创建 `source` 的独立副本。类的 `clone` 不是只 memcpy 对象字节：嵌套的 `str`、class、资源数组、元组字段会再 clone（见 `src/backend/memory_ops/clone.rs`）。`object`、引用、切片字段仍是浅拷贝（不 clone 所指对象）。`buf` 不能 `clone`（没有长度）。不是最后一次使用时，资源仍须写 `mv` 或 `clone`（见上文 last-use）。

### 删除操作

```coffee
rm variable
rm var1, var2, var3
```

提前释放。被借用的变量不能 `rm`。

### 借用

```coffee
let y: &int = &x
let z: &mut int = &mut x
return *y
```

规则（过程内）：地点可以是变量、字段 `p.x`、下标 `a[i]`。

- 同一地点可以有多个 `&`，或一个 `&mut`，不能同时存在。
- 借用期间不能对该地点赋值、`mv` 或 `rm`。
- 可变借用期间不能把该地点当普通值使用。
- 不能返回指向局部变量的引用；返回指向参数的引用可以。
- 块（`if` / `while` / `for` / `match` 臂）结束时，块内声明的引用绑定失效，loan 结束。

---

## 异常处理

没有 `try` / `catch`。未标记监听器的函数里，`raise` 向 stderr 打印并 `exit` 中止。带 `#name` 的函数里，只有该函数体中的 `raise` 调用 `name`（无穿透）；`name` 返回 `R`；`name` 里的 `raise` 仍中止。

可 `raise` 的类型只有内建类 `Error`，以及继承链到达 `Error` 的类（`class C of Error`，或 `class D of C` 且 `C` 已是异常类）。**不以**类名是否以 `Error` 结尾为准；`class FooError:` 若没有 `of Error`，只是普通类，不能 `raise`。源码不能再声明 `class Error`。

异常类保留普通类能力：额外字段、方法、`self`、再 `of` 子类。不要在子类上重声明 `code` / `note` / `e`（用继承字段）；额外字段用别的名字。构造与其它类相同（`C { ... }`）。`raise C(...)` 仍是 raise 语法糖（参数打进中止文本 / 监听器看到的 `Error` 前缀），不是另一套构造器。监听器仍只看到 `Error` 前缀；子类额外字段给持有子类类型的 Coffee 代码用。

```coffee
raise Error(...)
raise DivisionByZero(...)
raise DivisionByZero { code: 1, note: "Cannot divide by zero", e: (), extra: 0 }
```

**示例**:
```coffee
class DivisionByZero of Error:
    extra: int

    fn describe(self) => str:
        return self.note

fn on_err(err: Error) => int:
    return -1

fn divide(a: int, b: int) #on_err => int:
    if b == 0:
        raise DivisionByZero("Cannot divide by zero")
    return a / b
```

---

## C 语言互操作性

### .cfc 文件（C 函数声明）

.cfc 文件用于描述 C 函数签名：

```coffee
/#/ libc.cfc
c fn printf(format: string, args: object) => int:
c fn puts(s: string) => int:
c fn exit(status: int) => ():
```

### C 函数导入

```coffee
use printf, fprintf, exit in libc of c
use sin, cos in libm of c
```

### C 函数类型映射

| Coffee 类型 | C 类型 |
|------------|--------|
| `int(8)+` | `long long` |
| `int(4)+` | `int` |
| `int(2)+` | `short` |
| `int(1)+` | `char` |
| `int(8)-` | `unsigned long long` |
| `int(4)-` | `unsigned int` |
| `int(2)-` | `unsigned short` |
| `int(1)-` | `unsigned char` |
| `float(8)` | `double` |
| `float(4)` | `float` |
| `bool` | `_Bool` |
| `string` | `const char*` |
| `void` | `void` |

### Coffee 函数导出到 C

```coffee
c fn coffee_function(param: type) => return_type:
    body
```

---

## 运算符优先级

从高到低：

1. 函数调用: `()`
2. 成员访问: `.`
3. 数组索引: `[]`
4. 一元运算符: `!`, `~`, `-`
5. 乘除取模: `*`, `/`, `%`
6. 加减: `+`, `-`
7. 移位: `<<`, `>>`
8. 比较: `<`, `<=`, `>`, `>=`
9. 相等: `==`, `!=`
10. 位与: `&`
11. 位异或: `^`
12. 位或: `|`
13. 逻辑与: `&&`
14. 逻辑或: `||`
15. 赋值: `=`

---

## 示例

### Hello World

```coffee
use printf in libc of c

fn main() => int:
    printf("Hello, World!\n")
    return 0
```

### 类和方法调用

```coffee
use printf in libc of c
use sqrt in libm of c

class Point:
    x: int
    y: int

    fn new(x: int, y: int) => Point:
        return Point { x: x, y: y }

    fn distance_to(self, other: Point) => float:
        let dx: float = float(self.x - other.x)
        let dy: float = float(self.y - other.y)
        return sqrt(dx * dx + dy * dy)

fn main() => int:
    let p1: Point = Point.new(0, 0)
    let p2: Point = Point.new(3, 4)
    let dist: float = p1.distance_to(p2)
    printf("Distance: %.2f\n", dist)
    return 0
```

### 枚举使用

```coffee
enum Color:
    Red
    Green
    Blue

fn get_color_name(c: Color) => str:
    match c:
        Color.Red => "Red"
        Color.Green => "Green"
        Color.Blue => "Blue"

fn main() => int:
    let red: Color = Color.Red
    return 0
```

### 数组和循环

```coffee
use printf in libc of c

fn main() => int:
    let arr: [int; 5] = [1, 2, 3, 4, 5]
    let total: int = 0
    for item in arr:
        total = total + item
    printf("Sum: %d\n", total)
    return 0
```

---

## 附录

### 关键字完整列表

```
fn, c fn, class, type, packed, enum, of
if, elif, else, while, for, in, match
let, mv, clone, copy, rm, clean, raise, return, break, continue
use, in, of, as
true, false
int, float, bool, string, str, void
```

### 保留运算符

```
+ - * / % = < > <= >= == != ! ~ && || << >> . ( ) [ ] { } ; : , & | ^ ...
```

### 转义字符

| 转义字符 | 含义 |
|----------|------|
| `\n` | 换行 |
| `\t` | 制表符 |
| `\"` | 双引号 |
| `\\` | 反斜杠 |
| `\'` | 单引号 |
| `\0` | 空字符 |

---

**版本**: 0.3.9
**最后更新**: 2026-09-06