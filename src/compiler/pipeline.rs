//! Compilation Pipeline Module
//! 
//! This module orchestrates the complete Coffee compilation process through multiple stages.
//! It manages the flow from source code parsing to semantic analysis, type checking,
//! and final compilation result generation. The pipeline coordinates various compiler
//! components to transform Coffee source code into a structured compilation result
//! that can be used for code generation or error reporting.
//! 
//! The compilation pipeline consists of:
//! - Source code parsing into AST representation
//! - Import statement processing and validation
//! - Semantic analysis for scope and symbol resolution
//! - Type checking for variable declarations and memory operations
//! - Compilation result generation with diagnostics
//! 
//! The pipeline is designed to be stateless for each compilation operation while
//! maintaining shared state through the compilation session, allowing for both
//! single-file compilation and project-wide compilation with proper dependency
//! tracking.

use std::sync::Arc;

use crate::parser;
use crate::semantic::{AnalysisReport};
use crate::semantic::analyzer::{ScopeInfo, SymbolInfo};
use crate::diagnostics::{Diagnostic, Severity, ErrorKind};

use super::{CompilationResult, ImportResolver, CompilationStatistics, Session};

/// Compilation pipeline that coordinates different compilation stages
/// 
/// This structure manages the complete compilation process for Coffee source code.
/// It coordinates multiple stages including parsing, semantic analysis, type checking,
/// and import resolution. The pipeline maintains references to shared state through
/// the compilation session and uses the import resolver to handle module imports.
/// 
/// The pipeline is designed to be stateless for each compilation operation, relying
/// on the shared session state for persistent data between compilation stages.
/// 
/// # Examples
/// 
/// ```rust
/// use std::sync::Arc;
/// use crate::compiler::{Session, CompilationPipeline};
/// 
/// let session = Arc::new(Session::new());
/// let pipeline = CompilationPipeline::new(session);
/// 
/// let source = "fn main() => ():\n    print(\"Hello, World!\")";
/// let result = pipeline.compile(source, Some("main.cf"));
/// 
/// if result.success {
///     println!("Compilation successful!");
/// } else {
///     for error in result.errors {
///         eprintln!("Error: {}", error.format());
///     }
/// }
/// ```
pub struct CompilationPipeline {
    /// Import resolver used to process and validate import statements
    /// This handles all module import logic including dependency resolution
    import_resolver: ImportResolver,
    /// Shared compilation session containing global state
    /// This includes diagnostics emitter, analyzer, type checker, and configuration
    session: Arc<Session>,
}

impl CompilationPipeline {
    /// Create a new compilation pipeline with the given session
    /// 
    /// This function initializes a new compilation pipeline that shares state through
    /// the provided session. The session contains global compiler state including
    /// diagnostics emitter, semantic analyzer, type checker, and configuration.
    /// 
    /// The pipeline also creates an import resolver using the session's configuration
    /// to handle module import processing.
    /// 
    /// # Arguments
    /// 
    /// * `session` - The shared compilation session to use
    /// 
    /// # Returns
    /// 
    /// A new CompilationPipeline instance ready to process Coffee source code
    pub fn new(session: Arc<Session>) -> Self {
        let import_resolver = ImportResolver::new(session.config.clone().into(), session.clone());
        CompilationPipeline {
            import_resolver,
            session,
        }
    }

