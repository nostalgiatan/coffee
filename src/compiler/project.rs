//! Project Configuration Management
//! 
//! This module handles coffee.toml configuration files for project-based compilation.
//! It provides structures and methods for loading, validating, and using project
//! configuration data. The configuration system supports:
//! 
//! - Package metadata (name, version, authors, description)
//! - Dependencies (Coffee packages and C libraries)
//! - Build settings (source directory, output directory, main entry point)
//! - Target configuration (optimization level, target triple)
//! - C library integration (headers, include paths, link flags, static/dynamic linking)
//! 
//! The configuration is loaded from a coffee.toml file in the project root and
//! controls all aspects of the project's compilation and linking process.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

/// Coffee project configuration
/// 
/// This is the main configuration structure for Coffee projects, loaded from
/// a coffee.toml file. It encompasses all aspects of project configuration:
/// package metadata, dependencies, build settings, and target configuration.
/// 
/// The structure uses Serde's `#[serde(default)]` attribute to provide sensible
/// defaults for optional fields, making coffee.toml files more concise.
/// 
/// # Examples
/// 
/// ```toml
/// [package]
/// name = "my_project"
/// version = "1.0.0"
/// 
/// [build]
/// main = "src/main"
/// 
/// [target]
/// opt_level = 2
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    /// Package metadata including name, version, authors, and description
    pub package: Package,
    /// Dependencies including Coffee packages and C libraries
    #[serde(default)]
    pub dependencies: Dependencies,
    /// Build configuration including source/output directories and main entry
    #[serde(default)]
    pub build: BuildConfig,
    /// Target configuration including optimization level and target triple
    #[serde(default)]
    pub target: TargetConfig,
}

/// Package metadata
/// 
/// Contains metadata about the Coffee package including its name, version,
/// authors, and description. This information is used for project identification
/// and dependency management.
/// 
/// The package name must be unique within the project and follow standard
/// naming conventions (alphanumeric characters, underscores, and hyphens only).
/// 
/// # Examples
/// 
/// ```toml
/// [package]
/// name = "hello_world"
/// version = "0.1.0"
/// authors = ["Alice Developer <alice@example.com>"]
/// description = "A simple Hello World program"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Package {
    /// The package name - required and must be unique
    /// Only alphanumeric characters, underscores, and hyphens are allowed
    pub name: String,
    /// Package version following semantic versioning (SemVer)
    /// Defaults to "0.1.0" if not specified
    #[serde(default = "default_version")]
    pub version: String,
    /// List of package authors with optional email addresses
    #[serde(default)]
    pub authors: Vec<String>,
    /// Brief description of the package's purpose and functionality
    #[serde(default)]
    pub description: String,
}

/// Default version for packages when not specified in coffee.toml
fn default_version() -> String {
    "0.1.0".to_string()
}

/// Dependencies section
/// 
/// Manages all dependencies for the Coffee project, including the standard library,
/// Coffee package dependencies, and C library dependencies. This structure provides
/// a comprehensive dependency management system that supports both Coffee packages
/// and C libraries with detailed configuration options.
/// 
/// The dependencies system allows projects to specify version requirements for
/// Coffee packages and detailed linking information for C libraries.
/// 
/// # Examples
/// 
/// ```toml
/// [dependencies]
/// std = "0.2.0"
/// 
/// [dependencies.packages]
/// coffee-utils = "1.0.0"
/// 
/// [dependencies.c_libraries.libm]
/// name = "m"
/// headers = ["math.h"]
/// static_link = true
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Dependencies {
    /// Standard library dependency version
    /// This specifies the version of Coffee's standard library to use
    #[serde(default)]
    pub std: Option<String>,
    /// Coffee package dependencies
    /// Maps package names to their version requirements
    #[serde(default)]
    pub packages: HashMap<String, String>,
    /// C library dependencies
    /// Maps library identifiers to their detailed configuration
    #[serde(default)]
    pub c_libraries: HashMap<String, CLibrary>,
}

