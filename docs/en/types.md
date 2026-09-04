# Type System Module

## Overview

The Type System module (`src/types/`) provides comprehensive type checking, type inference, and type management for the Coffee language. It ensures type safety across all language constructs and enables automatic type inference where type annotations are omitted.

## Module Structure

```
src/types/
├── mod.rs         # Main type system module
├── definition.rs  # Type definitions
├── registry.rs    # Type registry and management
├── checker.rs     # Type checker
├── inference.rs   # Type inference engine
└── errors.rs      # Type system errors
```

## Core Components

### 1. Type Definitions (`definition.rs`)

Defines all types in the Coffee language.

**Built-in Types:**
```rust
pub enum Type {
    // Primitive types
    Int { signed: bool, bytes: usize },
    Float { bytes: usize },
    Bool,
    String,
    Void,

    // Composite types
    Array(Box<Type>, Option<usize>),      // [T; N] or [T]
    Slice(Box<Type>),                      // [T]
    Tuple(Vec<Type>),                      // (T1, T2, ...)
    Function(Vec<Type>, Box<Type>),        // fn(params) -> return
    Reference(Box<Type>, bool),            // &T or &mut T

    // User-defined types
    Struct(String, Vec<(String, Type)>),
    Enum(String, Vec<EnumVariant>),
    Class(String),

    // Type variables (for generics)
    TypeVar(String),
}
```

**Type Properties:**
- Size and alignment information
- Method dispatch capabilities
- Trait implementations
- Subtype relationships

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
- `int(8)+` (i64), `int(4)+` (i32), `int(2)+` (i16), `int(1)+` (i8)
- `int(8)-` (u64), `int(4)-` (u32), `int(2)-` (u16), `int(1)-` (u8)
- `float(8)` (f64), `float(4)` (f32)
- `bool`, `string`, `void`

### 3. Type Checker (`checker.rs`)

Validates type correctness across the program.

**Checking Modes:**
```rust
pub enum CheckingMode {
    Strict,        // Require explicit type annotations
    Inference,     // Infer types where possible
    Comprehensive, // Full type checking with inference
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
- Numeric type promotion (int to float, smaller int to larger int)
- Reference compatibility
- Subtype polymorphism

### 4. Type Inference (`inference.rs`)

Infers types for expressions without explicit annotations.

**Inference Algorithm (Hindley-Milner variant):**
1. Generate type variables for unannotated expressions
2. Generate constraints from expression structure
3. Unify constraints to solve for type variables
4. Substitute type variables with concrete types

**Inference Examples:**
```rust
// Infers: int
let x = 42;

// Infers: float
let y = 3.14;

// Infers: int (from function signature)
fn add(a: int, b: int) => int:
    return a + b  // a + b inferred as int

// Infers: [int]
let arr = [1, 2, 3];
```

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
    type_: Type::Int { signed: true, bytes: 4 },
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
- Trait system support (future)
- Generic type parameters (future)

## Future Enhancements

- Generic types and type parameters
- Trait system
- Associated types
- Type-level programming
- Dependent types
- Better error messages with type suggestions