//! Live compilation pipeline (parse → imports → semantic → types).
//!
//! `CompilerFrontend` holds a `Session` and forwards `compile` here so
//! single-file and project drivers share one algorithm.

use crate::coffee_debug;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::parser;
use crate::semantic::SemanticAnalyzer;
use crate::semantic::analyzer::{ScopeInfo, SymbolInfo};
use crate::semantic::AnalysisReport;
use crate::diagnostics::{
    CodeSnippet, Diagnostic, DiagnosticEmitter, ErrorKind, RelatedDiagnostic,
    RelationType, Severity, SourceLocation, Suggestion, SymbolType,
};
use crate::c;
use super::session::Session;
use super::unit::{CompilationStatus, CompilationUnit};
use super::{
    CompilationResult, CompilationStatistics, EmitKind, ImportSpec, ParsedModule,
};

/// Coordinates frontend stages using shared `Session` state.
pub struct CompilationPipeline {
    session: Arc<Session>,
}

impl CompilationPipeline {
    pub fn new(session: Arc<Session>) -> Self {
        CompilationPipeline { session }
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    /// Compile source code
    pub fn compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult {
        // Clear previous diagnostics
        self.session.emitter.clear();

        // Initialize statistics
        let mut stats = CompilationStatistics::default();
        stats.lines_parsed = source.lines().count();

        // Parse source code with error recovery
        let program = match self.parse_source(source, file_name) {
            Ok(stmts) => {
                stats.statements_parsed = stmts.len();
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
                    if let Err(diagnostic) = self.process_import(import) {
                        self.session.emitter.emit(diagnostic);
                    }
                }
            }
        }

        // Check for multiple entry points (`fn main` and/or `main(...)`)
        let main_count = Self::count_entry_points(&program);
        if main_count > 1 {
            self.session.emitter.emit(Diagnostic::new(
                Severity::Error,
                ErrorKind::InvalidSyntax { context: format!("file contains {} main() functions (only 1 allowed per file)", main_count) },
                format!("error: found {} main() entry points", main_count),
            ));

            // Collect diagnostics
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

            return CompilationResult {
                success: false,
                program: program.clone(),
                errors,
                warnings,
                hints,
                report: AnalysisReport {
                    scope_info: ScopeInfo::default(),
                    symbol_info: SymbolInfo::default(),
                    errors: vec![],
                },
                stats,
                std_exports: Vec::new(),
                imported_modules: Vec::new(),
                c_imports: self.get_c_imports(),
                cfc_symbols: self.get_cfc_symbols(),
            };
        }

        // Set C imports in semantic analyzer before analysis
        let c_imports = self.get_c_imports();
        self.session.analyzer.read().unwrap().set_c_imports(c_imports);

        // First pass: declare all functions (without analyzing bodies)
        // This supports mutual recursion
        coffee_debug!("DEBUG: compile_source: First pass: declaring all functions");
        for statement in &program.statements {
            if let parser::Statement::Function(func) = statement {
                let _ = self.session.analyzer.write().unwrap().declare_function_decl(func);
            }
        }

        // First pass: register all class and enum definitions
        coffee_debug!("DEBUG: compile_source: First pass: registering all class and enum definitions");
        for statement in &program.statements {
            match statement {
                parser::Statement::Class(class) => {
                    // Register class definition without analyzing methods
                    coffee_debug!("DEBUG: compile_source: Registering class '{}'", class.name);
                    let type_def = {
                        let analyzer = self.session.analyzer.read().unwrap();
                        let type_registry = analyzer.type_registry();
                        let reg = type_registry.read().unwrap();
                        match reg.bind_class(class) {
                            Ok(type_def) => {
                                coffee_debug!("DEBUG: compile_source: Successfully bound class '{}'", class.name);
                                Some(type_def)
                            }
                            Err(e) => {
                                coffee_debug!("DEBUG: Failed to bind class '{}': {}", class.name, e);
                                None
                            }
                        }
                    };
                    
                    // Now register the type definition (after releasing the read lock)
                    if let Some(type_def) = type_def {
                        if let Err(e) = self.session.analyzer.write().unwrap().analyze_type_def(&class.name, type_def) {
                            coffee_debug!("DEBUG: Failed to register class '{}': {}", class.name, e);
                        }
                    }
                }
                parser::Statement::Enum(enum_def) => {
                    // Register enum definition
                    coffee_debug!("DEBUG: compile_source: Registering enum '{}'", enum_def.name);
                    let type_def = {
                        let analyzer = self.session.analyzer.read().unwrap();
                        let type_registry = analyzer.type_registry();
                        let reg = type_registry.read().unwrap();
                        reg.bind_enum(enum_def).ok()
                    };
                    
                    // Now register the type definition (after releasing the read lock)
                    if let Some(type_def) = type_def {
                        if let Err(e) = self.session.analyzer.write().unwrap().analyze_type_def(&enum_def.name, type_def) {
                            coffee_debug!("DEBUG: Failed to register enum '{}': {}", enum_def.name, e);
                        }
                    }
                }
                _ => {}
            }
        }

        // Second pass: perform semantic analysis on all statements (skip class/enum since they're already registered)
        coffee_debug!("DEBUG: compile_source: Second pass: performing semantic analysis on {} statements", program.statements.len());
        for statement in &program.statements {
            // Skip class and enum statements since they were already registered in the first pass
            match statement {
                parser::Statement::Class(_) | parser::Statement::Enum(_) => {
                    // Already registered, skip
                    continue;
                }
                _ => {}
            }
            self.analyze_statement(statement);
            self.collect_statistics(statement, &mut stats);
        }

        // Perform type checking
        for statement in &program.statements {
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

        // Collect import specifications (both full and selective imports)
        let (full_modules, selective_imports) = if self.session.config.enable_imports {
            self.collect_import_specs(&program)
        } else {
            (Vec::new(), Vec::new())
        };

        // Expand program with imported modules (handles both full and selective imports)
        let has_imports = !full_modules.is_empty() || !selective_imports.is_empty();
        let expanded_program = if has_imports {
            match self.expand_program_with_imports_and_specs(&program, &full_modules, &selective_imports) {
                Ok(p) => p,
                Err(e) => {
                    self.session.emitter.emit(Diagnostic::new(
                        Severity::Error,
                        ErrorKind::InvalidImport {
                            import: "<module>".to_string(),
                            reason: e.clone(),
                        },
                        e
                    ));
                    program.clone()
                }
            }
        } else {
            program.clone()
        };

        // For backward compatibility, collect all imported module names
        let imported_modules: Vec<String> = full_modules.iter()
            .chain(selective_imports.iter().map(|s| &s.module))
            .cloned()
            .collect();

        CompilationResult {
            success,
            program: expanded_program,
            errors,
            warnings,
            hints,
            report,
            stats,
            std_exports: Vec::new(), // No longer using built-in std
            imported_modules,
            c_imports: self.get_c_imports(),
            cfc_symbols: self.get_cfc_symbols(),
        }
    }

    /// Parse source code using the complete program parser
    pub fn parse_source(&self, source: &str, _file_name: Option<&str>) -> Result<Vec<parser::Statement>, Vec<Diagnostic>> {
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

    /// Parse a single line
    #[allow(dead_code)]
    fn parse_line(&self, line: &str, line_num: usize, file_name: Option<&str>) -> Result<Option<parser::Statement>, Diagnostic> {
        // Try each parser in order
        // Memory operations
        if let Ok((remaining, op)) = parser::memory::parse_memory_op(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::MemoryOp(op)));
            }
        }