/// C library configuration
/// 
/// Specifies detailed configuration for linking with C libraries. This structure
/// allows fine-grained control over how C libraries are integrated into Coffee
/// projects, including static vs dynamic linking, include paths, and linker flags.
/// 
/// The C library integration system is a key feature of Coffee that enables
/// seamless interoperability with the C ecosystem. Libraries can be specified
/// by name with optional headers for function discovery, and various linking
/// options can be configured.
/// 
/// # Examples
/// 
/// ```toml
/// [dependencies.c_libraries.libm]
/// name = "m"
/// headers = ["math.h"]
/// static_link = true
/// static_lib_path = "/usr/lib64/libm.a"
/// 
/// [dependencies.c_libraries.libcurl]
/// name = "curl"
/// headers = ["curl/curl.h"]
/// link_flags = ["-lcurl"]
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CLibrary {
    /// Library name (e.g., "m" for libm, "curl" for libcurl)
    /// This is used to generate linker flags like -lm or -lcurl
    pub name: String,
    /// C header files to search for functions
    /// These are used when generating .cfc files from headers
    #[serde(default)]
    pub headers: Vec<String>,
    /// Custom include paths
    /// Additional paths to search for header files during compilation
    #[serde(default)]
    pub include_paths: Vec<String>,
    /// Linker flags (e.g., "-L/usr/local/lib")
    /// Additional flags to pass to the linker when linking with this library
    #[serde(default)]
    pub link_flags: Vec<String>,
    /// Static linking (true = link statically, false = link dynamically)
    /// Controls whether to link the library statically or dynamically
    #[serde(default)]
    pub static_link: bool,
    /// Static library path (if different from default)
    /// e.g., "/usr/lib/x86_64-linux-gnu/libm.a"
    /// Specifies the exact path to the static library file if non-standard
    #[serde(default)]
    pub static_lib_path: String,
}

/// Build configuration
/// 
/// Defines the build settings for a Coffee project, including the location of
/// source files, output directory, and main entry point. This configuration
/// controls how the compiler processes the project's source files and where
/// it places the build artifacts.
/// 
/// The build configuration provides flexibility in project structure while
/// maintaining sensible defaults that follow common conventions. It also
/// controls whether to generate C header files for FFI (Foreign Function Interface)
/// which enables calling Coffee functions from C code.
/// 
/// # Examples
/// 
/// ```toml
/// [build]
/// main = "src/main"
/// src_dir = "src"
/// target_dir = "build"
/// have_c = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildConfig {
    /// Main entry module (e.g., "src/main")
    /// Specifies the module containing the main function as a path relative to src_dir
    #[serde(default = "default_main")]
    pub main: String,
    /// Output directory
    /// Directory where build artifacts (object files, executables) are placed
    #[serde(default = "default_target_dir")]
    pub target_dir: String,
    /// Source directory
    /// Directory containing Coffee source files (.cf files)
    #[serde(default = "default_src_dir")]
    pub src_dir: String,
    /// Generate C header files for FFI (auto-generate to target/include)
    /// When true, generates .h files for Coffee functions enabling C interoperability
    #[serde(default)]
    pub have_c: bool,
}

impl Default for BuildConfig {
    /// Creates a default BuildConfig with standard values:
    /// - main: "src/main"
    /// - target_dir: "target"
    /// - src_dir: "src"
    /// - have_c: false
    fn default() -> Self {
        BuildConfig {
            main: default_main(),
            target_dir: default_target_dir(),
            src_dir: default_src_dir(),
            have_c: false,
        }
    }
}

/// Default value for the main entry module
/// 
/// Returns "src/main" as the default main entry point, following common
/// project structure conventions where the main module is located at
/// src/main.cf
fn default_main() -> String {
    "src/main".to_string()
}

/// Default value for the target directory
/// 
/// Returns "target" as the default output directory, following common
/// conventions used by build tools like Cargo
fn default_target_dir() -> String {
    "target".to_string()
}

/// Default value for the source directory
/// 
/// Returns "src" as the default source directory, following common
/// project structure conventions
fn default_src_dir() -> String {
    "src".to_string()
}

