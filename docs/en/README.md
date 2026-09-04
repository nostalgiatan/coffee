# Coffee Compiler Documentation

## Overview

Coffee is a modern programming language compiler written in Rust, using LLVM as its backend. It is an experimental statically-typed language with Python-like indentation syntax, supporting both functional and object-oriented programming paradigms, along with perfect bidirectional interoperability with C.

- **Project Name**: Coffee
- **Version**: 0.2.1
- **Development Environment**: Rust 2024 edition (Termux on Android)
- **Compiler Backend**: LLVM (via inkwell crate)
- **Parser**: Custom parser using nom library

## Documentation Structure

This documentation provides comprehensive information about the Coffee compiler architecture, modules, and implementation details.

### Core Modules

1. **[Parser Module](parser.md)** - Source code parsing and AST generation
2. **[Semantic Analysis Module](semantic.md)** - Scope management and symbol resolution
3. **[Type System Module](types.md)** - Type checking and inference
4. **[Backend Module](backend.md)** - LLVM code generation
5. **[C Integration Module](c_integration.md)** - C language FFI support
6. **[Compiler Frontend Module](compiler.md)** - Project compilation orchestration
7. **[Runtime Module](runtime.md)** - Standard runtime library

## Key Features

### Language Features
- Static type system
- Python-like indentation syntax
- Functional programming support
- Object-oriented programming (classes and inheritance)
- Pattern matching
- Advanced memory management (mv/clone/copy/rm/alloc/free/load/store/clean operations)
- Perfect bidirectional C interoperability
- Project management (coffee.toml)
- Exception handling (raise statements)
- Rich expression support
- Array indexing

### Compilation Features
- LLVM IR generation
- Multiple output formats (object file, assembly, bitcode, executable)
- JIT execution
- Cross-platform compilation support
- Optimization levels (O0-O3)
- Static/dynamic linking
- Cross-compilation support

### C Language Interoperability (Core Feature)
- **Bidirectional Interoperability**: Coffee functions can be called from C code, and C functions can be called from Coffee
- **.cfc Files**: Coffee's C function description files for describing C function signatures
- **c fn Syntax**: Generate .cfc files from C header files, even without .h files
- **No Header File Dependency**: Link C libraries via .cfc files without original header files
- **Automatic Type Mapping**: Automatic conversion between Coffee and C types
- **C Standard Library Support**: Built-in libc common function signatures
- **C Header Generation**: Generate C header files from Coffee functions
- **Clang Integration**: Use clang to parse C header files and generate .cfc files

## Quick Start

### Installation

```bash
# Build the compiler
cargo build --release

# Add to PATH (optional)
export PATH=$PATH:/path/to/coffee/target/release
```

### Basic Usage

```bash
# Compile a single file
coffee input.cf

# Generate LLVM IR
coffee --emit-llvm input.cf

# Generate assembly code
coffee --emit-asm input.cf

# Generate bitcode file
coffee --emit-bc input.cf

# Optimize compilation
coffee -O2 input.cf

# Generate executable
coffee --bin input.cf

# JIT execution
coffee --jit input.cf

# Initialize new project
coffee init

# Generate .cfc file from C header
coffee --gen-cfc input.h

# Static linking
coffee --static input.cf

# Specify target triple for cross-compilation
coffee --target x86_64-unknown-linux-gnu input.cf
```

### Project Structure

A typical Coffee project structure:

```
project_name/
├── coffee.toml          # Project configuration file
├── src/                 # Source code directory
│   ├── main.cf          # Main entry file
│   ├── lib/             # Library modules
│   │   ├── module1.cf
│   │   └── module2.cf
│   └── utils/           # Utility modules
│       └── helper.cf
├── lib/                 # C function description files (.cfc)
│   ├── libc.cfc
│   └── libm.cfc
├── target/              # Output directory
│   ├── debug/           # Debug version
│   ├── release/         # Release version
│   └── include/         # Generated C header files
└── tests/               # Test directory
    ├── test_module1.cf
    └── test_module2.cf
```

## Project Configuration (coffee.toml)

```toml
[package]
name = "project_name"
version = "0.1.0"
authors = ["Author Name <email@example.com>"]
description = "Project description"

[dependencies]
# Standard library dependency
std = "0.2.0"

# Coffee package dependencies
[dependencies.packages]
# coffee_package = "0.1.0"

# C library dependencies
[dependencies.c_libraries.libm]
name = "m"
headers = ["math.h"]
include_paths = ["/usr/include"]
link_flags = ["-L/usr/lib"]
static_link = false
static_lib_path = ""

[build]
main = "src/main"
target_dir = "target"
src_dir = "src"
have_c = true

[target]
opt_level = 2
triple = "x86_64-unknown-linux-gnu"
```

## Architecture Overview

The Coffee compiler follows a traditional multi-phase compilation architecture:

1. **Parsing Phase**: Source code → Abstract Syntax Tree (AST)
2. **Semantic Analysis Phase**: AST → Enhanced AST with symbol information
3. **Type Checking Phase**: Enhanced AST → Type-validated AST
4. **Code Generation Phase**: Type-validated AST → LLVM IR
5. **Optimization Phase**: LLVM IR → Optimized LLVM IR
6. **Code Emission Phase**: Optimized LLVM IR → Object files/Assembly/Executable

## Development Guidelines

### Coding Standards
- Follow Rust coding conventions
- Adhere to LLVM code generation best practices
- Use meaningful variable and function names
- Add appropriate comments and documentation

### Testing Practices
- Use Rust's testing framework for unit tests
- Use integration tests to validate compiler functionality
- Use performance tests to evaluate compiler optimization effects

### Contribution Guidelines
- Follow Rust best practices
- Maintain code style consistency
- Add appropriate test cases
- Update related documentation

## Dependencies

### Main Dependencies
- `inkwell`: LLVM bindings (v0.7.1)
- `nom`: Parser combinators (v8.0.0)
- `toml`: TOML configuration parsing
- `serde`: Serialization/deserialization
- `rayon`: Concurrent processing
- `fs2`: Cross-platform file locking
- `libc`: C library interface
- `clang-sys`: Clang system interface

## Project Status

Coffee compiler is an experimental modern compiler project focused on exploring novel programming language design and implementation techniques. The project supports complete compiler frontend, type checking, LLVM code generation and optimization, and implements seamless integration with C language. Its most notable feature is the ability to link C libraries through .cfc files without requiring original header files, making it particularly suitable for learning compiler design and implementation.

## License

Please refer to the project's LICENSE file for licensing information.

## Contact

For questions, suggestions, or contributions, please refer to the project repository.