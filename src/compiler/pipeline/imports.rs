use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::parser;
use crate::diagnostics::{
    CodeSnippet, Diagnostic, ErrorKind, RelatedDiagnostic, RelationType,
    Severity, SourceLocation, Suggestion, SymbolType,
};
use crate::c;
use super::CompilationPipeline;
use crate::compiler::{ImportSpec, ParsedModule};

impl CompilationPipeline {
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

    /// Collect detailed import specifications
    /// Returns both full module imports and selective (InModule) imports
    pub(super) fn collect_import_specs(&self, program: &parser::Program) -> (Vec<String>, Vec<ImportSpec>) {
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
                    parser::Import::InModuleWithLang { paths, module, lang, alias } => {
                        if lang == "c" {
                            continue;
                        }
                        // `use a, b in m` is parsed as InModuleWithLang with empty lang.
                        for symbol_name in paths {
                            let spec = ImportSpec {
                                module: module.clone(),
                                symbols: vec![symbol_name.clone()],
                                alias: alias.clone(),
                            };
                            if seen_modules.insert(module.clone()) {
                                selective_imports.push(spec);
                            } else if let Some(existing) = selective_imports
                                .iter_mut()
                                .find(|s| s.module == *module)
                            {
                                existing.symbols.push(symbol_name.clone());
                            }
                        }
                    }
                }
            }
        }

        (full_modules, selective_imports)
    }

    /// Expand program with both full module imports and selective imports
    pub(super) fn expand_program_with_imports_and_specs(
        &self,
        program: &parser::Program,
        full_modules: &[String],
        selective_imports: &[ImportSpec],
    ) -> Result<parser::Program, String> {
        use parser::Statement;

        let mut all_statements = Vec::new();
        let mut imported_spans = Vec::new();
        let mut declared_symbols = std::collections::HashSet::new();
        let missing_span = crate::types::definition::Span::new(0, 0);

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
            let loaded_module = self.load_module_expanded(module_path)
                .map_err(|e| format!("failed to load module '{}': {}", module_path, e.message))?;

            // Add all exported functions with module prefix; keep that stmt's module span.
            let renamed: Vec<(Statement, crate::types::definition::Span)> = loaded_module.statements
                .iter()
                .enumerate()
                .filter(|(_, stmt)| {
                    // Only include exported functions
                    if let Statement::Function(f) = stmt {
                        loaded_module.has_export(&f.name)
                    } else {
                        true
                    }
                })
                .map(|(idx, stmt)| {
                    let span = loaded_module.stmt_spans.get(idx).copied().unwrap_or(missing_span);
                    let renamed_stmt = if let Statement::Function(mut f) = stmt.clone() {
                        f.name = format!("{}.{}", module_path, f.name);
                        Statement::Function(f)
                    } else {
                        stmt.clone()
                    };
                    (renamed_stmt, span)
                })
                .collect();

            // Check for conflicts and add
            for (stmt, _) in &renamed {
                if let Statement::Function(f) = stmt {
                    if !declared_symbols.insert(f.name.clone()) {
                        return Err(format!("symbol conflict: function '{}' from module '{}'", f.name, module_path));
                    }
                }
            }

            for (stmt, span) in renamed {
                all_statements.push(stmt);
                imported_spans.push(span);
            }
        }

        // Process selective imports (import specific symbols)
        for spec in selective_imports {
            let loaded_module = self.load_module_expanded(&spec.module)
                .map_err(|e| format!("failed to load module '{}': {}", spec.module, e.message))?;

            // Only import the specified symbols (span from that module statement).
            let star = spec.symbols.iter().any(|s| s == "*");
            if star && spec.alias.is_some() {
                return Err(format!("use * in '{}' cannot take an alias", spec.module));
            }
            if star && spec.symbols.iter().any(|s| s != "*") {
                return Err(format!(
                    "use * in '{}' cannot be mixed with other symbol names",
                    spec.module
                ));
            }
            let symbol_names: Vec<String> = if star {
                loaded_module
                    .statements
                    .iter()
                    .filter_map(|stmt| {
                        if let Statement::Function(f) = stmt {
                            if loaded_module.has_export(&f.name) {
                                Some(f.name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                spec.symbols.clone()
            };
            for symbol_name in &symbol_names {
                let found = loaded_module.statements.iter().enumerate().find(|(_, stmt)| {
                    if let Statement::Function(f) = stmt {
                        f.name == *symbol_name && loaded_module.has_export(&f.name)
                    } else {
                        false
                    }
                });

                if let Some((idx, Statement::Function(f))) = found {
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
                    imported_spans.push(
                        loaded_module.stmt_spans.get(idx).copied().unwrap_or(missing_span),
                    );
                } else {
                    return Err(format!(
                        "symbol '{}' not found in module '{}' or not exported",
                        symbol_name, spec.module
                    ));
                }
            }

            // Nested `use` inside the loaded module was already expanded; bring
            // those extra functions so callees typecheck in this unit.
            for (idx, stmt) in loaded_module.statements.iter().enumerate() {
                if let Statement::Function(f) = stmt {
                    if spec.symbols.iter().any(|s| s == &f.name) {
                        continue;
                    }
                    if !loaded_module.has_export(&f.name) {
                        continue;
                    }
                    if !declared_symbols.insert(f.name.clone()) {
                        continue;
                    }
                    all_statements.push(Statement::Function(f.clone()));
                    imported_spans.push(
                        loaded_module.stmt_spans.get(idx).copied().unwrap_or(missing_span),
                    );
                }
            }
        }

        // Finally, add main program statements (keep their parse_program byte ranges).
        debug_assert_eq!(all_statements.len(), imported_spans.len());
        all_statements.extend(program.statements.clone());
        imported_spans.extend(program.stmt_spans.iter().copied());
        let stmt_spans = imported_spans;

        Ok(parser::Program {
            statements: all_statements,
            stmt_spans,
        })
    }

    /// Process an import statement during semantic analysis
    /// Validates that the imported module exists and is accessible
    /// The actual import expansion is handled by expand_program_with_imports_and_specs
    pub(super) fn process_import(&self, import: &parser::Import) -> Result<(), Diagnostic> {
        use parser::Import::*;

        match import {
            InModuleWithLang { paths, module, lang, alias } => {
                if lang.is_empty() {
                    if paths.iter().any(|p| p == "*") && paths.len() != 1 {
                        return Err(Diagnostic::new(
                            Severity::Error,
                            ErrorKind::InvalidImport {
                                import: format!("{} in {}", paths.join(", "), module),
                                reason: "use * cannot be mixed with other names".to_string(),
                            },
                            "use * cannot be mixed with other names".to_string(),
                        ));
                    }
                    for path in paths {
                        self.process_import_in_module(path, module, alias.as_deref())?;
                    }
                    return Ok(());
                }
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
                            let searched: Vec<String> = self
                                .cfc_search_dirs()
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect();
                            Err(crate::c::diag::cfc_not_found(module, &searched))
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
    pub(super) fn process_import_in_module(&self, path: &str, module: &str, _alias: Option<&str>) -> Result<(), Diagnostic> {
        // Load the target module to verify it exists and is valid
        let loaded_module = self.load_module(module)?;

        // Extract the symbol name from path
        // path could be "sqrt" or "std.math.sqrt"
        let symbol_name = match path.rsplit('.') {
            mut iter => iter.next().unwrap_or(path),  // Get the last component
        };

        if symbol_name == "*" {
            if _alias.is_some() {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidImport {
                        import: format!("* in {}", module),
                        reason: "use * cannot take an alias".to_string(),
                    },
                    "use * cannot take an alias".to_string(),
                ));
            }
            return Ok(());
        }

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

    fn cfc_search_dirs(&self) -> Vec<PathBuf> {
        let mut search_paths = Vec::new();
        search_paths.push(PathBuf::from("."));
        search_paths.push(PathBuf::from("lib"));

        for import_path in &self.session.config.import_paths {
            let path_str = import_path.to_string_lossy();
            if path_str != "." && path_str != "lib" && path_str != "./lib" {
                search_paths.push(import_path.clone());
            }
        }

        let push_unique = |paths: &mut Vec<PathBuf>, p: PathBuf| {
            if !paths.iter().any(|e| e == &p) {
                paths.push(p);
            }
        };
        push_unique(&mut search_paths, PathBuf::from("target/cfc"));
        if let Ok(cwd) = std::env::current_dir() {
            push_unique(&mut search_paths, cwd.join("target").join("cfc"));
        }
        search_paths
    }

    /// Load and parse a .cfc file for a C library
    /// Searches for lib{module}.cfc or {module}.cfc in multiple locations:
    /// 1. Current directory
    /// 2. ./lib/ directory (project mode)
    /// 3. Other import_paths from config
    /// 4. `target/cfc` (auto-generated) then `<cwd>/target/cfc`
    /// Returns true if the file was found and successfully parsed
    pub(super) fn load_cfc_file(&self, module: &str) -> bool {
        // Possible .cfc file names
        let lib_name = format!("lib{}.cfc", module);
        let plain_name = format!("{}.cfc", module);

        for base_dir in &self.cfc_search_dirs() {
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
    pub(super) fn parse_and_store_cfc(&self, module: &str, path: &PathBuf) -> bool {
        use crate::c;

        match fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|content| {
            c::parse_cfc_content(&content, module).map_err(|e| e.to_string())
        }) {
            Ok(symbol_table) => {
                let needs = symbol_table.needs.clone();
                {
                    let mut cfc_symbols = self.session.cfc_symbols.write().unwrap();
                    cfc_symbols.insert(module.to_string(), symbol_table);
                }
                for need in needs {
                    if matches!(need.as_str(), "libc" | "libm" | "c" | "m") {
                        continue;
                    }
                    let already = self
                        .session
                        .cfc_symbols
                        .read()
                        .map(|t| t.contains_key(&need))
                        .unwrap_or(false);
                    if already {
                        continue;
                    }
                    let _ = self.load_cfc_file(&need);
                }
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

    fn load_module_expanded(&self, import_path: &str) -> Result<ParsedModule, Diagnostic> {
        let loaded = self.load_module(import_path)?;
        {
            let mut stack = self.session.import_stack.write().unwrap();
            if stack.iter().any(|p| p == import_path) {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::CircularImport {
                        path: stack.iter().cloned().chain(Some(import_path.to_string())).collect(),
                    },
                    format!("circular import detected: {}", import_path),
                ));
            }
            stack.push(import_path.to_string());
        }

        let result = (|| {
            let program = parser::Program {
                statements: loaded.statements.clone(),
                stmt_spans: loaded.stmt_spans.clone(),
            };
            for statement in &program.statements {
                if let parser::Statement::Import(import) = statement {
                    if let Err(diagnostic) = self.process_import(import) {
                        return Err(diagnostic);
                    }
                }
            }
            let (full_modules, selective_imports) = self.collect_import_specs(&program);
            if full_modules.is_empty() && selective_imports.is_empty() {
                return Ok(loaded);
            }
            match self.expand_program_with_imports_and_specs(
                &program,
                &full_modules,
                &selective_imports,
            ) {
                Ok(expanded) => {
                    let exports = Self::extract_exports(&expanded.statements);
                    Ok(ParsedModule {
                        statements: expanded.statements,
                        stmt_spans: expanded.stmt_spans,
                        exports,
                    })
                }
                Err(e) => Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidImport {
                        import: import_path.to_string(),
                        reason: e.clone(),
                    },
                    e,
                )),
            }
        })();

        if let Ok(mut stack) = self.session.import_stack.write() {
            stack.pop();
        }
        result
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

        // Parse file (keep this module's `parse_program` byte ranges).
        let parsed = match parser::parse_program(&content) {
            Ok(program) => program,
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
        let exports = Self::extract_exports(&parsed.statements);

        let module = ParsedModule {
            statements: parsed.statements,
            stmt_spans: parsed.stmt_spans,
            exports,
        };

        // Cache the module
        if self.session.config.cache_modules {
            if let Ok(mut cache) = self.session.module_cache.write() {
                cache.insert(import_path.to_string(), module.clone());
            }
        }

        Ok(module)
    }
}

#[cfg(test)]
mod tests {
    use super::super::CompilationPipeline;
    use std::fs;

    fn unique_mod() -> String {
        format!(
            "cdep{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn load_cfc_file_searches_target_cfc() {
        let module = unique_mod();
        let dir = std::env::temp_dir().join(format!("coffee_load_cfc_{}", module));
        let cfc_dir = dir.join("target").join("cfc");
        fs::create_dir_all(&cfc_dir).unwrap();
        fs::write(
            cfc_dir.join(format!("lib{}.cfc", module)),
            "c fn cdep_ping() => int:\n",
        )
        .unwrap();

        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let pipeline = CompilationPipeline::new();
        let found = pipeline.load_cfc_file(&module);
        let has = pipeline.get_cfc_symbols().contains_key(&module);
        std::env::set_current_dir(&orig).unwrap();
        let _ = fs::remove_dir_all(&dir);

        assert!(found, "target/cfc/lib{module}.cfc must be on the search path");
        assert!(has);
    }

    #[test]
    fn load_cfc_file_dot_wins_over_target_cfc() {
        let module = unique_mod();
        let dir = std::env::temp_dir().join(format!("coffee_load_cfc_dot_{}", module));
        fs::create_dir_all(dir.join("target").join("cfc")).unwrap();
        fs::write(
            dir.join(format!("lib{}.cfc", module)),
            "c fn from_dot() => int:\n",
        )
        .unwrap();
        fs::write(
            dir.join("target").join("cfc").join(format!("lib{}.cfc", module)),
            "c fn from_target() => int:\n",
        )
        .unwrap();

        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let pipeline = CompilationPipeline::new();
        assert!(pipeline.load_cfc_file(&module));
        let table = pipeline.get_cfc_symbols().get(&module).cloned();
        std::env::set_current_dir(&orig).unwrap();
        let _ = fs::remove_dir_all(&dir);

        let table = table.expect("loaded table");
        assert!(table.symbols.contains_key("from_dot"));
        assert!(!table.symbols.contains_key("from_target"));
    }
}