/// Target configuration
/// 
/// Specifies the compilation target settings including optimization level and
/// target triple for cross-compilation. This configuration controls how the
/// compiler optimizes the generated code and which platform it targets.
/// 
/// The target configuration enables both performance tuning through optimization
/// levels and cross-platform compilation through target triples.
/// 
/// # Examples
/// 
/// ```toml
/// [target]
/// opt_level = 2
/// triple = "x86_64-unknown-linux-gnu"
/// ```
/// 
/// # Optimization Levels
/// 
/// - 0: No optimizations (fastest compilation)
/// - 1: Basic optimizations
/// - 2: Some optimizations
/// - 3: All optimizations (slowest compilation)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TargetConfig {
    /// Optimization level: 0-3
    /// Controls the level of optimization applied during compilation
    #[serde(default = "default_opt_level")]
    pub opt_level: u8,
    /// Target triple (empty string means use system default)
    /// Specifies the target platform in the form of arch-vendor-os (e.g., x86_64-unknown-linux-gnu)
    #[serde(default)]
    pub triple: String,
}

/// Default value for the optimization level
/// 
/// Returns 0 as the default optimization level, which means no optimizations
/// are applied (fastest compilation time)
fn default_opt_level() -> u8 {
    0
}

impl Default for TargetConfig {
    /// Creates a default TargetConfig with standard values:
    /// - opt_level: 0 (no optimizations)
    /// - triple: empty string (use system default)
    fn default() -> Self {
        TargetConfig {
            opt_level: 0,
            triple: String::new(),  // Empty means use system default
        }
    }
}

impl TargetConfig {
    /// Get the target triple to use (None means system default)
    /// 
    /// Returns an Option containing the target triple string if specified,
    /// or None if using the system default target.
    /// 
    /// # Returns
    /// 
    /// * `Some(&str)` - The target triple string if specified in the configuration
    /// * `None` - If no target triple is specified (use system default)
    pub fn get_target_triple(&self) -> Option<&str> {
        if self.triple.is_empty() {
            None
        } else {
            Some(&self.triple)
        }
    }
}

impl ProjectConfig {
    /// Get the target triple to use (None means system default)
    /// 
    /// Delegates to the target configuration to retrieve the target triple.
    /// 
    /// # Returns
    /// 
    /// * `Some(&str)` - The target triple string if specified in the configuration
    /// * `None` - If no target triple is specified (use system default)
    pub fn get_target_triple(&self) -> Option<&str> {
        self.target.get_target_triple()
    }

