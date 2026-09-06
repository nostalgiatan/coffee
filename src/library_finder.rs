//! Library file finder for Coffee compiler
//!
//! Provides unified library searching functionality for both static and dynamic libraries.
//! This module eliminates code duplication between main.rs and linker.rs.

use std::path::Path;

/// Filenames to try for `lib_name`, preferring shared objects over archives.
pub fn library_filenames(lib_name: &str) -> Vec<String> {
    if lib_name.starts_with("lib") {
        vec![format!("{}.so", lib_name), format!("{}.a", lib_name)]
    } else {
        vec![
            format!("lib{}.so", lib_name),
            format!("lib{}.a", lib_name),
        ]
    }
}

/// Find a library file in standard locations
///
/// This function searches for library files (both .so and .a) in a comprehensive
/// set of standard locations, including current directory, ./lib, Termux `$PREFIX`,
/// system paths, and user-specific paths. In each directory, `.so` is tried before `.a`.
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
/// 4. `$PREFIX/lib` (Termux; when `PREFIX` is set)
/// 5. `$PREFIX/usr/lib` (when `PREFIX` is set and that directory exists)
/// 6. `/usr/lib`
/// 7. `/usr/local/lib`
/// 8. `/lib`
/// 9. `/lib64`
/// 10. `~/.local/lib`
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
    find_library_file_in_paths(lib_name, &get_standard_search_paths())
}

