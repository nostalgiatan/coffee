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
pub mod version;
pub mod graph;
pub mod project;
pub mod pkg;
pub mod entry_point;
pub mod scheduler;
pub mod scanner;
pub mod linker;
pub mod builder;
pub mod import_resolver;
pub mod session;
pub mod pipeline;

pub use unit::CompilationUnit;
pub use version::{compiler_display_version, compiler_fingerprint};
pub use project::ProjectConfig;
pub use builder::ProjectBuilder;
pub use pipeline::CompilationPipeline;

/// Public name used by the driver and project builder; same type as `CompilationPipeline`.
pub type CompilerFrontend = CompilationPipeline;

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
use crate::diagnostics::Diagnostic;
use crate::c;
use std::path::PathBuf;

//=============================================================================
// Compilation Configuration
//=============================================================================

/// Compiler configuration
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// Enable import resolution
    pub enable_imports: bool,
    /// Import search paths
    pub import_paths: Vec<PathBuf>,
    /// Cache parsed modules
    pub cache_modules: bool,
    /// Target triple for cross-compilation
    pub target_triple: Option<String>,
    /// `-O0`..`-O3` forwarded to LLVM TargetMachine
    pub opt_level: u8,
    /// Packed bitfield class layout (`--enable-bitfields`)
    pub enable_bitfields: bool,
    /// Runtime bounds/null checks (`--enable-safety`)
    pub enable_safety: bool,
    /// Print struct/class layout after codegen (`--show-memory`)
    pub show_memory: bool,
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
            cache_modules: true,
            target_triple: None,
            opt_level: 0,
            enable_bitfields: false,
            enable_safety: false,
            show_memory: false,
        }
    }
}

impl CompilerConfig {
    /// Prepend the official std package import root (`src/` when present).
    pub fn prepend_official_std(&mut self) -> Result<(), String> {
        self.import_paths.insert(0, pkg::std_import_root()?);
        Ok(())
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
    /// Byte ranges from `parse_program` of this module's source (same length as `statements`).
    /// `Span::new(0, 0)` only when the module has no parse spans (e.g. `Program::new` / std stub).
    stmt_spans: Vec<crate::types::definition::Span>,
    /// Exported symbols
    pub exports: Vec<String>,
}

impl ParsedModule {
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
    /// Imported module paths (for tracking dependencies)
    pub imported_modules: Vec<String>,
    /// C library imports (for linking)
    pub c_imports: Vec<String>,
    /// C function symbol tables from .cfc files
    pub cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>,
    /// Lowered function MIR. Codegen compiles Coffee bodies from MIR only;
    /// `lower_function` Err is a frontend Error (no AST body fallback).
    pub hir_fns: Vec<crate::hir::MirFn>,
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
        let compiler = CompilerFrontend::new();
        let source = r#"
fn main() => int:
    return 0
"#;

        let result = compiler.compile(source, None);
        assert!(result.stats.statements_parsed > 0);
        assert!(result.success);
        assert_eq!(result.hir_fns.len(), 1);
        assert_eq!(result.hir_fns[0].name, "main");
    }

    /// `p.x` then `rm p` is valid; ownership already ran in `check_function`.
    /// Post-bind lowering must revive Dropped locals so HIR infer of `p.x`
    /// succeeds and `main` is complete MIR (no MIR-lower warning).
    #[test]
    fn test_member_then_rm_emits_complete_main_hir_without_lower_warning() {
        let compiler = CompilerFrontend::new();
        let source = r#"
class Point:
    x: int

fn main() => int:
    let p: Point = Point { x: 1 }
    let n: int = p.x
    rm p
    return n
"#;
        let result = compiler.compile(source, None);
        assert!(
            result.success,
            "valid field-access then rm must still succeed the frontend: {:?}",
            result.errors.iter().map(|e| e.format()).collect::<Vec<_>>()
        );
        assert!(
            result.errors.is_empty(),
            "lowering must not be a hard error: {:?}",
            result.errors.iter().map(|e| e.format()).collect::<Vec<_>>()
        );
        let main = result
            .hir_fns
            .iter()
            .find(|f| f.name == "main")
            .expect("main must be in hir_fns");
        assert!(main.complete, "main MIR must be complete");
        assert!(
            !result.warnings.iter().any(|w| {
                let t = w.format();
                t.contains("main") && (t.contains("MIR") || t.contains("lower") || t.contains("AST"))
            }),
            "must not emit MIR-lower warning, got: {:?}",
            result.warnings.iter().map(|w| w.format()).collect::<Vec<_>>()
        );
    }

