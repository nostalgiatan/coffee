//! Import Resolution Module
//! 
//! This module handles module import resolution and dependency management for the Coffee compiler.
//! It manages the resolution of import statements, tracks import dependencies, and prevents circular imports.
//! The module provides functionality for processing different types of import statements, including
//! simple imports, aliased imports, and language-specific imports (e.g., C library imports).
//! 
//! The import resolution system is responsible for:
//! - Validating import statement syntax
//! - Collecting import specifications from source code
//! - Detecting and preventing circular imports
//! - Managing import dependencies and module loading
//! - Handling different import types (Coffee modules, C libraries)
//! - Caching parsed modules for efficiency
//! 
//! The system maintains thread-safe data structures to support parallel compilation
//! while ensuring import dependencies are correctly resolved and validated.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::parser;
use crate::diagnostics::{Diagnostic, ErrorKind, Severity};

/// Import specification for selective imports
/// 
/// This structure represents a selective import specification, which specifies
/// a module and specific symbols to import from that module. It also supports
/// aliasing the imported symbols.
/// 
/// For example, in the Coffee language:
/// - `use sqrt in math` would create an ImportSpec with module="math", symbols=["sqrt"]
/// - `use sqrt as root in math` would create an ImportSpec with module="math", symbols=["sqrt"], alias=Some("root")
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::import_resolver::ImportSpec;
/// 
/// let spec = ImportSpec {
///     module: "math".to_string(),
///     symbols: vec!["sqrt".to_string(), "sin".to_string()],
///     alias: Some("math_funcs".to_string()),
/// };
/// ```
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImportSpec {
    /// Module path to import from (e.g., "math", "utils.string", etc.)
    pub module: String,
    /// Specific symbols to import from the module (empty means import all)
    pub symbols: Vec<String>,
    /// Optional alias for the import (when using 'as' keyword)
    pub alias: Option<String>,
}

/// Import resolver handles module import resolution and dependency management
/// 
/// This structure is responsible for resolving import statements in Coffee source code.
/// It manages the process of loading imported modules, caching them for performance,
/// detecting circular imports, and validating import syntax.
/// 
/// The resolver maintains:
/// - An import stack to detect circular imports
/// - A cache of parsed modules for faster subsequent imports
/// - Configuration options for import behavior
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::import_resolver::{ImportResolver, ImportConfig};
/// 
/// let config = ImportConfig::default();
/// let resolver = ImportResolver::new(config);
/// 
/// // Use the resolver to process imports in your Coffee code
/// // resolver.process_import(&some_import_statement);
/// ```
#[allow(dead_code)]
pub struct ImportResolver {
    /// Stack of currently importing modules used for cycle detection
    /// When importing module A which imports module B, both A and B will be on the stack
    import_stack: Arc<RwLock<Vec<String>>>,
    /// Cache of parsed modules to avoid re-parsing the same module multiple times
    /// This significantly improves performance when the same module is imported from multiple places
    module_cache: Arc<RwLock<HashMap<String, ParsedModule>>>,
    /// Configuration controlling import behavior (e.g., import paths, caching, etc.)
    config: ImportConfig,
    /// Module loader for loading and parsing Coffee module files
    module_loader: super::module_loader::ModuleLoader,
    /// Session reference for storing C imports
    session: Arc<super::session::Session>,
}

/// Configuration for import resolution
/// 
/// This structure holds all configuration options that control how the import
/// resolution system behaves. It allows customization of import paths, caching
/// behavior, and safety limits for import processing.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::import_resolver::ImportConfig;
/// use std::path::PathBuf;
/// 
/// let mut config = ImportConfig::default();
/// config.enable_imports = true;
/// config.import_paths.push(PathBuf::from("./my_modules"));
/// config.max_import_depth = 50;
/// config.cache_modules = true;
/// ```
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImportConfig {
    /// Whether to enable import resolution at all
    /// When false, all import statements will be ignored
    pub enable_imports: bool,
    /// List of directories to search for imported modules
    /// The compiler will look for modules in these paths in order
    pub import_paths: Vec<std::path::PathBuf>,
    /// Maximum depth of import chains to prevent infinite recursion
    /// If import A imports B which imports C etc., this limits how deep the chain can go
    pub max_import_depth: usize,
    /// Whether to cache parsed modules to avoid re-parsing
    /// This significantly improves performance when modules are imported multiple times
    pub cache_modules: bool,
}

