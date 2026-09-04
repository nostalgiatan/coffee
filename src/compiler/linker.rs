//! Object File Linking for Coffee Projects
//! 
//! This module handles linking of compiled object files into executables.
//! The linker provides functionality for combining object files and external
//! libraries into a final executable binary using the system linker (clang).
//! 
//! The linker supports both static and dynamic linking of C libraries, with
//! options for custom library paths, linker flags, and static library paths.
//! It manages the entire linking process, including:
//! 
//! - Combining multiple object files into a single executable
//! - Linking with system and custom C libraries
//! - Supporting both static and dynamic linking modes
//! - Handling library search paths
//! - Managing linker flags and options

use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;

use crate::library_finder::get_standard_search_paths;

/// Link options for C libraries
/// 
/// Contains configuration options for linking with C libraries during the
/// executable generation process. This structure allows specifying various
/// aspects of the linking process, including which libraries to link,
/// additional linker flags, library search paths, and static vs dynamic
/// linking preferences.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::linker::LinkOptions;
/// 
/// let mut options = LinkOptions::default();
/// options.libraries.push("m".to_string());  // Link with libm
/// options.libraries.push("curl".to_string());  // Link with libcurl
/// options.lib_paths.push("/usr/local/lib".to_string());
/// options.link_flags.push("-pthread".to_string());
/// ```
#[derive(Debug, Clone, Default)]
pub struct LinkOptions {
    /// C library names to link (e.g., ["m", "curl"])
    /// These will be linked with -l<name> flags
    pub libraries: Vec<String>,
    /// Additional linker flags (e.g., "-L/usr/local/lib")
    /// These flags are passed directly to the linker
    pub link_flags: Vec<String>,
    /// Custom library search paths
    /// These paths are added with -L flags for library lookup
    pub lib_paths: Vec<String>,
    /// Static libraries with their paths: [(lib_name, static_path)]
    /// For libraries with specific static paths
    pub static_libs: Vec<(String, String)>,
    /// Force static linking for all libraries
    /// When true, all libraries will be linked statically if possible
    pub force_static: bool,
}

/// Object file linker
/// 
/// Links compiled object files into executable binaries using clang. The linker
/// handles the final stage of the compilation process, combining multiple object
/// files and external libraries into a single executable program.
/// 
/// The linker supports various linking options including static and dynamic
/// library linking, custom search paths, and additional linker flags. It uses
/// the system's clang compiler as the linker backend.
/// 
/// # Examples
/// 
/// ```rust
/// use std::path::PathBuf;
/// use crate::compiler::linker::{Linker, LinkOptions};
/// 
/// let output_path = PathBuf::from("target/my_program");
/// let options = LinkOptions::default();
/// let linker = Linker::with_options(output_path, options);
/// 
/// let object_files = vec![PathBuf::from("obj1.o"), PathBuf::from("obj2.o")];
/// match linker.link(&object_files) {
///     Ok(()) => println!("Linking successful!"),
///     Err(e) => eprintln!("Linking failed: {}", e),
/// }
/// ```
pub struct Linker {
    /// Output path for the executable
    /// The path where the final executable will be created
    output_path: PathBuf,
    /// Link options
    /// Configuration for C library linking and other linker options
    options: LinkOptions,
}

impl Linker {
    /// Create a new linker with options
    /// 
    /// Initializes a linker instance with the specified output path and linking options.
    /// This is the primary constructor for creating a linker that can handle custom
    /// library linking requirements.
    /// 
    /// # Arguments
    /// 
    /// * `output_path` - The path where the final executable will be created (as a string or path-like)
    /// * `options` - Linking options including library specifications and flags
    /// 
    /// # Returns
    /// 
    /// A new Linker instance configured with the specified options
    pub fn with_options(output_path: impl Into<PathBuf>, options: LinkOptions) -> Self {
        Linker {
            output_path: output_path.into(),
            options,
        }
    }