    /// Execute the complete compilation pipeline on source code
    /// 
    /// This is the main entry point for the compilation process. It takes Coffee
    /// source code as input and processes it through all compilation stages:
    /// 1. Parsing the source code into an AST
    /// 2. Processing import statements
    /// 3. Performing semantic analysis
    /// 4. Type checking
    /// 5. Generating a compilation result with diagnostics
    /// 
    /// The method handles error recovery gracefully, continuing compilation even
    /// when encountering errors and reporting all diagnostics in the result.
    /// 
    /// # Arguments
    /// 
    /// * `source` - The Coffee source code to compile
    /// * `file_name` - Optional file name for error reporting (can be None for stdin or REPL)
    /// 
    /// # Returns
    /// 
    /// A CompilationResult containing the parsed program, diagnostics, and statistics
    pub fn compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult {
        // Initialize statistics
        let mut stats = CompilationStatistics::new();
        stats.update_line_count(source);

        // Parse source code with error recovery
        let program = match self.parse_source(source) {
            Ok(stmts) => {
                stats.update_statement_count(&stmts);
                parser::Program { statements: stmts }
            }
            Err(parse_errors) => {
                // Report parse errors but continue if possible
                for error in parse_errors {
                    self.session.emitter.emit(error);
                }
                parser::Program { statements: Vec::new() }
            }
        };

        // Process imports first (before semantic analysis)
        if self.session.config.enable_imports {
            for statement in &program.statements {
                if let parser::Statement::Import(import) = statement {
                    if let Err(diagnostic) = self.import_resolver.process_import(import) {
                        self.session.emitter.emit(diagnostic);
                    }
                }
            }
        }

        // Check for multiple main functions
        let main_count = self.count_main_functions(&program);
        if main_count > 1 {
            self.session.emitter.emit(Diagnostic::new(
                Severity::Error,
                ErrorKind::InvalidSyntax { context: format!("file contains {} main() functions (only 1 allowed per file)", main_count) },
                format!("error: found {} main() entry points", main_count),
            ));

            return self.create_error_result(program, stats);
        }

        // Set C imports in semantic analyzer before analysis
        let c_imports = self.session.get_c_imports();
        self.session.analyzer.read().unwrap().set_c_imports(c_imports);

        // Set C function symbols in semantic analyzer before analysis
        let cfc_symbols = self.session.get_cfc_symbols();
        self.session.analyzer.read().unwrap().set_cfc_symbols(cfc_symbols);

        // Build imported symbols map to register only imported symbols
        let mut imported_symbols_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for stmt in &program.statements {
            if let parser::Statement::Import(import_stmt) = stmt {
                match import_stmt {
                    parser::Import::Simple { path } => {
                        // `use math` -> import all symbols from math
                        imported_symbols_map.insert(path.clone(), Vec::new());
                    }
                    parser::Import::Aliased { path, .. } => {
                        // `use math as m` -> import all symbols from math
                        imported_symbols_map.insert(path.clone(), Vec::new());
                    }
                    parser::Import::InModule { path, module, .. } => {
                        // `use add in math` -> import only "add" from math
                        imported_symbols_map.entry(module.clone())
                            .or_insert_with(Vec::new)
                            .push(path.clone());
                    }
                    parser::Import::InModuleWithLang { paths, module, lang, .. } => {
                        if lang == "c" {
                            // C imports are handled separately via c_imports
                            continue;
                        }

                        if lang.is_empty() {
                            // Multi-symbol import: `use add, multiply in math`
                            // Import only the specified symbols from the module
                            for path in paths {
                                imported_symbols_map.entry(module.clone())
                                    .or_insert_with(Vec::new)
                                    .push(path.clone());
                            }
                        }
                    }
                }
            }
        }

        // Register imported module symbols in semantic analyzer BEFORE analyzing statements
        if self.session.config.enable_imports {
            let imported_modules_data = self.import_resolver.get_loaded_modules();

            for (module_name, parsed_module) in &imported_modules_data {
                // Get the list of symbols to import from this module
                let symbols_to_import = imported_symbols_map.get(module_name);

                // Register only the imported symbols from this module
                for stmt in &parsed_module.statements {
                    if let parser::Statement::Function(func) = stmt {
                        let should_import = if let Some(symbols) = symbols_to_import {
                            if symbols.is_empty() {
                                true // Import all symbols (full module import)
                            } else {
                                symbols.contains(&func.name)
                            }
                        } else {
                            false // Not importing from this module
                        };

                        if should_import {
                            // Register this function in the semantic analyzer
                            let _ = self.session.analyzer.write().unwrap().analyze_function_decl(func);
                        }
                    }
                }
            }
        }

        // First pass: declare all functions (without analyzing bodies)
        // This supports mutual recursion
        eprintln!("DEBUG: compile_source: First pass: declaring all functions");
        for statement in &program.statements {
            if let parser::Statement::Function(func) = statement {
                let _ = self.session.analyzer.write().unwrap().declare_function_decl(func);
            }
        }

        // Perform semantic analysis
        eprintln!("DEBUG: compile_source: Performing semantic analysis on {} statements", program.statements.len());
        for statement in &program.statements {
            eprintln!("DEBUG: compile_source: Analyzing statement: {:?}", statement);
            self.analyze_statement(statement);
            stats.collect_from_statement(statement);
        }

        // Perform type checking
        eprintln!("DEBUG: compile_source: Performing type checking on {} statements", program.statements.len());
        for statement in &program.statements {
            eprintln!("DEBUG: compile_source: Type checking statement: {:?}", statement);
            self.type_check_statement(statement);
        }

        // Collect semantic analysis errors
        let analyzer = self.session.analyzer.read().unwrap();
        for space_error in analyzer.errors() {
            let diagnostic: Diagnostic = space_error.into();
            self.session.emitter.emit(diagnostic);
        }
        drop(analyzer);

        // Collect type checker diagnostics
        let type_checker = self.session.type_checker.read().unwrap();
        for diagnostic in type_checker.diagnostics() {
            // Convert from types::errors::Diagnostic to diagnostics::Diagnostic
            let main_diagnostic = crate::diagnostics::Diagnostic::new(
                crate::diagnostics::Severity::Error,
                crate::diagnostics::ErrorKind::InvalidSyntax { context: "type check error".to_string() },
                diagnostic.message
            );
            self.session.emitter.emit(main_diagnostic);
        }
        drop(type_checker);

        // Generate analysis report with complete space tuple
        let analyzer = self.session.analyzer.read().unwrap();
        let report = analyzer.to_report_with_source(file_name.map(|s| s.to_string()));
        drop(analyzer);

        // Collect import specifications (both full and selective imports)
        let (full_modules, selective_imports) = if self.session.config.enable_imports {
            self.import_resolver.collect_import_specs(&program)
        } else {
            (Vec::new(), Vec::new())
        };

        // For backward compatibility, collect all imported module names
        let imported_modules: Vec<String> = full_modules.iter()
            .chain(selective_imports.iter().map(|s| &s.module))
            .cloned()
            .collect();

        // Collect diagnostics by severity
        let all_diagnostics = self.session.emitter.diagnostics();
        let errors: Vec<Diagnostic> = all_diagnostics.iter()
            .filter(|d| d.severity == Severity::Error)
            .cloned()
            .collect();
        let warnings: Vec<Diagnostic> = all_diagnostics.iter()
            .filter(|d| d.severity == Severity::Warning)
            .cloned()
            .collect();
        let hints: Vec<Diagnostic> = all_diagnostics.iter()
            .filter(|d| d.severity == Severity::Hint)
            .cloned()
            .collect();

        // Determine success
        let success = errors.is_empty();

        // Get loaded modules from import resolver
        let imported_modules_data = self.import_resolver.get_loaded_modules();

        // Build imported symbols map from import statements
        let mut imported_symbols: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for stmt in &program.statements {
            if let parser::Statement::Import(import_stmt) = stmt {
                match import_stmt {
                    parser::Import::Simple { path } => {
                        // `use math` -> import all symbols from math
                        imported_symbols.insert(path.clone(), Vec::new());
                    }
                    parser::Import::Aliased { path, .. } => {
                        // `use math as m` -> import all symbols from math
                        imported_symbols.insert(path.clone(), Vec::new());
                    }
                    parser::Import::InModule { path, module, .. } => {
                        // `use add in math` -> import only "add" from math
                        imported_symbols.entry(module.clone())
                            .or_insert_with(Vec::new)
                            .push(path.clone());
                    }
                    parser::Import::InModuleWithLang { paths, module, lang, .. } => {
                        if lang == "c" {
                            // C imports are handled separately via c_imports
                            continue;
                        }

                        if lang.is_empty() {
                            // Multi-symbol import: `use add, multiply in math`
                            // Import only the specified symbols from the module
                            for path in paths {
                                imported_symbols.entry(module.clone())
                                    .or_insert_with(Vec::new)
                                    .push(path.clone());
                            }
                        }
                    }
                }
            }
        }

        CompilationResult {
            success,
            program,
            errors,
            warnings,
            hints,
            report,
            stats,
            std_exports: Vec::new(), // No longer using built-in std
            imported_modules,
            imported_modules_data,
            imported_symbols,
            c_imports: self.session.get_c_imports(),
            cfc_symbols: self.session.get_cfc_symbols(),
        }
    }

