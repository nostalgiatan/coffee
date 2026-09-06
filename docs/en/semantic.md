# Semantic Analysis Module

## Overview

The Semantic Analysis module (`src/semantic/`) is responsible for analyzing the parsed AST to ensure it adheres to the semantic rules of the Coffee language. It performs scope management, symbol resolution, lifetime tracking, and comprehensive semantic validation.

## Module Structure

```
src/semantic/
├── mod.rs           # Semantic analysis module
├── analyzer/        # SemanticAnalyzer (not analyzer.rs)
│   ├── mod.rs
│   ├── decls.rs
│   ├── expr.rs
│   ├── memory.rs
│   ├── const_eval.rs
│   └── report.rs
├── scope.rs         # Scope management
├── symbols.rs       # Symbol table management
├── lifetime.rs      # Named lifetime params (LifetimeSpace)
└── c_builtins.rs    # Built-in libc/libm CFC tables
```

## Core Components

### 1. Semantic Analyzer (`analyzer/`)

The `SemanticAnalyzer` is the main component that orchestrates semantic analysis.

**Key Responsibilities:**
- Manage scopes and symbol tables
- Analyze declarations (variables, functions, classes, enums)
- Validate symbol usage
- Collect semantic errors. Borrow checking is `types::borrow` / `TypeChecker`, not the analyzer.
- Collect semantic errors

**Main Methods:**

```rust
pub struct SemanticAnalyzer {
    type_registry: Arc<RwLock<TypeRegistry>>,
    scopes: Vec<Scope>,
    symbols: SymbolTable,
    errors: Vec<SemanticError>,
    c_imports: Vec<String>,
    cfc_symbols: HashMap<String, CSymbolTable>,
}
```

**Analysis Methods:**
- `analyze_statement(&Statement) -> Result<(), SemanticError>`
- `analyze_function_decl(&Function) -> Result<(), SemanticError>`
- `analyze_class_def(&ClassDef) -> Result<(), SemanticError>`
- `analyze_variable_decl(&VariableDecl) -> Result<(), SemanticError>`
- `analyze_expression(&Expr) -> Result<(), SemanticError>`

### 2. Scope Management (`scope.rs`)

The scope system manages variable visibility and lifetime.

**Scope Hierarchy:**
- Global scope (module-level)
- Function scopes
- Block scopes (if, while, for, match)
- Class scopes

**Scope Operations:**
```rust
pub struct Scope {
    level: usize,
    parent: Option<usize>,
    variables: HashMap<String, Symbol>,
    functions: HashMap<String, FunctionSymbol>,
    classes: HashMap<String, ClassSymbol>,
}

impl Scope {
    pub fn new(level: usize, parent: Option<usize>) -> Self;
    pub fn enter(&mut self) -> usize;
    pub fn exit(&mut self) -> Option<Scope>;
    pub fn lookup(&self, name: &str) -> Option<&Symbol>;
    pub fn define(&mut self, name: String, symbol: Symbol) -> Result<(), SemanticError>;
}
```

**Scope Features:**
- Nested scope support
- Shadowing detection
- Variable lifetime tracking
- Scope-based symbol resolution

### 3. Symbol Table (`symbols.rs`)

Manages all symbols declared in the program.

**Symbol Types:**
```rust
pub enum Symbol {
    Variable(VariableSymbol),
    Function(FunctionSymbol),
    Class(ClassSymbol),
    Enum(EnumSymbol),
    Method(MethodSymbol),
}

pub struct VariableSymbol {
    pub name: String,
    pub type_: Type,
    pub is_mutable: bool,
    pub scope_level: usize,
    pub is_extern: bool,
}

pub struct FunctionSymbol {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub is_c_abi: bool,
    pub scope_level: usize,
}
```

**Symbol Table Operations:**
- Insert symbols
- Lookup symbols (with scope resolution)
- Check for duplicate declarations
- Export symbols for external use

### 4. Lifetime Analysis (`lifetime.rs`)

Tracks named lifetime parameters for functions. **Borrow/loan rules are enforced in `src/types/borrow.rs`.** `LifetimeSpace` is not a proof checker.

**Lifetime Features (analyzer):** named function lifetime params in `LifetimeSpace` only.

**Borrow rules (`types::borrow`):**
- References cannot outlive their referent (no `&local` in return)
- Mutable references are exclusive
- Multiple immutable references allowed