impl Default for ImportConfig {
    /// Creates a default import configuration
    /// 
    /// The default configuration includes common search paths (examples, src, lib, std, current dir)
    /// and enables imports with caching and a reasonable import depth limit.
    /// 
    /// # Returns
    ///
    /// A new ImportConfig instance with default values
    fn default() -> Self {
        ImportConfig {
            enable_imports: true,
            import_paths: vec![
                std::path::PathBuf::from("examples"),
                std::path::PathBuf::from("src"),
                std::path::PathBuf::from("lib"),
                std::path::PathBuf::from("std"),
                std::path::PathBuf::from("."),
            ],
            max_import_depth: 100,
            cache_modules: true,
        }
    }
}

impl From<super::CompilerConfig> for ImportConfig {
    fn from(config: super::CompilerConfig) -> Self {
        ImportConfig {
            enable_imports: config.enable_imports,
            import_paths: config.import_paths,
            max_import_depth: config.max_import_depth,
            cache_modules: config.cache_modules,
        }
    }
}

/// A parsed module
/// 
/// This structure represents a Coffee source file that has been parsed into an AST.
/// It contains the parsed statements, information about exported symbols, and the
/// original file path. This is used by the import system to manage and cache
/// parsed modules.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::import_resolver::ParsedModule;
/// use crate::parser;
/// use std::path::PathBuf;
/// 
/// let module = ParsedModule {
///     statements: vec![],  // Parsed AST statements
///     exports: vec!["my_function".to_string()],  // Exported symbols
///     file_path: PathBuf::from("src/my_module.cf"),
/// };
/// 
/// assert!(module.has_export("my_function"));
/// assert_eq!(module.exported_symbols(), &["my_function"]);
/// ```
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParsedModule {
    /// The list of statements that make up the module's code
    /// These are the parsed AST nodes representing the module's functionality
    pub statements: Vec<parser::Statement>,
    /// List of symbols (functions, classes, etc.) that this module exports
    /// Only these symbols can be imported by other modules
    pub exports: Vec<String>,
    /// Path to the source file that was parsed to create this module
    /// Used for error reporting and dependency tracking
    #[allow(dead_code)]
    pub file_path: std::path::PathBuf,
}

impl ParsedModule {
    /// Get the module name from file path
    /// 
    /// Extracts the module name from the file path by removing the directory
    /// path and file extension. This is used to identify the module in import
    /// statements and error messages.
    /// 
    /// # Returns
    /// 
    /// The module name as a string, or "unknown" if the file stem cannot be determined
    #[allow(dead_code)]
    pub fn module_name(&self) -> String {
        self.file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Check if a symbol is exported from this module
    /// 
    /// Determines whether the specified symbol is in the module's export list.
    /// This is used during import validation to ensure that only exported
    /// symbols can be imported from other modules.
    /// 
    /// # Arguments
    /// 
    /// * `symbol` - The name of the symbol to check for export
    /// 
    /// # Returns
    /// 
    /// * `true` if the symbol is exported from this module
    /// * `false` otherwise
    pub fn has_export(&self, symbol: &str) -> bool {
        self.exports.iter().any(|e| e == symbol)
    }

    /// Get all exported symbols
    /// 
    /// Returns a slice containing all symbols that are exported from this module.
    /// These are the symbols that can be imported by other modules.
    /// 
    /// # Returns
    /// 
    /// A slice of strings representing the exported symbols
    pub fn exported_symbols(&self) -> &[String] {
        &self.exports
    }
}

impl ImportResolver {
    /// Create a new import resolver with the given configuration and session
    ///
    /// This function initializes a new ImportResolver instance with empty import
    /// stack and module cache. The configuration determines how imports will be
    /// resolved, including search paths, caching behavior, and recursion limits.
    ///
    /// # Arguments
    ///
    /// * `config` - The configuration to use for this import resolver
    /// * `session` - The compilation session for storing C imports
    ///
    /// # Returns
    ///
    /// A new ImportResolver instance ready to process import statements
    pub fn new(config: ImportConfig, session: Arc<super::session::Session>) -> Self {
        // Create module loader config from import config
        let loader_config = super::module_loader::ModuleLoaderConfig {
            import_paths: config.import_paths.clone(),
            cache_modules: config.cache_modules,
        };

        let module_loader = super::module_loader::ModuleLoader::new(loader_config);

        ImportResolver {
            import_stack: Arc::new(RwLock::new(Vec::new())),
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            module_loader,
            session,
        }
    }

