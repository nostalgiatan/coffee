//! Control flow compilation for Coffee compiler
//!
//! Provides utilities for converting values to boolean conditions used by
//! MIR control-flow codegen.

mod cond;

pub use cond::value_to_bool;
