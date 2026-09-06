//! Semantic Analysis Module
//!
//! This module provides advanced semantic analysis capabilities including:
//! - Scope management and variable visibility
//! - LifetimeSpace (future `'a`; intra-procedural borrows are `types::borrow`)
//! - Symbol declaration and resolution
//! - Comprehensive semantic analysis coordination

#![allow(dead_code)]

pub mod scope;
pub mod lifetime;
pub mod symbols;
pub mod analyzer;

// Re-export main types for convenience

pub use analyzer::{SemanticAnalyzer, AnalysisReport};