    /// Process an import statement and load the module
    ///
    /// This method validates the import statement and actually loads the imported module.
    /// It checks for circular imports using the import stack, uses the module loader to
    /// load the module file, parses it, and recursively processes imports within the module.
    ///
    /// For C library imports (e.g., `use printf in libc of c`), this method
    /// validates the language specification and collects the imports for later
    /// processing by the linker.
    ///
    /// # Arguments
    ///
    /// * `import` - The import statement to process
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the import was processed successfully
    /// * `Err(Diagnostic)` if the import failed with error details
    pub fn process_import(&self, import: &parser::Import) -> Result<(), Diagnostic> {
        // Handle C library imports separately
        if let parser::Import::InModuleWithLang { paths, module, lang, .. } = import {
            if lang == "c" {
                // Collect C library imports and store them in session
                for path in paths {
                    // Format: "library:symbol" for specific symbol
                    let import_str = format!("{}:{}", module, path);

                    // Store C import in session
                    if let Ok(mut c_imports) = self.session.c_imports.write() {
                        if !c_imports.contains(&import_str) {
                            c_imports.push(import_str);
                        }
                    }
                }
                
                // Load the corresponding .cfc file if it exists
                let cfc_filename = format!("lib{}.cfc", module);
                let mut found_cfc = false;
                
                // Search for the .cfc file in import paths
                for base_path in &self.config.import_paths {
                    let mut cfc_path = base_path.clone();
                    cfc_path.push(&cfc_filename);
                    
                    if cfc_path.exists() {
                        // Parse the .cfc file and add its symbols to session
                        match crate::c::parser::parse_cfc_file(&cfc_path) {
                            Ok(symbol_table) => {
                                if let Ok(mut cfc_symbols) = self.session.cfc_symbols.write() {
                                    cfc_symbols.insert(module.clone(), symbol_table);
                                }
                                found_cfc = true;
                                break;
                            }
                            Err(e) => {
                                // Don't fail the entire compilation if we can't parse the .cfc file
                                // Just emit a warning diagnostic - the C function might be available through other means
                                eprintln!("Warning: Could not parse .cfc file '{}': {}", cfc_path.display(), e);
                            }
                        }
                    }
                }
                
                // If no .cfc file was found, try to use built-in symbols
                if !found_cfc {
                    // Try to check if this is a built-in C library (libc, libm, etc.)
                    // The built-in symbols are already loaded in the session
                    if let Ok(cfc_symbols) = self.session.cfc_symbols.read() {
                        if !cfc_symbols.contains_key(module) {
                            eprintln!("Warning: No .cfc file found for C library '{}', using built-in signatures if available", module);
                        }
                    }
                }
                
                return Ok(());
            }

            // Handle multi-symbol imports: "use add, multiply in math"
            if lang.is_empty() {
                // This is a Coffee module import with multiple symbols
                let module_path = module.clone();

                // Check for circular imports and depth limit
                {
                    let stack = self.import_stack.read().unwrap();
                    if stack.contains(&module_path) {
                        return Err(Diagnostic::new(
                            Severity::Error,
                            ErrorKind::CircularImport {
                                path: stack.iter().cloned().chain(std::iter::once(module_path.clone())).collect(),
                            },
                            format!("circular import detected: module '{}' imports itself", module_path),
                        ));
                    }

                    if stack.len() >= self.config.max_import_depth {
                        return Err(Diagnostic::new(
                            Severity::Error,
                            ErrorKind::InvalidImport {
                                import: module_path.clone(),
                                reason: format!("import depth exceeds maximum of {}", self.config.max_import_depth),
                            },
                            format!("import chain too deep (max: {})", self.config.max_import_depth),
                        ));
                    }
                }

                // Load the module (will use cache if available)
                let parsed_module = self.module_loader.load_module(&module_path)?;

                // Cache the module
                if self.config.cache_modules {
                    let mut cache = self.module_cache.write().unwrap();
                    cache.insert(module_path.clone(), parsed_module.clone());
                }

                // Note: We don't recursively process imports in multi-symbol case
                // because the module is already loaded and will be processed with other imports
                return Ok(());
            }
        }

        // Extract module path from the import statement
        let module_path = match import {
            parser::Import::Simple { path } => path.clone(),
            parser::Import::Aliased { path, .. } => path.clone(),
            parser::Import::InModule { module, .. } => module.clone(),
            parser::Import::InModuleWithLang { module, .. } => module.clone(),
        };

        // Check for circular imports
        {
            let stack = self.import_stack.read().unwrap();
            if stack.contains(&module_path) {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::CircularImport {
                        path: stack.iter().cloned().chain(std::iter::once(module_path.clone())).collect(),
                    },
                    format!("circular import detected: module '{}' imports itself", module_path),
                ));
            }

