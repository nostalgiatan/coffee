# Runtime Module

## Overview

The Runtime module (`src/runtime/`) provides the standard runtime library support for the Coffee language. It contains embedded runtime functions and utilities that are automatically linked when user code uses runtime features such as exception handling (`raise` statements).

## Module Structure

```
src/runtime/
├── mod.rs          # Main runtime module
└── embedded.rs     # Embedded runtime library
```

## Core Components

### 1. Embedded Runtime (`embedded.rs`)

The embedded runtime provides essential runtime functions written in Coffee itself:

```rust
pub const RUNTIME_SOURCE: &str = r#"
/# Coffee Standard Runtime Library #/

use puts, exit in libc of c

c fn coffee_panic(msg: string, len: int) => ():
    puts("PANIC: ")
    exit(1)
"#;
```

### 2. Runtime Symbols

The runtime module also declares symbols that should be automatically available:

```rust
pub const RUNTIME_SYMBOLS: &[(&str, &str)] = &[
    ("coffee_panic", "c fn coffee_panic(msg: string, len: int) => ()"),
];
```

## Runtime Functions

### coffee_panic

The panic function is called when the program encounters an unrecoverable error:

```coffee
c fn coffee_panic(msg: string, len: int) => ():
    puts("PANIC: ")
    exit(1)
```

**Parameters:**
- `msg`: Error message string
- `len`: Length of the message

**Behavior:**
- Prints "PANIC: " to stdout
- Exits the program with status code 1

**Usage:**
The panic function is automatically called by the compiler when:
- Unhandled exceptions occur
- Critical runtime errors are detected
- Assertion failures happen

## Integration with Compiler

### Automatic Linking

The runtime is automatically linked when needed:

```rust
// In compiler.rs
if uses_runtime_features {
    link_runtime();
}
```

### Runtime Detection

The compiler detects runtime feature usage:

```rust
// Check if raise statement is used
let uses_raise = program.statements.iter().any(|stmt| {
    matches!(stmt, Statement::Raise(_))
});

if uses_raise {
    // Link runtime
}
```

### Symbol Injection

Runtime symbols are automatically injected into the symbol table:

```rust
for (name, signature) in runtime::RUNTIME_SYMBOLS {
    symbol_table.declare(name, signature);
}
```

## Runtime Features

### Exception Handling

The runtime supports **raising** exceptions with `raise` only. `try` / `catch` / `except` are not implemented; there is no catch handler in the language.

```coffee
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("division by zero")
    return a / b
```

When an exception is raised:
1. The runtime captures the exception
2. The stack is unwound
3. `coffee_panic` may be called (there is no `catch` to recover)

### Panic Handling

The runtime handles panics gracefully:

```coffee
fn assert(condition: bool, msg: string) => ():
    if not condition:
        raise AssertionError(msg)
```

### Error Reporting

The runtime provides error reporting capabilities:

```coffee
c fn coffee_panic(msg: string, len: int) => ():
    puts("PANIC: ")
    # Additional error handling
    exit(1)
```

## Usage Examples

### Basic Exception Handling

```coffee
fn safe_divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("cannot divide by zero")
    return a / b

main(safe_divide(10, 2))  # Returns 5
```

### Custom Errors

```coffee
enum MyError:
    InvalidInput(string)
    OutOfRange(int, int)

fn validate(value: int, min: int, max: int) => ():
    if value < min or value > max:
        raise OutOfRange(value, max)
```

### Panic on Error

```coffee
fn require(condition: bool, msg: string) => ():
    if not condition:
        raise AssertionError(msg)
```

## Runtime Architecture

### Minimal Design

The Coffee runtime follows a minimal design philosophy:

1. **Embedded**: Runtime is embedded in the compiler
2. **Automatic**: No manual linking required
3. **Lightweight**: Minimal overhead
4. **Self-contained**: All runtime code in Coffee

### No Garbage Collector

Coffee does not use a garbage collector:

- Memory is managed through explicit operations (`mv`, `clone`, `copy`, `rm`)
- Ownership system prevents memory leaks
- Deterministic memory cleanup

### No Standard Library

Coffee does not have a traditional standard library:

- Core functions are built into the language
- C library functions are imported via FFI
- User libraries can be created and shared

## Runtime vs. Standard Library

### Traditional Languages

Most languages have a large standard library:

```
Python:
  - os, sys, json, datetime, collections, itertools, ...

Java:
  - java.lang, java.util, java.io, java.net, ...

C++:
  - STL: vector, map, string, algorithm, ...
```

### Coffee Approach

Coffee takes a different approach:

```
Coffee:
  - Minimal runtime (panic, basic error handling)
  - C library integration for everything else
  - User-defined libraries
```

