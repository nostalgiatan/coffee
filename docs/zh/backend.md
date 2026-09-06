# 后端模块

## 概述

后端模块（`src/backend/`）负责使用 LLVM 从分析后的 Coffee AST 生成原生代码。它将 Coffee 语言构造转换为 LLVM IR，应用优化，并生成各种输出格式，包括目标文件、汇编、位码和可执行文件。

## 模块结构

```
src/backend/
├── mod.rs            # Backend、LLVM I/O、JIT（`compile_and_run` 接收 hir_fns）
├── codegen/          # CodeGenerator 协调器
├── context.rs        # LLVM 上下文
├── types.rs          # LLVM 类型映射
├── class_layout.rs   # 继承字段摊平 / LLVM 成员顺序
├── mir_gen.rs        # 完整语句 MIR → LLVM
├── mir_raise.rs      # abort 与 `#name` 监听器
├── match_gen/        # 遗留 AST match 辅助（Coffee 函数必须从 MIR 编译）
├── opt_passes.rs     # 与 clang 对齐的 LLVM 优化档
├── arithmetic/       # 整数/浮点算术
├── control_flow/     # 控制流
├── memory_ops/       # 内存操作（`clone.rs` 深 clone、`drop.rs` 复合 drop）（`clone.rs` 深 clone、`drop.rs` 复合 drop）
├── memory/           # 内存布局
├── functions/        # 函数
├── expressions.rs    # 表达式
├── expr/             # MIR 表达式取值（`compile_hir_expr_typed`）
├── variables.rs      # 变量
├── statements.rs     # 语句
├── classes.rs        # 类和枚举
├── type_inference.rs # 类型推断
└── error.rs          # 监听器用 Error 布局
```

## 核心组件

### 1. Backend（`mod.rs`）

主 `Backend` 结构体管理 LLVM 上下文、模块和构建器。

```rust
pub struct Backend<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    target_triple: Option<String>,
}
```

**关键方法：**
- `new(context, module_name)` - 使用默认目标创建后端
- `with_target(context, module_name, target_triple)` - 创建支持交叉编译的后端
- `get_ir()` - 获取 LLVM IR 字符串
- `verify()` - 验证模块正确性
- `write_object_file(path)` - 生成目标文件（.o）
- `write_assembly_file(path)` - 生成汇编文件（.s）
- `write_bitcode(path)` - 生成位码文件（.bc）
- `write_ir(path)` - 生成 LLVM IR 文件（.ll）

**目标三元组支持：**
- 通过目标三元组支持交叉编译
- 平台特定的三元组清理（例如，Android API 级别移除）
- 默认目标检测

### 2. 代码生成器（`codegen/`）

`CodeGenerator` 将 Coffee AST 转换为 LLVM IR。

```rust
pub struct CodeGenerator<'ctx> {
    backend: &'ctx Backend<'ctx>,
    value_map: HashMap<String, inkwell::values::BasicValueEnum<'ctx>>,
    function_map: HashMap<String, inkwell::values::FunctionValue<'ctx>>,
    current_function: Option<inkwell::values::FunctionValue<'ctx>>,
}
```

**生成方法：**
- `compile_program_with_hir(program, c_imports, cfc_symbols, hir_fns)` — 驱动入口；完整 MIR 走 `mir_gen.rs`；不进 `hir_fns` 是编译错误（`missing MIR for function`），不是 AST 回退
- `compile_program(...)` — 死接口：直接 `Err`，要求使用 `compile_program_with_hir`（禁止用空 MIR 编函数体）
- `compile_statement(stmt)` - 编译单个语句
- `compile_expression(expr)` - 编译表达式
- `compile_function(func)` - 编译函数定义
- `compile_class(class)` - 编译类定义

### 3. 类型映射（`types.rs`）

将 Coffee 类型映射到 LLVM 类型。

**类型映射：**
```rust
pub fn coffee_type_to_llvm<'ctx>(context: &'ctx Context, type_: &Type) -> BasicTypeEnum<'ctx> {
    match type_ {
        Type::Int { signed: true, bytes: 8 } => context.i64_type().into(),
        Type::Int { signed: true, bytes: 4 } => context.i32_type().into(),
        Type::Int { signed: true, bytes: 2 } => context.i16_type().into(),
        Type::Int { signed: true, bytes: 1 } => context.i8_type().into(),
        Type::Int { signed: false, bytes: 8 } => context.i64_type().into(),
        Type::Float { bytes: 8 } => context.f64_type().into(),
        Type::Float { bytes: 4 } => context.f32_type().into(),
        Type::Bool => context.bool_type().into(),
        Type::String => context.i8_type().ptr_type(AddressSpace::default()).into(),
        // ... 更多类型
    }
}
```

### 4. 算术操作（`arithmetic/`）

为算术和逻辑操作生成 LLVM IR。

**支持的操作：**
- 二进制：`+`, `-`, `*`, `/`, `%`
- 比较：`==`, `!=`, `<`, `>`, `<=`, `>=`
- 逻辑：`&&`, `||`, `!`
- 位运算：`&`, `|`, `^`, `<<`, `>>`

**示例：**
```rust
fn compile_binary_op(&mut self, left: &Expr, op: &BinaryOperator, right: &Expr)
    -> Result<BasicValueEnum<'ctx>, BackendError>
{
    let left_val = self.compile_expression(left)?;
    let right_val = self.compile_expression(right)?;

    match op {
        BinaryOperator::Add => Ok(self.builder.build_int_add(
            left_val.into_int_value(),
            right_val.into_int_value(),
            "add"
        ).into()),
        // ... 更多操作符
    }
}
```

### 5. 控制流（`control_flow/`）

为控制流构造生成 LLVM IR。

**If 表达式：**
```coffee
if condition:
    then_block
