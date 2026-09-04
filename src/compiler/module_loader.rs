//! Module Loading Module
//! 
//! This module handles loading, parsing, and caching of Coffee modules for the compiler.
//! It manages the process of locating module files in the file system, parsing them
//! into AST representations, and caching the results for performance. The module loader
//! handles module resolution using configurable search paths and provides diagnostic
//! information when modules cannot be found or parsed.
//! 
//! The module loader system provides:
//! - Thread-safe module caching for improved performance
//! - Configurable search paths for module resolution
//! - Detailed diagnostic information for error reporting
//! - Support for .cf file extension
//! - Export symbol extraction from loaded modules
//! 
//! The loader uses RwLock for thread-safe access to the module cache, making it
//! suitable for use in parallel compilation scenarios.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::fs;

use crate::parser;
use crate::diagnostics::{Diagnostic, ErrorKind, Severity};

use crate::compiler::import_resolver::ParsedModule;

/// Module loader handles loading and caching of Coffee modules
/// 
/// This structure is responsible for finding, loading, parsing, and caching
/// Coffee source files. It manages the module resolution process using a
/// configurable set of search paths and maintains an in-memory cache of
/// parsed modules to avoid re-parsing the same module multiple times.
/// 
/// The loader handles the conversion of module names (like "std.math") to
/// file paths (like "std/math.cf") and provides detailed diagnostics when
/// modules cannot be found or parsed correctly.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::module_loader::{ModuleLoader, ModuleLoaderConfig};
/// 
/// let config = ModuleLoaderConfig::default();
/// let loader = ModuleLoader::new(config);
/// 
/// match loader.load_module("std.math") {
///     Ok(module) => println!("Loaded module with {} statements", module.statements.len()),
///     Err(diag) => eprintln!("Failed to load module: {}", diag.format()),
/// }
/// ```
#[allow(dead_code)]
pub struct ModuleLoader {
    /// Thread-safe cache of loaded modules indexed by module path
    /// This improves performance by avoiding re-parsing modules that have already been loaded
    module_cache: Arc<RwLock<HashMap<String, ParsedModule>>>,
    /// Configuration controlling module loading behavior
    /// Includes search paths and caching settings
    config: ModuleLoaderConfig,
}

/// Configuration for module loading
/// 
/// This structure holds all configuration options that control how the module
/// loader behaves. It allows customization of where modules are searched for
/// and whether parsed modules should be cached for performance.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::module_loader::ModuleLoaderConfig;
/// use std::path::PathBuf;
/// 
/// let mut config = ModuleLoaderConfig::default();
/// config.import_paths.push(PathBuf::from("./my_modules"));
/// config.cache_modules = false; // Disable caching for testing
/// ```
#[derive(Debug, Clone)]
pub struct ModuleLoaderConfig {
    /// List of directories to search for modules when resolving imports
    /// The loader will search in these directories in order until it finds the requested module
    pub import_paths: Vec<std::path::PathBuf>,
    /// Whether to cache parsed modules to avoid re-parsing
    /// When true, modules are stored in memory after first load and reused on subsequent loads
    pub cache_modules: bool,
}

impl Default for ModuleLoaderConfig {
    /// Creates a default module loader configuration
    /// 
    /// The default configuration includes common search paths (examples, src, lib, std, current dir)
    /// and enables module caching for performance.
    /// 
    /// # Returns
    ///
    /// A new ModuleLoaderConfig instance with default values
    fn default() -> Self {
        ModuleLoaderConfig {
            import_paths: vec![
                std::path::PathBuf::from("examples"),
                std::path::PathBuf::from("src"),
                std::path::PathBuf::from("lib"),
                std::path::PathBuf::from("std"),
                std::path::PathBuf::from("."),
            ],
            cache_modules: true,
        }
    }
}

impl From<super::CompilerConfig> for ModuleLoaderConfig {
    fn from(config: super::CompilerConfig) -> Self {
        ModuleLoaderConfig {
            import_paths: config.import_paths,
            cache_modules: config.cache_modules,
        }
    }
}

