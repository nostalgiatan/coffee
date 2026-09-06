//! Function compilation for Coffee compiler code generation
//! 
//! This module handles function declaration, compilation, and main() generation.
//! It provides utilities for declaring functions with proper signatures, handling
//! both Coffee and C ABI compatibility, and generating appropriate return instructions.
//! The module includes support for external function declarations and runtime function
//! declarations required by the compiler. It also manages function signatures for
//! imported functions and C library functions.

mod declare;
mod returns;
mod compile;
mod entry;

pub use declare::{
    c_abi_fn_type, coffee_llvm_fn_name, declare_external_function, declare_function,
    declare_runtime_functions,
};

/// Names declared for codegen internals; omit from "did you mean" lists.
pub fn omit_from_unknown_fn_help(name: &str) -> bool {
    name.starts_with("coffee_")
        || matches!(name, "printf" | "puts" | "malloc" | "free" | "exit" | "strlen")
}
pub(crate) use declare::const_exit_status_one;
pub use returns::build_implicit_return;
