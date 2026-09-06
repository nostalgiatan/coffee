//! Parsed-module type used by codegen import maps.
//!
//! Live import loading lives in `CompilationPipeline` (`pipeline/imports.rs`).
//! The old `ImportResolver` / `ImportConfig` driver split had no remaining
//! call sites and was removed.

use crate::parser;

/// A parsed module (AST statements plus export list).
///
/// Codegen still takes `HashMap<String, import_resolver::ParsedModule>` for
/// imported Coffee modules. Construction of live modules uses
/// `compiler::ParsedModule` in the pipeline; this type remains for the
/// backend signature.
#[derive(Debug, Clone)]
pub struct ParsedModule {
    /// Parsed AST statements
    pub statements: Vec<parser::Statement>,
    /// Exported symbol names
    #[allow(dead_code)] // Public field of the codegen import-map type
    pub exports: Vec<String>,
    /// Source file path (set when a module is loaded)
    #[allow(dead_code)] // Public field of the codegen import-map type
    pub file_path: std::path::PathBuf,
}
