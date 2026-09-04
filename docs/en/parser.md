# Parser Module

## Overview

The Parser module (`src/parser/`) is responsible for parsing Coffee source code into an Abstract Syntax Tree (AST). It implements a custom parser using the `nom` parser combinator library, supporting Coffee's unique syntax features including indentation-based block structures, memory management operations, pattern matching, and C language integration.

## Module Structure

```
src/parser/
├── mod.rs           # Main parser module and coordination
├── import.rs        # Import statement parsing
├── function.rs      # Function definition parsing
├── main.rs          # Main entry point parsing
├── if.rs            # If expression parsing
├── while.rs         # While loop parsing
├── match.rs         # Match expression parsing
├── for.rs           # For loop parsing
├── var.rs           # Variable declaration and control flow parsing
├── comment.rs       # Comment parsing
├── class.rs         # Class and enum definition parsing
├── memory.rs        # Memory operation parsing
├── expr.rs          # Expression parsing
├── error.rs         # Error types and handling
├── tracker.rs       # Block tracking for indentation
├── indent.rs        # Indentation handling
└── raise.rs         # Raise statement parsing
```

## Core Components

### 1. Main Parser (`mod.rs`)

The main parser orchestrates the entire parsing process through the `parse_program` function.

#### Key Functions

**`parse_program(input: &str) -> Result<Program, Vec<ParseError>>`**

Parses an entire Coffee source file into an AST. The function processes input line by line, identifying appropriate parsing strategies for each statement based on structure and content.

**Parsing Strategy:**
1. Skip empty lines and whole-line comments
2. Check for multiline comments (`/#* ... *#/`)
3. Attempt to parse multiline structures (functions, classes, enums, control flow)
4. Try to parse single-line statements
5. Smart error detection and recovery

**Error Recovery:**
The parser continues parsing even when encountering syntax errors, collecting all errors for comprehensive reporting.

**`parse_multiline_statement(lines: &[&str]) -> Option<(Statement, usize)>`**

Attempts to parse complex multiline constructs including:
- Functions (both `fn` and `c fn`)
- Classes (both normal and `packed`)
- Enums
- Control flow statements (if, while, for, match)

**`parse_single_line_statement(line: &str) -> Option<Statement>`**

Parses single-line statements using quick dispatch based on the first word:
- Main entry points (`main(...)`)
- Import statements (`use ...`)
- Variable declarations (`let ...`)
- Return/break/continue statements
- Comments
- Memory operations
- Expression statements

**`collect_multiline_content(lines: &[&str], keyword: &str) -> Option<(String, usize)>`**

Collects the content of a multiline construct by tracking indentation levels and block structure using a `BlockTracker`.

### 2. Block Tracking (`tracker.rs`)

The `BlockTracker` is responsible for correctly identifying nested structures and determining when blocks end.

**Key Features:**
- Tracks indentation levels
- Manages nested block types (functions, classes, control flow)
- Handles special cases like `elif`/`else` clauses
- Validates block closure

**Block Types:**
```rust
pub enum BlockType {
    Function,
    Class,
    Enum,
    IfStatement,
    WhileLoop,
    ForLoop,
    MatchExpr,
}
```

### 3. Statement Types

The parser supports all Coffee language constructs through the `Statement` enum:

```rust
pub enum Statement {
    Import(Import),
    Function(Function),
    Main(MainEntry),
    If(IfExpr),
    While(WhileLoop),
    Match(MatchExpr),
    For(ForLoop),
    VariableDecl(VariableDecl),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    SingleLineComment(SingleLineComment),
    MultiLineComment(MultiLineComment),
    Class(ClassDef),
    Enum(EnumDef),
    MemoryOp(MemoryOp),
    Raise(RaiseStmt),
    Expr(Box<Expr>),
}
```

### 4. Import Parsing (`import.rs`)

Handles various import statement formats:

**Import Types:**
- Simple: `use module_name`
- Aliased: `use module_name as alias`
- In-module: `use function_name in module_name`
- In-module with alias: `use function_name in module_name as alias`
- Multi-symbol: `use func1, func2 in module_name`
- C library: `use printf in libc of c`

### 5. Function Parsing (`function.rs`)

Parses function definitions with support for:

**Function Types:**
- Regular Coffee functions: `fn name(params) => return_type:`
- C ABI functions: `c fn name(params) => return_type:`
- Functions with error handlers: `fn name(params) #error_handler => return_type:`

**Components:**
- Function name
- Parameters (name, type, default values)
- Return type
- Function body (statements)
- C ABI flag

### 6. Class Parsing (`class.rs`)

Handles class and enum definitions:

**Class Types:**
- Normal classes: `class ClassName:`
- Packed classes: `packed class ClassName:` (no padding between fields)
- Enums: `enum EnumName:`

**Class Features:**
- Field definitions with types
- Method definitions
- Inheritance: `class Derived of Parent:`
- Bitfield support: `field: type:bit_width`