impl ModuleLoader {
    /// Create a new module loader with the given configuration
    /// 
    /// This function initializes a new module loader instance with an empty cache.
    /// The configuration determines where the loader will search for modules and
    /// whether it will cache parsed modules for performance.
    /// 
    /// # Arguments
    /// 
    /// * `config` - The configuration to use for this module loader
    /// 
    /// # Returns
    /// 
    /// A new ModuleLoader instance ready to load Coffee modules
    pub fn new(config: ModuleLoaderConfig) -> Self {
        ModuleLoader {
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Load a module by name or path
    /// 
    /// This method attempts to locate and load a Coffee module by its name or path.
    /// It first checks the module cache to see if the module has already been loaded
    /// and parsed. If not, it searches for the module file in the configured search
    /// paths, reads the file, parses it into an AST, and stores it in the cache.
    ///
    /// Module names like "std.math" are converted to file paths like "std/math.cf"
    /// and searched for in the configured import paths.
    ///
    /// # Arguments
    ///
    /// * `module_path` - The module name or path to load (e.g., "std.math", "utils.string")
    /// 
    /// # Returns
    /// 
    /// * `Ok(ParsedModule)` - Successfully loaded and parsed module
    /// * `Err(Diagnostic)` - Error diagnostic if loading or parsing failed
    pub fn load_module(&self, module_path: &str) -> Result<ParsedModule, Diagnostic> {
        // Check cache first
        if self.config.cache_modules {
            let cache = self.module_cache.read().unwrap();
            if let Some(cached_module) = cache.get(module_path) {
                return Ok(cached_module.clone());
            }
            drop(cache);
        }

        // Try to find the module file in search paths
        let module_file = self.find_module_file(module_path)?;
        
        // Read the file
        let content = fs::read_to_string(&module_file)
            .map_err(|e| Diagnostic::new(
                Severity::Error,
                ErrorKind::ModuleNotFound { module: module_path.to_string() },
                format!("failed to read module file '{}': {}", module_path, e),
            ))?;

        // Parse the module
        let statements = match parser::parse_program(&content) {
            Ok(program) => program.statements,
            Err(parse_errors) => {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidSyntax { 
                        context: format!("failed to parse module '{}'", module_path) 
                    },
                    format!("failed to parse module '{}': {:?}", module_path, parse_errors),
                ));
            }
        };

        // Extract exports (for now, we'll consider all functions as potentially exported)
        let exports = self.extract_exports(&statements);

        let module = ParsedModule {
            statements,
            exports,
            file_path: module_file,
        };

        // Cache the module if caching is enabled
        if self.config.cache_modules {
            let mut cache = self.module_cache.write().unwrap();
            cache.insert(module_path.to_string(), module.clone());
        }

        Ok(module)
    }

    /// Find a module file in the configured search paths
    ///
    /// This internal method converts a module path like "std.math" to a
    /// file path like "std/math.cf" and searches for it in the configured
    /// import paths.
    ///
    /// # Arguments
    ///
    /// * `module_path` - The module path to search for (e.g., "std.math")
    ///
    /// # Returns
    ///
    /// * `Ok(PathBuf)` - Path to the found module file
    /// * `Err(Diagnostic)` - Error diagnostic if the module file was not found
    fn find_module_file(&self, module_path: &str) -> Result<std::path::PathBuf, Diagnostic> {
        // SECURITY: Validate module path before processing
        // Check for null bytes (prevents string injection)
        if module_path.contains('\0') {
            return Err(Diagnostic::new(
                Severity::Error,
                ErrorKind::InvalidImport {
                    import: module_path.to_string(),
                    reason: "null byte detected in module path".to_string(),
                },
                "security error: null byte detected in module path\n  = note: null bytes can be used to bypass path validation",
            ));
        }

        // Check for path traversal attempts in module name
        if module_path.contains("..") {
            return Err(Diagnostic::new(
                Severity::Error,
                ErrorKind::InvalidImport {
                    import: module_path.to_string(),
                    reason: "path traversal detected".to_string(),
                },
                format!("security error: path traversal detected in module '{}'\n  = note: parent directory references (..) are not allowed for security reasons", module_path),
            ));
        }

        // Convert module path to file path (e.g., "std.math" -> "std/math.cf")
        let relative_path = module_path.replace('.', "/");

        for base_path in &self.config.import_paths {
            let mut path = base_path.clone();
            path.push(&relative_path);
            path.set_extension("cf");

            // SECURITY: Check if resolved path is a symbolic link
            if path.is_symlink() {
                return Err(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidImport {
                        import: module_path.to_string(),
                        reason: "symbolic link detected".to_string(),
                    },
                    format!("security error: module file '{}' is a symbolic link\n  = note: symbolic links could bypass security checks", path.display()),
                ));
            }

            // SECURITY: Verify the file is within allowed directories
            // Convert base_path to absolute path for proper comparison
            if let Ok(abs_base) = base_path.canonicalize() {
                if let Ok(abs_path) = path.canonicalize() {
                    // Check if canonical path starts with base path (prevent symlink attacks)
                    if !abs_path.starts_with(&abs_base) {
                        return Err(Diagnostic::new(
                            Severity::Error,
                            ErrorKind::InvalidImport {
                                import: module_path.to_string(),
                                reason: "file outside search path".to_string(),
                            },
                            format!("security error: module '{}' is outside the configured search path\n  = note: resolved path '{}' is not under '{}'",
                                module_path, abs_path.display(), abs_base.display()),
                        ));
                    }

                    // Check for access to sensitive system directories
                    let path_str = abs_path.to_string_lossy();
                    let blocked_prefixes = [
                        "/etc/", "/sys/", "/proc/", "/dev/", "/root/",
                        "/usr/bin/", "/usr/sbin/", "/bin/", "/sbin/"
                    ];

                    for prefix in &blocked_prefixes {
                        if path_str.starts_with(prefix) {
                            return Err(Diagnostic::new(
                                Severity::Error,
                                ErrorKind::InvalidImport {
                                    import: module_path.to_string(),
                                    reason: "access to system directory".to_string(),
                                },
                                format!("security error: attempted to access system directory '{}'\n  = note: access to {} is blocked for security reasons",
                                    module_path, prefix.trim_end_matches('/')),
                            ));
                        }
                    }
                }
            }

            if path.exists() {
                return Ok(path);
            }
        }

