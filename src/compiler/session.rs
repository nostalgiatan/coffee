//! Compilation Session Management
//! 
//! This module handles the shared state and configuration for a compilation session.
//! The Session struct serves as a central repository for all compiler components
//! and state that need to be shared across different phases of the compilation process.
//! 
//! The session manages:
//! - Semantic analysis components
//! - Type checking functionality
//! - Diagnostic reporting
//! - Compiler configuration settings
//! - C library import tracking
//! - C function symbol tables from .cfc files
//! 
//! The session uses Arc and RwLock for thread-safe access to shared components,
//! enabling parallel compilation where different threads can access the same
//! compiler state safely.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::path::PathBuf;

use crate::diagnostics::DiagnosticEmitter;
use crate::semantic::SemanticAnalyzer;
use crate::types::{TypeChecker, TypeRegistry};
use crate::c;

/// Compilation session that manages shared state across compilation
/// 
/// The Session structure provides a centralized repository for all shared
/// components and state needed during a compilation session. It uses thread-safe
/// data structures to allow concurrent access by different compilation threads.
/// 
/// The session stores important compiler components such as the semantic analyzer,
/// type checker, and diagnostic emitter. It also tracks C library imports and
/// maintains symbol tables from parsed .cfc files.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::session::Session;
/// 
/// // Create a new compilation session
/// let session = Session::new();
/// 
/// // Use the session components during compilation
/// {
///     let analyzer = session.analyzer.read().unwrap();
///     // Use analyzer for semantic analysis
/// }
/// 
/// // Add C imports during compilation
/// session.c_imports.write().unwrap().push("printf".to_string());
/// ```
pub struct Session {
    /// Semantic analyzer
    /// Thread-safe access to the semantic analysis component
    pub analyzer: Arc<RwLock<SemanticAnalyzer>>,
    /// Type checker
    /// Thread-safe access to the type checking component
    pub type_checker: Arc<RwLock<TypeChecker>>,
    /// Diagnostic emitter
    /// Handles compilation diagnostics and error reporting
    pub emitter: DiagnosticEmitter,
    /// Compiler configuration
    /// Stores compilation settings and options
    pub config: CompilerConfig,
    /// C library imports collected during compilation
    /// Thread-safe list of C functions imported during the session
    pub c_imports: Arc<RwLock<Vec<String>>>,
    /// CFC symbol tables (parsed .cfc files)
    /// Thread-safe map of symbol tables from .cfc files
    pub cfc_symbols: Arc<RwLock<HashMap<String, c::CSymbolTable>>>,
}

impl Session {
    /// Create a new compilation session
    /// 
    /// Creates a new compilation session with default configuration settings.
    /// This is the standard way to initialize a session when no custom
    /// configuration is needed.
    /// 
    /// # Returns
    /// 
    /// A new Session instance with default configuration
    pub fn new() -> Self {
        Self::with_config(CompilerConfig::default())
    }

    /// Create a new compilation session with custom configuration
    /// 
    /// Creates a new compilation session with the specified configuration.
    /// This allows for customizing compiler behavior through the configuration
    /// parameters.
    /// 
    /// # Arguments
    /// 
    /// * `config` - The compiler configuration to use for this session
    /// 
    /// # Returns
    /// 
    /// A new Session instance with the specified configuration
    pub fn with_config(config: CompilerConfig) -> Self {
        // Create the shared type registry with builtin types
        let type_registry = Arc::new(RwLock::new(TypeRegistry::root()));

        let analyzer = Arc::new(RwLock::new(SemanticAnalyzer::new(type_registry.clone())));
        let type_checker = Arc::new(RwLock::new(
            crate::types::TypeChecker::new(type_registry.clone(), crate::types::CheckingMode::Comprehensive)
        ));

        Session {
            analyzer,
            type_checker,
            emitter: DiagnosticEmitter::new(),
            config,
            c_imports: Arc::new(RwLock::new(Vec::new())),
            cfc_symbols: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the list of C library imports
    /// 
    /// Retrieves a copy of the list of C library imports that have been
    /// collected during this compilation session. This list is built as
    /// C functions are imported in source files.
    /// 
    /// # Returns
    /// 
    /// A vector of strings representing the C library imports
    pub fn get_c_imports(&self) -> Vec<String> {
        self.c_imports.read()
            .map(|imports| imports.clone())
            .unwrap_or_default()
    }

    /// Get C function symbol tables from .cfc files
    /// 
    /// Retrieves a copy of the symbol tables parsed from .cfc files during
    /// this compilation session. These tables contain information about C
    /// functions imported from external libraries.
    /// 
    /// # Returns
    /// 
    /// A HashMap mapping C library names to their symbol tables
    pub fn get_cfc_symbols(&self) -> std::collections::HashMap<String, c::CSymbolTable> {
        self.cfc_symbols.read()
            .map(|symbols| symbols.clone())
            .unwrap_or_default()
    }
}

/// Compiler configuration
/// 
/// Contains configuration settings that control various aspects of the
/// compilation process. These settings affect how the compiler resolves
/// imports, handles caching, and manages recursion limits during compilation.
/// 
/// The configuration includes settings for import resolution, search paths,
/// recursion depth limits, module caching, and cross-compilation targeting.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompilerConfig {
    /// Enable import resolution
    /// When true, the compiler will attempt to resolve and process import statements
    pub enable_imports: bool,
    /// Import search paths
    /// List of directories where the compiler looks for imported modules
    pub import_paths: Vec<PathBuf>,
    /// Maximum recursion depth for imports
    /// Prevents infinite recursion in circular import scenarios
    pub max_import_depth: usize,
    /// Cache parsed modules
    /// When true, parsed modules are cached to improve compilation performance
    pub cache_modules: bool,
    /// Target triple for cross-compilation
    /// Specifies the target platform in the format arch-vendor-os (e.g., x86_64-unknown-linux-gnu)
    pub target_triple: Option<String>,
}

impl Default for CompilerConfig {
    /// Creates a default compiler configuration
    /// 
    /// The default configuration includes:
    /// - Import resolution enabled
    /// - Common search paths (current directory, examples, src, lib, std)
    /// - Maximum import depth of 100 levels
    /// - Module caching enabled
    /// - No specific target triple (uses system default)
    /// 
    /// # Returns
    /// 
    /// A CompilerConfig instance with default values
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