    fn assert_complete_main_hir(source: &str, file: &str) {
        let compiler = CompilerFrontend::new();
        let result = compiler.compile(source, Some(file));
        assert!(
            result.success,
            "frontend must succeed: {:?}",
            result.errors.iter().map(|e| e.format()).collect::<Vec<_>>()
        );
        let main = result
            .hir_fns
            .iter()
            .find(|f| f.name == "main")
            .expect("main must be in hir_fns so codegen compiles from MIR");
        assert!(main.complete, "main MIR must be complete");
        assert!(
            !result.warnings.iter().any(|w| {
                let t = w.format();
                t.contains("compiling from AST") || t.contains("failed to lower")
            }),
            "must not omit main from MIR: {:?}",
            result.warnings.iter().map(|w| w.format()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_match_main_emits_complete_hir() {
        assert_complete_main_hir(
            r#"
fn main() => int:
    let x: int = 1
    match x:
        1 => 10
        _ => 0
    rm x
    return 0
"#,
            "match_main.cf",
        );
    }

    #[test]
    fn test_for_main_emits_complete_hir() {
        assert_complete_main_hir(
            r#"
fn main() => int:
    for i in 0..3:
        let x: int = i
        rm x
    return 0
"#,
            "for_main.cf",
        );
    }

    #[test]
    fn test_mir_lower_failure_is_hard_error_not_ast_warning() {
        let compiler = CompilerFrontend::new();
        // Typecheck rejects enum patterns on int; lowering also Errs (not simple).
        // The MIR diagnostic must be Error, not Warning "compiling from AST".
        let source = r#"
enum Color:
    Red
    Blue

fn main() => int:
    let x: int = 1
    match x:
        1 => 0
        Color.Red => 0
        _ => 0
    rm x
    return 0
"#;
        let result = compiler.compile(source, Some("mixed_match.cf"));
        assert!(
            !result.success,
            "lower_function Err must fail the frontend (success=false)"
        );
        assert!(
            result.errors.iter().any(|e| {
                let t = e.format();
                t.contains("failed to lower") || t.contains("cannot lower match")
            }),
            "lower_function Err must be a hard Error, got errors={:?} warnings={:?}",
            result.errors.iter().map(|e| e.format()).collect::<Vec<_>>(),
            result.warnings.iter().map(|w| w.format()).collect::<Vec<_>>()
        );
        assert!(
            !result.warnings.iter().any(|w| w.format().contains("compiling from AST")),
            "must not warn-and-AST-compile, got: {:?}",
            result.warnings.iter().map(|w| w.format()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_missing_coffee_module_import_is_frontend_error() {
        let compiler = CompilerFrontend::new();
        let source = r#"
use nosuch_coffee_mod

fn main() => int:
    return 0
"#;
        let result = compiler.compile(source, None);
        assert!(
            !result.success,
            "failed Coffee-module load must fail the frontend, not skip into codegen\n{:?}",
            result.errors
        );
        assert!(
            result.errors.iter().any(|e| {
                let t = e.format();
                t.contains("nosuch_coffee_mod") || t.contains("import") || t.contains("module")
            }),
            "expected an import/module diagnostic:\n{:?}",
            result.errors.iter().map(|e| e.format()).collect::<Vec<_>>()
        );
    }

    fn write_import_fixture(dir: &std::path::Path, stem: &str, source: &str) {
        std::fs::create_dir_all(dir).expect("fixture dir");
        std::fs::write(dir.join(format!("{stem}.cf")), source).expect("write module");
    }

    /// `load_module` must keep `parse_program` byte ranges from the module file.
    #[test]
    fn load_module_keeps_parse_program_stmt_spans() {
        let dir = std::env::temp_dir().join(format!(
            "coffee_import_spans_load_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let module_src = "fn helper() => int:\n    return 1\n\nfn extra() => int:\n    return 2\n";
        write_import_fixture(&dir, "spanmod", module_src);

        let mut config = CompilerConfig::default();
        config.import_paths = vec![dir.clone()];
        let compiler = CompilerFrontend::with_config(config);
        let loaded = compiler.load_module("spanmod").expect("load spanmod");
        let parsed = crate::parser::parse_program(module_src).expect("parse module");

        assert_eq!(loaded.statements.len(), parsed.statements.len());
        assert_eq!(loaded.stmt_spans.len(), loaded.statements.len());
        assert_eq!(loaded.stmt_spans, parsed.stmt_spans);
        assert!(
            loaded.stmt_spans.iter().all(|s| s.end > s.start),
            "module spans must be real parse ranges, got {:?}",
            loaded.stmt_spans
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Imported prefix uses the module's own spans (zipped with prepended stmts);
    /// main-file suffix stays `program.stmt_spans`. Lengths stay equal.
    #[test]
    fn expand_imports_copy_module_spans_main_suffix_unchanged() {
        let dir = std::env::temp_dir().join(format!(
            "coffee_import_spans_expand_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let module_src = "fn helper() => int:\n    return 1\n\nfn extra() => int:\n    return 2\n";
        write_import_fixture(&dir, "spanmod", module_src);

        let main_src = "use helper in spanmod\n\nfn main() => int:\n    return 0\n";

        let mut config = CompilerConfig::default();
        config.import_paths = vec![dir.clone()];
        let compiler = CompilerFrontend::with_config(config);
        let result = compiler.compile(main_src, Some("main.cf"));
        let module_parsed = crate::parser::parse_program(module_src).expect("parse module");
        let main_parsed = crate::parser::parse_program(main_src).expect("parse main");

        assert_eq!(
            result.program.statements.len(),
            result.program.stmt_spans.len(),
            "statements and stmt_spans must stay equal length"
        );

        let imported_len = result.program.statements.len() - main_parsed.statements.len();
        assert_eq!(imported_len, 1, "selective import prepends one function");

        let helper_idx = module_parsed
            .statements
            .iter()
            .position(|s| matches!(s, crate::parser::Statement::Function(f) if f.name == "helper"))
            .expect("helper in module");
        assert_eq!(
            result.program.stmt_spans[0],
            module_parsed.stmt_spans[helper_idx],
            "imported stmt must keep that function's module span, not 0,0 or extra's span"
        );
        assert_ne!(
            result.program.stmt_spans[0],
            crate::types::definition::Span::new(0, 0)
        );
        assert_eq!(
            &result.program.stmt_spans[imported_len..],
            &main_parsed.stmt_spans[..],
            "main-file suffix must keep parse_program ranges from the main source"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
