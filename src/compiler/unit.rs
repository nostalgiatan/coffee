//! Compilation Unit Management
//! 
//! This module manages compilation units in Coffee projects. Each .cf source file
//! represents an independent compilation unit that can be compiled separately
//! and then linked together to form the final executable.
//! 
//! The module provides structures and functionality for:
//! 
//! - Tracking compilation units and their metadata
//! - Managing dependencies between units
//! - Calculating and caching file hashes for incremental compilation
//! - Tracking compilation status of each unit
//! - Determining which units need recompilation based on changes
//! 
//! The compilation unit system enables efficient incremental compilation by
//! only recompiling units that have changed or whose dependencies have changed.

use std::collections::HashSet;
use std::path::PathBuf;
use std::fs;

/// Compilation status
/// 
/// Represents the current compilation state of a compilation unit. This enum
/// tracks the progress of each unit through the compilation process.
/// 
/// # Variants
/// 
/// * `NotCompiled` - The unit has not yet been compiled
/// * `Compiling` - The unit is currently being compiled
/// * `Compiled(PathBuf)` - The unit has been successfully compiled, with the path to the object file
/// * `Failed(String)` - The compilation of the unit failed, with an error message
#[derive(Debug, Clone, PartialEq)]
pub enum CompilationStatus {
    /// Not yet compiled - Initial state of a compilation unit
    NotCompiled,
    /// Currently compiling - The unit is in the process of being compiled
    #[allow(dead_code)]
    Compiling,
    /// Successfully compiled (path to .o file) - The unit has been successfully compiled
    Compiled(PathBuf),
    /// Compilation failed - The compilation failed with an error message
    #[allow(dead_code)]
    Failed(String),
}

/// A single compilation unit (one .cf file)
/// 
/// Represents a single Coffee source file (.cf) as a compilation unit. Each
/// compilation unit contains metadata about the source file, its dependencies,
/// and its compilation state. This structure is fundamental to Coffee's
/// incremental compilation system.
/// 
/// Compilation units allow for modular compilation where individual files
/// can be compiled separately and only recompiled when necessary, improving
/// build performance for large projects.
/// 
/// # Examples
/// 
/// ```rust
/// use std::path::PathBuf;
/// use crate::compiler::unit::{CompilationUnit, CompilationStatus};
/// 
/// let mut unit = CompilationUnit::new(
///     "main",
///     PathBuf::from("src/main.cf"),
///     PathBuf::from("target/debug/main.o")
/// );
/// 
/// // Add dependencies
/// unit.add_dependency("utils");
/// unit.add_dependency("io");
/// 
/// // Calculate hash for incremental compilation
/// unit.calculate_hash().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct CompilationUnit {
    /// Module name (e.g., "main", "lib.utils")
    /// This represents the logical name of the module in the Coffee project
    pub name: String,
    /// Source file path
    /// The path to the .cf source file for this unit
    pub source: PathBuf,
    /// Object file path (output)
    /// The path where the compiled object file (.o) will be stored
    pub object: PathBuf,
    /// Imported modules (dependencies)
    /// List of module names that this unit depends on
    pub dependencies: Vec<String>,
    /// Source file content hash
    /// SHA-256 hash of the source file content for incremental compilation
    pub hash: String,
    /// Compilation status
    /// Current state of the compilation process for this unit
    pub status: CompilationStatus,
}

impl CompilationUnit {
    /// Create a new compilation unit
    /// 
    /// Creates a new compilation unit with the given name, source file path,
    /// and object file path. The unit is initialized with an empty dependency
    /// list, no hash value, and a "NotCompiled" status.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The module name as a string or string-like value
    /// * `source` - Path to the source file (.cf)
    /// * `object` - Path where the compiled object file (.o) will be stored
    /// 
    /// # Returns
    /// 
    /// A new CompilationUnit instance
    pub fn new(name: impl Into<String>, source: PathBuf, object: PathBuf) -> Self {
        let name = name.into();
        CompilationUnit {
            name: name.clone(),
            source,
            object,
            dependencies: Vec::new(),
            hash: String::new(),
            status: CompilationStatus::NotCompiled,
        }
    }

    /// Calculate hash of the source file
    /// 
    /// Computes a SHA-256 hash of the source file content. This hash is used
    /// for incremental compilation to determine if the source file has changed
    /// and needs to be recompiled.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully calculated and stored the hash
    /// * `Err(String)` - Error message if file reading or hashing failed
    pub fn calculate_hash(&mut self) -> Result<(), String> {
        use std::io::Read;

        let mut file = fs::File::open(&self.source)
            .map_err(|e| format!("failed to open source file: {}", e))?;

        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| format!("failed to read source file: {}", e))?;

        // Use SHA-256 for content hashing
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let hash = hasher.finalize();

