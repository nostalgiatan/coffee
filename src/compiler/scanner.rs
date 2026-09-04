//! Source File Scanning for Coffee Projects
//! 
//! This module provides functionality for discovering and scanning Coffee source files
//! (.cf extension) in project directories. The source scanner is responsible for
//! recursively traversing the project's source directory to find all Coffee source
//! files that need to be compiled.
//! 
//! The scanner handles:
//! - Recursive directory traversal
//! - File extension filtering (.cf files only)
//! - Error handling for inaccessible directories
//! - Proper path resolution for discovered files
//! 
//! It is typically used early in the compilation process to determine the set of
//! source files that need to be processed by the compilation pipeline.

use std::path::{Path, PathBuf};
use std::fs;

/// Source file scanner
/// 
/// Recursively scans directories for Coffee source files (.cf extension).
/// The scanner is responsible for discovering all Coffee source files in
/// a project that need to be compiled.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::scanner::SourceScanner;
/// 
/// let scanner = SourceScanner::new("src");
/// match scanner.scan() {
///     Ok(files) => {
///         println!("Found {} Coffee source files", files.len());
///         for file in files {
///             println!("  - {}", file.display());
///         }
///     }
///     Err(e) => eprintln!("Scan failed: {}", e),
/// }
/// ```
pub struct SourceScanner {
    /// Source directory to scan
    /// The root directory where the recursive scan begins
    src_dir: PathBuf,
}

impl SourceScanner {
    /// Create a new source scanner
    /// 
    /// Initializes a scanner to look for Coffee source files in the specified
    /// source directory.
    /// 
    /// # Arguments
    /// 
    /// * `src_dir` - The source directory path to scan (as a string or path-like)
    /// 
    /// # Returns
    /// 
    /// A new SourceScanner instance
    pub fn new(src_dir: impl Into<PathBuf>) -> Self {
        SourceScanner {
            src_dir: src_dir.into(),
        }
    }

    /// Scan for all .cf files recursively
    /// 
    /// Recursively traverses the source directory to find all Coffee source files
    /// with the .cf extension.
    /// 
    /// # Returns
    /// 
    /// * `Ok(Vec<PathBuf>)` - Vector containing paths to all discovered .cf files
    /// * `Err(String)` - Error message if scanning fails (e.g., directory doesn't exist)
    pub fn scan(&self) -> Result<Vec<PathBuf>, String> {
        let src_dir = Path::new(&self.src_dir);

        if !src_dir.exists() {
            return Err(format!("source directory '{}' does not exist", src_dir.display()));
        }

        let mut sources = Vec::new();
        self.collect_cf_files(src_dir, &mut sources)?;

        Ok(sources)
    }

    /// Recursively collect .cf files from directory
    /// 
    /// Internal method that recursively traverses a directory and its subdirectories
    /// to find all files with the .cf extension.
    /// 
    /// # Arguments
    /// 
    /// * `dir` - The directory path to scan
    /// * `sources` - Mutable vector to collect discovered file paths
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully collected files
    /// * `Err(String)` - Error message if directory reading fails
    fn collect_cf_files(&self, dir: &Path, sources: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries = fs::read_dir(dir)
            .map_err(|e| format!("failed to read directory '{}': {}", dir.display(), e))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| format!("failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                self.collect_cf_files(&path, sources)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("cf") {
                sources.push(path);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_creation() {
        let scanner = SourceScanner::new("src");
        assert_eq!(scanner.src_dir, PathBuf::from("src"));
    }
}
