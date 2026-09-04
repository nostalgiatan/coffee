// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License
//
// Coffee Compiler Frontend
//
// This module provides the complete compiler frontend implementation including:
// - Source code parsing with error recovery
// - Semantic analysis using space-based semantic model
// - Import resolution and module loading
// - Complete error reporting and diagnostics

pub mod unit;
pub mod graph;
pub mod project;
pub mod entry_point;
pub mod scheduler;
pub mod scanner;
pub mod linker;
pub mod builder;
pub mod import_resolver;
pub mod module_loader;
pub mod session;
pub mod pipeline;

pub use unit::{CompilationUnit, CompilationStatus};
pub use project::ProjectConfig;
pub use builder::ProjectBuilder;
pub use pipeline::CompilationPipeline;

/// Output artifact requested by the driver (`coffee.toml` and single-file share this).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmitKind {
    Object,
    LlvmIr,
    Bitcode,
    Assembly,
    #[default]
    Binary,
}

use crate::parser;
use crate::semantic::AnalysisReport;
use crate::diagnostics::{Diagnostic, DiagnosticEmitter};
use crate::c;
use std::path::{Path, PathBuf};

//=============================================================================
// Compilation Configuration
//=============================================================================

/// Compiler configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompilerConfig {
    /// Enable import resolution
    pub enable_imports: bool,
    /// Import search paths
    pub import_paths: Vec<PathBuf>,
    /// Maximum recursion depth for imports
    pub max_import_depth: usize,
    /// Cache parsed modules
    pub cache_modules: bool,
    /// Target triple for cross-compilation
    pub target_triple: Option<String>,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        let mut import_paths = Vec::new();

        // Add current working directory first
        if let Ok(cwd) = std::env::current_dir() {
            import_paths.push(cwd);
        }

        // Add common search paths
        import_paths.push(PathBuf::from("examples"));
        import_paths.push(PathBuf::from("src"));
        import_paths.push(PathBuf::from("lib"));
        import_paths.push(PathBuf::from("std"));
        import_paths.push(PathBuf::from("."));

        CompilerConfig {
            enable_imports: true,
            import_paths,
            max_import_depth: 100,
            cache_modules: true,
            target_triple: None,
        }
    }
}

//=============================================================================
// Compilation Result
//=============================================================================

/// A parsed module
#[derive(Debug, Clone)]
pub struct ParsedModule {
    /// Module statements
    statements: Vec<parser::Statement>,
    /// Exported symbols
    pub exports: Vec<String>,
    /// File path
    #[allow(dead_code)]
    pub file_path: PathBuf,
}

impl ParsedModule {
    /// Get the module name from file path
    #[allow(dead_code)]
    pub fn module_name(&self) -> String {
        self.file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Check if a symbol is exported from this module
    pub fn has_export(&self, symbol: &str) -> bool {
        self.exports.iter().any(|e| e == symbol)
    }

    /// Get all exported symbols
    pub fn exported_symbols(&self) -> &[String] {
        &self.exports
    }
}

/// Result of compilation
#[derive(Debug, Clone)]
pub struct CompilationResult {
    /// Success flag
    pub success: bool,
    /// Parsed program AST (including imported modules)
    pub program: parser::Program,
    /// Errors
    pub errors: Vec<Diagnostic>,
    /// Warnings
    pub warnings: Vec<Diagnostic>,
    /// Hints
    pub hints: Vec<Diagnostic>,
    /// Analysis report
    pub report: AnalysisReport,
    /// Compilation statistics
    pub stats: CompilationStatistics,
    /// Standard library exports (for backend)
    pub std_exports: Vec<String>,
    /// Imported module paths (for tracking dependencies)
    pub imported_modules: Vec<String>,
    /// C library imports (for linking)
    pub c_imports: Vec<String>,
    /// C function symbol tables from .cfc files
    pub cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>,
}

/// Compilation statistics
#[derive(Debug, Clone, Default)]
pub struct CompilationStatistics {
    /// Number of lines parsed
    pub lines_parsed: usize,
    /// Number of statements parsed
    pub statements_parsed: usize,
    /// Number of imports processed
    pub imports_processed: usize,
    /// Number of modules loaded
    pub modules_loaded: usize,
    /// Number of functions declared
    pub functions_declared: usize,
    /// Number of classes declared
    pub classes_declared: usize,
    /// Number of enums declared
    pub enums_declared: usize,
    /// Number of variables declared
    pub variables_declared: usize,
    /// Number of assignments made
    pub assignments_made: usize,
    /// Number of return statements
    pub return_statements: usize,
    /// Number of if expressions
    pub if_expressions: usize,
    /// Number of while loops
    pub while_loops: usize,
    /// Number of for loops
    pub for_loops: usize,
    /// Number of match expressions
    pub match_expressions: usize,
    /// Number of break statements
    pub break_statements: usize,
    /// Number of continue statements
    pub continue_statements: usize,
    /// Number of memory operations
    pub memory_operations: usize,
    /// Number of raise statements
    pub raise_statements: usize,
    /// Number of comments
    pub comments_count: usize,
    /// Number of expression statements (standalone function calls, etc.)
    pub expression_statements: usize,
}

//=============================================================================
// Compiler Frontend
//=============================================================================

/// Thin driver: holds a `Session` and forwards compile to `CompilationPipeline`.
pub struct CompilerFrontend {
    session: std::sync::Arc<session::Session>,
    pipeline: CompilationPipeline,
}

impl CompilerFrontend {
    pub fn new() -> Self {
        let session = std::sync::Arc::new(session::Session::new());
        let pipeline = CompilationPipeline::new(session.clone());
        CompilerFrontend { session, pipeline }
    }