        // Build detailed error message showing all searched paths
        let searched_paths: Vec<String> = self.config.import_paths
            .iter()
            .map(|base_path| {
                let mut path = base_path.clone();
                path.push(&relative_path);
                path.set_extension("cf");
                format!("  {}", path.display())
            })
            .collect();

        let error_msg = if searched_paths.is_empty() {
            format!("module '{}' not found (no search paths configured)", module_path)
        } else {
            format!(
                "module '{}' not found\n  = note: searched in the following locations:\n{}\n  = help: use -I <path> to add more search paths",
                module_path,
                searched_paths.join("\n")
            )
        };

        Err(Diagnostic::new(
            Severity::Error,
            ErrorKind::ModuleNotFound { module: module_path.to_string() },
            error_msg,
        ))
    }

    /// Extract export information from statements
    /// 
    /// This internal method scans through the statements of a parsed module
    /// to identify which symbols (functions, classes, enums, etc.) should
    /// be considered as exports. Currently, it considers all functions,
    /// classes, and enums as potentially exported.
    /// 
    /// # Arguments
    /// 
    /// * `statements` - The list of statements from a parsed module
    /// 
    /// # Returns
    /// 
    /// A vector of export names found in the module
    fn extract_exports(&self, statements: &[crate::parser::Statement]) -> Vec<String> {
        let mut exports = Vec::new();

        for stmt in statements {
            match stmt {
                crate::parser::Statement::Function(func) => {
                    // For now, consider all functions as potentially exported
                    // In the future, we could have specific export syntax
                    exports.push(func.name.clone());
                }
                crate::parser::Statement::Class(class) => {
                    exports.push(class.name.clone());
                }
                crate::parser::Statement::Enum(enum_def) => {
                    exports.push(enum_def.name.clone());
                }
                _ => {}
            }
        }

        exports
    }

    /// Check if a module is currently cached
    /// 
    /// This method checks whether a module with the given path is currently
    /// stored in the module cache. This is useful for determining if a module
    /// has already been loaded without triggering the loading process.
    /// 
    /// # Arguments
    /// 
    /// * `module_path` - The module path to check in the cache
    /// 
    /// # Returns
    /// 
    /// * `true` if the module is cached and caching is enabled
    /// * `false` otherwise
    pub fn is_cached(&self, module_path: &str) -> bool {
        if !self.config.cache_modules {
            return false;
        }
        let cache = self.module_cache.read().unwrap();
        cache.contains_key(module_path)
    }

    /// Get the number of currently cached modules
    /// 
    /// This method returns a count of how many modules are currently stored
    /// in the module cache. This can be useful for debugging or monitoring
    /// memory usage related to module caching.
    /// 
    /// # Returns
    /// 
    /// The number of cached modules, or 0 if caching is disabled
    pub fn cached_module_count(&self) -> usize {
        if !self.config.cache_modules {
            return 0;
        }
        let cache = self.module_cache.read().unwrap();
        cache.len()
    }

    /// Clear the module cache
    /// 
    /// This method removes all modules from the cache. This can be useful when
    /// reloading code or when the cache needs to be reset for memory management
    /// purposes.
    /// 
    /// # Arguments
    /// 
    /// None
    pub fn clear_cache(&self) {
        if self.config.cache_modules {
            let mut cache = self.module_cache.write().unwrap();
            cache.clear();
        }
    }
}