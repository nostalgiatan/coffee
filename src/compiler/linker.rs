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

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::fs;
use std::io;
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
    /// Coffee `-O0`..`-O3` forwarded to the clang driver
    pub opt_level: u8,
    /// Coffee `--target` triple forwarded as clang `--target=<triple>`
    pub target_triple: Option<String>,
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

        let mut options = self.options.clone();
        for (lib_name, static_path) in options.static_libs.iter_mut() {
            if static_path.is_empty() {
                let static_lib = format!("lib{}.a", lib_name);
                if let Some(found) = self.find_static_library(&static_lib) {
                    *static_path = found;
                }
            }
        }

        let mut cmd = Command::new("clang");
        cmd.args(clang_link_args(object_files, &self.output_path, &options));

        match cmd.output() {
            Ok(output) => {
                if !output.status.success() {
                    return Err(format_clang_link_failure(
                        &String::from_utf8_lossy(&output.stderr),
                    ));
                }
            }
            Err(e) => return Err(format_clang_spawn_error(&e)),
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

    /// Get the output path (unit tests)
    #[cfg(test)]
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }
}

/// Strip a leading `lib` prefix so `libc` / `m` / `curl` share one `-l` name.
pub fn lib_short_name(lib_name: &str) -> String {
    let lib_name = lib_name.trim();
    if let Some(rest) = lib_name.strip_prefix("lib") {
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
    lib_name.to_string()
}

/// Library name from a `library:symbol` C import (or the whole string if no colon).
pub fn c_import_library_name(c_import: &str) -> &str {
    c_import.split(':').next().unwrap_or(c_import)
}

/// Single-file `--bin` / CLI link options from C imports and `--static` / `--static-lib`.
pub fn link_options_from_c_imports(
    c_imports: &[String],
    force_static: bool,
    static_lib_names: &[String],
    opt_level: u8,
    target_triple: Option<String>,
) -> LinkOptions {
    let mut options = LinkOptions {
        force_static,
        opt_level,
        target_triple,
        ..Default::default()
    };

    let static_shorts: Vec<String> = static_lib_names
        .iter()
        .map(|s| lib_short_name(s))
        .filter(|s| !s.is_empty())
        .collect();

    let mut seen = HashSet::new();
    for c_import in c_imports {
        let short_name = lib_short_name(c_import_library_name(c_import));
        if short_name.is_empty() || !seen.insert(short_name.clone()) {
            continue;
        }
        if static_shorts.iter().any(|s| s == &short_name) {
            options.static_libs.push((short_name, String::new()));
        } else {
            options.libraries.push(short_name);
        }
    }

    for short_name in static_shorts {
        if options.static_libs.iter().any(|(n, _)| n == &short_name) {
            continue;
        }
        options.static_libs.push((short_name, String::new()));
    }

    options
}

/// User-facing clang link failure (stderr plus missing-library help).
pub fn format_clang_link_failure(stderr: &str) -> String {
    let mut msg = format!("linking failed:\n{}", stderr);
    if clang_stderr_is_missing_library(stderr) {
        msg.push_str("\n  = help: library file not found. Ensure:\n");
        msg.push_str("    1. The library file (.so or .a) exists in the current directory or specified -L path\n");
        msg.push_str("    2. The library name in the import statement matches the file (e.g., 'use x in hello of c' requires libhello.so)\n");
        if stderr.contains("-lc") || stderr.contains("libc") {
            msg.push_str("  = note: unable to find library -lc is expected for fully static links on some Android toolchains\n");
        }
    }
    msg
}

/// `-O0`..`-O3` and optional `--target=<triple>` for the clang driver.
pub fn clang_driver_flags(opt_level: u8, target_triple: Option<&str>) -> Vec<String> {
    let mut args = vec![format!("-O{}", opt_level.min(3))];
    if let Some(triple) = target_triple {
        let triple = triple.trim();
        if !triple.is_empty() {
            args.push(format!("--target={}", triple));
        }
    }
    args
}

/// True when clang/lld could not resolve a `-l` library (including Android `-lc`).
pub fn clang_stderr_is_missing_library(stderr: &str) -> bool {
    stderr.contains("cannot find -l")
        || stderr.contains("cannot open")
        || stderr.contains("unable to find library")
}

/// Assemble clang argv (excluding the `clang` program name).
pub fn clang_link_args(
    object_files: &[PathBuf],
    output_path: &Path,
    options: &LinkOptions,
) -> Vec<String> {
    let mut args = clang_driver_flags(options.opt_level, options.target_triple.as_deref());

    for obj in object_files {
        let Some(path) = obj.to_str() else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        args.push(path.to_string());
    }

    if options.force_static {
        args.push("-static".to_string());
    }

    let mut seen_l_paths = HashSet::new();
    if !options.force_static {
        args.push("-L.".to_string());
        seen_l_paths.insert(".".to_string());
    }
    for lib_path in &options.lib_paths {
        let lib_path = lib_path.trim();
        if lib_path.is_empty() {
            continue;
        }
        if seen_l_paths.insert(lib_path.to_string()) {
            args.push(format!("-L{}", lib_path));
        }
    }

    let mut seen_flags = HashSet::new();
    let mut linked_libs = HashSet::new();
    for flag in &options.link_flags {
        let flag = flag.trim();
        if flag.is_empty() {
            continue;
        }
        if seen_flags.insert(flag.to_string()) {
            args.push(flag.to_string());
        }
        if let Some(name) = lib_name_from_l_flag(flag) {
            linked_libs.insert(name);
        }
    }

    let mut seen_static = HashSet::new();
    for (lib_name, static_path) in &options.static_libs {
        let lib_name = lib_name.trim();
        if lib_name.is_empty() || !seen_static.insert(lib_name.to_string()) {
            continue;
        }
        linked_libs.insert(lib_name.to_string());
        if !static_path.is_empty() {
            args.push(static_path.clone());
        } else {
            args.push("-Wl,-Bstatic".to_string());
            args.push(format!("-l{}", lib_name));
            args.push("-Wl,-Bdynamic".to_string());
        }
    }

    for lib in &options.libraries {
        let lib = lib.trim();
        if lib.is_empty() {
            continue;
        }
        if !linked_libs.insert(lib.to_string()) {
            continue;
        }
        args.push(format!("-l{}", lib));
    }

    if !options.force_static {
        args.push("-Wl,-rpath,.".to_string());
        args.push("-Wl,-rpath,$ORIGIN".to_string());
    }

    args.push("-o".to_string());
    args.push(output_path.display().to_string());
    args
}

fn lib_name_from_l_flag(flag: &str) -> Option<String> {
    let flag = flag.trim();
    flag.strip_prefix("-l").filter(|name| !name.is_empty()).map(str::to_string)
}

fn format_clang_spawn_error(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "clang not found: install clang to link object files into an executable".to_string()
    } else {
        format!("failed to run linker: {}", err)
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

    #[test]
    fn clang_link_args_adds_rpath_dot_and_origin_for_dynamic_libs() {
        let opts = LinkOptions {
            libraries: vec!["m".into()],
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert!(args.contains(&"-Wl,-rpath,.".to_string()));
        assert!(args.contains(&"-Wl,-rpath,$ORIGIN".to_string()));
        assert!(args.contains(&"-lm".to_string()));
    }

    #[test]
    fn clang_link_args_omits_rpath_when_force_static() {
        let opts = LinkOptions {
            libraries: vec!["m".into()],
            force_static: true,
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert!(!args.iter().any(|a| a.contains("rpath")));
        assert!(args.contains(&"-static".to_string()));
        assert!(args.contains(&"-lm".to_string()));
    }

    #[test]
    fn clang_link_args_deduplicates_library_names() {
        let opts = LinkOptions {
            libraries: vec!["m".into(), "m".into(), "curl".into()],
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert_eq!(args.iter().filter(|a| *a == "-lm").count(), 1);
        assert_eq!(args.iter().filter(|a| *a == "-lcurl").count(), 1);
    }

    #[test]
    fn clang_link_args_skips_minus_l_when_static_path_already_present() {
        let opts = LinkOptions {
            libraries: vec!["m".into()],
            static_libs: vec![("m".into(), "/usr/lib/libm.a".into())],
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert!(args.contains(&"/usr/lib/libm.a".to_string()));
        assert!(!args.iter().any(|a| a == "-lm"));
    }

    #[test]
    fn clang_link_args_empty_static_path_uses_bstatic_bdynamic() {
        let opts = LinkOptions {
            libraries: vec!["m".into()],
            static_libs: vec![("m".into(), String::new())],
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        let bstatic = args
            .iter()
            .position(|a| a == "-Wl,-Bstatic")
            .expect("expected -Wl,-Bstatic");
        assert_eq!(args[bstatic + 1], "-lm");
        assert_eq!(args[bstatic + 2], "-Wl,-Bdynamic");
        assert_eq!(args.iter().filter(|a| *a == "-lm").count(), 1);
    }

    #[test]
    fn clang_link_args_skips_minus_l_already_in_link_flags() {
        let opts = LinkOptions {
            libraries: vec!["curl".into()],
            link_flags: vec!["-lcurl".into()],
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert_eq!(args.iter().filter(|a| *a == "-lcurl").count(), 1);
    }

    #[test]
    fn clang_link_args_skips_empty_optional_flags_and_lib_paths() {
        let opts = LinkOptions {
            libraries: vec!["m".into()],
            link_flags: vec!["".into(), "  ".into(), "-pthread".into()],
            lib_paths: vec!["".into(), "/opt/lib".into()],
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert!(!args.iter().any(|a| a.is_empty() || a == "-L" || a == "-L "));
        assert!(args.contains(&"-pthread".to_string()));
        assert!(args.contains(&"-L/opt/lib".to_string()));
    }

    #[test]
    fn clang_link_args_places_objects_before_output() {
        let args = clang_link_args(
            &[PathBuf::from("a.o"), PathBuf::from("b.o")],
            Path::new("exe"),
            &LinkOptions::default(),
        );
        let a = args.iter().position(|x| x == "a.o").expect("a.o");
        let b = args.iter().position(|x| x == "b.o").expect("b.o");
        let o = args.iter().position(|x| x == "-o").expect("missing -o");
        assert!(a < b && b < o);
        assert_eq!(args[o + 1], "exe");
        assert_eq!(o + 2, args.len());
    }

    #[test]
    fn clang_driver_flags_match_opt_and_target() {
        assert_eq!(clang_driver_flags(0, None), vec!["-O0".to_string()]);
        assert_eq!(clang_driver_flags(2, None), vec!["-O2".to_string()]);
        assert_eq!(clang_driver_flags(9, None), vec!["-O3".to_string()]);
        assert_eq!(
            clang_driver_flags(1, Some("aarch64-linux-android")),
            vec![
                "-O1".to_string(),
                "--target=aarch64-linux-android".to_string()
            ]
        );
        assert_eq!(clang_driver_flags(0, Some("  ")), vec!["-O0".to_string()]);
    }

    #[test]
    fn clang_link_args_forwards_opt_and_target() {
        let opts = LinkOptions {
            opt_level: 3,
            target_triple: Some("x86_64-unknown-linux-gnu".into()),
            ..Default::default()
        };
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert_eq!(args[0], "-O3");
        assert_eq!(args[1], "--target=x86_64-unknown-linux-gnu");
        assert!(args.contains(&"a.o".to_string()));
    }

    #[test]
    fn link_options_from_c_imports_parses_library_symbol_and_static() {
        let opts = link_options_from_c_imports(
            &["libc:printf".into(), "m:sin".into(), "libc:puts".into()],
            true,
            &["m".into(), "curl".into()],
            2,
            Some("aarch64-linux-android".into()),
        );
        assert!(opts.force_static);
        assert_eq!(opts.opt_level, 2);
        assert_eq!(opts.target_triple.as_deref(), Some("aarch64-linux-android"));
        assert!(opts.libraries.contains(&"c".to_string()));
        assert!(!opts.libraries.iter().any(|l| l == "m"));
        assert!(opts.static_libs.iter().any(|(n, p)| n == "m" && p.is_empty()));
        assert!(opts.static_libs.iter().any(|(n, p)| n == "curl" && p.is_empty()));
        let args = clang_link_args(&[PathBuf::from("a.o")], Path::new("out"), &opts);
        assert!(args.contains(&"-static".to_string()));
        assert!(args.contains(&"-lc".to_string()));
        assert!(args.contains(&"-lm".to_string()));
        assert!(args.contains(&"-lcurl".to_string()));
    }

    #[test]
    fn format_clang_link_failure_keeps_android_lc_help() {
        let stderr = "ld.lld: error: unable to find library -lc\n";
        let msg = format_clang_link_failure(stderr);
        assert!(clang_stderr_is_missing_library(stderr));
        assert!(msg.contains("-lc"), "expected -lc in {msg}");
        assert!(msg.contains("unable to find library"), "expected existing-style help in {msg}");
        assert!(msg.to_lowercase().contains("link"), "expected link in {msg}");
    }

    #[test]
    fn format_clang_spawn_error_is_clear_when_clang_missing() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "entity not found");
        let msg = format_clang_spawn_error(&err);
        let lower = msg.to_lowercase();
        assert!(lower.contains("clang"), "expected clang in error, got {msg}");
        assert!(lower.contains("not found"), "expected not found in error, got {msg}");
    }
}