    /// Parse source code using the complete program parser
    /// 
    /// This internal method handles the parsing stage of the compilation pipeline.
    /// It takes Coffee source code as a string and attempts to parse it into
    /// a vector of AST statements. If parsing fails, it converts the parse errors
    /// into diagnostic objects that can be reported to the user.
    /// 
    /// # Arguments
    /// 
    /// * `source` - The Coffee source code to parse
    /// 
    /// # Returns
    /// 
    /// * `Ok(Vec<parser::Statement>)` - Successfully parsed statements
    /// * `Err(Vec<Diagnostic>)` - Parsing errors converted to diagnostics
    fn parse_source(&self, source: &str) -> Result<Vec<parser::Statement>, Vec<Diagnostic>> {
        match parser::parse_program(source) {
            Ok(program) => Ok(program.statements),
            Err(parse_errors) => {
                // Convert parse errors to diagnostics using the enhanced error system
                let diagnostics: Vec<Diagnostic> = parse_errors
                    .into_iter()
                    .map(|e| e.into())
                    .collect();
                Err(diagnostics)
            }
        }
    }

    /// Analyze a single statement semantically
    /// 
    /// This internal method performs semantic analysis on a single statement
    /// using the shared semantic analyzer from the compilation session.
    /// Semantic analysis includes scope resolution, symbol binding, and
    /// other non-type-related semantic checks.
    /// 
    /// # Arguments
    /// 
    /// * `statement` - The statement to analyze
    fn analyze_statement(&self, statement: &parser::Statement) {
        let result = self.session.analyzer.write().unwrap().analyze_statement(statement);
        if let Err(error) = result {
            // For now, just collect errors in the analyzer - they'll be reported later
            self.session.analyzer.write().unwrap().add_error(error);
        }
    }