else:
    else_block
```

**While 循环：**
```coffee
while condition:
    loop_body
```

**For 循环：**
```coffee
for item in collection:
    loop_body
```

**Match 表达式：**
```coffee
match value:
    pattern1 => result1
    pattern2 => result2
    _ => default
```

### 6. 内存操作（`memory_ops/`）

为 Coffee 的内存管理操作生成 LLVM IR（`compile.rs`、`clone.rs`、`drop.rs`）。

**操作：**
- `mv` - 移动所有权（memcpy + 使源无效）
- `clone` - 深拷贝：先浅拷贝再替换资源指针。嵌套 `str` / class / 资源数组 / 元组字段会再 clone；`object`、引用、切片字段仍 memcpy（不 clone 所指）。见 `src/backend/memory_ops/clone.rs`
- `rm` / 作用域结束 - 类型驱动 drop（`drop.rs`）：`[T; N]` 与 Coffee `[T]`（胖指针 `{ptr,len}`）随容器 drop **元素**；class 切片字段不 `free` 缓冲区指针。`object` 不 `free` 载荷。`buf` drop 会 `free` 该指针
- `copy` / `clean out` - 仍可解析，类型检查为错误，不是受支持的 codegen 路径

**实现：**
```rust
fn compile_mv(&mut self, source: &str, target: &str) -> Result<(), BackendError> {
    let source_val = self.value_map.get(source).unwrap();
    let target_val = self.value_map.get(target).unwrap();

    // 生成 memcpy
    let size = self.get_type_size(source_val.get_type());
    self.builder.build_memcpy(
        target_val.into_pointer_value(),
        source_val.into_pointer_value(),
        size,
        None
    );

    Ok(())
}
```

### 7. 函数（`functions/`）

为函数定义和调用生成 LLVM IR。

**函数生成：**
```rust
fn compile_function(&mut self, func: &Function) -> Result<(), BackendError> {
    // 创建函数类型
    let param_types: Vec<BasicMetadataTypeEnum> = func.params.iter()
        .map(|p| coffee_type_to_llvm(self.backend.context, &p.type_).into())
        .collect();

    let fn_type = func.return_type.to_llvm_type(self.backend.context)
        .fn_type(&param_types, false);

    // 添加函数到模块
    let fn_value = self.backend.module.add_function(
        &func.name,
        fn_type,
        None
    );

    // 创建入口块
    let entry_block = self.backend.context.append_basic_block(fn_value, "entry");
    self.builder.position_at_end(entry_block);

    // 编译函数体
    for stmt in &func.body.statements {
        self.compile_statement(stmt)?;
    }

    Ok(())
}
```

**C ABI 支持：**
- Extern "C" 链接
- C 兼容的调用约定
- C 互操作的类型映射

### 8. 类（`classes.rs`）

为类和枚举定义生成 LLVM IR。

**类生成：**
```rust
fn compile_class(&mut self, class: &ClassDef) -> Result<(), BackendError> {
    // 为类字段创建结构体类型
    let field_types: Vec<BasicTypeEnum> = class.fields.iter()
        .map(|f| coffee_type_to_llvm(self.backend.context, &f.type_))
        .collect();

    let struct_type = self.backend.context.struct_type(&field_types, class.is_packed);

    // 注册类类型
    self.type_map.insert(class.name.clone(), struct_type.into());

    // 编译方法
    for method in &class.methods {
        self.compile_method(method, &struct_type)?;
    }

    Ok(())
}
```

**枚举生成：**
- 基于标记的表示
- 变体特定数据
- 模式匹配支持

### 9. JIT 执行

后端支持 JIT（即时）执行以进行即时测试。

```rust
pub fn compile_and_run(
    source: &Program,
    c_imports: &[String],
    cfc_symbols: HashMap<String, CSymbolTable>,
    hir_fns: Vec<crate::hir::MirFn>,
    user_args: &[String],
) -> Result<i32, String>
{
    // 创建上下文和后端
    let context = Context::create();
    let backend = Backend::new(&context, "coffee_jit");

    // 与 AOT 同一条代码生成路径
    let mut codegen = CodeGenerator::new(&backend);
    codegen.compile_program_with_hir(source, c_imports, cfc_symbols, hir_fns)?;

    // 验证模块
    backend.verify()?;

    // 创建 JIT 执行引擎
    let mut execution_engine = backend.module.create_jit_execution_engine(OptimizationLevel::None)?;

    // 映射 C 库函数
    map_c_library_functions(&mut execution_engine, &backend.module, c_imports)?;

    // 查找并执行 main 函数
    let main_fn = backend.module.get_function("main")
        .ok_or("No main() entry point found")?;

    // 执行 main 函数
    unsafe {
        let main_fn_ptr = execution_engine.get_function_address("main")?;
        let main_fn: MainFn = std::mem::transmute(main_fn_ptr);
        let result = main_fn(argc, argv);
        Ok(result)
    }
}
```

## 代码生成过程

### 1. 模块设置
- 创建 LLVM 模块
- 设置目标三元组
- 初始化构建器

### 2. 类型注册
- 注册所有用户定义类型
- 创建类型映射
- 生成类型元数据

### 3. 函数声明
- 声明所有函数（前向声明）
- 生成函数签名
- 在函数映射中注册

### 4. 代码生成
- 生成函数体
- 编译语句和表达式
- 处理控制流

### 5. 优化
- 应用 LLVM 优化
- 内联函数
- 死代码消除

### 6. 代码输出
- 验证模块
- 写入输出文件
- 生成可执行文件

## C 集成

### C 函数链接

后端通过以下方式处理 C 函数调用：
1. 在 LLVM IR 中声明外部 C 函数
2. 在运行时（JIT）或链接时（AOT）映射 C 库符号
3. 处理 Coffee 和 C 之间的类型转换

**示例：**
```llvm
declare i32 @printf(i8*, ...)
```

### 类型映射

| Coffee 类型 | C 类型 | LLVM 类型 |
|-------------|--------|-----------|
| `int(4)+` | `int` | `i32` |
| `float(8)` | `double` | `double` |
| `string` | `const char*` | `i8*` |
| `bool` | `_Bool` | `i1` |

## 优化

后端支持 LLVM 优化级别：
- **O0**：无优化（最快编译）
- **O1**：基本优化
- **O2**：标准优化
- **O3**：激进优化（最慢编译）

**应用的优化：**
- 常量折叠
- 死代码消除
- 函数内联
- 循环优化
- 向量化（适用时）

## 错误处理

**后端错误：**
```rust
pub enum BackendError {
    CodeGenerationError { message: String },
    TypeConversionError { from: Type, to: String },
    UndefinedSymbol { name: String },
    VerificationError { message: String },
}
```

## 使用示例

```rust
use coffee::backend::{Backend, CodeGenerator};
use inkwell::context::Context;

let context = Context::create();
let backend = Backend::new(&context, "my_module");

let mut codegen = CodeGenerator::new(&backend);
codegen.compile_program_with_hir(&program, &c_imports, cfc_symbols, hir_fns)?;

// 验证
backend.verify()?;

// 写入目标文件
backend.write_object_file(std::path::Path::new("output.o"))?;

// 或使用 JIT 执行（同样传入 hir_fns）
let result = coffee::backend::compile_and_run(&program, &c_imports, cfc_symbols, hir_fns, &[])?;
println!("程序退出代码: {}", result);
```

## 设计考虑

### 1. 内存安全
- 正确的内存分配和释放
- 安全的指针操作
- 无未定义行为

### 2. 性能
- 高效的 LLVM IR 生成
- 最小的运行时开销
- 激进的优化

### 3. 可移植性
- 跨平台支持
- 多种目标架构
- ABI 兼容性

## 未来增强

- 向量指令生成
- SIMD 支持
- GPU 代码生成
- 更好的内联汇编支持
- 配置文件引导优化（PGO）
- 链接时优化（LTO）