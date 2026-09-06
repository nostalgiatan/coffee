//! Types Module
//!
//! This module provides the unified type system for the Coffee language,
//! consolidating functionality from the original ty and space modules.

pub mod definition;
pub mod registry;
pub mod newtype;
pub mod checker;
pub mod borrow;
pub mod last_use;
pub mod errors;
pub mod mono;

// Re-export only the types that are actually used
pub use definition::{
    Type, SpaceKind, type_from_str,
};

pub use registry::TypeRegistry;
pub use checker::{TypeChecker, CheckingMode};

pub use errors::{TypeSystemError};