            // Check import depth limit
            if stack.len() >= self.config.max_import_depth {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidImport {
                        import: module_path.clone(),
                        reason: format!("import depth exceeds maximum of {}", self.config.max_import_depth),
                    },
                    format!("import chain too deep (max: {})", self.config.max_import_depth),
                ));
            }
        }

        // Check if already loaded in cache
        if self.config.cache_modules {
            let cache = self.module_cache.read().unwrap();
            if cache.contains_key(&module_path) {
                return Ok(());
            }
            drop(cache);
        }

        // Push to import stack
        {
            let mut stack = self.import_stack.write().unwrap();
            stack.push(module_path.clone());
        }

        // Load the module
        let parsed_module = self.module_loader.load_module(&module_path)?;

        // Cache the module
        if self.config.cache_modules {
            let mut cache = self.module_cache.write().unwrap();
            cache.insert(module_path.clone(), parsed_module.clone());
        }

        // Recursively process imports in the loaded module
        for statement in &parsed_module.statements {
            if let parser::Statement::Import(sub_import) = statement {
                // Skip if already being processed
                let sub_module_path = match sub_import {
                    parser::Import::Simple { path } => path.clone(),
                    parser::Import::Aliased { path, .. } => path.clone(),
                    parser::Import::InModule { module, .. } => module.clone(),
                    parser::Import::InModuleWithLang { module, .. } => module.clone(),
                };

                let stack = self.import_stack.read().unwrap();
                if !stack.contains(&sub_module_path) {
                    drop(stack);
                    self.process_import(&sub_import)?;
                }
            }
        }

        // Pop from import stack
        {
            let mut stack = self.import_stack.write().unwrap();
            stack.pop();
        }

        Ok(())
    }

    /// Get all loaded modules from the cache
    ///
    /// Returns a map of module path to parsed module
    pub fn get_loaded_modules(&self) -> std::collections::HashMap<String, ParsedModule> {
        let cache = self.module_cache.read().unwrap();
        cache.clone()
    }

    /// Collect detailed import specifications from a program
    /// 
    /// This method scans a parsed Coffee program and extracts all import statements,
    /// categorizing them into two groups:
    /// 1. Full module imports (e.g., `use utils`, `use math as m`)
    /// 2. Selective imports (e.g., `use sqrt in math`, `use println in std`)
    /// 
    /// The method also handles deduplication of imports - if the same module is
    /// imported multiple times, it will only appear once in the results.
    /// 
    /// C library imports (e.g., `use printf in libc of c`) are not included in
    /// the results as they are handled separately by the C integration system.
    /// 
    /// # Arguments
    /// 
    /// * `program` - The parsed Coffee program to scan for imports
    /// 
    /// # Returns
    /// 
    /// A tuple containing:
    /// - A vector of full module import paths
    /// - A vector of selective import specifications
    pub fn collect_import_specs(&self, program: &parser::Program) -> (Vec<String>, Vec<ImportSpec>) {
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
                        // They're collected separately
                    }
                }
            }
        }

        (full_modules, selective_imports)
    }

    /// Check if there are circular imports or if import depth exceeds the limit
    /// 
    /// This method checks the current import stack to detect circular imports
    /// and to ensure that the import depth does not exceed the configured limit.
    /// Circular imports are when module A imports module B, and module B directly
    /// or indirectly imports module A.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if no circular imports are detected and depth is within limits
    /// * `Err(Diagnostic)` if circular imports are found or depth limit exceeded
    pub fn check_circular_imports(&self) -> Result<(), Diagnostic> {
        let stack = self.import_stack.read().unwrap();
        if stack.len() > self.config.max_import_depth {
            return Err(Diagnostic::new(
                Severity::Error,
                ErrorKind::CircularImport { path: stack.clone() },
                format!("circular import detected: {}", stack.join(" -> ")),
            ));
        }
        Ok(())
    }

    /// Get the current import stack for debugging purposes
    /// 
    /// This method returns a copy of the current import stack, which shows
    /// the chain of imports that are currently being processed. This is useful
    /// for error reporting and debugging circular import issues.
    /// 
    /// # Returns
    /// 
    /// A vector of module names representing the current import chain
    pub fn import_stack(&self) -> Vec<String> {
        self.import_stack.read().unwrap().clone()
    }

    /// Add a module to the import stack
    /// 
    /// This method should be called when starting to process an import.
    /// It adds the module name to the import stack to track the import chain
    /// and detect circular imports.
    /// 
    /// # Arguments
    /// 
    /// * `module` - The name of the module being imported
    pub fn push_import(&self, module: &str) {
        self.import_stack.write().unwrap().push(module.to_string());
    }

    /// Remove a module from the import stack
    /// 
    /// This method should be called when finished processing an import.
    /// It removes the top module from the import stack, maintaining the
    /// correct import chain for circular import detection.
    /// 
    /// # Arguments
    /// 
    /// None
    pub fn pop_import(&self) {
        self.import_stack.write().unwrap().pop();
    }
}