        self.hash = format!("{:x}", hash);
        Ok(())
    }

    /// Check if this unit needs recompilation
    /// 
    /// Determines whether this compilation unit needs to be recompiled based on:
    /// - Its current compilation status
    /// - Whether the object file exists
    /// - Whether the source file hash matches the cached hash
    /// 
    /// # Returns
    /// 
    /// * `true` - The unit needs recompilation
    /// * `false` - The unit does not need recompilation
    #[allow(dead_code)]
    pub fn needs_recompilation(&self) -> bool {
        match &self.status {
            CompilationStatus::NotCompiled => true,
            CompilationStatus::Compiling => false,
            CompilationStatus::Failed(_) => true,
            CompilationStatus::Compiled(obj_path) => {
                // Check if object file exists
                if !obj_path.exists() {
                    return true;
                }

                // Check hash
                if self.hash.is_empty() {
                    return true;
                }

                // Compare with cached hash if available
                if let Ok(cached_hash) = Self::load_cached_hash(&self.object) {
                    cached_hash != self.hash
                } else {
                    true
                }
            }
        }
    }

    /// Save hash to cache file
    /// 
    /// Stores the calculated hash in a cache file with the same name as the
    /// object file but with a .hash extension. This allows for incremental
    /// compilation by comparing the current hash with the cached one.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully saved the hash to cache
    /// * `Err(String)` - Error message if writing to cache failed
    pub fn save_hash_cache(&self) -> Result<(), String> {
        let cache_file = self.object.with_extension("hash");
        fs::write(&cache_file, &self.hash)
            .map_err(|e| format!("failed to write hash cache: {}", e))?;
        Ok(())
    }

    /// Load cached hash
    /// 
    /// Reads the cached hash from a .hash file associated with the object file.
    /// This is used to determine if the source file has changed since the last
    /// compilation.
    /// 
    /// # Arguments
    /// 
    /// * `object_path` - Path to the object file whose hash cache should be loaded
    /// 
    /// # Returns
    /// 
    /// * `Ok(String)` - The cached hash value
    /// * `Err(String)` - Error message if reading the cache file failed
    fn load_cached_hash(object_path: &PathBuf) -> Result<String, String> {
        let cache_file = object_path.with_extension("hash");
        fs::read_to_string(&cache_file)
            .map_err(|e| format!("failed to read hash cache: {}", e))
    }

    /// Check if the cached hash matches the current hash
    /// Returns Ok(true) if cache matches, Ok(false) if cache differs or object doesn't exist
    /// 
    /// Compares the current source file hash with the cached hash to determine
    /// if the file has changed since the last compilation. This is a key part
    /// of the incremental compilation system.
    /// 
    /// # Returns
    /// 
    /// * `Ok(true)` - The cached hash matches the current hash (no recompilation needed)
    /// * `Ok(false)` - The hashes differ or no cache exists (recompilation needed)
    /// * `Err(String)` - Error occurred while checking the cache
    pub fn check_hash_cache(&self) -> Result<bool, String> {
        // If object file doesn't exist, need to compile
        if !self.object.exists() {
            return Ok(false);
        }

        // Try to load cached hash
        match Self::load_cached_hash(&self.object) {
            Ok(cached_hash) => Ok(cached_hash == self.hash),
            Err(_) => Ok(false),  // No cache found, need to compile
        }
    }

    /// Add a dependency
    /// 
    /// Adds a module name to the list of dependencies for this compilation unit.
    /// The dependency will only be added if it's not already in the list.
    /// 
    /// # Arguments
    /// 
    /// * `dep` - The module name to add as a dependency (as a string or string-like)
    pub fn add_dependency(&mut self, dep: impl Into<String>) {
        let dep = dep.into();
        if !self.dependencies.contains(&dep) {
            self.dependencies.push(dep);
        }
    }

    /// Get all unique dependencies (including transitive)
    /// 
    /// Collects all direct and transitive dependencies of this compilation unit.
    /// This is useful for determining the full set of modules that this unit
    /// depends on, including dependencies of its dependencies.
    /// 
    /// # Arguments
    /// 
    /// * `all_units` - A reference to a HashMap containing all compilation units in the project
    /// 
    /// # Returns
    /// 
    /// A HashSet containing all unique module names this unit depends on
    #[allow(dead_code)]
    pub fn all_dependencies(&self, all_units: &std::collections::HashMap<String, CompilationUnit>) -> HashSet<String> {
        let mut deps = HashSet::new();
        self.collect_dependencies(all_units, &mut deps);
        deps
    }

    /// Recursively collect all dependencies
    fn collect_dependencies(&self, all_units: &std::collections::HashMap<String, CompilationUnit>, deps: &mut HashSet<String>) {
        for dep in &self.dependencies {
            if deps.insert(dep.clone()) {
                if let Some(unit) = all_units.get(dep) {
                    unit.collect_dependencies(all_units, deps);
                }
            }
        }
    }
}