**Enum Features:**
- Simple variants: `Red`, `Green`, `Blue`
- Variants with data: `SomeValue(arg_type)`
- Struct-like variants: `Rgb(r: int, g: int, b: int)`

### 7. Control Flow Parsing

#### If Expressions (`if.rs`)
```coffee
if condition:
    body
elif condition2:
    body2
else:
    body3
```

#### While Loops (`while.rs`)
```coffee
while condition:
    body
```

#### For Loops (`for.rs`)
Supports multiple iteration patterns:
```coffee
# Collection iteration
for item in collection:
    body

# Range iteration
for i in range(start, end):
    body

# Range syntax
for i in start..end:
    body
```

#### Match Expressions (`match.rs`)
```coffee
match value:
    pattern1 => result1
    pattern2 => result2
    _ => default_result
```

### 8. Memory Operations (`memory.rs`)

Parses Coffee's unique memory management operations:

**Operations:**
- `mv source target` - Move ownership
- `clone source target` - Create a copy
- `copy source target` - Share reference
- `rm variable` - Delete variable
- `rm var1, var2, var3` - Batch delete
- `clean out` - Clean all variables
- `clean out except var1, var2` - Clean except specified
- `clean out var1, var2` - Clean specified variables

### 9. Expression Parsing (`expr.rs`)

Parses various expression types:

**Expression Types:**
- Literals (integers, floats, strings, booleans)
- Variables
- Binary operations (`+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`)
- Unary operations (`!`, `-`)
- Function calls
- Member access (`object.field`)
- Array indexing (`array[index]`)
- Format strings (`f"Hello {name}!"`)
- Tuples (`(value1, value2, value3)`)

### 10. Error Handling (`error.rs`)

Provides comprehensive error detection and reporting:

**Error Types:**
```rust
pub enum ParseError {
    GenericSyntaxError { line: usize, context: String, hint: String },
    UnexpectedToken { line: usize, expected: String, found: String },
    MissingColon { line: usize },
    InvalidIndentation { line: usize, expected: usize, found: usize },
    UnterminatedBlock { line: usize },
    // ... more error types
}
```

**Smart Error Detection:**
The parser analyzes syntax errors to provide specific, actionable error messages with hints for correction.

### 11. Comment Parsing (`comment.rs`)

Supports both comment styles:
- Single-line: `/#/ This is a comment`
- Multi-line: `/#* This is a multi-line comment *#/`

### 12. Raise Statements (`raise.rs`)

Parses exception raising:
```coffee
raise ErrorType(arguments)
```

## Parsing Algorithm

The parser uses a hybrid approach:

1. **Line-based parsing**: Processes source code line by line
2. **Indentation tracking**: Uses `BlockTracker` to manage block structure
3. **Keyword dispatch**: Quickly identifies statement types by first word
4. **Error recovery**: Continues parsing after errors to collect all issues

### Multiline Content Collection

The `collect_multiline_content` function implements sophisticated block tracking:

1. Establish base indentation level on first line
2. Track nested blocks using a stack
3. Handle special cases (elif/else continuation)
4. Detect block end conditions:
   - Dedentation past base level
   - Different top-level keyword at same level
   - Same keyword at base level with no nesting

### Statement Type Detection

The `StatementType` enum provides quick categorization:

```rust
pub enum StatementType {
    Function,
    Class,
    Enum,
    Import,
    VariableDecl,
    If,
    While,
    For,
    Match,
    Return,
    Break,
    Continue,
    Comment,
    Main,
    MemoryOp,
    Unknown,
}
```

## Design Decisions

### 1. Nom Parser Combinators

Using `nom` provides:
- Composable parsing primitives
- Good error messages
- Efficient parsing
- Easy testing of individual parsers

### 2. Indentation-Based Blocks

Coffee's Python-like indentation requires:
- Explicit indentation tracking
- Block validation
- Special handling for control flow continuations

### 3. Error Recovery Strategy

The parser:
- Continues after errors
- Collects all errors
- Provides context and hints
- Doesn't fail fast on first error

### 4. Hybrid Parsing Approach

Combining:
- Single-line parsing for simple statements
- Multiline parsing for complex structures
- Block tracking for proper nesting

## Usage Example

```rust
use coffee::parser;

let source = r#"
fn add(x: int, y: int) => int:
    return x + y

main(add(1, 2))
"#;

match parser::parse_program(source) {
    Ok(program) => {
        println!("Parsed {} statements", program.statements.len());
        for stmt in program.statements {
            println!("{:?}", stmt);
        }
    }
    Err(errors) => {
        for error in errors {
            eprintln!("Parse error: {:?}", error);
        }
    }
}
```

## Testing

The parser module should be tested for:
- Correct parsing of all language constructs
- Error detection and reporting
- Edge cases (empty input, malformed code)
- Nested structures
- Comments and whitespace handling

## Future Enhancements

Potential improvements:
- Better error recovery
- More precise error locations
- Syntax highlighting support
- IDE integration (language server)
- Macro system support
- Better error messages with code snippets