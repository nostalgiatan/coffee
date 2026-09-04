//! Types Module
//!
//! This module provides the unified type system for the Coffee language,
//! consolidating functionality from the original ty and space modules.

#![allow(dead_code)]

pub mod definition;
pub mod registry;
pub mod checker;
pub mod inference;
pub mod errors;

// Re-export only the types that are actually used
pub use definition::{
    Type, SpaceKind, type_from_str,
    // Commenting out unused exports:
    // TypeDef, Entity, Binding, Span, SpaceId,
    // Visibility, Path, QueryResult, Region, Space, ClassField,
    // EnumVariant, VariantField, MethodSignature, ReceiverKind,
    // TypeConstraint,
};

pub use registry::TypeRegistry;
pub use checker::{TypeChecker, CheckingMode};
// Commenting out unused checker exports:


// Commenting out unused inference exports:
// pub use inference::{TypeInferencer, InferenceContext};

pub use errors::{TypeSystemError};
// Commenting out unused error exports:
// pub use errors::{Diagnostic, DiagnosticLevel};