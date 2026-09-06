# Diagnostics Module

## Overview

The Diagnostics module (`src/diagnostics.rs`) provides a comprehensive error reporting and diagnostic system for the Coffee compiler. It offers professional, color-coded error messages with rich context, suggestions, and related information to help developers quickly understand and fix issues.

## Module Purpose

The diagnostics system serves several critical purposes:

1. **Error Reporting**: Clear, structured error messages
2. **Context Awareness**: Source code location and snippets
3. **Suggestions**: Helpful hints for fixing issues
4. **Error Classification**: Categorized errors with codes
5. **Error Chaining**: Cause-and-effect relationships
6. **Similar Name Detection**: "Did you mean?" suggestions

## Core Components

### 1. Severity Levels (`Severity`)

Severity levels indicate the importance of diagnostic messages:

```rust
pub enum Severity {
    Error,   // Prevents compilation
    Warning, // Does not prevent compilation but should be addressed
    Hint,    // Suggestion for improvement
}
```

**Color Coding:**
- `Error`: Red (`\x1b[31m`)
- `Warning`: Yellow (`\x1b[33m`)
- `Hint`: Cyan (`\x1b[36m`)

### 2. Source Location (`SourceLocation`)

Tracks precise source code positions:

```rust
pub struct SourceLocation {
    pub file: Option<String>,      // File path
    pub line_start: usize,         // Start line (1-based)
    pub line_end: usize,           // End line (1-based)
    pub column_start: usize,       // Start column (1-based)
    pub column_end: usize,         // End column (1-based)
    pub byte_offset: usize,        // Byte offset in source
    pub byte_length: usize,        // Byte length
}
```

**Usage:**
```rust
let loc = SourceLocation::with_file("example.cf", 10, 5, 15);
println!("{}", loc.format()); // "example.cf:10:5"
```

### 3. Error Classification (`ErrorKind`)

Categorizes errors with unique error codes:

```rust
pub enum ErrorKind {
    // Parse Errors (E001-E099)
    InvalidSyntax { context: String },
    UnexpectedToken { token: String, expected: Vec<String> },
    IncompleteInput { expected: String },

    // Type Errors (E100-E199)
    TypeMismatch { expected: String, found: String },
    UnknownType { name: String },
    IncompatibleTypes { ty1: String, ty2: String },

    // Symbol Errors (E200-E299)
    UndefinedSymbol { name: String, symbol_type: SymbolType },
    DuplicateDefinition { name: String, previous: SourceLocation },
    InvalidSymbolAccess { name: String, reason: String },

    // Memory Errors (E300-E399)
    UseAfterMove { name: String },
    UseAfterDrop { name: String },
    InvalidMemoryOperation { operation: String, reason: String },
    BorrowViolation { variable: String, reason: String },

    // Lifetime Errors (E400-E499)
    LifetimeError { variable: String, reason: String },
    BorrowConflict { variable: String },

    // Import Errors (E500-E599)
    ModuleNotFound { module: String },
    CircularImport { path: Vec<String> },
    InvalidImport { import: String, reason: String },

    // Constraint Errors (E600-E699)
    ConstraintViolation { constraint: String, reason: String },

    // Code Generation Errors (E700-E799)
    CodeGeneration { stage: String, details: String },

    // Verification Errors (E800-E899)
    Verification { phase: String, details: String },

    // Link Errors (E900-E999)
    LinkError { details: String },
}
```

**Error Code Ranges:**
- `E001-E099`: Parse errors
- `E100-E199`: Type errors
- `E200-E299`: Symbol errors
- `E300-E399`: Memory errors
- `E400-E499`: Lifetime errors
- `E500-E599`: Import errors
- `E600-E699`: Constraint errors
- `E700-E799`: Code generation errors
- `E800-E899`: Verification errors
- `E900-E999`: Link errors

### 4. Symbol Types (`SymbolType`)

Classifies symbol categories:

```rust
pub enum SymbolType {
    Variable,
    Function,
    Type,
    Module,
    Lifetime,
    Constant,
    Method,
    Field,
}
```

### 5. Diagnostic Message (`Diagnostic`)

