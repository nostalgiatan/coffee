//! Live compilation pipeline (parse → imports → semantic → types).
//!
//! Single-file (`main.rs`) and project (`ProjectBuilder`) both construct
//! `CompilationPipeline` (`CompilerFrontend` is a type alias).

use crate::coffee_debug;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::parser;
use crate::semantic::analyzer::{ScopeInfo, SymbolInfo};
use crate::semantic::AnalysisReport;
use crate::diagnostics::{
    Diagnostic, ErrorKind, Severity,
};
#[cfg(test)]
use crate::diagnostics::DiagnosticEmitter;
use super::session::Session;
use super::unit::{CompilationStatus, CompilationUnit};
use super::{
    CompilationResult, CompilationStatistics, EmitKind,
};

mod parse;
mod imports;
mod collect;

/// Coordinates frontend stages using shared `Session` state.
pub struct CompilationPipeline {
    session: Arc<Session>,
}

impl Default for CompilationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilationPipeline {
    pub fn new() -> Self {
        Self::with_session(Arc::new(Session::new()))
    }

    pub fn with_config(config: super::CompilerConfig) -> Self {
        Self::with_session(Arc::new(Session::with_config(config)))
    }

    pub fn with_session(session: Arc<Session>) -> Self {
        CompilationPipeline { session }
    }

