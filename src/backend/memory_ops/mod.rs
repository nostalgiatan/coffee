//! Memory operations for Coffee compiler code generation
//! 
//! This module handles memory operations for the Coffee compiler, including
//! clone, move, and remove operations. It provides comprehensive
//! lifetime tracking and memory safety checks to prevent common memory
//! management errors such as use-after-free, double-free, and memory leaks.
//! The module implements Rust-like ownership semantics for the Coffee language,
//! tracking variable lifetimes and ensuring proper resource management.

mod clone;
mod compile;
mod context;
mod drop;
mod lifetime;

#[allow(unused_imports)] // public API: crate::backend::memory_ops::{...}
pub use compile::{
    compile_clone, compile_memory_op, compile_move, compile_remove,
    compile_remove_multiple,
};
pub use clone::clone_coffee_place;
pub use drop::drop_coffee_place;
pub use context::MemoryContext;
#[allow(unused_imports)] // public API: LifetimeDiagnostic, LifetimeInfo
pub use lifetime::{DiagSeverity, LifetimeDiagnostic, LifetimeInfo, VariableState};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_context_creation() {
        let ctx = MemoryContext::new();
        assert!(!ctx.is_moved("test_var"));
    }

    #[test]
    fn test_memory_context_track_moved() {
        let mut ctx = MemoryContext::new();
        ctx.mark_moved("my_var".to_string());
        assert!(ctx.is_moved("my_var"));
        assert!(!ctx.is_moved("other_var"));
    }

    #[test]
    fn test_memory_context_unmark() {
        let mut ctx = MemoryContext::new();
        ctx.mark_moved("my_var".to_string());
        ctx.unmark_moved("my_var");
        assert!(!ctx.is_moved("my_var"));
    }

    #[test]
    fn test_memory_context_default() {
        let ctx = MemoryContext::default();
        assert!(!ctx.is_moved("anything"));
    }

    #[test]
    fn use_then_move_is_not_m007() {
        let mut ctx = MemoryContext::new();
        ctx.register_birth("b".into());
        ctx.mark_initialized("b");
        ctx.record_use("b");
        ctx.mark_moved("b".into());
        let diags = ctx.run_lifetime_checks().expect("checks");
        assert!(
            !diags.iter().any(|d| d.code == "M007"),
            "mv after a prior use is not use-after-move: {:?}",
            diags
        );
    }

    #[test]
    fn use_after_move_is_m007() {
        let mut ctx = MemoryContext::new();
        ctx.register_birth("b".into());
        ctx.mark_initialized("b");
        ctx.mark_moved("b".into());
        ctx.current_line = 2;
        ctx.record_use("b");
        let diags = ctx.run_lifetime_checks().expect("checks");
        assert!(
            diags.iter().any(|d| d.code == "M007"),
            "expected M007, got {:?}",
            diags
        );
    }
}