The complete diagnostic message structure:

```rust
pub struct Diagnostic {
    pub severity: Severity,
    pub kind: ErrorKind,
    pub message: String,
    pub location: SourceLocation,
    pub code: String,                    // Error code (e.g., "E200")
    pub snippets: Vec<CodeSnippet>,      // Code snippets showing error
    pub suggestions: Vec<Suggestion>,    // Fix suggestions
    pub related: Vec<RelatedDiagnostic>, // Related diagnostics
    pub cause: Option<Box<Diagnostic>>,  // Error chain
    pub similar_names: Vec<String>,      // "Did you mean?" suggestions
}
```

## Diagnostic Features

### 1. Code Snippets

Shows the exact source code with highlighting:

```rust
pub struct CodeSnippet {
    pub lines: Vec<String>,              // Source code lines
    pub line_start: usize,               // Line number of first line
    pub highlights: Vec<(usize, usize, usize)>, // Highlight ranges
}
```

**Example Output:**
```
    10 | fn add(a: int, b: int) => int:
    11 |     return a + b
    12 | }
       |     ^--- error: unexpected closing brace
```

### 2. Suggestions

Provides helpful suggestions for fixing issues:

```rust
pub struct Suggestion {
    pub message: String,
}
```

**Example Output:**
```
    = help: remove the closing brace
    = help: use 'end' keyword instead of '}'
```

### 3. Related Diagnostics

Shows related errors or notes:

```rust
pub struct RelatedDiagnostic {
    pub relation: RelationType,
    pub location: SourceLocation,
    pub message: String,
}

pub enum RelationType {
    CausedBy,
}
```

**Example Output:**
```
error[E200]: undefined variable: 'x'
   --> example.cf:5:10
    |
  5 |     return x + y
    |            ^ this variable is not defined
    |
note[E200]: variable 'x' was defined here
   --> example.cf:3:5
    |
  3 | let x: int = 10
    |     ^--- but it went out of scope
```

### 4. Error Chaining

Shows cause-and-effect relationships:

```rust
pub cause: Option<Box<Diagnostic>>,
```

**Example Output:**
```
error[E900]: link error: library 'mylib' not found
   --> main.cf:1:1
    |
  1 | use myfunction in mylib of c
    | ^--- cannot find library

caused by:
    error[E500]: module not found 'mylib'
       --> search path: ./, ./lib, /usr/lib
```

### 5. Similar Name Detection

Suggests similar names for typos:

```rust
pub fn levenshtein_distance(a: &str, b: &str) -> usize;
pub fn find_similar_names(target: &str, candidates: &[String], max_distance: usize, max_results: usize) -> Vec<String>;
```

**Example Output:**
```
error[E200]: undefined variable: 'lenght'
   --> example.cf:3:10
    |
  3 | let lenght: int = 10
    |     ^------ did you mean: `length`
```

## Diagnostic Emitter

The `DiagnosticEmitter` collects and manages diagnostics:

```rust
pub struct DiagnosticEmitter {
    diagnostics: Arc<RwLock<Vec<Diagnostic>>>,
    config: EmitterConfig,
}

impl DiagnosticEmitter {
    pub fn new() -> Self;
    pub fn emit(&self, diagnostic: Diagnostic);
    pub fn diagnostics(&self) -> Vec<Diagnostic>;
    pub fn clear(&self);
}
```

## Diagnostic Formatting

### Colored Output

```rust
pub fn format(&self) -> String;
```

**Example:**
```
error[E200]: undefined variable: 'x'
   --> example.cf:5:10
    |
  5 |     return x + y
    |            ^ this variable is not defined
    |
    | this is a symbol error
    = help: declare the variable before use
```

### Plain Output

```rust
pub fn format_plain(&self) -> String;
```

**Example:**
```
error[E200]: undefined variable: 'x'
   --> example.cf:5:10
  5 |     return x + y
    |            ^ this variable is not defined
help: declare the variable before use
```

## Error Categories

### Parse Errors (E001-E099)

**E001: Invalid Syntax**
```coffee
# Error
fn add(a: int, b: int => int:
```