/// Collection of compilation units
/// 
/// Manages a collection of CompilationUnit instances for a Coffee project.
/// This structure provides methods for accessing, managing, and querying
/// compilation units as a group, which is essential for project-wide operations
/// like dependency analysis and compilation scheduling.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::unit::{CompilationUnits, CompilationUnit};
/// use std::path::PathBuf;
/// 
/// let mut units = CompilationUnits::new();
/// 
/// // Add a compilation unit
/// let unit = CompilationUnit::new(
///     "main",
///     PathBuf::from("src/main.cf"),
///     PathBuf::from("target/debug/main.o")
/// );
/// units.add(unit);
/// 
/// // Get a unit by name
/// if let Some(main_unit) = units.get("main") {
///     println!("Found unit: {}", main_unit.name);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CompilationUnits {
    units: std::collections::HashMap<String, CompilationUnit>,
}

impl CompilationUnits {
    /// Create a new empty collection of compilation units
    /// 
    /// # Returns
    /// 
    /// A new CompilationUnits instance with no units
    pub fn new() -> Self {
        CompilationUnits {
            units: std::collections::HashMap::new(),
        }
    }

    /// Add a compilation unit
    /// 
    /// Adds a compilation unit to the collection, keyed by its module name.
    /// If a unit with the same name already exists, it will be replaced.
    /// 
    /// # Arguments
    /// 
    /// * `unit` - The CompilationUnit to add to the collection
    pub fn add(&mut self, unit: CompilationUnit) {
        let name = unit.name.clone();
        self.units.insert(name, unit);
    }

    /// Get a compilation unit by name
    /// 
    /// Retrieves a reference to a compilation unit by its module name.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The module name of the unit to retrieve
    /// 
    /// # Returns
    /// 
    /// * `Some(&CompilationUnit)` - Reference to the unit if found
    /// * `None` - If no unit with the given name exists
    #[allow(dead_code)]
    pub fn get(&self, name: &str) -> Option<&CompilationUnit> {
        self.units.get(name)
    }

    /// Get mutable reference to a compilation unit
    /// 
    /// Retrieves a mutable reference to a compilation unit by its module name.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The module name of the unit to retrieve
    /// 
    /// # Returns
    /// 
    /// * `Some(&mut CompilationUnit)` - Mutable reference to the unit if found
    /// * `None` - If no unit with the given name exists
    #[allow(dead_code)]
    pub fn get_mut(&mut self, name: &str) -> Option<&mut CompilationUnit> {
        self.units.get_mut(name)
    }

    /// Get all units
    /// 
    /// Provides access to all compilation units in the collection.
    /// 
    /// # Returns
    /// 
    /// A reference to the internal HashMap containing all units
    pub fn all(&self) -> &std::collections::HashMap<String, CompilationUnit> {
        &self.units
    }

    /// Get all units as mutable
    /// 
    /// Provides mutable access to all compilation units in the collection.
    /// 
    /// # Returns
    /// 
    /// A mutable reference to the internal HashMap containing all units
    #[allow(dead_code)]
    pub fn all_mut(&mut self) -> &mut std::collections::HashMap<String, CompilationUnit> {
        &mut self.units
    }

    /// Find units that need recompilation
    /// 
    /// Identifies all compilation units that require recompilation based on
    /// their current status and hash comparison.
    /// 
    /// # Returns
    /// 
    /// A vector of module names that need to be recompiled
    #[allow(dead_code)]
    pub fn find_dirty(&self) -> Vec<String> {
        self.units.iter()
            .filter(|(_, unit)| unit.needs_recompilation())
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get all object files (in dependency order)
    /// 
    /// Collects paths to all object files in the order they should be linked,
    /// respecting dependency relationships between compilation units.
    /// 
    /// # Returns
    /// 
    /// A vector of paths to object files in the correct linking order
    #[allow(dead_code)]
    pub fn object_files(&self) -> Vec<PathBuf> {
        let mut objects = Vec::new();
        let mut visited = std::collections::HashSet::new();

        for (_, unit) in &self.units {
            self.collect_objects(&unit.name, &mut objects, &mut visited);
        }

        objects
    }

    /// Recursively collect object files in dependency order
    fn collect_objects(&self, name: &str, objects: &mut Vec<PathBuf>, visited: &mut HashSet<String>) {
        if let Some(unit) = self.units.get(name) {
            // First collect dependencies
            for dep in &unit.dependencies {
                if visited.insert(dep.clone()) {
                    self.collect_objects(dep, objects, visited);
                }
            }

            // Then collect this unit's object
            if let CompilationStatus::Compiled(obj_path) = &unit.status {
                if visited.insert(name.to_string()) {
                    objects.push(obj_path.clone());
                }
            }
        }
    }
}

impl Default for CompilationUnits {
    /// Creates a default (empty) CompilationUnits collection
    /// 
    /// # Returns
    /// 
    /// A new CompilationUnits instance with no units
    fn default() -> Self {
        Self::new()
    }
}
