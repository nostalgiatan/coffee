# C Integration Module

## Overview

The C Integration module (`src/c/`) provides comprehensive Foreign Function Interface (FFI) support between Coffee and C libraries. It enables bidirectional interoperability, allowing Coffee functions to call C functions and C code to call Coffee functions.

## Module Structure

```
src/c/
├── mod.rs        # Main C integration module
├── parser.rs     # .cfc file parser
├── signature.rs  # C function signature management
├── generator.rs  # .cfc file generator from C headers
└── header_gen.rs # C header generator from Coffee functions
```

## Core Components

### 1. .cfc File Format

The `.cfc` (C Function Declaration) file is Coffee's unique approach to C integration. It declares C function signatures in Coffee syntax, allowing Coffee programs to link with C libraries without requiring the original C header files.

**Example .cfc File:**
```coffee
# libc.cfc - C standard library functions
c fn printf(format: string, args: object) => int:
c fn puts(s: string) => int:
c fn putchar(c: int) => int:
c fn exit(status: int) => ():
c fn malloc(size: int) => int:
c fn free(ptr: int) => ():
c fn strlen(s: string) => int:
c fn strcmp(s1: string, s2: string) => int:
```

**Advantages:**
- No need for original C headers
- Coffee syntax for better readability
- Automatic type mapping
- Easy to maintain and update

### 2. C Symbol Table (`signature.rs`)

Manages C function signatures parsed from .cfc files.

```rust
pub struct CSymbol {
    pub name: String,
    pub return_type: Type,
    pub params: Vec<(String, Type)>,
    pub is_variadic: bool,
}

pub struct CSymbolTable {
    pub functions: HashMap<String, CSymbol>,
}

impl CSymbolTable {
    pub fn from_cfc(content: &str) -> Result<Self, ParseError>;
    pub fn get_function(&self, name: &str) -> Option<&CSymbol>;
    pub fn add_function(&mut self, symbol: CSymbol);
}
```

### 3. .cfc Parser (`parser.rs`)

Parses .cfc files to extract C function signatures.

**Parsing Rules:**
- Function declarations start with `c fn`
- Parameters have format `name: type`
- Return type follows `=>`
- Variadic functions use `...` in parameters

**Example Parse:**
```coffee
c fn printf(format: string, ...) => int:
```

**Parsed Result:**
```rust
CSymbol {
    name: "printf",
    return_type: Type::Int { signed: true, bytes: 4 },
    params: vec![
        ("format".to_string(), Type::String),
    ],
    is_variadic: true,
}
```

### 4. .cfc Generator (`generator.rs`)

Generates .cfc files from C headers using clang.

**Generation Process:**
1. Parse C header using clang-sys
2. Extract function declarations
3. Map C types to Coffee types
4. Generate .cfc syntax

**Usage:**
```bash
coffee --gen-cfc /usr/include/math.h > libm.cfc
```

**Generated Output:**
```coffee
# libm.cfc
c fn sin(x: float(8)) => float(8):
c fn cos(x: float(8)) => float(8):
c fn sqrt(x: float(8)) => float(8):
c fn pow(x: float(8), y: float(8)) => float(8):
```

### 5. C Header Generator (`header_gen.rs`)

Generates C header files from Coffee functions, enabling C code to call Coffee functions.

**Generation Process:**
1. Parse Coffee source code
2. Extract function definitions
3. Map Coffee types to C types
4. Generate C header syntax

**Example Coffee Code:**
```coffee
fn coffee_add(x: int(4)+, y: int(4)+) => int(4)+:
    return x + y

fn coffee_greet(name: string) => string:
    return name
```

**Generated C Header:**
```c
// coffee_functions.h
#ifndef COFFEE_FUNCTIONS_H
#define COFFEE_FUNCTIONS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int coffee_add(int32_t x, int32_t y);
const char* coffee_greet(const char* name);

#ifdef __cplusplus
}
#endif

#endif // COFFEE_FUNCTIONS_H
```

## Type Mapping

### Coffee to C Types

| Coffee Type | C Type | Description |
|-------------|--------|-------------|
| `int(8)+` | `int64_t` | 64-bit signed integer |
| `int(4)+` | `int32_t` | 32-bit signed integer |
| `int(2)+` | `int16_t` | 16-bit signed integer |
| `int(1)+` | `int8_t` | 8-bit signed integer |
| `int(8)-` | `uint64_t` | 64-bit unsigned integer |
| `int(4)-` | `uint32_t` | 32-bit unsigned integer |
| `int(2)-` | `uint16_t` | 16-bit unsigned integer |
| `int(1)-` | `uint8_t` | 8-bit unsigned integer |
| `float(8)` | `double` | 64-bit floating point |
| `float(4)` | `float` | 32-bit floating point |
| `bool` | `_Bool` | Boolean type |
| `string` | `const char*` | C string |
| `void` | `void` | No return value |
| `(T1, T2)` | `struct Tuple_2` | Tuple |
| `[T]` | `struct Slice_T` | Slice |
| `&T` | `const T*` | Immutable reference |
| `&mut T` | `T*` | Mutable reference |

