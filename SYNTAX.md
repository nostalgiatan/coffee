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
11. [内存管理](#内存管理)
12. [异常处理](#异常处理)
13. [C 语言互操作性](#c-语言互操作性)
14. [运算符优先级](#运算符优先级)
15. [示例](#示例)

---

## 概述

Coffee 是一个静态类型、编译型的编程语言，具有以下特点：

- 类似 Python 的缩进语法
- 静态类型系统，支持类型推断
- 函数式和面向对象编程支持
- 完整的 C 语言互操作性
- 模式匹配和内存管理操作

**文件扩展名**: `.cf`

**入口函数**: 使用 `main()` 声明主函数

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
fn, c fn, class, packed, enum, of
if, elif, else, while, for, in, match
let, mv, clone, copy, rm, clean, raise, return
use, in, of
true, false
int, float, bool, string, str, void
```

### 注释

```coffee
/#/ 单行注释，持续到行尾

/#* 多行注释
可以跨越多行
*#/
```

### 字符串字面量

```coffee
"Hello, World!"      # 普通字符串
"Line 1\nLine 2"     # 支持转义字符
f"Hello {name}"      # 格式化字符串（f-string）
```

---

## 类型系统

### 基础类型

| 类型 | 描述 | 大小 | 对应 C 类型 |
|------|------|------|-------------|
| `int` | 默认整数类型 | 64 位 | `long long` |
| `int(1)+` | 8 位有符号整数 | 8 位 | `int8_t` / `char` |
| `int(2)+` | 16 位有符号整数 | 16 位 | `int16_t` / `short` |
| `int(4)+` | 32 位有符号整数 | 32 位 | `int32_t` / `int` |
| `int(8)+` | 64 位有符号整数 | 64 位 | `int64_t` / `long long` |
| `int(1)-` | 8 位无符号整数 | 8 位 | `uint8_t` / `unsigned char` |
| `int(2)-` | 16 位无符号整数 | 16 位 | `uint16_t` / `unsigned short` |
| `int(4)-` | 32 位无符号整数 | 32 位 | `uint32_t` / `unsigned int` |
| `int(8)-` | 64 位无符号整数 | 64 位 | `uint64_t` / `unsigned long long` |
| `float` | 默认浮点类型 | 64 位 | `double` |
| `float(4)` | 32 位浮点数 | 32 位 | `float` |
| `float(8)` | 64 位浮点数 | 64 位 | `double` |
| `bool` | 布尔类型 | 8 位 | `_Bool` / `bool` |
| `str` / `string` | 字符串类型 | 指针 | `const char*` |
| `void` | 空类型 | - | `void` |

### 复合类型

#### 数组

```coffee
[T; N]      # 固定大小数组，N 为编译时常量
[T]         # 切片类型（动态大小，仅用于声明）
```

**示例**:
```coffee
let arr: [int; 5] = [1, 2, 3, 4, 5]
```

**注意**: 
- 切片类型 `[T]` 目前不能用于函数参数，函数参数必须使用固定大小数组 `[T; N]`
- 数组索引访问使用 `array[index]` 语法

#### 元组

```coffee
(T1, T2, T3, ...)      # 任意数量的类型
```

**示例**:
```coffee
let pair: (int, str) = (42, "hello")
let triple: (int, float, bool) = (1, 3.14, true)
```

#### 函数类型

```coffee
fn(T1, T2, ...) => R     # 函数类型，参数列表和返回类型
```

**示例**:
```coffee
let callback: fn(int, float) => bool
```

#### 引用类型

```coffee
&T        # 不可变引用
&mut T    # 可变引用
```

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
return          # 对于 void 函数
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
break       # 跳出循环
continue    # 继续下一次迭代
```

---

## 表达式

### 字面量

```coffee
42          # 整数字面量
3.14        # 浮点数字面量
true        # 布尔值 true
false       # 布尔值 false
"hello"     # 字符串字面量
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
-x           # 负号
!x           # 逻辑非
~x           # 位非（按位取反）
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
int(value)       # 转换为整数
float(value)     # 转换为浮点数
bool(value)      # 转换为布尔值
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

### 带错误处理的函数

```coffee
fn function_name(param: type) #error_handler => return_type:
    body
```

### 匿名函数 / 闭包

```coffee
let callback: fn(int) => int = fn(x: int) => int:
    return x * 2
```

### 主函数入口

```coffee
main(function_name())
main(function_name(arg1, arg2))
```

**示例**:
```coffee
fn my_main(argc: int, argv: ...) => int:
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
    field1: type1:1    # 1 位字段
    field2: type2:7    # 7 位字段
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

### 枚举值使用

```coffee
let color: Color = Color::Red
```

### 枚举变体字段

枚举变体可以包含字段：

```coffee
# 无字段变体（单元变体）
Red

# 单字段变体（元组风格）
Green(int)

# 多字段变体（命名风格）
Rgb(r: int, g: int, b: int)
```

**注意**: 目前枚举变体带参数（如 `Color::Rgb(255, 128, 0)`）在代码生成阶段存在类型转换问题，暂时不支持。建议使用无字段的枚举变体。

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
```

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

**注意**: import 语句必须放在所有函数定义之前（全局作用域），不能放在函数内部。

---

## 内存管理

### 移动操作（转移所有权）

```coffee
mv source target
```

将 `source` 的所有权转移给 `target`，`source` 变为无效。

### 克隆操作（创建副本）

```coffee
clone source target
```

创建 `source` 的深拷贝到 `target`。

### 复制操作（共享引用）

```coffee
copy source target
```

创建 `source` 的共享引用到 `target`。

### 删除操作

```coffee
rm variable
rm var1, var2, var3
```

删除变量，释放内存。

### 作用域清理

```coffee
clean out                    # 清理所有变量
clean out except var1, var2  # 除指定变量外清理所有
clean out var1, var2         # 清理指定变量
```

**重要**: Coffee 语言要求所有变量必须在作用域结束前显式清理（使用 `rm`），不支持隐式内存清理。这是为了确保内存安全和避免内存泄漏。

---

## 异常处理

### 抛出异常

```coffee
raise ErrorType(arguments)
```

**示例**:
```coffee
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("Cannot divide by zero")
    return a / b
```

---

## C 语言互操作性

### .cfc 文件（C 函数声明）

.cfc 文件用于描述 C 函数签名：

```coffee
# libc.cfc
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
15. 赋值: `=`, `+=`, `-=`, `*=`, `/=`

---

## 示例

### Hello World

```coffee
fn main() => int:
    printf("Hello, World!\n")
    return 0

main(main())
```

### 类和方法调用

```coffee
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
    use sqrt in libm of c
    
    let p1: Point = Point::new(0, 0)
    let p2: Point = Point::new(3, 4)
    let dist: float = p1.distance_to(p2)
    
    printf("Distance: %.2f\n", dist)
    return 0

main(main())
```

### 枚举使用

```coffee
enum Color:
    Red
    Green
    Blue

fn get_color_name(c: Color) => str:
    match c:
        Color::Red => "Red"
        Color::Green => "Green"
        Color::Blue => "Blue"

fn main() => int:
    let red: Color = Color::Red
    
    rm red
    return 0

main(main())
```

### 数组和循环

```coffee
fn main() => int:
    let arr: [int] = [1, 2, 3, 4, 5]
    let total: int = 0
    for i in 0..5:
        total = total + arr[i]
    rm i
    printf("Sum: %d\n", total)
    rm total, arr
    return 0

main(main())
```

---

## 附录

### 关键字完整列表

```
fn, c fn, class, packed, enum, of
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

**版本**: 0.2.0
**最后更新**: 2026-03-01