# Type System Module

## Overview

The Type System module (`src/types/`) provides comprehensive type checking, type inference, and type management for the Coffee language. It ensures type safety across all language constructs and enables automatic type inference where type annotations are omitted.

## Module Structure

```
src/types/
├── mod.rs           # Type system module
├── definition/      # Type definitions (not definition.rs)
│   ├── mod.rs
│   └── tests.rs
├── registry.rs      # Type registry
├── mono.rs          # Generics v1: token substitute → List__int
├── checker/         # TypeChecker (not checker.rs)
│   ├── mod.rs
│   ├── values.rs
│   ├── stmt.rs
│   ├── expr/        # expression checking (not a single expr.rs)
│   ├── call.rs
│   ├── match_check.rs
│   ├── memory.rs
│   ├── import_main.rs
│   ├── compat.rs
│   └── tests.rs
├── borrow.rs        # Intra-procedural borrow checker
├── last_use.rs      # Last-use implicit move
└── errors.rs        # Type system errors
```

## Core Components

### 1. Type Definitions (`definition/`)

Defines all types in the Coffee language.

Surface syntax `int(N)+` / `int(N)-` / `float(N)` uses **N as a byte count** (`int(4)+` is C `int`). The enum stores **bit width**: `bits = 8 * N`.

**Built-in Types:**
```rust
pub enum Type {
    Int { bits: u8, signed: bool },
    Float { bits: u8 },
    Bool,
    String,
    Void,
    Unit,
    Array { elem: Box<Type>, size: usize },
    Slice(Box<Type>),
    Tuple(Vec<Type>),
    Function { params: Vec<Type>, return_type: Box<Type> },
    Ref { elem: Box<Type>, mutable: bool },
    NamedType { name: String },
    App { name: String, args: Vec<Type> },  // List<int>
    Variadic,
}
```

**Type Properties:**
- Size and alignment information
- Method dispatch on concrete (monomorphized) names
- Subtype relationships

**Generics v1 (`mono.rs`):** `Type::App` instantiates by substituting type-param tokens. `List<int>` becomes the concrete class `List__int`; methods are `List__int_push`. Syntax: `class List<T>:` / `fn id<T>(x: T) => T`. Nested `List<List<int>>` is one type. There is no generic std `List` (std ships `IntBuf`) and no trait bounds (`T: Trait` is not a feature).

**`buf`:** builtin resource (`NamedType "buf"`): owned malloc pointer; drop `free`s it; `clone buf` is a type error. `object` is an unfreed C handle.

**Slices:** Coffee `[T]` is a fat pointer on Coffee `fn` params/returns. `c fn` still rejects slices. Last-use implicit move: `src/types/last_use.rs`.

### 2. Type Registry (`registry.rs`)

Manages all types in the program.

**Registry Operations:**
```rust
pub struct TypeRegistry {
    types: HashMap<String, Type>,
    next_type_id: usize,
}

impl TypeRegistry {
    pub fn root() -> Self;  // Create with built-in types

    pub fn register_type(&mut self, name: String, type_: Type) -> Result<(), TypeSystemError>;
    pub fn lookup_type(&self, name: &str) -> Option<&Type>;
    pub fn get_type_id(&self, type_: &Type) -> Option<usize>;

    pub fn is_subtype(&self, sub: &Type, sup: &Type) -> bool;
    pub fn unify(&mut self, t1: &Type, t2: &Type) -> Result<Type, TypeSystemError>;
}
```

**Built-in Types:**
- `int(8)+` (i64, 8 bytes), `int(4)+` (i32, 4 bytes), `int(2)+` (i16), `int(1)+` (i8)
- `int(8)-` (u64), `int(4)-` (u32), `int(2)-` (u16), `int(1)-` (u8)
- `float(8)` (f64), `float(4)` (f32)
- `bool`, `string`, `void`
- Builtin class `Error` `{ code: int, note: str, e: object }` (not user-redefinable)

**Exceptions (`raise`):** Allowed iff the type is `Error` or a class whose inheritance chain reaches `Error` (`class C of Error`, then further subclasses). Extra fields and methods on exception classes are allowed (do not redeclare `code` / `note` / `e`). A class named `*Error` without `of Error` is a normal class and cannot be raised. `#name` listeners still take `err: Error` and may only rely on that prefix.

### 3. Type Checker (`checker/`)

Validates type correctness across the program.

**Checking Modes:**
```rust
pub enum CheckingMode {
    Simple,        // Structural types only
    Comprehensive, // Ownership, memory ops, borrow checker
}
```