    /// Type check a single statement
    /// 
    /// This internal method performs type checking on a single statement
    /// using the shared type checker from the compilation session.
    /// Currently, only variable declarations and memory operations are
    /// subject to type checking.
    /// 
    /// # Arguments
    /// 
    /// * `statement` - The statement to type check
    fn type_check_statement(&self, statement: &parser::Statement) {
        let mut type_checker = self.session.type_checker.write().unwrap();

        match statement {
            parser::Statement::VariableDecl(decl) => {
                if let Err(diagnostic) = type_checker.check_variable_decl(decl) {
                    type_checker.report(diagnostic.into());
                }
            }
            parser::Statement::MemoryOp(op) => {
                if let Err(diagnostic) = type_checker.check_memory_op(op) {
                    type_checker.report(diagnostic);
                }
            }
            _ => {
                // Other statements don't need type checking yet
            }
        }
    }

    /// Count main functions in the program
    /// 
    /// This internal method counts how many main entry points exist in the program.
    /// Coffee programs should have at most one main function, so this check helps
    /// catch potential errors where multiple main functions are defined.
    /// 
    /// # Arguments
    /// 
    /// * `program` - The parsed program to check
    /// 
    /// # Returns
    /// 
    /// The number of main entry points found in the program
    fn count_main_functions(&self, program: &parser::Program) -> usize {
        program.statements.iter()
            .filter(|stmt| matches!(stmt, parser::Statement::Main(_)))
            .count()
    }

    /// Create an error result when compilation fails early
    /// 
    /// This internal method creates a compilation result that indicates failure
    /// when compilation cannot proceed due to early errors (such as parsing errors).
    /// It collects all current diagnostics from the session and creates a result
    /// with the appropriate error status.
    /// 
    /// # Arguments
    /// 
    /// * `program` - The partially parsed program
    /// * `stats` - Compilation statistics gathered so far
    /// 
    /// # Returns
    /// 
    /// A CompilationResult with success=false and all current diagnostics
    fn create_error_result(&self, program: parser::Program, stats: CompilationStatistics) -> CompilationResult {
        let all_diagnostics = self.session.emitter.diagnostics();
        let errors: Vec<Diagnostic> = all_diagnostics.iter()
            .filter(|d| d.severity == Severity::Error)
            .cloned()
            .collect();
        let warnings: Vec<Diagnostic> = all_diagnostics.iter()
            .filter(|d| d.severity == Severity::Warning)
            .cloned()
            .collect();
        let hints: Vec<Diagnostic> = all_diagnostics.iter()
            .filter(|d| d.severity == Severity::Hint)
            .cloned()
            .collect();

        CompilationResult {
            success: false,
            program,
            errors,
            warnings,
            hints,
            report: AnalysisReport {
                scope_info: ScopeInfo::default(),
                symbol_info: SymbolInfo::default(),
                errors: vec![],
            },
            stats,
            std_exports: Vec::new(), // No longer using built-in std
            imported_modules: Vec::new(),
            imported_modules_data: std::collections::HashMap::new(),
            imported_symbols: std::collections::HashMap::new(),
            c_imports: self.session.get_c_imports(),
            cfc_symbols: self.session.get_cfc_symbols(),
        }
    }
}