**Analysis Checks:**
```rust
pub fn analyze_lifetime(&mut self, expr: &Expr) -> Result<(), SemanticError> {
    match expr {
        Expr::Variable(name) => {
            self.check_variable_lifetime(name)?;
        }
        Expr::BinaryOp { left, op, right } => {
            self.analyze_lifetime(left)?;
            self.analyze_lifetime(right)?;
        }
        // ... more cases
    }
    Ok(())
}
```

## Analysis Process

The semantic analysis follows these phases:

### Phase 1: Declaration Collection
1. Create global scope
2. Collect all top-level declarations
3. Build symbol table
4. Detect duplicate declarations

### Phase 2: Statement Analysis
1. Enter appropriate scope for each statement
2. Analyze declarations within scope
3. Validate symbol usage
4. Check semantic rules

### Phase 3: Expression Analysis
1. Validate variable references
2. Check function calls
3. Validate operator usage
4. Type inference integration

### Phase 4: Lifetime Validation
1. Track variable lifetimes
2. Validate borrow rules
3. Check move semantics
4. Ensure reference safety

## Semantic Checks

### Variable Declaration Checks
- Type annotation validity
- Initializer expression type compatibility
- Duplicate variable detection
- Shadowing validation

### Function Declaration Checks
- Parameter name uniqueness
- Return type validity
- C ABI validation
- Function signature uniqueness

### Class Declaration Checks
- Field name uniqueness
- Field type validity
- Inheritance validity
- Method signature uniqueness

### Expression Checks
- Variable existence
- Function call validity
- Operator type compatibility
- Array indexing bounds (where possible)

### Memory Operation Checks
- Variable existence for `mv`, `clone`, `copy`, `rm`
- Type compatibility for memory operations
- Ownership transfer validation
- Borrowing rule compliance

## Error Handling

**Semantic Error Types:**
```rust
pub enum SemanticError {
    UndefinedVariable { name: String, location: Location },
    UndefinedFunction { name: String, location: Location },
    DuplicateDeclaration { name: String, location: Location },
    TypeMismatch { expected: Type, found: Type, location: Location },
    InvalidBorrow { reason: String, location: Location },
    InvalidMove { reason: String, location: Location },
    LifetimeError { reason: String, location: Location },
    // ... more error types
}
```

**Error Reporting:**
- Collect all errors during analysis
- Provide detailed error messages
- Include source location information
- Suggest fixes when possible

## Integration with Other Modules

### With Parser
- Receives AST from parser
- Analyzes parsed statements and expressions
- Reports semantic errors

### With Type System
- Uses type registry for type validation
- Performs type checking on expressions
- Integrates with type inference

### With Backend
- Provides symbol information for code generation
- Supplies type information for LLVM IR generation
- Validates C function signatures

## Usage Example

```rust
use coffee::semantic::SemanticAnalyzer;
use coffee::parser;

let source = r#"
fn add(x: int, y: int) => int:
    return x + y

let result: int = add(1, 2)
"#;

// Parse source
let program = parser::parse_program(source)?;

// Create semantic analyzer
let type_registry = Arc::new(RwLock::new(TypeRegistry::root()));
let mut analyzer = SemanticAnalyzer::new(type_registry);

// Analyze program
for statement in program.statements {
    analyzer.analyze_statement(&statement)?;
}

// Check for errors
if analyzer.has_errors() {
    for error in analyzer.errors() {
        eprintln!("Semantic error: {:?}", error);
    }
} else {
    println!("Semantic analysis passed");
}
```

## Design Considerations

### 1. Multi-Pass Analysis
The analyzer performs multiple passes:
- Declaration collection pass
- Definition analysis pass
- Usage validation pass

### 2. Incremental Analysis
Support for analyzing code incrementally:
- Analyze individual statements
- Maintain state between analyses
- Reuse symbol tables

### 3. Error Recovery
Continue analysis after errors:
- Collect all errors
- Provide comprehensive reporting
- Don't stop at first error

### 4. C Integration
Special handling for C functions:
- C function symbol tables
- Type mapping between Coffee and C
- ABI validation

## Testing

Test coverage should include:
- Correct symbol resolution
- Proper scope management
- Lifetime validation
- Error detection and reporting
- Edge cases (shadowing, circular dependencies)
- C function integration

## Future Enhancements

Potential improvements:
- More precise lifetime analysis
- Advanced borrow checking
- Generic type parameter analysis
- Trait system support
- Module system enhancement
- Better error messages with suggestions