    /// Link object files into an executable
    /// 
    /// Performs the actual linking process, combining the provided object files
    /// with specified libraries to create an executable at the configured output path.
    /// This method executes the clang linker with appropriate flags based on the
    /// linking options.
    /// 
    /// # Arguments
    /// 
    /// * `object_files` - A slice of PathBuf references to the object files to link
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully created the executable
    /// * `Err(String)` - Error message if linking failed
    pub fn link(&self, object_files: &[PathBuf]) -> Result<(), String> {
        // Create output directory if needed
        if let Some(parent) = self.output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output directory: {}", e))?;
        }

        // Link using clang
        let mut cmd = Command::new("clang");

        // Add object files
        for obj in object_files {
            cmd.arg(obj.to_str().unwrap());
        }

        // Add static linking flag if force_static is enabled
        if self.options.force_static {
            cmd.arg("-static");
        }

        // Add library search paths
        for lib_path in &self.options.lib_paths {
            cmd.arg(format!("-L{}", lib_path));
        }

        // Add linker flags
        for flag in &self.options.link_flags {
            cmd.arg(flag);
        }

        // Add static libraries first (specified paths)
        for (lib_name, static_path) in &self.options.static_libs {
            if !static_path.is_empty() {
                // Use explicit static library path
                cmd.arg(static_path);
            } else {
                // Try to find static library in standard locations
                let static_lib = format!("lib{}.a", lib_name);
                let found = self.find_static_library(&static_lib);
                if let Some(path) = found {
                    cmd.arg(path);
                } else {
                    // Fallback to -Bstatic -l{name} -Bdynamic
                    cmd.arg("-Wl,-Bstatic");
                    cmd.arg(format!("-l{}", lib_name));
                    cmd.arg("-Wl,-Bdynamic");
                }
            }
        }

        // Add dynamic libraries
        for lib in &self.options.libraries {
            // Check if this library is in static_libs (already handled)
            let is_static = self.options.static_libs.iter()
                .any(|(name, _)| name == lib);

            if !is_static {
                cmd.arg(format!("-l{}", lib));
            }
        }

        // Add rpath for runtime library search (only for dynamic linking)
        if !self.options.force_static && !self.options.libraries.is_empty() {
            cmd.arg("-Wl,-rpath,.");
            cmd.arg("-Wl,-rpath,$ORIGIN");
        }

        // Output file
        cmd.arg("-o").arg(&self.output_path);

        match cmd.output() {
            Ok(output) => {
                if !output.status.success() {
                    return Err(format!("linking failed:\n{}", String::from_utf8_lossy(&output.stderr)));
                }
            }
            Err(e) => {
                return Err(format!("failed to run linker: {}", e));
            }
        }

        Ok(())
    }

    /// Find static library in standard locations
    /// 
    /// Searches for a static library file in common system library directories
    /// as well as custom paths specified in the linking options. This is used
    /// to locate static library files when only the library name is provided.
    /// 
    /// # Arguments
    /// 
    /// * `lib_name` - The name of the library file to search for (e.g., "libm.a")
    /// 
    /// # Returns
    /// 
    /// * `Some(String)` - The full path to the library if found
    /// * `None` - If the library was not found in any of the search locations
    fn find_static_library(&self, lib_name: &str) -> Option<String> {
        let search_paths = get_standard_search_paths();

        for base_path in search_paths {
            let full_path = format!("{}/{}", base_path, lib_name);
            if PathBuf::from(&full_path).exists() {
                return Some(full_path);
            }
        }

        // Also try lib_paths from options
        for lib_path in &self.options.lib_paths {
            let full_path = format!("{}/{}", lib_path, lib_name);
            if PathBuf::from(&full_path).exists() {
                return Some(full_path);
            }
        }

        None
    }

    /// Get the output path
    /// 
    /// Provides access to the configured output path where the linker will
    /// create the executable.
    /// 
    /// # Returns
    /// 
    /// A reference to the Path representing the output path
    #[allow(dead_code)]
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linker_creation() {
        let linker = Linker::with_options("test/output", LinkOptions::default());
        assert_eq!(linker.output_path(), PathBuf::from("test/output"));
    }
}
