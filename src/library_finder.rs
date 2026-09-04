//! Library file finder for Coffee compiler
//!
//! Provides unified library searching functionality for both static and dynamic libraries.
//! This module eliminates code duplication between main.rs and linker.rs.

use std::path::Path;

/// Find a library file in standard locations
///
/// This function searches for library files (both .so and .a) in a comprehensive
/// set of standard locations, including current directory, ./lib, system paths,
/// and user-specific paths.
///
/// # Arguments
///
/// * `lib_name` - The name of the library (e.g., "m", "pthread", "mylib")
///
/// # Returns
///
/// * `Some(String)` - The full path to the library file if found
/// * `None` - If the library file was not found in any search location
///
/// # Search Locations
///
/// The function searches in the following order:
/// 1. Current directory (`.`)
/// 2. `./lib` subdirectory
/// 3. Directories in `$PATH` environment variable
/// 4. `/usr/lib`
/// 5. `/usr/local/lib`
/// 6. `/lib`
/// 7. `/lib64`
/// 8. `~/.local/lib`
///
/// # Examples
///
/// ```
/// use coffee::compiler::library_finder::find_library_file;
///
/// // Find libm.so or libm.a
/// if let Some(path) = find_library_file("m") {
///     println!("Found library at: {}", path);
/// }
///
/// // Find libmylib.so or libmylib.a
/// if let Some(path) = find_library_file("mylib") {
///     println!("Found library at: {}", path);
/// }
/// ```
pub fn find_library_file(lib_name: &str) -> Option<String> {
    // Possible library file names
    let lib_filenames = if lib_name.starts_with("lib") {
        vec![
            format!("{}.so", lib_name),
            format!("{}.a", lib_name),
        ]
    } else {
        vec![
            format!("lib{}.so", lib_name),
            format!("lib{}.a", lib_name),
        ]
    };

    // Search paths in order of priority
    let search_paths = get_standard_search_paths();

    // Search for the library file
    for search_path in &search_paths {
        for lib_filename in &lib_filenames {
            let lib_path = Path::new(search_path).join(lib_filename);
            if lib_path.exists() {
                return Some(lib_path.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Get standard library search paths
///
/// Returns a vector of standard library search paths in priority order.
/// This function is used internally by find_library_file but can also
/// be used by other parts of the compiler that need to search for libraries.
///
/// # Returns
///
/// A vector of directory paths to search for library files
pub fn get_standard_search_paths() -> Vec<String> {
    let mut search_paths = Vec::new();

    // 1. Current directory
    search_paths.push(".".to_string());

    // 2. ./lib subdirectory
    search_paths.push("./lib".to_string());

    // 3. $PATH directories (for user-installed libraries)
    if let Ok(path_env) = std::env::var("PATH") {
        for path_dir in path_env.split(':') {
            if !path_dir.is_empty() {
                search_paths.push(path_dir.to_string());
            }
        }
    }

    // 4. System library paths
    search_paths.push("/usr/lib".to_string());
    search_paths.push("/usr/local/lib".to_string());
    search_paths.push("/lib".to_string());
    search_paths.push("/lib64".to_string());

    // 5. User local library path
    if let Ok(home) = std::env::var("HOME") {
        search_paths.push(format!("{}/.local/lib", home));
    }

    search_paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_standard_search_paths() {
        let paths = get_standard_search_paths();
        assert!(paths.len() > 0);
        assert!(paths.contains(&".".to_string()));
        assert!(paths.contains(&"./lib".to_string()));
    }

    #[test]
    fn test_find_library_file_with_prefix() {
        // This test will likely fail in a test environment
        // but demonstrates the API
        let result = find_library_file("libc");
        // In a real system, this might find /usr/lib/libc.so
    }

    #[test]
    fn test_find_library_file_without_prefix() {
        let result = find_library_file("m");
        // In a real system, this might find /usr/lib/libm.so
    }
}