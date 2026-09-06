//! Entry Point Management for Project Compilation
//! 
//! This module handles selection and validation of the program entry point in Coffee projects.
//! The entry point manager allows specifying either the default entry from project configuration
//! or a custom entry file, with logic for resolving paths and validating the entry point.
//! 
//! The entry point is crucial for determining where the compiler starts processing the source
//! code. The manager ensures there is exactly one main() function in the entry file, which is
//! required for successful compilation and execution.

use std::path::{Path, PathBuf};
use super::project::ProjectConfig;

/// Entry point manager
/// 
/// Manages the selection of the program's entry point, which can be:
/// - The default entry from project configuration (coffee.toml)
/// - A custom entry file specified by the user
/// 
/// The entry point manager provides functionality to set custom entry points,
/// resolve paths relative to the project structure, validate the entry file,
/// and ensure that there is exactly one main() function in the entry file.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::project::ProjectConfig;
/// use crate::compiler::entry_point::EntryPointManager;
/// use std::path::PathBuf;
/// 
/// // Create from project config
/// let config = ProjectConfig::from_file(std::path::Path::new("coffee.toml")).unwrap();
/// let mut manager = EntryPointManager::from_config(config);
/// 
/// // Set a custom entry point
/// manager.set_custom_entry("src/custom_main.cf");
/// 
/// // Validate the entry point
/// let exists = |path: &PathBuf| std::path::Path::new(path).exists();
/// let count_main = |path: &PathBuf| 1; // Simplified
/// assert!(manager.validate(exists, count_main).is_ok());
/// ```
pub struct EntryPointManager {
    /// Project configuration containing the default main entry point
    config: ProjectConfig,
    /// Custom entry file (overrides config.main)
    custom_entry: Option<PathBuf>,
}

impl EntryPointManager {
    /// Create a new entry point manager from project config
    /// 
    /// Initializes an entry point manager with the given project configuration.
    /// The manager will initially use the default entry point specified in the
    /// configuration (config.build.main).
    /// 
    /// # Arguments
    /// 
    /// * `config` - The project configuration containing the default entry point
    /// 
    /// # Returns
    /// 
    /// A new EntryPointManager instance
    pub fn from_config(config: ProjectConfig) -> Self {
        EntryPointManager {
            config,
            custom_entry: None,
        }
    }

    /// Set a custom entry file (overrides config)
    /// 
    /// Allows specifying a custom entry file that will override the default
    /// entry point from the project configuration. The path can be:
    /// - An absolute path
    /// - A relative path from project root
    /// - A path relative to src_dir (e.g., "src/utils.cf" or "utils.cf")
    /// 
    /// # Arguments
    /// 
    /// * `entry` - A string slice representing the path to the custom entry file
    pub fn set_custom_entry(&mut self, entry: &str) {
        self.custom_entry = Some(PathBuf::from(entry));
    }

    /// Check if a custom entry is set
    /// 
    /// Determines whether a custom entry point has been set, which would
    /// override the default entry point from the project configuration.
    /// 
    /// # Returns
    /// 
    /// * `true` - If a custom entry point has been set
    /// * `false` - If using the default entry point from configuration
    #[cfg(test)]
    pub fn has_custom_entry(&self) -> bool {
        self.custom_entry.is_some()
    }

    /// Get the entry file path
    /// 
    /// Returns the path to the entry file, which will be the custom entry
    /// if set, otherwise the default from the project configuration.
    /// 
    /// # Returns
    /// 
    /// A PathBuf representing the path to the entry file
    pub fn get_entry_file(&self) -> PathBuf {
        if let Some(ref custom) = self.custom_entry {
            self.resolve_custom_entry(custom)
        } else {
            self.config.main_file_path()
        }
    }

    /// Resolve custom entry path relative to project structure
    /// 
    /// This method resolves the path of a custom entry file according to the
    /// project's directory structure. It handles both absolute and relative paths.
    /// 
    /// # Arguments
    /// 
    /// * `custom` - A reference to a Path object representing the custom entry path
    /// 
    /// # Returns
    /// 
    /// A PathBuf with the resolved path to the entry file
    fn resolve_custom_entry(&self, custom: &Path) -> PathBuf {
        // If absolute, use as-is
        if custom.is_absolute() {
            return custom.to_path_buf();
        }

        let custom_str = custom.to_str().unwrap_or("");
        let src_dir = &self.config.build.src_dir;

        // Check if already includes src_dir
        if custom_str.starts_with(&format!("{}/", src_dir)) || custom_str.starts_with(src_dir) {
            self.config.resolve_from_root(custom)
        } else {
            self.config.src_dir_path().join(custom)
        }
    }

    /// Validate that the entry file exists and has exactly one main() statement
    /// 
    /// Performs validation checks on the entry file to ensure:
    /// 1. The file exists
    /// 2. The file contains exactly one main() function
    /// 
    /// This is crucial for correct program execution since the main() function
    /// is the entry point for the compiled program.
    /// 
    /// # Arguments
    /// 
    /// * `source_exists` - A function that checks if a file exists
    /// * `count_main` - A function that counts the number of main() functions in a file
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the entry file is valid
    /// * `Err(String)` - If validation fails with an error message
    pub fn validate(&self, source_exists: impl Fn(&PathBuf) -> bool,
                    count_main: impl Fn(&PathBuf) -> usize) -> Result<(), String> {
        let entry_file = self.get_entry_file();

        if !source_exists(&entry_file) {
            return Err(format!("entry file '{}' does not exist", entry_file.display()));
        }

        let main_count = count_main(&entry_file);
        if main_count == 0 {
            return Err(format!("no main() statement found in entry file '{}'", entry_file.display()));
        }

        if main_count > 1 {
            return Err(format!("entry file '{}' contains {} main() statements (only 1 allowed)",
                              entry_file.display(), main_count));
        }

        Ok(())
    }

    /// Get the entry file name for display
    /// 
    /// Returns a string representation of the entry file path suitable for
    /// display to users in error messages and logs.
    /// 
    /// # Returns
    /// 
    /// A String containing the display name of the entry file
    #[cfg(test)]
    pub fn entry_display_name(&self) -> String {
        let entry = self.get_entry_file();
        format!("{}", entry.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_config() -> ProjectConfig {
        toml::from_str("[package]\nname = \"demo\"\n").unwrap()
    }

    #[test]
    fn test_default_entry() {
        let manager = EntryPointManager::from_config(demo_config());
        assert_eq!(manager.get_entry_file(), PathBuf::from("src/main.cf"));
        assert!(!manager.has_custom_entry());
        assert_eq!(manager.entry_display_name(), "src/main.cf");
    }

    #[test]
    fn test_custom_entry_resolution() {
        let mut manager = EntryPointManager::from_config(demo_config());
        manager.set_custom_entry("utils.cf");
        assert_eq!(manager.get_entry_file(), PathBuf::from("src/utils.cf"));
        manager.set_custom_entry("src/custom.cf");
        assert_eq!(manager.get_entry_file(), PathBuf::from("src/custom.cf"));
    }
}
