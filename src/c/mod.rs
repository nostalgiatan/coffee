//! C FFI (Foreign Function Interface) Support Module
//! 
//! This module provides comprehensive integration capabilities between Coffee and C libraries.
//! It implements a unique approach to C integration using .cfc (C Function Declaration) files,
//! which allows Coffee programs to call C functions without requiring the original C header files.
//! 
//! The module handles several key aspects of C integration:
//! 
//! - **Parsing .cfc files**: Reading C function declarations from .cfc files that describe C library interfaces
//! - **Generating .cfc files**: Creating .cfc files from C headers using clang-sys for libraries that don't have .cfc files
//! - **Managing C function signatures**: Storing and managing C function signatures for type checking and code generation
//! - **Code generator integration**: Ensuring C function calls are properly generated in the output code
//! - **C header generation**: Creating C header files from Coffee functions, allowing C code to call Coffee functions
//! 
//! The .cfc file format is a Coffee-specific format that declares C function signatures in Coffee syntax,
//! making it possible to use C libraries without having the original C header files available.

pub mod parser;
pub mod signature;
pub mod generator;
pub mod header_gen;

pub use signature::{CSymbol, CSymbolTable};
pub use parser::{parse_cfc_file, CFCParseError};