**E002: Unexpected Token**
```coffee
# Error
let x: int = 10 20
```

**E003: Incomplete Input**
```coffee
# Error
fn add(a: int, b: int):
```

### Type Errors (E100-E199)

**E100: Type Mismatch**
```coffee
# Error
let x: int = "hello"
```

**E101: Unknown Type**
```coffee
# Error
let x: mytype = 10
```

**E102: Incompatible Types**
```coffee
# Error
fn add(a: int, b: string) => int:
```

### Symbol Errors (E200-E299)

**E200: Undefined Symbol**
```coffee
# Error
fn example() => int:
    return x  # x is not defined
```

**E201: Duplicate Definition**
```coffee
# Error
let x: int = 10
let x: int = 20  # duplicate
```

**E202: Invalid Symbol Access**
```coffee
# Error
fn example() => int:
    let x: int = 10
    rm x
    return x  # use after drop
```

### Memory Errors (E300-E399)

**E300: Use After Move**
```coffee
# Error
let x: int = 10
mv x y
return x  # use after move
```

**E301: Use After Drop**
```coffee
# Error
let x: int = 10
rm x
return x  # use after drop
```

**E302: Invalid Memory Operation**
```coffee
# Error
mv x x  # invalid: moving to itself
```

**E303: Borrow Violation**
```coffee
# Error
let x: int = 10
let y: &int = &x
mv x z  # cannot move while borrowed
```

### Lifetime Errors (E400-E499)

**E400: Lifetime Error**
```coffee
# Error
fn get_ref() => &int:
    let x: int = 10
    return &x  # returns reference to local variable
```

**E401: Borrow Conflict**
```coffee
# Error
let x: int = 10
let y: &mut int = &mut x
let z: &int = &x  # cannot borrow while mutably borrowed
```

### Import Errors (E500-E599)

**E500: Module Not Found**
```coffee
# Error
use nonexistent_module
```

**E501: Circular Import**
```coffee
# Error
# module_a.cf
use module_b

# module_b.cf
use module_a
```

**E502: Invalid Import**
```coffee
# Error
use symbol in nonexistent_module
```

### Code Generation Errors (E700-E799)

**E700: Code Generation Error**
```rust
// Internal error during LLVM IR generation
```

### Verification Errors (E800-E899)

**E800: Verification Error**
```rust
// LLVM IR verification failed
```

### Link Errors (E900-E999)

**E900: Link Error**
```coffee
# Error
use myfunction in nonexistent_lib of c
```

## Integration with Other Modules

### Parser Integration

The parser creates diagnostics for syntax errors:

```rust
impl From<parser::ParseError> for Diagnostic {
    fn from(error: parser::ParseError) -> Self {
        Diagnostic::new(
            Severity::Error,
            ErrorKind::InvalidSyntax { context: error.message },
            error.message
        ).with_location(error.location)
    }
}
```

### Type System Integration

The type system creates diagnostics for type errors:

```rust
impl From<types::TypeSystemError> for Diagnostic {
    fn from(error: types::TypeSystemError) -> Self {
        match error {
            TypeSystemError::TypeMismatch { expected, found, .. } => {
                Diagnostic::new(
                    Severity::Error,
                    ErrorKind::TypeMismatch {
                        expected: format!("{:?}", expected),
                        found: format!("{:?}", found),
                    },
                    format!("type mismatch: expected {}, found {:?}", expected, found)
                )
            }
            // ... other cases
        }
    }
}
```

### Semantic Analysis Integration

The semantic analyzer creates diagnostics for semantic errors:

```rust
impl From<semantic::SpaceError> for Diagnostic {
    fn from(error: semantic::SpaceError) -> Self {
        Diagnostic::new(
            Severity::Error,
            ErrorKind::UndefinedSymbol {
                name: error.name,
                symbol_type: SymbolType::Variable,
            },
            format!("undefined variable: '{}'", error.name)
        ).with_similar_names(error.suggestions)
    }
}
```

## Usage Examples

### Creating a Simple Diagnostic