**Type Checking Methods:**
```rust
pub struct TypeChecker {
    registry: Arc<RwLock<TypeRegistry>>,
    mode: CheckingMode,
    diagnostics: Vec<Diagnostic>,
}

impl TypeChecker {
    pub fn check_variable_decl(&mut self, decl: &VariableDecl) -> Result<(), Diagnostic>;
    pub fn check_function_decl(&mut self, func: &Function) -> Result<(), Diagnostic>;
    pub fn check_expression(&mut self, expr: &Expr) -> Result<Type, Diagnostic>;
    pub fn check_memory_op(&mut self, op: &MemoryOp) -> Result<(), Diagnostic>;

    pub fn check_types_compatible(&self, t1: &Type, t2: &Type) -> bool;
    pub fn coerce_type(&self, from: &Type, to: &Type) -> Result<Type, Diagnostic>;
}
```

**Type Compatibility Rules:**
- Exact match for most types
- Integer/float *literals* may fit a narrower annotated width if the value is in range
- `object` C handles vs pointer-sized ints as documented in the type checker
- `&T` / `&mut T` follow the borrow checker (shared XOR exclusive)

### 3b. Borrow checker (`borrow.rs`)

Intra-procedural loans on **variable, field (`p.x`), and index (`a[i]`) places**. Wired from `TypeChecker` (`&x`, `&mut x`, `*r`, move/assign/rm, returning `&local`). A call whose callee returns `&T` may keep argument loans past the statement (same module; no `'a`). See `docs/superpowers/specs/2026-09-05-borrow-checker.md`.

Expression types are inferred and checked in `checker/`, not a separate `inference.rs` module.

### 5. Type System Errors (`errors.rs`)

Comprehensive type error reporting.

**Error Types:**
```rust
pub enum TypeSystemError {
    UndefinedType { name: String },
    TypeMismatch { expected: Type, found: Type },
    AmbiguousType { expr: String },
    InferenceFailed { reason: String },
    InvalidCoercion { from: Type, to: Type },
    RecursiveType { name: String },
}
```

## Type Checking Process

### 1. Declaration Checking
- Validate type annotations
- Register user-defined types
- Check recursive type definitions

### 2. Expression Checking
- Infer expression types
- Validate operator usage
- Check function call compatibility
- Validate array indexing

### 3. Statement Checking
- Validate variable declarations
- Check return statements
- Validate control flow types
- Check memory operations

### 4. Function Checking
- Validate parameter types
- Check return type consistency
- Validate function calls
- Check C ABI compatibility

## Type Inference

**Supported Inference:**
- Variable initialization
- Function return expressions
- Array literals
- Tuple literals
- Conditional expressions
- Lambda expressions (if supported)

**Inference Limitations:**
- Requires type annotations for function parameters
- User-defined types need explicit definitions
- C function signatures must be declared

## Integration

### With Semantic Analysis
- Provides type information for symbol table
- Validates type annotations in declarations
- Performs type-based semantic checks

### With Backend
- Supplies type information for LLVM IR generation
- Generates type metadata
- Handles type conversions in code generation

### With C Integration
- Maps Coffee types to C types
- Validates C function signatures
- Handles type conversions for FFI

## Usage Example

```rust
use coffee::types::{TypeChecker, TypeRegistry, CheckingMode};

let type_registry = Arc::new(RwLock::new(TypeRegistry::root()));
let mut checker = TypeChecker::new(type_registry.clone(), CheckingMode::Comprehensive);

// Check variable declaration
let decl = VariableDecl {
    name: "x".to_string(),
    type_: Type::Int { bits: 32, signed: true },
    init: Some(Box::new(Expr::Literal(Literal::Int(42)))),
};
checker.check_variable_decl(&decl)?;

// Check expression
let expr = Expr::BinaryOp {
    left: Box::new(Expr::Variable("x".to_string())),
    op: BinaryOperator::Add,
    right: Box::new(Expr::Literal(Literal::Int(10))),
};
let result_type = checker.check_expression(&expr)?;
```

## Design Considerations

### 1. Type Safety
- All expressions have well-defined types
- Type conversions are explicit (except safe promotions)
- No implicit conversions that lose information

### 2. Performance
- Type registry uses efficient lookups
- Type checking is O(n) for most operations
- Inference uses efficient unification algorithm

### 3. Extensibility
- Easy to add new types
- Generics v1 (`Type::App` / `mono.rs`); no `T: Trait`
- Trait system (not shipping)

## Future Enhancements

- Trait bounds and a generic std `List` (std has `IntBuf`, not `List<T>`)
- Associated types
- Type-level programming
- Dependent types
- Better error messages with type suggestions