    /// Compile source code
    pub fn compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult {
        // Clear previous diagnostics (emitter + leftover frontend error lists)
        self.session.emitter.clear();
        self.session.analyzer.write().unwrap().clear_errors();
        self.session.type_checker.write().unwrap().clear_errors();

        // Initialize statistics
        let mut stats = CompilationStatistics::default();
        stats.lines_parsed = source.lines().count();

        // Parse source code with error recovery
        let mut parse_ok = true;
        let program = match parser::parse_program(source) {
            Ok(program) => {
                stats.statements_parsed = program.statements.len();
                program
            }
            Err(parse_errors) => {
                parse_ok = false;
                for error in parse_errors {
                    self.session.emitter.emit(error.into());
                }
                parser::Program::new(Vec::new())
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

        let (full_modules, selective_imports) = if self.session.config.enable_imports {
            self.collect_import_specs(&program)
        } else {
            (Vec::new(), Vec::new())
        };
        let imported_modules: Vec<String> = full_modules.iter()
            .chain(selective_imports.iter().map(|s| &s.module))
            .cloned()
            .collect();
        let has_imports = !full_modules.is_empty() || !selective_imports.is_empty();
        let mut program = if has_imports {
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
                    program
                }
            }
        } else {
            program
        };

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
                imported_modules: Vec::new(),
                c_imports: self.get_c_imports(),
                cfc_symbols: self.get_cfc_symbols(),
                hir_fns: Vec::new(),
            };
        }

        // Parse failures already emitted diagnostics; skip analysis on the empty program
        // so we do not invent follow-on type errors.
        if parse_ok {
        self.register_nominal_newtypes(&program);
        {
            let type_checker = self.session.type_checker.write().unwrap();
            let registry = type_checker.type_registry();
            let reg = registry.write().unwrap();
            for statement in &program.statements {
                match statement {
                    parser::Statement::Class(class) => reg.register_class_template(class),
                    parser::Statement::Function(func) => reg.register_fn_template(func),
                    _ => {}
                }
            }
        }
        // First pass: declare all functions (without analyzing bodies)
        // This supports mutual recursion
        coffee_debug!("DEBUG: compile_source: First pass: declaring all functions");
        for statement in &program.statements {
            if let parser::Statement::Function(func) = statement {
                if !func.type_params.is_empty() {
                    continue;
                }
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
                                self.session.emitter.emit(Diagnostic::new(
                                    Severity::Error,
                                    ErrorKind::InvalidType {
                                        name: class.name.clone(),
                                        reason: e.to_string(),
                                    },
                                    format!("failed to bind class '{}': {}", class.name, e),
                                ));
                                None
                            }
                        }
                    };
                    
                    // Now register the type definition (after releasing the read lock)
                    if let Some(type_def) = type_def {
                        if let Err(e) = self.session.analyzer.write().unwrap().analyze_type_def(&class.name, type_def) {
                            coffee_debug!("DEBUG: Failed to register class '{}': {}", class.name, e);
                            self.session.emitter.emit(Diagnostic::new(
                                Severity::Error,
                                ErrorKind::InvalidType {
                                    name: class.name.clone(),
                                    reason: e.to_string(),
                                },
                                format!("failed to register class '{}': {}", class.name, e),
                            ));
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
                            match reg.bind_enum(enum_def) {
                                Ok(td) => Some(td),
                                Err(e) => {
                                    self.session.emitter.emit(Diagnostic::new(
                                        Severity::Error,
                                        ErrorKind::InvalidType {
                                            name: enum_def.name.clone(),
                                            reason: e.to_string(),
                                        },
                                        format!("failed to bind enum '{}': {}", enum_def.name, e),
                                    ));
                                    None
                                }
                            }
                        };
                    
                    // Now register the type definition (after releasing the read lock)
                    if let Some(type_def) = type_def {
                        if let Err(e) = self.session.analyzer.write().unwrap().analyze_type_def(&enum_def.name, type_def) {
                            coffee_debug!("DEBUG: Failed to register enum '{}': {}", enum_def.name, e);
                            self.session.emitter.emit(Diagnostic::new(
                                Severity::Error,
                                ErrorKind::InvalidType {
                                    name: enum_def.name.clone(),
                                    reason: e.to_string(),
                                },
                                format!("failed to register enum '{}': {}", enum_def.name, e),
                            ));
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
                    continue;
                }
                parser::Statement::Function(func) if !func.type_params.is_empty() => {
                    continue;
                }
                _ => {}
            }
            self.analyze_statement(statement);
            self.collect_statistics(statement, &mut stats);
        }

        // Register fn signatures before bodies (mutual recursion). Skip names
        // already bound by semantic `declare_function_decl`.
        coffee_debug!("DEBUG: compile_source: declaring function signatures for type checker");
        {
            let mut type_checker = self.session.type_checker.write().unwrap();
            for statement in &program.statements {
                if let parser::Statement::Function(func) = statement {
                    let _ = type_checker.declare_function_sig(func);
                }
            }
        }

        // Perform type checking
        for statement in &program.statements {
            self.type_check_statement(statement);
        }
        {
            let mut type_checker = self.session.type_checker.write().unwrap();
            let registry = type_checker.type_registry();
            let inst_classes = registry.read().unwrap().cloned_instantiated_classes();
            let inst_fns = registry.read().unwrap().cloned_instantiated_fns();
            drop(registry);
            for class in &inst_classes {
                if let Err(err) = type_checker.check_class(class) {
                    if !type_checker.errors().iter().any(|e| e.to_string() == err.to_string()) {
                        type_checker.record_check_error(err);
                    }
                }
            }
            for func in &inst_fns {
                if let Err(err) = type_checker.check_function(func) {
                    if !type_checker.errors().iter().any(|e| e.to_string() == err.to_string()) {
                        type_checker.record_check_error(err);
                    }
                }
            }
            let registry = type_checker.type_registry();
            registry.read().unwrap().inject_monomorphized(&mut program);
        }
        }

        // Collect semantic analysis errors
        let analyzer = self.session.analyzer.read().unwrap();
        for space_error in analyzer.errors() {
            let diagnostic: Diagnostic = space_error.into();
            self.session.emitter.emit(diagnostic);
        }
        drop(analyzer);

        // Collect type checker diagnostics (E100 TypeMismatch via Diagnostic::from)
        let type_checker = self.session.type_checker.read().unwrap();
        for type_system_error in type_checker.errors() {
            self.session.emitter.emit(Diagnostic::from(type_system_error.clone()));
        }
        drop(type_checker);

        // Lower functions to MIR after typecheck. Parse-fail paths leave this empty.
        let mut hir_fns: Vec<crate::hir::MirFn> = Vec::new();
        if parse_ok {
            let mut type_checker = self.session.type_checker.write().unwrap();
            let type_registry = type_checker.type_registry();
            let mut reserved: HashSet<String> = HashSet::new();
            for statement in &program.statements {
                if let parser::Statement::Function(func) = statement {
                    reserved.insert(func.name.clone());
                }
                if let parser::Statement::Class(class) = statement {
                    for method in &class.methods {
                        reserved.insert(method.to_standalone_function(&class.name).name);
                    }
                }
            }
            // Class methods first: codegen compiles classes before top-level bodies.
            for statement in &program.statements {
                if let parser::Statement::Class(class) = statement {
                    for method in &class.methods {
                        let func = method.to_standalone_function(&class.name);
                        let mut taken = reserved.clone();
                        for f in &hir_fns {
                            taken.insert(f.name.clone());
                        }
                        if let Some(mir) = self.lower_function_mir(
                            &mut type_checker,
                            &type_registry,
                            &func,
                            func.name.clone(),
                            taken,
                        ) {
                            hir_fns.push(mir);
                        }
                        if let parser::function::FunctionBody::Block(body) = &func.body {
                            self.collect_nested_functions(
                                &mut type_checker,
                                &type_registry,
                                body,
                                &func.name,
                                &reserved,
                                &mut hir_fns,
                            );
                        }
                    }
                }
            }
            for statement in &program.statements {
                if let parser::Statement::Function(func) = statement {
                    if let Some(mir) = self.lower_function_mir(
                        &mut type_checker,
                        &type_registry,
                        func,
                        func.name.clone(),
                        Self::mir_name_taken(&reserved, &hir_fns),
                    ) {
                        hir_fns.push(mir);
                    }
                    if let parser::function::FunctionBody::Block(body) = &func.body {
                        self.collect_nested_functions(
                            &mut type_checker,
                            &type_registry,
                            body,
                            &func.name,
                            &reserved,
                            &mut hir_fns,
                        );
                    }
                }
            }
        }

        for mir in &hir_fns {
            if mir.complete {
                let _ = crate::hir::to_ssa(mir);
            }
        }

        // Generate analysis report with complete space tuple
        let analyzer = self.session.analyzer.read().unwrap();
        let report = analyzer.to_report_with_source(file_name.map(|s| s.to_string()));
        drop(analyzer);

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
        let success = errors.is_empty();

        CompilationResult {
            success,
            program,
            errors,
            warnings,
            hints,
            report,
            stats,
            imported_modules,
            c_imports: self.get_c_imports(),
            cfc_symbols: self.get_cfc_symbols(),
            hir_fns,
        }
    }


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
        let before = type_checker.errors().len();
        if let Err(err) = type_checker.check_statement(statement) {
            // Keep the original TypeSystemError. Round-tripping through Diagnostic
            // maps E200 to NotFound ("… not found in Symbol space") and drops "undefined".
            if type_checker.errors().len() == before {
                type_checker.record_check_error(err);
            }
        }
    }

    /// Get the diagnostic emitter
    #[cfg(test)]
    pub fn emitter(&self) -> &DiagnosticEmitter {
        &self.session.emitter
    }

    /// Compile a single module to an object file.
    /// On success, returns a memory-layout report when `--show-memory` and the module has structs.
    pub fn compile_module_to_object(&mut self, unit: &mut CompilationUnit, is_entry: bool) -> Result<Option<String>, String> {
        self.emit_module(unit, is_entry, EmitKind::Object, None)
    }

    /// Codegen a module and write object, LLVM IR, bitcode, or assembly.
    /// On success, returns a memory-layout report when `--show-memory` and the module has structs.
    /// Callers must print reports on the main thread (do not `println!` from rayon workers).
    pub fn emit_module(
        &mut self,
        unit: &mut CompilationUnit,
        is_entry: bool,
        emit: EmitKind,
        output_file: Option<&Path>,
    ) -> Result<Option<String>, String> {
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
                    parser::Import::InModuleWithLang { module, lang, .. } => {
                        if lang == "c" {
                            continue;
                        }
                        module.clone()
                    }
                };
                unit.add_dependency(module_path);
            }
        }

        // Filter out main() for non-entry modules (standalone-test mains)
        let program = if is_entry {
            result.program
        } else {
            let (statements, stmt_spans): (Vec<_>, Vec<_>) = result
                .program
                .statements
                .into_iter()
                .zip(result.program.stmt_spans)
                .filter(|(stmt, _)| !matches!(stmt, parser::Statement::Main(_)))
                .unzip();
            parser::Program {
                statements,
                stmt_spans,
            }
        };

        // Generate LLVM IR from the fully compiled program
        let context = Context::create();
        let mut backend = Backend::with_target(&context, &unit.name, self.session.config.target_triple.clone());
        let opt = Backend::optimization_level(self.session.config.opt_level);
        backend.set_opt_level(opt);

        let mut codegen = crate::backend::codegen::CodeGenerator::new(&backend);
        codegen.enable_bitfields(self.session.config.enable_bitfields);
        codegen.enable_safety(self.session.config.enable_safety);

        codegen.compile_program_with_hir(&program, &result.c_imports, result.cfc_symbols, result.hir_fns)
            .map_err(|e| format!("codegen error: {}", e))?;

        let memory_report = if self.session.config.show_memory && codegen.has_structs() {
            Some(codegen.get_memory_layout_report())
        } else {
            None
        };

        // Verify
        backend.verify()
            .map_err(|e| format!("verification error: {}", e))?;

        backend
            .optimize(opt)
            .map_err(|e| format!("optimization error: {}", e))?;

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
                        backend.write_bitcode(&path)?;
                    }
                    EmitKind::Assembly => {
                        backend.write_assembly_file(&path)
                            .map_err(|e| format!("failed to write assembly: {}", e))?;
                    }
                    _ => unreachable!(),
                }
                unit.status = CompilationStatus::Compiled(path);
                return Ok(memory_report);
            }
            EmitKind::Object | EmitKind::Binary => {
                backend.write_object_file(&unit.object)
                    .map_err(|e| format!("failed to write object file: {}", e))?;
                unit.save_hash_cache()?;
                unit.status = CompilationStatus::Compiled(unit.object.clone());
                Ok(memory_report)
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_tiny_fn_lowers_complete_mir_without_panic() {
        let src = "fn f() => int:\n    return 1\n";
        let result = CompilationPipeline::new().compile(src, Some("tiny.cf"));
        assert!(result.success, "errors: {:?}", result.errors);
        assert!(!result.hir_fns.is_empty());
        assert!(result.hir_fns.iter().all(|f| f.complete));
    }

    #[test]
    fn compile_calls_to_ssa_on_complete_mir() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/compiler/pipeline/mod.rs"),
        )
        .unwrap();
        assert!(
            text.contains("crate::hir::to_ssa"),
            "compile() must call to_ssa for every complete MirFn"
        );
    }
}