/// Search `search_paths` for `lib_name`, trying `.so` then `.a` in each directory.
pub fn find_library_file_in_paths(lib_name: &str, search_paths: &[String]) -> Option<String> {
    let lib_filenames = library_filenames(lib_name);

    for search_path in search_paths {
        for lib_filename in &lib_filenames {
            let lib_path = Path::new(search_path).join(lib_filename);
            if lib_path.exists() {
                return Some(lib_path.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Comma-separated search locations for diagnostics (keeps callers from copying the list).
pub fn searched_locations_note() -> String {
    get_standard_search_paths().join(", ")
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
    let mut seen = std::collections::HashSet::new();

    let mut push_path = |path: String| {
        if path.is_empty() {
            return;
        }
        if seen.insert(path.clone()) {
            search_paths.push(path);
        }
    };

    push_path(".".to_string());
    push_path("./lib".to_string());

    if let Ok(path_env) = std::env::var("PATH") {
        for path_dir in path_env.split(':') {
            push_path(path_dir.to_string());
        }
    }

    if let Ok(prefix) = std::env::var("PREFIX") {
        if !prefix.is_empty() {
            push_path(format!("{}/lib", prefix));
            let prefix_usr_lib = format!("{}/usr/lib", prefix);
            if Path::new(&prefix_usr_lib).is_dir() {
                push_path(prefix_usr_lib);
            }
        }
    }

    push_path("/usr/lib".to_string());
    push_path("/usr/local/lib".to_string());
    push_path("/lib".to_string());
    push_path("/lib64".to_string());

    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            push_path(format!("{}/.local/lib", home));
        }
    }

    search_paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn index_of(paths: &[String], needle: &str) -> Option<usize> {
        paths.iter().position(|p| p == needle)
    }

    #[test]
    fn test_library_filenames_prefer_so_then_a() {
        assert_eq!(library_filenames("m"), vec!["libm.so", "libm.a"]);
        assert_eq!(library_filenames("libc"), vec!["libc.so", "libc.a"]);
        assert_eq!(library_filenames("libfoo"), vec!["libfoo.so", "libfoo.a"]);
    }

    #[test]
    fn test_get_standard_search_paths() {
        let paths = get_standard_search_paths();
        assert!(!paths.is_empty());
        assert_eq!(paths[0], ".");
        assert_eq!(paths[1], "./lib");
        for expected in ["/usr/lib", "/usr/local/lib", "/lib", "/lib64"] {
            assert!(
                paths.contains(&expected.to_string()),
                "missing {expected} in {paths:?}"
            );
        }
    }

    #[test]
    fn test_search_paths_skip_empty_and_dedupe() {
        let paths = get_standard_search_paths();
        assert!(!paths.iter().any(|p| p.is_empty()));
        let mut seen = std::collections::HashSet::new();
        for p in &paths {
            assert!(seen.insert(p.clone()), "duplicate search path: {p}");
        }
    }

    #[test]
    fn test_prefix_lib_inserted_before_usr_lib() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_prefix = std::env::var("PREFIX").ok();
        unsafe {
            std::env::set_var("PREFIX", "/tmp/coffee-test-prefix");
        }

        let paths = get_standard_search_paths();
        let prefix_lib = "/tmp/coffee-test-prefix/lib".to_string();
        let prefix_usr_lib = "/tmp/coffee-test-prefix/usr/lib".to_string();

        assert!(paths.contains(&prefix_lib));
        assert!(
            !paths.contains(&prefix_usr_lib),
            "$PREFIX/usr/lib must be omitted when the directory is absent"
        );

        let i_prefix = index_of(&paths, &prefix_lib).unwrap();
        let i_usr = index_of(&paths, "/usr/lib").unwrap();
        assert!(i_prefix < i_usr, "PREFIX/lib should be searched before /usr/lib");

        match old_prefix {
            Some(v) => unsafe { std::env::set_var("PREFIX", v) },
            None => unsafe { std::env::remove_var("PREFIX") },
        }
    }

    #[test]
    fn test_prefix_usr_lib_when_present() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!(
            "coffee-libfinder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let usr_lib = tmp.join("usr").join("lib");
        fs::create_dir_all(&usr_lib).unwrap();

        let old_prefix = std::env::var("PREFIX").ok();
        unsafe {
            std::env::set_var("PREFIX", tmp.to_str().unwrap());
        }

        let paths = get_standard_search_paths();
        let prefix_lib = tmp.join("lib").to_string_lossy().to_string();
        let prefix_usr_lib = usr_lib.to_string_lossy().to_string();
        assert!(paths.contains(&prefix_lib));
        assert!(paths.contains(&prefix_usr_lib));

        let i_lib = index_of(&paths, &prefix_lib).unwrap();
        let i_usr_prefix = index_of(&paths, &prefix_usr_lib).unwrap();
        let i_usr = index_of(&paths, "/usr/lib").unwrap();
        assert!(i_lib < i_usr_prefix);
        assert!(i_usr_prefix < i_usr);

        match old_prefix {
            Some(v) => unsafe { std::env::set_var("PREFIX", v) },
            None => unsafe { std::env::remove_var("PREFIX") },
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_find_library_file_prefers_so_over_a() {
        let tmp = std::env::temp_dir().join(format!(
            "coffee-libfinder-so-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("libpref.so"), b"").unwrap();
        fs::write(tmp.join("libpref.a"), b"").unwrap();

        let found = find_library_file_in_paths("pref", &[tmp.to_string_lossy().to_string()]);
        assert!(
            found.as_deref().is_some_and(|p| p.ends_with("libpref.so")),
            "expected .so, got {found:?}"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_find_library_file_falls_back_to_archive() {
        let tmp = std::env::temp_dir().join(format!(
            "coffee-libfinder-a-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("libonlya.a"), b"").unwrap();

        let found = find_library_file_in_paths("onlya", &[tmp.to_string_lossy().to_string()]);
        assert!(
            found.as_deref().is_some_and(|p| p.ends_with("libonlya.a")),
            "expected .a, got {found:?}"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_searched_locations_note_uses_live_paths() {
        let note = searched_locations_note();
        let paths = get_standard_search_paths();
        assert_eq!(note, paths.join(", "));
        assert!(note.contains('.'));
        assert!(note.contains("./lib"));
        assert!(note.contains("/usr/lib"));
    }

    #[test]
    fn test_find_library_file_with_prefix() {
        let _ = find_library_file("libc");
    }

    #[test]
    fn test_find_library_file_without_prefix() {
        let _ = find_library_file("m");
    }
}