```rust
use diagnostics::{Diagnostic, ErrorKind, Severity};

let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::UndefinedSymbol {
        name: "x".to_string(),
        symbol_type: SymbolType::Variable,
    },
    "undefined variable: 'x'"
);

println!("{}", diagnostic.format());
```

### Creating a Diagnostic with Location

```rust
let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::TypeMismatch {
        expected: "int".to_string(),
        found: "string".to_string(),
    },
    "type mismatch: expected int, found string"
).with_location(SourceLocation::with_file("example.cf", 5, 10, 20));
```

### Adding Suggestions

```rust
let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::InvalidSyntax {
        context: "missing closing brace".to_string(),
    },
    "missing closing brace"
).with_suggestion(Suggestion::new("add a closing brace '}' at the end"));
```

### Adding Code Snippets

```rust
let snippet = CodeSnippet::new(
    vec![
        "fn add(a: int, b: int) => int:".to_string(),
        "    return a + b".to_string(),
    ],
    1
);

let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::InvalidSyntax { context: "missing return type".to_string() },
    "missing return type"
).with_snippet(snippet);
```

### Error Chaining

```rust
let cause = Diagnostic::new(
    Severity::Error,
    ErrorKind::ModuleNotFound { module: "mylib".to_string() },
    "module 'mylib' not found"
);

let diagnostic = Diagnostic::new(
    Severity::Error,
    ErrorKind::InvalidImport {
        import: "mylib".to_string(),
        reason: "module not found".to_string(),
    },
    "cannot import 'mylib'"
).with_cause(Box::new(cause));
```

### Using Diagnostic Emitter

```rust
let emitter = DiagnosticEmitter::new();

// Emit diagnostics
emitter.emit(diagnostic1);
emitter.emit(diagnostic2);

// Get all diagnostics
let diagnostics = emitter.diagnostics();

// Clear diagnostics
emitter.clear();
```

## Best Practices

### 1. Provide Clear Messages

```rust
// Good
"undefined variable: 'x'"

// Bad
"error: variable not found"
```

### 2. Include Source Location

```rust
// Always include location
diagnostic.with_location(location)
```

### 3. Provide Helpful Suggestions

```rust
diagnostic.with_suggestion(Suggestion::new(
    "declare the variable before use"
))
```

### 4. Use Appropriate Severity

```rust
// Use Error for critical issues
Diagnostic::new(Severity::Error, ...)

// Use Warning for non-critical issues
Diagnostic::new(Severity::Warning, ...)

// Use Hint for suggestions
Diagnostic::new(Severity::Hint, ...)
```

### 5. Add Code Snippets

```rust
diagnostic.with_snippet(snippet)
```

### 6. Suggest Similar Names

```rust
diagnostic.with_similar_names(vec!["length".to_string()])
```

## Configuration

### Emitter Configuration

```rust
pub struct EmitterConfig {
    pub show_colors: bool,
}
```

**Usage:**
```rust
let config = EmitterConfig {
    show_colors: false, // Disable colors
};

let emitter = DiagnosticEmitter::with_config(config);
```

## Performance Considerations

### 1. String Allocation

Minimize string allocations by using `&str` where possible:

```rust
// Good
Diagnostic::new(Severity::Error, ..., "message")

// Avoid
Diagnostic::new(Severity::Error, ..., format!("message: {}", x))
```

### 2. Diagnostic Collection

Use `Arc<RwLock>` for efficient sharing:

```rust
diagnostics: Arc<RwLock<Vec<Diagnostic>>>
```

### 3. Similar Name Search

Limit search space for performance:

```rust
let similar = find_similar_names(
    target,
    &candidates,
    max_distance: 3,  // Limit distance
    max_results: 5    // Limit results
);
```

## Future Enhancements

- Multi-line error spans
- Better error recovery suggestions
- Error code documentation
- Interactive error fixing
- Error suppression directives
- Custom error handlers
- Error aggregation
- Error trend analysis
- Machine learning for suggestions
- IDE integration support

## See Also

- [Parser Module](parser.md) - Parsing and syntax errors
- [Semantic Analysis Module](semantic.md) - Semantic errors
- [Type System Module](types.md) - Type errors
- [Compiler Module](compiler.md) - Compilation orchestration