### Complex Types

**Tuples:**
```coffee
# Coffee
fn process(data: (int(4)+, string, float(8))) => (bool, int(4)+):
```

```c
// C
struct Tuple_3_i32_str_f64 {
    int32_t field0;
    const char* field1;
    double field2;
};

struct Tuple_2_bool_i32 {
    _Bool field0;
    int32_t field1;
};

struct Tuple_2_bool_i32 process(struct Tuple_3_i32_str_f64 data);
```

**Slices:**
```coffee
# Coffee
fn sum(arr: [int(4)+]) => int(4)+:
```

```c
// C
struct Slice_i32 {
    int32_t* data;
    size_t len;
};

int32_t sum(struct Slice_i32 arr);
```

## Bidirectional Interoperability

### Coffee Calling C

```coffee
# Import C functions
use printf, puts, exit in libc of c
use sin, cos, sqrt in libm of c

fn main() => ():
    printf("Hello from Coffee!\n")
    let angle: float(8) = 3.14159 / 2.0
    let result: float(8) = sin(angle)
    printf("sin(π/2) = %.2f\n", result)
    exit(0)
```

### C Calling Coffee

**Coffee Code:**
```coffee
c fn coffee_multiply(a: int(4)+, b: int(4)+) => int(4)+:
    return a * b
```

**Generated Header:**
```c
// coffee_multiply.h
int32_t coffee_multiply(int32_t a, int32_t b);
```

**C Code:**
```c
#include <stdio.h>
#include "coffee_multiply.h"

int main() {
    int32_t result = coffee_multiply(5, 7);
    printf("5 * 7 = %d\n", result);
    return 0;
}
```

## Import Syntax

### Simple Import
```coffee
use printf in libc of c
```

### Multiple Functions
```coffee
use printf, fprintf, puts in libc of c
```

### Import with Alias
```coffee
use print in libc of c as printf
```

## Library Configuration

### coffee.toml Configuration

```toml
[dependencies.c_libraries.libm]
name = "m"
headers = ["math.h"]
include_paths = ["/usr/include"]
link_flags = ["-L/usr/lib"]
static_link = false
static_lib_path = ""
```

**Configuration Options:**
- `name`: Library name (e.g., "m" for libm)
- `headers`: C header files for function discovery
- `include_paths`: Additional include directories
- `link_flags`: Additional linker flags
- `static_link`: Force static linking
- `static_lib_path`: Path to static library file

## Built-in C Functions

Coffee includes built-in signatures for common C library functions:

**I/O Functions:**
- `printf`, `fprintf`, `puts`, `putchar`, `fwrite`

**String Functions:**
- `strlen`, `strcmp`, `strcpy`

**Memory Functions:**
- `malloc`, `free`

**Process Control:**
- `exit`, `abort`

**Math Functions:**
- `abs`, `sin`, `cos`, `sqrt`, `pow`

## Error Handling

**C Integration Errors:**
```rust
pub enum CIntegrationError {
    ParseError { message: String },
    TypeMappingError { c_type: String },
    SymbolNotFound { name: String },
    HeaderGenerationError { message: String },
}
```

## Usage Example

### Using C Functions in Coffee

```coffee
# Import C functions
use printf, scanf, malloc, free in libc of c

fn read_and_print() => ():
    # Allocate memory
    let buffer: int = malloc(1024)
    
    # Read input
    printf("Enter your name: ")
    scanf("%s", buffer)
    
    # Print greeting
    printf("Hello, %s!\n", buffer)
    
    # Free memory
    free(buffer)

main(read_and_print())
```

### Generating .cfc Files

```bash
# Generate .cfc from C header
coffee --gen-cfc /usr/include/curl/curl.h > libcurl.cfc

# Use generated .cfc
use curl_easy_init, curl_easy_perform in libcurl of c
```

## Design Considerations

### 1. No Header Dependency
- .cfc files are self-contained
- No need for original C headers at link time
- Simplifies distribution and deployment

### 2. Type Safety
- Strong typing at Coffee level
- Automatic type checking
- Prevents common C errors

### 3. Performance
- Zero-cost abstractions
- Direct function calls
- Minimal runtime overhead

### 4. Portability
- Works across platforms
- Handles different ABIs
- Cross-compilation support

## Future Enhancements

- Automatic .cfc generation from shared libraries
- Callback function support
- Struct and enum mapping
- Macro support
- Better error messages
- C++ integration