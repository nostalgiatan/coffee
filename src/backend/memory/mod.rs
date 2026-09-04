//! Memory Management Module - Modular Architecture
//! 
//! This module provides a comprehensive memory layout and safety system for the Coffee compiler.
//! It is organized into focused submodules that handle different aspects of memory management:
//! 
//! - `types`: Fundamental memory layout types (Size, Align, Layout)
//! - `structs`: Struct layout optimization algorithms
//! - `collector`: Layout collection and reporting tools
//! - `bitfields`: Bit field layout and storage optimization
//! - `safety`: Runtime memory safety checks
//! 
//! The memory management system is designed to optimize memory usage while ensuring
//! safety and correctness. It includes sophisticated algorithms for struct field
//! reordering to minimize padding, bit field packing for efficient flag storage,
//! and runtime checks to prevent common memory errors.

pub mod types;      // Basic memory types (Layout, Size, Align)
pub mod structs;    // Struct layout optimization
pub mod collector;  // Layout collection and reporting
pub mod bitfields;  // Bit field support
pub mod safety;     // Memory safety checks

// Re-exports for convenience - only export what's actually used
pub use types::{Layout};
pub use structs::{StructLayout};
pub use collector::{LayoutCollector};