    /// Load coffee.toml from path
    /// 
    /// Reads and parses a coffee.toml configuration file from the specified path.
    /// The method handles file reading, TOML parsing, and configuration validation.
    /// 
    /// # Arguments
    /// 
    /// * `path` - A reference to a Path object pointing to the coffee.toml file
    /// 
    /// # Returns
    /// 
    /// * `Ok(ProjectConfig)` - Successfully parsed and validated configuration
    /// * `Err(String)` - Error message describing what went wrong during loading
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use std::path::Path;
    /// 
    /// let config = ProjectConfig::from_file(Path::new("coffee.toml"));
    /// match config {
    ///     Ok(cfg) => println!("Loaded config for project: {}", cfg.package.name),
    ///     Err(e) => eprintln!("Failed to load config: {}", e),
    /// }
    /// ```
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("failed to read config: {}", e))?;

        let config: ProjectConfig = toml::from_str(&content)
            .map_err(|e| format!("failed to parse config: {}", e))?;

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate project configuration
    /// 
    /// Performs validation checks on the project configuration to ensure it meets
    /// requirements for successful compilation. Checks include:
    /// 
    /// - Package name is not empty and contains only valid characters
    /// - Main entry point is specified
    /// - Optimization level is within valid range (0-3)
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Configuration is valid
    /// * `Err(String)` - Error message describing validation failure
    fn validate(&self) -> Result<(), String> {
        // Check package name
        if self.package.name.is_empty() {
            return Err("package name cannot be empty".to_string());
        }

        // Check package name format (only alphanumeric, underscore, hyphen)
        if !self.package.name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("invalid package name: '{}'", self.package.name));
        }

        // Check main entry point
        if self.build.main.is_empty() {
            return Err("main entry point cannot be empty".to_string());
        }

        // Check opt level
        if self.target.opt_level > 3 {
            return Err(format!("invalid opt-level: {} (must be 0-3)", self.target.opt_level));
        }

        Ok(())
    }

    /// Get the full path to the main entry file
    /// 
    /// Converts the main module path (e.g., "src/main") to a file path by:
    /// 1. Replacing dots with path separators (e.g., "src.main" becomes "src/main")
    /// 2. Adding the .cf file extension
    /// 
    /// # Returns
    /// 
    /// A PathBuf representing the full path to the main entry file
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // If build.main = "src/main", returns PathBuf containing "src/main.cf"
    /// // If build.main = "lib.utils", returns PathBuf containing "lib/utils.cf"
    /// ```
    pub fn main_file_path(&self) -> PathBuf {
        let main_path = format!("{}.cf", self.build.main.replace('.', "/"));
        PathBuf::from(&main_path)
    }

    /// Get the output directory path
    /// 
    /// Constructs the output directory path based on the target configuration.
    /// The directory name is determined by the optimization level:
    /// - opt_level 0: "debug" profile
    /// - opt_level 3: "release" profile
    /// - opt_level 1 or 2: "opt2" profile
    /// 
    /// # Returns
    /// 
    /// A PathBuf representing the full output directory path
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // If build.target_dir = "target" and opt_level = 0
    /// // Returns PathBuf containing "target/debug"
    /// // If build.target_dir = "build" and opt_level = 3
    /// // Returns PathBuf containing "build/release"
    /// ```
    pub fn output_dir(&self) -> PathBuf {
        let profile_dir = if self.target.opt_level == 0 {
            "debug"
        } else if self.target.opt_level == 3 {
            "release"
        } else {
            "opt2"
        };

        PathBuf::from(&self.build.target_dir)
            .join(profile_dir)
    }

    /// Get the final executable name
    /// 
    /// Constructs the executable name by appending the system-specific executable
    /// suffix (e.g., ".exe" on Windows) to the package name.
    /// 
    /// # Returns
    /// 
    /// A String representing the final executable name
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// // If package.name = "my_app" on Linux, returns "my_app"
    /// // If package.name = "my_app" on Windows, returns "my_app.exe"
    /// ```
    pub fn executable_name(&self) -> String {
        format!("{}{}", self.package.name, std::env::consts::EXE_SUFFIX)
    }

    /// Get all include paths from C libraries
    /// 
    /// Collects all include paths from all C library dependencies in the project.
    /// This is used during compilation to provide the compiler with paths to
    /// C header files needed for FFI.
    /// 
    /// # Returns
    /// 
    /// A Vec<String> containing all include paths from C library dependencies
    pub fn get_c_include_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for lib in self.dependencies.c_libraries.values() {
            paths.extend(lib.include_paths.clone());
        }
        paths
    }

    /// Get all linker flags from C libraries
    /// 
    /// Collects all linker flags from all C library dependencies in the project.
    /// These flags are passed to the linker when linking with C libraries.
    /// 
    /// # Returns
    /// 
    /// A Vec<String> containing all linker flags from C library dependencies
    pub fn get_c_link_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();
        for lib in self.dependencies.c_libraries.values() {
            flags.extend(lib.link_flags.clone());
        }
        flags
    }

    /// Get all C libraries with their link type (static/dynamic)
    /// 
    /// Returns information about how each C library should be linked:
    /// whether statically or dynamically, and the path to the static library
    /// if specified.
    /// 
    /// # Returns
    /// 
    /// A Vec<(String, bool, String)> where each tuple contains:
    /// - library_name: The name of the library
    /// - is_static: Whether to link statically (true) or dynamically (false)
    /// - static_lib_path: The path to the static library file, if specified
    pub fn get_c_library_link_types(&self) -> Vec<(String, bool, String)> {
        self.dependencies.c_libraries
            .values()
            .map(|lib| (
                lib.name.clone(),
                lib.static_link,
                lib.static_lib_path.clone()
            ))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let config_str = r#"
[package]
name = "test"
version = "0.1.0"

[build]
main = "src/main"

[target]
opt_level = 2
"#;

        let config: ProjectConfig = toml::from_str(config_str).unwrap();
        assert_eq!(config.package.name, "test");
        assert_eq!(config.target.opt_level, 2);
    }

    #[test]
    fn test_invalid_package_name() {
        let config_str = r#"
[package]
name = "test@#$"
version = "0.1.0"

[build]
main = "src/main"
"#;

        let config: ProjectConfig = toml::from_str(config_str).unwrap();
        assert!(config.validate().is_err());
    }
}
