# Backend Module

## Overview

The Backend module (`src/backend/`) is responsible for generating native code from the analyzed Coffee AST using LLVM. It translates Coffee language constructs into LLVM IR, applies optimizations, and produces various output formats including object files, assembly, bitcode, and executables.

## Module Structure

```
src/backend/
├── mod.rs            # Backend, LLVM I/O, JIT (`compile_and_run` takes hir_fns)
├── codegen/          # CodeGenerator coordinator (`program`, types, layout)
├── context.rs        # LLVM context management
├── types.rs          # LLVM type mapping
├── class_layout.rs   # Inherited field flatten / LLVM member order
├── mir_gen.rs        # Complete statement-MIR → LLVM
├── mir_raise.rs      # abort vs `#name` listener
├── mir_expr.rs / mir_return.rs / mir_nested.rs
├── match_gen/        # unused AST match helpers (Coffee functions must compile from MIR)
├── opt_passes.rs     # clang-aligned LLVM opt levels
├── arithmetic/       # int/float arithmetic
├── control_flow/     # if / while / for AST helpers
├── memory_ops/       # Memory operation code generation
├── memory/           # Memory layout and management
├── functions/        # Function code generation
├── expressions.rs    # Expression code generation
├── expr/             # typed / value helpers used by MIR (`compile_hir_expr_typed`)
├── variables.rs      # Variable code generation
├── statements.rs     # Statement code generation
├── classes.rs        # Class and enum code generation
├── type_inference.rs # Type inference for code generation
└── error.rs          # LLVM Error {code, note, e} layout for listeners
```

## Core Components

### 1. Backend (`mod.rs`)

The main `Backend` struct manages LLVM context, module, and builder.

```rust
pub struct Backend<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    target_triple: Option<String>,
}
```

**Key Methods:**
- `new(context, module_name)` - Create backend with default target
- `with_target(context, module_name, target_triple)` - Create with cross-compilation support
- `get_ir()` - Get LLVM IR as string
- `verify()` - Verify module correctness
- `write_object_file(path)` - Generate object file (.o)
- `write_assembly_file(path)` - Generate assembly file (.s)
- `write_bitcode(path)` - Generate bitcode file (.bc)
- `write_ir(path)` - Generate LLVM IR file (.ll)

**Target Triple Support:**
- Cross-compilation via target triples
- Platform-specific triple cleaning (e.g., Android API level removal)
- Default target detection

### 2. Code Generator (`codegen/`)

The `CodeGenerator` translates Coffee AST to LLVM IR.

```rust
pub struct CodeGenerator<'ctx> {
    backend: &'ctx Backend<'ctx>,
    value_map: HashMap<String, inkwell::values::BasicValueEnum<'ctx>>,
    function_map: HashMap<String, inkwell::values::FunctionValue<'ctx>>,
    current_function: Option<inkwell::values::FunctionValue<'ctx>>,
}
```

**Generation Methods:**
- `compile_program_with_hir(program, c_imports, cfc_symbols, hir_fns)` — driver entry; complete MIR in `mir_gen.rs`; omitted `hir_fns` is a compile error (`missing MIR for function`), not an AST body
- `compile_program(...)` — dead trap API: returns `Err` telling drivers to use `compile_program_with_hir` (must not compile function bodies from empty MIR)
- `compile_statement(stmt)` - Compile individual statements
- `compile_expression(expr)` - Compile expressions
- `compile_function(func)` - Compile function definitions
- `compile_class(class)` - Compile class definitions

### 3. Type Mapping (`types.rs`)

Maps Coffee types to LLVM types.

**Type Mappings:**
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
        // ... more types
    }
}
```

### 4. Arithmetic Operations (`arithmetic/`)

Generates LLVM IR for arithmetic and logical operations.

**Supported Operations:**
- Binary: `+`, `-`, `*`, `/`, `%`
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `<<`, `>>`

**Example:**
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
        // ... more operators
    }
}
```

### 5. Control Flow (`control_flow/`)

Generates LLVM IR for control flow constructs.

**If Expressions:**
```coffee
if condition:
    then_block
else:
    else_block
```

**While Loops:**
```coffee
while condition:
    loop_body
```

**For Loops:**
```coffee
for item in collection:
    loop_body
```

**Match Expressions:**
```coffee
match value:
    pattern1 => result1
    pattern2 => result2
    _ => default