        // Variable declaration
        if let Ok((remaining, decl)) = parser::var::parse_variable_decl(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::VariableDecl(decl)));
            }
        }

        // Function
        if let Ok((remaining, func)) = parser::function::parse_function(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Function(func)));
            }
        }

        // Class
        if let Ok((remaining, class)) = parser::class::parse_class(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Class(class)));
            }
        }

        // Enum
        if let Ok((remaining, enum_def)) = parser::class::parse_enum(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Enum(enum_def)));
            }
        }

        // If expression
        if let Ok((remaining, if_expr)) = parser::r#if::parse_if(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::If(if_expr)));
            }
        }

        // While loop
        if let Ok((remaining, while_loop)) = parser::r#while::parse_while(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::While(while_loop)));
            }
        }

        // For loop
        if let Ok((remaining, for_loop)) = parser::r#for::parse_for(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::For(for_loop)));
            }
        }

        // Match expression
        if let Ok((remaining, match_expr)) = parser::r#match::parse_match(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Match(match_expr)));
            }
        }

        // Return statement
        if let Ok((remaining, ret)) = parser::var::parse_return(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Return(ret)));
            }
        }

        // Break statement
        if let Ok((remaining, _)) = parser::var::parse_break(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Break(parser::var::BreakStmt {})));
            }
        }

        // Continue statement
        if let Ok((remaining, _)) = parser::var::parse_continue(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Continue(parser::var::ContinueStmt {})));
            }
        }

        // Import statement
        if let Ok((remaining, import)) = parser::import::parse_import(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::Import(import)));
            }
        }

        // Comments
        if let Ok((remaining, comment)) = parser::comment::parse_single_line_comment(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::SingleLineComment(comment)));
            }
        }

        if let Ok((remaining, comment)) = parser::comment::parse_multi_line_comment(line) {
            if remaining.trim().is_empty() {
                return Ok(Some(parser::Statement::MultiLineComment(comment)));
            }
        }

        // If no parser matched, report error
        Err(Diagnostic::new(
            Severity::Error,
            ErrorKind::InvalidSyntax {
                context: format!("unable to parse statement: {}", line),
            },
            "unable to parse statement"
        ).with_location(SourceLocation::with_file(
            file_name.unwrap_or("<input>"),
            line_num,
            1,
            line.len()
        )))
    }

    /// Analyze a statement
    fn analyze_statement(&self, statement: &parser::Statement) {
        let result = self.session.analyzer.write().unwrap().analyze_statement(statement);
        if let Err(error) = result {
            // For now, just collect errors in the analyzer - they'll be reported later
            self.session.analyzer.write().unwrap().add_error(error);
        }
    }

    /// Type check a statement
    fn type_check_statement(&self, statement: &parser::Statement) {
        let mut type_checker = self.session.type_checker.write().unwrap();
        if let Err(err) = type_checker.check_statement(statement) {
            type_checker.report(err.into());
        }
    }

    /// Collect statistics from a statement
    fn collect_statistics(&self, statement: &parser::Statement, stats: &mut CompilationStatistics) {
        match statement {
            parser::Statement::Import(_) => stats.imports_processed += 1,
            parser::Statement::Function(_) => stats.functions_declared += 1,
            parser::Statement::Main(_) => {} // Main entry point - no specific stat needed
            parser::Statement::Class(_) => stats.classes_declared += 1,
            parser::Statement::Enum(_) => stats.enums_declared += 1,
            parser::Statement::VariableDecl(_) => stats.variables_declared += 1,
            parser::Statement::Assignment(_, _) => stats.assignments_made += 1,
            parser::Statement::Return(_) => stats.return_statements += 1,
            parser::Statement::If(_) => stats.if_expressions += 1,
            parser::Statement::While(_) => stats.while_loops += 1,
            parser::Statement::For(_) => stats.for_loops += 1,
            parser::Statement::Match(_) => stats.match_expressions += 1,
            parser::Statement::Break(_) => stats.break_statements += 1,
            parser::Statement::Continue(_) => stats.continue_statements += 1,
            parser::Statement::MemoryOp(_) => stats.memory_operations += 1,
            parser::Statement::Raise(_) => stats.raise_statements += 1,
            parser::Statement::SingleLineComment(_) | parser::Statement::MultiLineComment(_) => {
                stats.comments_count += 1
            }
            parser::Statement::Expr(_) => {
                stats.expression_statements += 1
            }
        }
    }

    /// Get all diagnostics
    #[allow(dead_code)]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.session.emitter.diagnostics()
    }

    /// Get the diagnostic emitter
    #[allow(dead_code)]
    pub fn emitter(&self) -> &DiagnosticEmitter {
        &self.session.emitter
    }

    /// Format all diagnostics
    #[allow(dead_code)]
    pub fn format_diagnostics(&self, diagnostics: &[Diagnostic]) -> String {
        let mut output = String::new();

        for diagnostic in diagnostics {
            output.push_str(&diagnostic.format());
            output.push('\n');
        }

        output
    }

    /// Get the semantic analyzer
    #[allow(dead_code)]
    pub fn analyzer(&self) -> Arc<RwLock<SemanticAnalyzer>> {
        self.session.analyzer.clone()
    }

    /// Get the list of C library imports
    pub fn get_c_imports(&self) -> Vec<String> {
        self.session.c_imports.read()
            .map(|imports| imports.clone())
            .unwrap_or_default()
    }

    /// Get C function symbol tables from .cfc files
    pub fn get_cfc_symbols(&self) -> std::collections::HashMap<String, c::CSymbolTable> {
        self.session.cfc_symbols.read()
            .map(|symbols| symbols.clone())
            .unwrap_or_default()
    }


    /// Collect all imported module paths from the program
    #[allow(dead_code)]
    fn collect_imported_modules(&self, program: &parser::Program) -> Vec<String> {
        let mut modules = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for stmt in &program.statements {
            if let parser::Statement::Import(import) = stmt {
                let module_path = match import {
                    parser::Import::Simple { path } => path.clone(),
                    parser::Import::Aliased { path, .. } => path.clone(),
                    parser::Import::InModule { module, .. } => module.clone(),
                    parser::Import::InModuleWithLang { .. } => {
                        // C library imports with language spec don't add module paths
                        continue;
                    }
                };

                if seen.insert(module_path.clone()) {
                    modules.push(module_path);
                }
            }
        }

        modules
    }

    /// Collect detailed import specifications
    /// Returns both full module imports and selective (InModule) imports
    fn collect_import_specs(&self, program: &parser::Program) -> (Vec<String>, Vec<ImportSpec>) {
        let mut full_modules = Vec::new();
        let mut selective_imports = Vec::new();
        let mut seen_modules = std::collections::HashSet::new();

        for stmt in &program.statements {
            if let parser::Statement::Import(import) = stmt {
                match import {
                    parser::Import::Simple { path } => {
                        if seen_modules.insert(path.clone()) {
                            full_modules.push(path.clone());
                        }
                    }
                    parser::Import::Aliased { path, .. } => {
                        if seen_modules.insert(path.clone()) {
                            full_modules.push(path.clone());
                        }
                    }
                    parser::Import::InModule { path, module, alias } => {
                        // Extract symbol name from path (could be "sqrt" or "math.sqrt")
                        let symbol_name = path.rsplit('.')
                            .next().unwrap_or(path)
                            .to_string();

                        let spec = ImportSpec {
                            module: module.clone(),
                            symbols: vec![symbol_name.clone()],
                            alias: alias.clone(),
                        };

                        if seen_modules.insert(module.clone()) {
                            selective_imports.push(spec);
                        } else {
                            // Module already imported, add symbol to existing import
                            if let Some(existing) = selective_imports.iter_mut()
                                .find(|s| s.module == *module) {
                                existing.symbols.push(symbol_name);
                            }
                        }
                    }
                    parser::Import::InModuleWithLang { .. } => {
                        // C library imports with language spec don't add to module imports
                        // They're collected separately in c_imports
                    }
                }
            }
        }

        (full_modules, selective_imports)
    }

    /// Expand the program by including imported module statements
    #[allow(dead_code)]
    fn expand_program_with_imports(
        &self,
        program: &parser::Program,
        imported_modules: &[String],
    ) -> Result<parser::Program, String> {
        use parser::Statement;

        let mut all_statements = Vec::new();
        let mut declared_symbols = std::collections::HashSet::new();

        // First, collect symbols from main program
        for stmt in &program.statements {
            if let Statement::Function(f) = stmt {
                if !declared_symbols.insert(f.name.clone()) {
                    return Err(format!("duplicate symbol '{}' in main program", f.name));
                }
            }
        }

        // Then, for each imported module, check for conflicts and merge
        for module_path in imported_modules {
            let loaded_module = self.load_module(module_path)
                .map_err(|e| {
                    // Use SymbolType::Module for module-related errors
                    format!("failed to load module '{}': {}", module_path, e.message)
                })?;

            // Track module info using the fields and methods
            let _module_name = loaded_module.module_name();
            let exported = loaded_module.exported_symbols();

            // Validate that functions are properly exported
            for stmt in &loaded_module.statements {
                if let Statement::Function(f) = stmt {
                    if loaded_module.has_export(&f.name) {
                        // Function is properly exported
                        let _ = (f.name.clone(), exported.len());
                    }
                }
            }

            // Rename imported functions with module prefix
            let renamed_statements: Vec<Statement> = loaded_module.statements
                .iter()
                .map(|stmt| {
                    if let Statement::Function(mut f) = stmt.clone() {
                        // Add module prefix to function name
                        f.name = format!("{}.{}", module_path, f.name);
                        Statement::Function(f)
                    } else {
                        stmt.clone()
                    }
                })
                .collect();

            // Check for symbol conflicts after renaming
            for stmt in &renamed_statements {
                if let Statement::Function(f) = stmt {
                    if !declared_symbols.insert(f.name.clone()) {
                        return Err(format!(
                            "symbol conflict: function '{}' is defined in multiple modules",
                            f.name
                        ));
                    }
                }
            }

            // Add renamed function definitions
            all_statements.extend(renamed_statements);
        }

        // Finally, add main program statements
        all_statements.extend(program.statements.clone());

        Ok(parser::Program { statements: all_statements })
    }

    /// Expand program with both full module imports and selective imports
    fn expand_program_with_imports_and_specs(
        &self,
        program: &parser::Program,
        full_modules: &[String],
        selective_imports: &[ImportSpec],
    ) -> Result<parser::Program, String> {
        use parser::Statement;

        let mut all_statements = Vec::new();
        let mut declared_symbols = std::collections::HashSet::new();

        // First, collect symbols from main program
        for stmt in &program.statements {
            if let Statement::Function(f) = stmt {
                if !declared_symbols.insert(f.name.clone()) {
                    return Err(format!("duplicate symbol '{}' in main program", f.name));
                }
            }
        }

        // Process full module imports (import all exported symbols)
        for module_path in full_modules {
            let loaded_module = self.load_module(module_path)
                .map_err(|e| format!("failed to load module '{}': {}", module_path, e.message))?;

            // Add all exported functions with module prefix
            let renamed_statements: Vec<Statement> = loaded_module.statements
                .iter()
                .filter(|stmt| {
                    // Only include exported functions
                    if let Statement::Function(f) = stmt {
                        loaded_module.has_export(&f.name)
                    } else {
                        true
                    }
                })
                .map(|stmt| {
                    if let Statement::Function(mut f) = stmt.clone() {
                        f.name = format!("{}.{}", module_path, f.name);
                        Statement::Function(f)
                    } else {
                        stmt.clone()
                    }
                })
                .collect();

            // Check for conflicts and add
            for stmt in &renamed_statements {
                if let Statement::Function(f) = stmt {
                    if !declared_symbols.insert(f.name.clone()) {
                        return Err(format!("symbol conflict: function '{}' from module '{}'", f.name, module_path));
                    }
                }
            }

            all_statements.extend(renamed_statements);
        }

        // Process selective imports (import specific symbols)
        for spec in selective_imports {
            let loaded_module = self.load_module(&spec.module)
                .map_err(|e| format!("failed to load module '{}': {}", spec.module, e.message))?;

            // Only import the specified symbols
            for symbol_name in &spec.symbols {
                // Find the function in the module
                let found = loaded_module.statements.iter().find(|stmt| {
                    if let Statement::Function(f) = stmt {
                        f.name == *symbol_name && loaded_module.has_export(&f.name)
                    } else {
                        false
                    }
                });

                if let Some(&Statement::Function(ref f)) = found {
                    // Determine the final name (with alias if provided)
                    let final_name = spec.alias.as_ref().unwrap_or(symbol_name);

                    // Check for conflicts
                    if !declared_symbols.insert(final_name.clone()) {
                        return Err(format!("symbol conflict: function '{}' is already defined", final_name));
                    }

                    // Clone and rename the function with its final name
                    let mut imported_func = f.clone();
                    imported_func.name = final_name.clone();
                    all_statements.push(Statement::Function(imported_func));
                } else {
                    return Err(format!(
                        "symbol '{}' not found in module '{}' or not exported",
                        symbol_name, spec.module
                    ));
                }
            }
        }

        // Finally, add main program statements
        all_statements.extend(program.statements.clone());

        Ok(parser::Program { statements: all_statements })
    }

    /// Process an import statement during semantic analysis
    /// Validates that the imported module exists and is accessible
    /// The actual import expansion is handled by expand_program_with_imports_and_specs
    fn process_import(&self, import: &parser::Import) -> Result<(), Diagnostic> {
        use parser::Import::*;

        match import {
            InModuleWithLang { paths, module, lang, .. } => {
                // Import symbol from module with language specification
                // e.g., `use printf in libc of c`
                if lang == "c" {
                    // Try to find and parse .cfc file
                    let cfc_found = self.load_cfc_file(module);

                    // Collect the import for linking
                    if let Ok(mut c_imports) = self.session.c_imports.write() {
                        // Store both library and symbols as "library:symbol" for later processing
                        for path in paths {
                            let full_name = format!("{}:{}", module, path);
                            if !c_imports.contains(&full_name) {
                                c_imports.push(full_name);
                            }
                        }
                    }

                    // If .cfc file not found:
                    // - For libc and libm: don't error, they have built-in default signatures
                    // - For other libraries: error, they need .cfc files
                    if !cfc_found {
                        if module == "libc" || module == "libm" {
                            // Built-in libraries, use default signatures - no error
                            Ok(())
                        } else {
                            // Other libraries need .cfc files
                            Err(Diagnostic::new(
                                Severity::Error,
                                ErrorKind::ModuleNotFound {
                                    module: format!("lib{}.cfc or {}.cfc", module, module),
                                },
                                format!("error: .cfc file not found for library '{}'", module)
                            ))
                        }
                    } else {
                        Ok(())
                    }
                } else {
                    // For other languages, we could extend this later
                    Err(Diagnostic::new(
                        Severity::Error,
                        ErrorKind::InvalidImport {
                            import: format!("{} in {} of {}", paths.join(", "), module, lang),
                            reason: format!("language '{}' is not supported", lang),
                        },
                        format!("unsupported language: {}", lang)
                    ))
                }
            }
            Simple { path } => {
                // Load the module to validate it exists and is accessible
                let _loaded_module = self.load_module(path)?;
                Ok(())
            }
            Aliased { path, .. } => {
                // Load the module to validate it exists and is accessible
                let _loaded_module = self.load_module(path)?;
                Ok(())
            }
            InModule { path, module, alias } => {
                // For "use x in y", we want to import from module y
                self.process_import_in_module(path, module, alias.as_deref())
            }
        }
    }

    /// Process "use x in y" style imports
    /// Imports specific symbol(s) from a module
    /// Syntax: use <symbol> in <module> [as <alias>]
    /// Examples:
    ///   use sqrt in utils           // Import sqrt from utils
    ///   use math.sqrt in std        // Import math.sqrt from std
    ///   use sqrt as root in utils   // Import sqrt from utils as 'root'
    fn process_import_in_module(&self, path: &str, module: &str, _alias: Option<&str>) -> Result<(), Diagnostic> {
        // Load the target module to verify it exists and is valid
        let loaded_module = self.load_module(module)?;

        // Extract the symbol name from path
        // path could be "sqrt" or "std.math.sqrt"
        let symbol_name = match path.rsplit('.') {
            mut iter => iter.next().unwrap_or(path),  // Get the last component
        };

        // Verify the symbol is exported from the module
        if !loaded_module.has_export(symbol_name) {
            return Err(Diagnostic::new(
                Severity::Error,
                ErrorKind::InvalidSymbolAccess {
                    name: path.to_string(),
                    reason: format!("symbol '{}' is not exported from module '{}'. Available exports: {}",
                        symbol_name, module, loaded_module.exported_symbols().join(", ")),
                },
                format!("cannot import '{}' from module '{}': symbol not found", path, module),
            ));
        }

        // Import validation successful - the actual import expansion
        // will be handled by expand_program_with_imports_and_specs
        Ok(())
    }

    /// Load and parse a .cfc file for a C library
    /// Searches for lib{module}.cfc or {module}.cfc in multiple locations:
    /// 1. Current directory
    /// 2. ./lib/ directory (project mode)
    /// 3. Other import_paths from config
    /// Returns true if the file was found and successfully parsed
    fn load_cfc_file(&self, module: &str) -> bool {
        // Possible .cfc file names
        let lib_name = format!("lib{}.cfc", module);
        let plain_name = format!("{}.cfc", module);

        // Search paths to try (in order of priority)
        let mut search_paths = Vec::new();

        // 1. Current directory (highest priority for backwards compatibility)
        search_paths.push(PathBuf::from("."));
        // 2. lib/ directory (project mode convention)
        search_paths.push(PathBuf::from("lib"));

        // 3. Add all configured import paths
        for import_path in &self.session.config.import_paths {
            // Skip current directory and lib since we already added them
            let path_str = import_path.to_string_lossy();
            if path_str != "." && path_str != "lib" && path_str != "./lib" {
                search_paths.push(import_path.clone());
            }
        }

        // Try each search path
        for base_dir in &search_paths {
            // Try lib{module}.cfc first
            let lib_path = base_dir.join(&lib_name);
            if lib_path.exists() {
                return self.parse_and_store_cfc(module, &lib_path);
            }

            // Then try {module}.cfc
            let plain_path = base_dir.join(&plain_name);
            if plain_path.exists() {
                return self.parse_and_store_cfc(module, &plain_path);
            }
        }

        false
    }

    /// Parse a .cfc file and store its symbol table
    fn parse_and_store_cfc(&self, module: &str, path: &PathBuf) -> bool {
        use crate::c;

        match c::parse_cfc_file(&path) {
            Ok(symbol_table) => {
                // Store the symbol table
                let mut cfc_symbols = self.session.cfc_symbols.write().unwrap();
                cfc_symbols.insert(module.to_string(), symbol_table);
                true
            }
            Err(e) => {
                // Parse error - emit warning
                self.session.emitter.emit(Diagnostic::new(
                    Severity::Warning,
                    ErrorKind::InvalidImport {
                        import: path.display().to_string(),
                        reason: format!("failed to parse .cfc file: {}", e),
                    },
                    format!("warning: failed to parse .cfc file '{}': {}", path.display(), e)
                ));
                false
            }
        }
    }

    /// Resolve an import path to a file path
    pub fn resolve_import_path(&self, import_path: &str) -> Result<PathBuf, String> {
        // Handle absolute paths first
        let path = PathBuf::from(import_path);
        if path.is_absolute() {
            if path.exists() {
                return Ok(path);
            }
            return Err(format!("absolute path does not exist: {}", import_path));
        }

        // Try each search path
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        for base_dir in &self.session.config.import_paths {
            // Direct path: base_dir/lib.utils.cf
            let direct = base_dir.join(format!("{}.cf", import_path));
            if direct.exists() {
                return Ok(direct);
            }

            // Replace dots with slashes: lib.utils -> lib/utils.cf
            let dotted_path = import_path.replace('.', "/");
            let dotted = base_dir.join(format!("{}.cf", dotted_path));
            if dotted.exists() {
                return Ok(dotted);
            }
        }

        // Build detailed error message
        let mut search_paths = Vec::new();
        for base_dir in &self.session.config.import_paths {
            let direct = base_dir.join(format!("{}.cf", import_path));
            let dotted = base_dir.join(format!("{}.cf", import_path.replace('.', "/")));
            search_paths.push(format!("  - {}", direct.display()));
            search_paths.push(format!("  - {}", dotted.display()));
        }

        Err(format!(
            "cannot find module '{}'\nworking directory: {}\nsearched paths:\n{}\nhint: ensure the module file exists and the path is correct",
            import_path,
            cwd.display(),
            search_paths.join("\n")
        ))
    }

    /// Load and parse a module
    pub fn load_module(&self, import_path: &str) -> Result<ParsedModule, Diagnostic> {
        // Check for circular imports
        if let Ok(stack) = self.session.import_stack.read() {
            if stack.contains(&import_path.to_string()) {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::CircularImport {
                        path: stack.iter().cloned().chain(Some(import_path.to_string())).collect(),
                    },
                    format!("circular import detected: {}", import_path)
                ));
            }
        }

        // Check cache
        if self.session.config.cache_modules {
            if let Ok(cache) = self.session.module_cache.read() {
                if let Some(module) = cache.get(import_path) {
                    return Ok(module.clone());
                }
            }
        }

        // Resolve file path
        let file_path = match self.resolve_import_path(import_path) {
            Ok(path) => path,
            Err(msg) => {
                // Create source location with file information
                let location = crate::diagnostics::SourceLocation::with_file(
                    import_path,
                    1,
                    1,
                    import_path.len() + 1
                );

                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::UndefinedSymbol {
                        name: import_path.to_string(),
                        symbol_type: SymbolType::Module,
                    },
                    msg
                ).with_location(location));
            }
        };

        // Read file
        let mut file = match fs::File::open(&file_path) {
            Ok(f) => f,
            Err(e) => {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidImport {
                        import: import_path.to_string(),
                        reason: format!("cannot read file: {}", e),
                    },
                    format!("cannot read file: {}", file_path.display())
                ));
            }
        };

        let mut content = String::new();
        if let Err(e) = file.read_to_string(&mut content) {
            return Err(Diagnostic::new(
                Severity::Error,
                ErrorKind::InvalidImport {
                    import: import_path.to_string(),
                    reason: format!("cannot read file: {}", e),
                },
                format!("cannot read file content: {}", file_path.display())
            ));
        }

        // Parse file
        let statements = match self.parse_source(&content, Some(file_path.to_str().unwrap_or(import_path))) {
            Ok(stmts) => stmts,
            Err(errors) => {
                // Create a code snippet for better error reporting
                let lines: Vec<String> = content.lines().take(5).map(|s| s.to_string()).collect();
                let snippet = CodeSnippet::new(lines, 1);
                let location = SourceLocation::with_file(file_path.to_str().unwrap_or(import_path), 1, 1, 1);

                // Add a suggestion
                let suggestion = Suggestion::new("check the file for syntax errors");

                // Add related diagnostics
                let related = RelatedDiagnostic::new(
                    RelationType::CausedBy,
                    SourceLocation::new(1, 1, 1),
                    format!("parse errors in module: {}", errors.len())
                );

                let diagnostic = Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidImport {
                        import: import_path.to_string(),
                        reason: format!("parse error in module: {} error(s)", errors.len()),
                    },
                    format!("failed to parse module: {}", import_path)
                )
                .with_location(location)
                .with_snippet(snippet)
                .with_suggestion(suggestion)
                .with_related(related);

                return Err(diagnostic);
            }
        };

        // Extract exports
        let exports = Self::extract_exports(&statements);

        let module = ParsedModule {
            statements,
            exports,
            file_path,
        };

        // Cache the module
        if self.session.config.cache_modules {
            if let Ok(mut cache) = self.session.module_cache.write() {
                cache.insert(import_path.to_string(), module.clone());
            }
        }

        Ok(module)
    }

    /// 创建标准库模块
    #[allow(dead_code)]
    pub(crate) fn create_std_module(&self) -> ParsedModule {
        // 创建标准库的虚拟语句和导出
        let std_exports = vec![
            // 数学函数
            "abs".to_string(),      // 绝对值
            "min".to_string(),      // 最小值
            "max".to_string(),      // 最大值
            "sqrt".to_string(),     // 平方根
            "pow".to_string(),      // 幂运算
            "sin".to_string(),      // 正弦
            "cos".to_string(),      // 余弦
            "tan".to_string(),      // 正切

            // 字符串函数
            "strlen".to_string(),   // 字符串长度
            "substr".to_string(),   // 子字符串
            "concat".to_string(),   // 连接字符串
            "trim".to_string(),     // 去除空白

            // 数组/列表函数
            "len".to_string(),      // 长度
            "push".to_string(),     // 添加元素
            "pop".to_string(),      // 移除元素
            "sort".to_string(),     // 排序
            "reverse".to_string(),  // 反转

            // 输入/输出函数
            "print".to_string(),    // 打印
            "println".to_string(),  // 打印并换行
            "read".to_string(),     // 读取输入

            // 类型转换函数
            "to_string".to_string(), // 转换为字符串
            "to_int".to_string(),   // 转换为整数
            "to_float".to_string(), // 转换为浮点数
            "to_bool".to_string(),  // 转换为布尔值

            // 内存和系统函数
            "sizeof".to_string(),   // 获取大小
            "memcpy".to_string(),   // 内存复制
            "gc".to_string(),       // 垃圾回收
        ];

        ParsedModule {
            statements: Vec::new(), // 标准库不需要语句，只提供符号
            exports: std_exports,
            file_path: std::path::PathBuf::from("<std>"), // 虚拟路径
        }
    }

    /// Extract exported symbols from statements
    fn extract_exports(statements: &[parser::Statement]) -> Vec<String> {
        let mut exports = Vec::new();

        for statement in statements {
            match statement {
                parser::Statement::Function(func) => {
                    exports.push(func.name.clone());
                }
                parser::Statement::Class(class) => {
                    exports.push(class.name.clone());
                }
                parser::Statement::Enum(enum_def) => {
                    exports.push(enum_def.name.clone());
                }
                parser::Statement::VariableDecl(var_decl) => {
                    // Export all variables (could add visibility modifiers later)
                    exports.push(var_decl.name.clone());
                }
                _ => {}
            }
        }

        exports
    }

    /// Compile a single module to an object file
    pub fn compile_module_to_object(&mut self, unit: &mut CompilationUnit, is_entry: bool) -> Result<(), String> {
        self.emit_module(unit, is_entry, EmitKind::Object, None)
    }

    /// Codegen a module and write object, LLVM IR, bitcode, or assembly.
    pub fn emit_module(
        &mut self,
        unit: &mut CompilationUnit,
        is_entry: bool,
        emit: EmitKind,
        output_file: Option<&Path>,
    ) -> Result<(), String> {
        use crate::backend::Backend;
        use inkwell::context::Context;

        // Calculate hash
        unit.calculate_hash()?;

        // Read source file
        let source = fs::read_to_string(&unit.source)
            .map_err(|e| format!("failed to read source: {}", e))?;

        // Same frontend as single-file: parse + imports + semantic + type-check
        let result = self.compile(&source, Some(unit.source.to_str().unwrap_or("")));

        if !result.success {
            let mut error_msg = String::new();
            for diag in &result.errors {
                error_msg.push_str(&format!("{}\n", diag.format()));
            }
            return Err(error_msg);
        }

        // Track Coffee-module dependencies (C imports live on CompilationResult)
        for stmt in &result.program.statements {
            if let parser::Statement::Import(import) = stmt {
                let module_path = match import {
                    parser::Import::Simple { path } => path.clone(),
                    parser::Import::Aliased { path, .. } => path.clone(),
                    parser::Import::InModule { path, .. } => path.clone(),
                    parser::Import::InModuleWithLang { paths, module: _, lang, .. } => {
                        if lang == "c" {
                            continue;
                        } else {
                            paths.join(",")
                        }
                    }
                };
                unit.add_dependency(module_path);
            }
        }

        // Filter out main() for non-entry modules (standalone-test mains)
        let program = if is_entry {
            result.program
        } else {
            parser::Program {
                statements: result.program.statements.into_iter()
                    .filter(|stmt| !matches!(stmt, parser::Statement::Main(_)))
                    .collect(),
            }
        };

        // Generate LLVM IR from the fully compiled program
        let context = Context::create();
        let backend = Backend::with_target(&context, &unit.name, self.session.config.target_triple.clone());

        let mut codegen = crate::backend::codegen::CodeGenerator::new(&backend);

        codegen.compile_program(&program, &result.c_imports, result.cfc_symbols)
            .map_err(|e| format!("codegen error: {}", e))?;

        // Verify
        backend.verify()
            .map_err(|e| format!("verification error: {}", e))?;

        // Create output directory if needed
        if let Some(parent) = unit.object.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output directory: {}", e))?;
        }

        match emit {
            EmitKind::LlvmIr | EmitKind::Bitcode | EmitKind::Assembly => {
                let path = output_file
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| {
                        let stem = unit.source
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("a");
                        match emit {
                            EmitKind::LlvmIr => PathBuf::from(format!("{}.ll", stem)),
                            EmitKind::Bitcode => PathBuf::from(format!("{}.bc", stem)),
                            EmitKind::Assembly => PathBuf::from(format!("{}.s", stem)),
                            _ => unit.object.clone(),
                        }
                    });
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("failed to create output directory: {}", e))?;
                    }
                }
                match emit {
                    EmitKind::LlvmIr => {
                        backend.write_ir(&path)
                            .map_err(|e| format!("failed to write LLVM IR: {}", e))?;
                    }
                    EmitKind::Bitcode => {
                        if !backend.write_bitcode(&path) {
                            return Err("failed to write bitcode".to_string());
                        }
                    }
                    EmitKind::Assembly => {
                        backend.write_assembly_file(&path)
                            .map_err(|e| format!("failed to write assembly: {}", e))?;
                    }
                    _ => unreachable!(),
                }
                unit.status = CompilationStatus::Compiled(path);
                return Ok(());
            }
            EmitKind::Object | EmitKind::Binary => {
                backend.write_object_file(&unit.object)
                    .map_err(|e| format!("failed to write object file: {}", e))?;
                unit.save_hash_cache()?;
                unit.status = CompilationStatus::Compiled(unit.object.clone());
                Ok(())
            }
        }
    }

    /// Count program entry points: `main(...)` statements and functions named `main`.
    /// Both in one file (or two `fn main`) is an error.
    pub fn count_entry_points(program: &parser::Program) -> usize {
        program.statements.iter().filter(|stmt| match stmt {
            parser::Statement::Main(_) => true,
            parser::Statement::Function(f) => f.name == "main",
            _ => false,
        }).count()
    }

    /// Deprecated alias; counts all entry points, not only `Statement::Main`.
    pub fn count_main_functions(program: &parser::Program) -> usize {
        Self::count_entry_points(program)
    }


    /// Format a compilation result for display
    pub fn format_result(&self, result: &CompilationResult) -> String {
        let mut output = String::new();

        if result.success {
            output.push_str("✓ Compilation successful\n");
        } else {
            output.push_str("✗ Compilation failed\n");
        }

        output.push_str(&format!("  Statistics:\n"));
        output.push_str(&format!("    Lines parsed: {}\n", result.stats.lines_parsed));
        output.push_str(&format!("    Statements: {}\n", result.stats.statements_parsed));
        output.push_str(&format!("    Imports: {}\n", result.stats.imports_processed));
        output.push_str(&format!("    Modules loaded: {}\n", result.stats.modules_loaded));
        output.push_str(&format!("    Functions: {}\n", result.stats.functions_declared));
        output.push_str(&format!("    Classes: {}\n", result.stats.classes_declared));
        output.push_str(&format!("    Enums: {}\n", result.stats.enums_declared));
        output.push_str(&format!("    Variables: {}\n", result.stats.variables_declared));

        // Add standard library exports information
        if !result.std_exports.is_empty() {
            output.push_str(&format!("    Std exports: {}\n", result.std_exports.len()));
        }

        // Add analysis report information
        output.push_str(&format!("  Analysis:\n"));
        output.push_str(&format!("    Scopes: {}\n", result.report.scope_info.total_scopes));
        output.push_str(&format!("    Scope depth: {}\n", result.report.scope_info.depth));
        output.push_str(&format!("    Symbols declared: {}\n", result.report.symbol_info.declared));
        output.push_str(&format!("    Symbols referenced: {}\n", result.report.symbol_info.referenced));
        if !result.report.symbol_info.unused.is_empty() {
            output.push_str(&format!("    Unused symbols: {}\n", result.report.symbol_info.unused.join(", ")));
        }

        if !result.errors.is_empty() {
            output.push_str(&format!("\n  Errors: {}\n", result.errors.len()));
            for error in &result.errors {
                output.push_str(&format!("    {}\n", error.format_plain()));
            }
        }

        if !result.warnings.is_empty() {
            output.push_str(&format!("\n  Warnings: {}\n", result.warnings.len()));
        }

        if !result.hints.is_empty() {
            output.push_str(&format!("\n  Hints: {}\n", result.hints.len()));
        }

        output
    }

}