    #[allow(dead_code)]
    pub fn with_config(config: CompilerConfig) -> Self {
        let session = std::sync::Arc::new(session::Session::with_config(config));
        let pipeline = CompilationPipeline::new(session.clone());
        CompilerFrontend { session, pipeline }
    }

    pub fn compile(&mut self, source: &str, file_name: Option<&str>) -> CompilationResult {
        self.pipeline.compile(source, file_name)
    }

    pub fn parse_source(&self, source: &str, file_name: Option<&str>) -> Result<Vec<parser::Statement>, Vec<Diagnostic>> {
        self.pipeline.parse_source(source, file_name)
    }

    pub fn compile_module_to_object(&mut self, unit: &mut CompilationUnit, is_entry: bool) -> Result<(), String> {
        self.pipeline.compile_module_to_object(unit, is_entry)
    }

    pub fn emit_module(
        &mut self,
        unit: &mut CompilationUnit,
        is_entry: bool,
        emit: EmitKind,
        output_file: Option<&Path>,
    ) -> Result<(), String> {
        self.pipeline.emit_module(unit, is_entry, emit, output_file)
    }

    pub fn count_entry_points(program: &parser::Program) -> usize {
        CompilationPipeline::count_entry_points(program)
    }

    pub fn count_main_functions(program: &parser::Program) -> usize {
        CompilationPipeline::count_entry_points(program)
    }

    pub fn format_result(&self, result: &CompilationResult) -> String {
        self.pipeline.format_result(result)
    }

    pub fn get_c_imports(&self) -> Vec<String> {
        self.session.get_c_imports()
    }

    pub fn get_cfc_symbols(&self) -> std::collections::HashMap<String, c::CSymbolTable> {
        self.session.get_cfc_symbols()
    }

    #[allow(dead_code)]
    pub fn emitter(&self) -> &DiagnosticEmitter {
        &self.session.emitter
    }

    #[allow(dead_code)]
    pub(crate) fn create_std_module(&self) -> ParsedModule {
        self.pipeline.create_std_module()
    }
}

impl Default for CompilerFrontend {
    fn default() -> Self {
        Self::new()
    }
}

/// Import specification for selective imports
#[derive(Debug, Clone)]
pub(crate) struct ImportSpec {
    pub module: String,
    pub symbols: Vec<String>,
    pub alias: Option<String>,
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_creation() {
        let compiler = CompilerFrontend::new();
        assert!(compiler.emitter().diagnostics().is_empty());
    }

    #[test]
    fn test_config_default() {
        let config = CompilerConfig::default();
        assert!(config.enable_imports);
        assert!(!config.import_paths.is_empty());
    }

    #[test]
    fn test_simple_compilation() {
        let mut compiler = CompilerFrontend::new();
        let source = r#"
fn main() => int:
    return 0
"#;

        let result = compiler.compile(source, None);
        assert!(result.stats.statements_parsed > 0);
    }

    #[test]
    fn test_std_library_exports() {
        let compiler = CompilerFrontend::new();
        let std_module = compiler.create_std_module();
        assert!(std_module.exports.contains(&"print".to_string()));
        assert!(std_module.exports.contains(&"println".to_string()));
        assert!(std_module.exports.contains(&"sqrt".to_string()));
    }
}