**Benefits:**
- Smaller binary size
- Faster compilation
- More flexibility
- Better C interoperability

## Future Enhancements

### Planned Runtime Features

1. **Enhanced Error Handling**
   - Stack traces
   - Error contexts
   - Custom error types

2. **Runtime Reflection**
   - Type information
   - Dynamic dispatch
   - Metadata access

3. **Concurrency Support**
   - Thread spawning
   - Synchronization primitives
   - Async/await

4. **Standard Library**
   - Collection types
   - String utilities
   - File I/O helpers

5. **Memory Management**
   - Reference counting
   - Smart pointers
   - Memory pools

### Experimental Features

1. **JIT Compilation**
   - Runtime code generation
   - Dynamic optimization
   - Hot code reloading

2. **Foreign Function Interface**
   - Dynamic library loading
   - Callback registration
   - Type marshaling

3. **Debugging Support**
   - Breakpoints
   - Variable inspection
   - Step execution

## Performance Considerations

### Zero-Cost Abstractions

The runtime is designed with zero-cost abstractions:

- No runtime overhead for unused features
- Compile-time optimizations
- Minimal runtime checks

### Inline Functions

Runtime functions are inlined when possible:

```rust
// Compiler may inline panic checks
if unlikely(error) {
    coffee_panic(msg, len);
}
```

### Static Linking

Runtime is statically linked:

- No runtime dependencies
- Portable executables
- Fast startup

## Security Considerations

### Panic Safety

The runtime ensures panic safety:

- Clean stack unwinding
- Resource cleanup
- No memory leaks

### Error Isolation

Errors are properly isolated:

- No undefined behavior
- No data corruption
- Safe error propagation

### Input Validation

Runtime validates inputs:

- String length checks
- Array bounds checking
- Null pointer prevention

## Testing

### Runtime Tests

Runtime functions are tested extensively:

```rust
#[test]
fn test_panic() {
    // Test panic behavior
}

#[test]
fn test_exception_handling() {
    // Test exception propagation
}
```

### Integration Tests

Runtime is tested in integration:

```rust
#[test]
fn test_runtime_integration() {
    // Test runtime with user code
}
```

## Best Practices

### 1. Use Exceptions Judiciously

```coffee
# Good: Use exceptions for exceptional cases
fn open_file(path: string) => File:
    if not file_exists(path):
        raise FileNotFoundError(path)
    return File(path)

# Bad: Use exceptions for control flow
fn get_first(arr: [int]) => int:
    if arr.len() == 0:
        raise EmptyArrayError
    return arr[0]  # Better to use Option type
```

### 2. Provide Clear Error Messages

```coffee
# Good
raise ValueError(f"expected positive number, got {value}")

# Bad
raise ValueError
```

### 3. Handle Errors Appropriately

Coffee has `raise` only. `try` / `catch` / `except` are not implemented, so you cannot catch a raised exception in Coffee. Check conditions before calling code that would `raise`, or return an error value instead of raising when recovery is needed.

```coffee
# Recover without catch: validate, then proceed
fn process(input: string) => Result:
    if not is_valid(input):
        return Error("invalid input")
    return parse(input)
```

### 4. Clean Up Resources

```coffee
# Good: Clean up before panic
fn critical_operation() => Result:
    let resource = acquire_resource()
    if error_occurs():
        release_resource(resource)
        raise CriticalError
    return Success(resource)

# Bad: Leak resources on panic
fn critical_operation() => Result:
    let resource = acquire_resource()
    if error_occurs():
        raise CriticalError  # Resource leaked!
    return Success(resource)
```

## Comparison with Other Languages

### Python

Python can catch exceptions with `try` / `except`. Coffee cannot: only `raise` exists; `try` / `catch` / `except` are not implemented.

```python
# Python — catch is language syntax
try:
    result = 10 / 0
except ZeroDivisionError as e:
    print(f"Error: {e}")
```

```coffee
# Coffee — raise only; no try/catch
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("division by zero")
    return a / b
```

### Rust

```rust
// Rust
fn divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        Err("division by zero".to_string())
    } else {
        Ok(a / b)
    }
}
```

```coffee
# Coffee
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("division by zero")
    return a / b
```

### C++

```cpp
// C++
int divide(int a, int b) {
    if (b == 0) {
        throw std::runtime_error("division by zero");
    }
    return a / b;
}
```

```coffee
# Coffee
fn divide(a: int, b: int) => int:
    if b == 0:
        raise DivisionByZero("division by zero")
    return a / b
```

## See Also

- [C Integration Module](c_integration.md) - C language FFI
- [Parser Module](parser.md) - Parsing raise statements
- [Backend Module](backend.md) - Code generation for runtime
- [Compiler Module](compiler.md) - Runtime linking