```

### 6. Memory Operations (`memory_ops.rs`)

Generates LLVM IR for Coffee's memory management operations.

**Operations:**
- `mv` - Move ownership (memcpy + invalidate source)
- `clone` - Deep copy of nested `str` / class / resource array / tuple fields (`memory_ops/clone.rs`). `object`, refs, and slice fields stay memcpy-only (no pointee clone).
- `rm` - Early drop. `[T; N]` and Coffee `[T]` (fat `{ptr,len}`) drop resource **elements** with the container; class slice fields do **not** `free` the buffer pointer. `object` fields do not `free` the pointee. `buf` drop does `free` the pointer.

`copy` and `clean out` are type errors (not codegen features).

**Implementation:**
```rust
fn compile_mv(&mut self, source: &str, target: &str) -> Result<(), BackendError> {
    let source_val = self.value_map.get(source).unwrap();
    let target_val = self.value_map.get(target).unwrap();

    // Generate memcpy
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

### 7. Functions (`functions/`)

Generates LLVM IR for function definitions and calls.

**Function Generation:**
```rust
fn compile_function(&mut self, func: &Function) -> Result<(), BackendError> {
    // Create function type
    let param_types: Vec<BasicMetadataTypeEnum> = func.params.iter()
        .map(|p| coffee_type_to_llvm(self.backend.context, &p.type_).into())
        .collect();

    let fn_type = func.return_type.to_llvm_type(self.backend.context)
        .fn_type(&param_types, false);

    // Add function to module
    let fn_value = self.backend.module.add_function(
        &func.name,
        fn_type,
        None
    );

    // Create entry block
    let entry_block = self.backend.context.append_basic_block(fn_value, "entry");
    self.builder.position_at_end(entry_block);

    // Compile body
    for stmt in &func.body.statements {
        self.compile_statement(stmt)?;
    }

    Ok(())
}
```

**C ABI Support:**
- Extern "C" linkage
- C-compatible calling convention
- Type mapping for C interop

### 8. Classes (`classes.rs`)

Generates LLVM IR for class and enum definitions.

**Class Generation:**
```rust
fn compile_class(&mut self, class: &ClassDef) -> Result<(), BackendError> {
    // Create struct type for class fields
    let field_types: Vec<BasicTypeEnum> = class.fields.iter()
        .map(|f| coffee_type_to_llvm(self.backend.context, &f.type_))
        .collect();

    let struct_type = self.backend.context.struct_type(&field_types, class.is_packed);

    // Register class type
    self.type_map.insert(class.name.clone(), struct_type.into());

    // Compile methods
    for method in &class.methods {
        self.compile_method(method, &struct_type)?;
    }

    Ok(())
}
```

**Enum Generation:**
- Tag-based representation
- Variant-specific data
- Pattern matching support

### 9. JIT Execution

The backend supports JIT (Just-In-Time) execution for immediate testing.

```rust
pub fn compile_and_run(
    source: &Program,
    c_imports: &[String],
    cfc_symbols: HashMap<String, CSymbolTable>,
    hir_fns: Vec<crate::hir::MirFn>,
    user_args: &[String],
) -> Result<i32, String>
{
    // Create context and backend
    let context = Context::create();
    let backend = Backend::new(&context, "coffee_jit");

    // Same codegen path as AOT
    let mut codegen = CodeGenerator::new(&backend);
    codegen.compile_program_with_hir(source, c_imports, cfc_symbols, hir_fns)?;

    // Verify module
    backend.verify()?;

    // Create JIT execution engine
    let mut execution_engine = backend.module.create_jit_execution_engine(OptimizationLevel::None)?;

    // Map C library functions
    map_c_library_functions(&mut execution_engine, &backend.module, c_imports)?;

    // Find and execute main function
    let main_fn = backend.module.get_function("main")
        .ok_or("No main() entry point found")?;

    // Execute main function
    unsafe {
        let main_fn_ptr = execution_engine.get_function_address("main")?;
        let main_fn: MainFn = std::mem::transmute(main_fn_ptr);
        let result = main_fn(argc, argv);
        Ok(result)
    }
}
```

## Code Generation Process

### 1. Module Setup
- Create LLVM module
- Set target triple
- Initialize builder

### 2. Type Registration
- Register all user-defined types
- Create type mappings
- Generate type metadata

### 3. Function Declaration
- Declare all functions (forward declarations)
- Generate function signatures
- Register in function map

### 4. Code Generation
- Generate function bodies
- Compile statements and expressions
- Handle control flow

### 5. Optimization
- Apply LLVM optimizations
- Inline functions
- Dead code elimination

### 6. Code Emission
- Verify module
- Write output files
- Generate executables

## C Integration

### C Function Linking

The backend handles C function calls through:
1. Declare external C functions in LLVM IR
2. Map C library symbols at runtime (JIT) or link time (AOT)
3. Handle type conversions between Coffee and C

**Example:**
```llvm
declare i32 @printf(i8*, ...)
```

### Type Mapping

| Coffee Type | C Type | LLVM Type |
|-------------|--------|-----------|
| `int(4)+` | `int` | `i32` |
| `float(8)` | `double` | `double` |
| `string` | `const char*` | `i8*` |
| `bool` | `_Bool` | `i1` |

## Optimization

The backend supports LLVM optimization levels:
- **O0**: No optimizations (fastest compilation)
- **O1**: Basic optimizations
- **O2**: Standard optimizations
- **O3**: Aggressive optimizations (slowest compilation)

**Optimizations Applied:**
- Constant folding
- Dead code elimination
- Function inlining
- Loop optimizations
- Vectorization (when applicable)

## Error Handling

**Backend Errors:**
```rust
pub enum BackendError {
    CodeGenerationError { message: String },
    TypeConversionError { from: Type, to: String },
    UndefinedSymbol { name: String },
    VerificationError { message: String },
}
```

## Usage Example

```rust
use coffee::backend::{Backend, CodeGenerator};
use inkwell::context::Context;

let context = Context::create();
let backend = Backend::new(&context, "my_module");

let mut codegen = CodeGenerator::new(&backend);
codegen.compile_program_with_hir(&program, &c_imports, cfc_symbols, hir_fns)?;

// Verify
backend.verify()?;

// Write object file
backend.write_object_file(std::path::Path::new("output.o"))?;

// Or execute with JIT (same hir_fns handoff)
let result = coffee::backend::compile_and_run(&program, &c_imports, cfc_symbols, hir_fns, &[])?;
println!("Program exited with code: {}", result);
```

## Design Considerations

### 1. Memory Safety
- Proper memory allocation and deallocation
- Safe pointer operations
- No undefined behavior

### 2. Performance
- Efficient LLVM IR generation
- Minimal runtime overhead
- Aggressive optimizations

### 3. Portability
- Cross-platform support
- Multiple target architectures
- ABI compatibility

## Future Enhancements

- Vector instruction generation
- SIMD support
- GPU code generation
- Better inline assembly support
- Profile-guided optimization (PGO)
- Link-time optimization (LTO)