//! Generate .cfc Files from C Headers/Source Files
//! 
//! This module provides functionality to automatically generate .cfc (C Function Declaration)
//! files from C header files (.h) or C source files (.c) using the clang-sys library.
//! This allows Coffee programs to interface with C libraries without requiring the original
//! C header files to be available in the .cfc format.
//! 
//! The module includes:
//! 
//! - A complete C parsing pipeline using LLVM's Clang
//! - Automatic type mapping from C types to Coffee types
//! - System include path detection for accurate parsing
//! - Generation of properly formatted .cfc files
//! 
//! This functionality is essential for the Coffee compiler's C integration system,
//! as it enables the use of any C library by generating the necessary .cfc files
//! from the original C headers.

#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::Write;

use crate::c::dep_graph::{
    derive_linker_candidates, format_cfc_meta, is_satellite_name, parse_cfc_meta_comments,
    posix_or_compiler_cut, CfcFileMeta,
};
use crate::c::diag;
use crate::c::parse_cfc_file;
use crate::c::CTypeDef;
use crate::library_finder::{find_library_file, get_standard_search_paths};

// Re-export clang types for convenience
pub use clang_sys;

use clang_sys::*;

#[derive(Debug, Clone)]
pub(crate) struct CFunctionDecl {
    pub name: String,
    pub parameters: Vec<(String, String)>,
    pub return_type: String,
    pub is_variadic: bool,
}

#[derive(Default)]
pub(crate) struct ClangCollect {
    pub functions: Vec<CFunctionDecl>,
    pub types: HashMap<String, CTypeDef>,
    pub owned_headers: HashSet<String>,
    pub needs: Vec<String>,
    pub other_libs: Vec<(String, PathBuf)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileOwner {
    Current,
    Skip,
    Other(String),
}

#[derive(Default)]
struct VisitDump {
    functions: Vec<CXCursor>,
    type_decls: Vec<CXCursor>,
    include_parent: HashMap<PathBuf, PathBuf>,
    failed_includes: Vec<(PathBuf, String)>,
}

struct ClassifyCtx<'a> {
    main_path: PathBuf,
    current_linker: &'a str,
    root_stem: &'a str,
    include_parent: &'a HashMap<PathBuf, PathBuf>,
    system_paths: &'a [String],
    header_claims: &'a HashMap<String, String>,
}

struct MapCtx<'a> {
    types: &'a mut HashMap<String, CTypeDef>,
    in_field: bool,
    stack: &'a mut HashSet<String>,
}

/// Callback function for visiting AST nodes during Clang parsing
/// 
/// This is a C-compatible callback function that gets called by Clang for each
/// AST node during the traversal of the C header's AST. It identifies function
/// declarations and adds them to the collection of function declarations.
/// 
/// This function is part of the Clang AST visitor pattern and is called for
/// each cursor (AST node) in the parsed C header file.
/// 
/// # Safety
/// 
/// This is an unsafe function because it handles raw pointers from Clang.
/// The safety is guaranteed because:
/// 1. `client_data` is guaranteed by Clang to be a valid pointer to our Vec
/// 2. We're only dereferencing it to access the Vec and then calling visit_cursor_raw
/// 3. The pointer was properly cast from a mutable reference in parse_with_clang
/// 4. We're not modifying the data structure in a way that would cause memory issues
extern "C" fn visit_callback(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> i32 {
    unsafe {
        let dump = &mut *(client_data as *mut VisitDump);
        record_cursor(cursor, dump);
    }
    CXChildVisit_Recurse
}

/// Generate .cfc file from C header or source
///
/// This function creates a Coffee-compatible .cfc (C Function Declaration) file
/// from a C header (.h) or source (.c) file. It uses Clang to parse the C file
/// and extract function declarations, then converts the C types to Coffee types
/// and formats them in the .cfc syntax.
///
/// The generated .cfc file can then be used by the Coffee compiler to interface
/// with the C library without requiring the original C header file.
///
/// The function performs several steps:
/// 1. Validates the input file exists and has the correct extension
/// 2. Extracts the library name from the input file path
/// 3. Parses the C file using Clang to extract function declarations
/// 4. Converts C types to Coffee types
/// 5. Formats the declarations in .cfc syntax
/// 6. Writes the result to the specified output path
///
/// # Arguments
///
/// * `input_path` - Path to the input .h or .c file to parse
/// * `output_path` - Optional output path for the generated .cfc file
///                   If None, defaults to "lib{name}.cfc" where {name} is derived from input
/// * `extra_includes` - Additional include paths to use during C parsing
///
/// # Returns
///
/// * `Ok(String)` - The path to the successfully generated .cfc file
/// * `Err(String)` - An error message describing what went wrong
///
/// # Errors
///
/// This function can return errors for:
/// * Input file not found
/// * Invalid file extension (must be .h or .c)
/// * C parsing errors (malformed C header)
/// * File writing errors
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use coffee::c::generator::generate_cfc;
///
/// // Generate a .cfc file from a C header (would fail in test environment)
/// // let result = generate_cfc("stdio.h", Some("libstdio.cfc"), &[]);
/// ```
pub fn generate_cfc<P: AsRef<Path>>(
    input_path: P,
    output_path: Option<P>,
    extra_includes: &[String],
) -> Result<String, String> {
    let output_buf = output_path.as_ref().map(|p| p.as_ref().to_path_buf());
    generate_cfc_with_visit(
        input_path.as_ref(),
        output_buf.as_deref(),
        extra_includes,
        &mut HashSet::new(),
        None,
    )
}

fn generate_cfc_with_visit(
    input_path: &Path,
    output_path: Option<&Path>,
    extra_includes: &[String],
    visiting: &mut HashSet<PathBuf>,
    root_out_dir: Option<&Path>,
) -> Result<String, String> {
    if !input_path.exists() {
        let searched = searched_include_dirs(extra_includes);
        return Err(format!(
            "input file not found: {}\n{}\n{}",
            input_path.display(),
            diag::header_not_found(&input_path.display().to_string(), &searched).format_plain(),
            cfc_input_hint()
        ));
    }

    let ext = input_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if ext != "h" && ext != "c" {
        return Err(format!(
            "expected .h or .c file, got: {}\n{}",
            ext,
            cfc_input_hint()
        ));
    }

    let canon = normalize_path(input_path);
    if !visiting.insert(canon.clone()) {
        return Err(diag::c_dep_cycle(&[canon.display().to_string()]).format_plain());
    }

    let library_name = extract_library_name_from_path(input_path);

    let out_dir = root_out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| output_dir_for(output_path));
    let claims = load_cfc_header_claims(&out_dir);

    let mut collected = parse_with_clang_owned(input_path, extra_includes, &claims)
        .map_err(|e| format!("failed to parse C file: {}", e))?;

    for (need, header) in collected.other_libs.clone() {
        if need == "c" || need == "m" {
            continue;
        }
        if find_existing_cfc(&need, &out_dir).is_some() {
            continue;
        }
        if find_library_file(&need).is_none() {
            let searched = get_standard_search_paths();
            visiting.remove(&canon);
            return Err(diag::library_not_installed(&need, &searched).format_plain());
        }
        if !header.exists() {
            let searched = searched_include_dirs(extra_includes);
            visiting.remove(&canon);
            return Err(
                diag::header_not_found(&header.display().to_string(), &searched).format_plain()
            );
        }
        let dest = out_dir.join(format!("lib{need}.cfc"));
        if let Err(e) = generate_cfc_with_visit(
            &header,
            Some(&dest),
            extra_includes,
            visiting,
            Some(&out_dir),
        ) {
            visiting.remove(&canon);
            return Err(e);
        }
    }

    strip_types_owned_by_needs(&mut collected, &out_dir);

    let content = format_cfc_content(&library_name, &collected);

    let output_path = output_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(format!("lib{}.cfc", library_name)));

    let mut file = File::create(&output_path)
        .map_err(|e| format!("failed to create output file: {}", e))?;

    file.write_all(content.as_bytes())
        .map_err(|e| format!("failed to write output: {}", e))?;

    Ok(output_path.display().to_string())
}

fn output_dir_for(output_path: Option<&Path>) -> PathBuf {
    match output_path {
        Some(p) => p
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
        None => PathBuf::from("."),
    }
}

fn searched_include_dirs(extra_includes: &[String]) -> Vec<String> {
    let mut v = vec![".".to_string()];
    v.extend(extra_includes.iter().cloned());
    v.extend(get_system_include_paths());
    v
}

fn find_existing_cfc(module: &str, out_dir: &Path) -> Option<PathBuf> {
    let name = format!("lib{module}.cfc");
    let candidates = [
        out_dir.join(&name),
        PathBuf::from(&name),
        PathBuf::from("lib").join(&name),
        out_dir.join("cfc").join(&name),
        PathBuf::from("cfc").join(&name),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn cfc_module_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    stem.strip_prefix("lib").unwrap_or(stem).to_string()
}

fn load_cfc_header_claims(out_dir: &Path) -> HashMap<String, String> {
    let mut claims = HashMap::new();
    let dirs = [
        out_dir.to_path_buf(),
        PathBuf::from("."),
        PathBuf::from("lib"),
        PathBuf::from("cfc"),
        out_dir.join("cfc"),
    ];
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let path = ent.path();
            if path.extension().and_then(|e| e.to_str()) != Some("cfc") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let meta = parse_cfc_meta_comments(&text);
            let module = meta
                .module
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| cfc_module_from_path(&path));
            for h in meta.headers {
                let base = Path::new(&h)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(h.as_str())
                    .to_string();
                if !base.is_empty() {
                    claims.entry(base).or_insert_with(|| module.clone());
                }
            }
        }
    }
    claims
}

fn strip_types_owned_by_needs(collected: &mut ClangCollect, out_dir: &Path) {
    for need in &collected.needs {
        let Some(path) = find_existing_cfc(need, out_dir) else {
            continue;
        };
        let Ok(table) = parse_cfc_file(&path) else {
            continue;
        };
        for name in table.type_defs.keys() {
            collected.types.remove(name);
        }
    }
}

/// Extract library name from input file path (`zlib.h` → `z`).
fn extract_library_name_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    if stem == "zlib" || stem == "libzlib" {
        "z".to_string()
    } else {
        stem.to_string()
    }
}

/// Parse C header/source using clang-sys and extract function declarations
/// 
/// This function uses the Clang C parser to parse a C header or source file
/// and extract all function declarations. It leverages the clang-sys crate
/// to interface with the Clang C library directly.
/// 
/// The function automatically detects system include paths and combines them
/// with any extra include paths provided by the caller. It then creates a
/// Clang translation unit and traverses the AST to find function declarations.
/// 
/// This is a critical function in the .cfc generation pipeline, as it provides
/// the bridge between C headers and Coffee's function declaration system.
/// 
/// # Arguments
/// 
/// * `header_path` - Path to the C header (.h) or source (.c) file to parse
/// * `extra_includes` - Additional include paths to use when parsing the C file
/// 
/// # Returns
/// 
/// * `Ok(Vec<CFunctionDecl>)` - Vector of extracted function declarations
/// * `Err(String)` - Error message if parsing fails
/// 
/// # Safety
/// 
/// This function contains unsafe code blocks because it interfaces with C APIs
/// through FFI. The safety is maintained by:
/// 1. Properly constructing C string arguments
/// 2. Validating all C API return values
/// 3. Properly disposing of Clang resources
/// 4. Using safe callbacks for AST traversal
#[allow(dead_code)] // kept so crate-private tests can grep the source; callers use parse_with_clang_owned
pub(crate) fn parse_with_clang(header_path: &Path, extra_includes: &[String]) -> Result<ClangCollect, String> {
    parse_with_clang_owned(header_path, extra_includes, &HashMap::new())
}

fn parse_with_clang_owned(
    header_path: &Path,
    extra_includes: &[String],
    header_claims: &HashMap<String, String>,
) -> Result<ClangCollect, String> {
    use std::ffi::{c_char, CString};

    let path_str = header_path
        .to_str()
        .ok_or_else(|| format!("input path is not valid UTF-8: {}", header_path.display()))?;
    let path_cstr = CString::new(path_str)
        .map_err(|e| format!("failed to convert path: {}", e))?;

    // Dynamically detect system include paths from clang
    let system_paths = get_system_include_paths();

    // Combine system paths with extra include paths
    let mut all_paths = system_paths;
    for extra_path in extra_includes {
        all_paths.push(extra_path.clone());
    }

    // Keep CStrings alive for the duration of the clang call (as_ptr is not owning).
    let include_args: Vec<CString> = all_paths
        .iter()
        .map(|p| {
            CString::new(format!("-I{}", p))
                .map_err(|e| format!("invalid -I include path {p:?}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let c_args: Vec<*const c_char> = include_args.iter().map(|s| s.as_ptr()).collect();

    unsafe {
        // SAFETY: path_cstr / include_args stay alive for the whole parse; index and TU
        // are disposed on every path (including parse errors).
        // displayDiagnostics=0: we surface errors via collect_tu_diagnostics / Coffee diags.
        let index = clang_createIndex(0, 0);
        if index.is_null() {
            return Err(format!(
                "failed to create clang index.\n{}",
                cfc_input_hint()
            ));
        }

        let flags = CXTranslationUnit_SkipFunctionBodies | CXTranslationUnit_KeepGoing;
        let mut tu: CXTranslationUnit = core::ptr::null_mut();
        let parse_err = clang_parseTranslationUnit2(
            index,
            path_cstr.as_ptr(),
            if c_args.is_empty() {
                core::ptr::null()
            } else {
                c_args.as_ptr()
            },
            c_args.len() as i32,
            core::ptr::null_mut(),
            0,
            flags,
            &mut tu,
        );

        if parse_err != CXError_Success || tu.is_null() {
            let diagnostics = collect_tu_diagnostics(tu);
            if !tu.is_null() {
                clang_disposeTranslationUnit(tu);
            }
            clang_disposeIndex(index);
            return Err(format_clang_failure(parse_err, &diagnostics));
        }

        let diagnostics = collect_tu_diagnostics(tu);
        if let Some(missing) = missing_include_from_diagnostics(&diagnostics) {
            if !posix_or_compiler_cut(Path::new(&missing)) {
                clang_disposeTranslationUnit(tu);
                clang_disposeIndex(index);
                let searched = searched_include_dirs(extra_includes);
                return Err(diag::header_not_found(&missing, &searched).format_plain());
            }
        }

        let mut dump = VisitDump::default();
        let cursor = clang_getTranslationUnitCursor(tu);

        clang_visitChildren(
            cursor,
            visit_callback,
            &mut dump as *mut _ as CXClientData,
        );

        let collected = finish_collect(header_path, dump, extra_includes, header_claims)?;

        clang_disposeTranslationUnit(tu);
        clang_disposeIndex(index);
        Ok(collected)
    }
}

/// Hint printed with `-c` / `--gen-cfc` failures so users know accepted inputs and `-I`.
fn cfc_input_hint() -> &'static str {
    "-c/--gen-cfc accepts .h and .c files; pass extra include directories with -I <dir>."
}

fn format_clang_failure(parse_err: CXErrorCode, diagnostics: &str) -> String {
    let mut msg = format!(
        "libclang failed to parse C input (CXErrorCode={}).\n{}",
        parse_err,
        cfc_input_hint()
    );
    if !diagnostics.is_empty() {
        msg.push_str("\nclang diagnostics:\n");
        msg.push_str(diagnostics);
    }
    msg
}

fn missing_include_from_diagnostics(diagnostics: &str) -> Option<String> {
    for line in diagnostics.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("file not found") {
            continue;
        }
        if let Some(start) = line.find('\'') {
            if let Some(rel) = line[start + 1..].find('\'') {
                let name = &line[start + 1..start + 1 + rel];
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
        if let Some(start) = line.find('"') {
            if let Some(rel) = line[start + 1..].find('"') {
                let name = &line[start + 1..start + 1 + rel];
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Collect formatted libclang diagnostics from a translation unit (may be null).
unsafe fn collect_tu_diagnostics(tu: CXTranslationUnit) -> String {
    if tu.is_null() {
        return String::new();
    }
    let n = unsafe { clang_getNumDiagnostics(tu) };
    let opts = unsafe { clang_defaultDiagnosticDisplayOptions() };
    let mut out = String::new();
    for i in 0..n {
        let diag = unsafe { clang_getDiagnostic(tu, i) };
        if diag.is_null() {
            continue;
        }
        let formatted = cxstring_to_string(unsafe { clang_formatDiagnostic(diag, opts) });
        unsafe { clang_disposeDiagnostic(diag) };
        if formatted.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&formatted);
    }
    out
}

/// Get system include paths (CMake-style detection)
/// Tries multiple compilers in order: clang, gcc, cc
fn get_system_include_paths() -> Vec<String> {
    // List of compilers to try (in order of preference)
    let compilers = vec!["clang", "clang++", "gcc", "g++", "cc"];

    for compiler in compilers {
        if let Ok(paths) = try_get_include_paths(compiler) {
            if !paths.is_empty() {
                return paths;
            }
        }
    }

    // Fallback: empty paths
    Vec::new()
}

/// Try to get include paths from a specific compiler
fn try_get_include_paths(compiler: &str) -> Result<Vec<String>, String> {
    use std::process::Command;
    use std::path::Path;

    let output = Command::new(compiler)
        .arg("-v")
        .arg("-E")
        .arg("-x")
        .arg("c")
        .arg("/dev/null")
        .output()
        .map_err(|e| format!("failed to execute {}: {}", compiler, e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut paths = Vec::new();
    let mut in_search_list = false;

    for line in stderr.lines() {
        if line.contains("#include <...> search starts here") {
            in_search_list = true;
            continue;
        }
        if line.contains("End of search list") {
            break;
        }
        if in_search_list {
            let path = line.trim();
            if !path.is_empty() && !path.starts_with('#') {
                // Convert to absolute path
                let abs_path = if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    std::fs::canonicalize(path)
                        .unwrap_or_else(|_| PathBuf::from(path))
                };
                paths.push(abs_path.to_string_lossy().to_string());
            }
        }
    }

    Ok(paths)
}

unsafe fn record_cursor(cursor: CXCursor, dump: &mut VisitDump) {
    let kind = unsafe { clang_getCursorKind(cursor) };
    if kind == CXCursor_FunctionDecl {
        dump.functions.push(cursor);
        return;
    }
    if kind == CXCursor_StructDecl
        || kind == CXCursor_UnionDecl
        || kind == CXCursor_EnumDecl
        || kind == CXCursor_TypedefDecl
    {
        dump.type_decls.push(cursor);
        return;
    }
    if kind == CXCursor_InclusionDirective {
        let Some(parent) = cursor_file_path(cursor) else {
            return;
        };
        let included_file = unsafe { clang_getIncludedFile(cursor) };
        if included_file.is_null() {
            let spelling = cxstring_to_string(unsafe { clang_getCursorSpelling(cursor) });
            if !spelling.is_empty() && !posix_or_compiler_cut(Path::new(&spelling)) {
                dump.failed_includes
                    .push((normalize_path(&parent), spelling));
            }
            return;
        }
        let name = cxstring_to_string(unsafe { clang_getFileName(included_file) });
        if name.is_empty() {
            return;
        }
        let included = normalize_path(Path::new(&name));
        dump.include_parent
            .entry(included)
            .or_insert_with(|| normalize_path(&parent));
    }
}

fn finish_collect(
    header_path: &Path,
    dump: VisitDump,
    extra_includes: &[String],
    header_claims: &HashMap<String, String>,
) -> Result<ClangCollect, String> {
    let current_linker = extract_library_name_from_path(header_path);
    let root_stem = header_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let main_path = normalize_path(header_path);
    let mut system_paths = get_system_include_paths();
    if let Ok(prefix) = std::env::var("PREFIX") {
        system_paths.push(format!("{prefix}/include"));
    }
    let ctx = ClassifyCtx {
        main_path: main_path.clone(),
        current_linker: &current_linker,
        root_stem: &root_stem,
        include_parent: &dump.include_parent,
        system_paths: &system_paths,
        header_claims,
    };

    let mut collected = ClangCollect::default();
    if let Some(name) = main_path.file_name().and_then(|n| n.to_str()) {
        collected.owned_headers.insert(name.to_string());
    }

    let mut cache: HashMap<PathBuf, FileOwner> = HashMap::new();
    let mut stack: HashSet<PathBuf> = HashSet::new();

    for (parent, name) in &dump.failed_includes {
        let owner = classify_file(parent, &ctx, &mut cache, &mut stack);
        if owner == FileOwner::Current {
            let searched = searched_include_dirs(extra_includes);
            return Err(diag::header_not_found(name, &searched).format_plain());
        }
    }

    for cursor in dump.functions {
        let invalid = unsafe { clang_isInvalidDeclaration(cursor) } != 0;
        if invalid {
            continue;
        }
        let linkage = unsafe { clang_getCursorLinkage(cursor) };
        if linkage == CXLinkage_Internal {
            continue;
        }
        let Some(file) = cursor_file_path(cursor) else {
            continue;
        };
        let file = normalize_path(&file);
        let owner = classify_file(&file, &ctx, &mut cache, &mut stack);
        match owner {
            FileOwner::Skip => {}
            FileOwner::Other(need) => {
                if !collected.needs.iter().any(|n| n == &need) {
                    collected.needs.push(need.clone());
                }
                if !collected
                    .other_libs
                    .iter()
                    .any(|(n, p)| n == &need && p == &file)
                {
                    collected.other_libs.push((need, file));
                }
            }
            FileOwner::Current => {
                if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
                    collected.owned_headers.insert(name.to_string());
                }
                if let Ok(func_decl) = unsafe { extract_function_decl_raw(cursor, &mut collected) } {
                    collected.functions.push(func_decl);
                }
            }
        }
    }

    for cursor in dump.type_decls {
        let Some(file) = cursor_file_path(cursor) else {
            continue;
        };
        let file = normalize_path(&file);
        if classify_file(&file, &ctx, &mut cache, &mut stack) != FileOwner::Current {
            continue;
        }
        if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
            collected.owned_headers.insert(name.to_string());
        }
        let ty = unsafe { clang_getCursorType(cursor) };
        let mut tstack = HashSet::new();
        let mut mctx = MapCtx {
            types: &mut collected.types,
            in_field: false,
            stack: &mut tstack,
        };
        let _ = unsafe { coffee_from_cx(ty, &mut mctx, 0) };
    }

    Ok(collected)
}

fn classify_file(
    path: &Path,
    ctx: &ClassifyCtx<'_>,
    cache: &mut HashMap<PathBuf, FileOwner>,
    stack: &mut HashSet<PathBuf>,
) -> FileOwner {
    if let Some(existing) = cache.get(path) {
        return existing.clone();
    }
    if !stack.insert(path.to_path_buf()) {
        return FileOwner::Current;
    }

    let owner = classify_file_inner(path, ctx, cache, stack);
    stack.remove(path);
    cache.insert(path.to_path_buf(), owner.clone());
    owner
}

fn classify_file_inner(
    path: &Path,
    ctx: &ClassifyCtx<'_>,
    cache: &mut HashMap<PathBuf, FileOwner>,
    stack: &mut HashSet<PathBuf>,
) -> FileOwner {
    let basename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if let Some(module) = ctx.header_claims.get(basename) {
        if module != ctx.current_linker {
            return FileOwner::Other(module.clone());
        }
        return FileOwner::Current;
    }

    if posix_or_compiler_cut(path) {
        return FileOwner::Skip;
    }
    if let Some(need) = other_library_linker(basename, ctx.current_linker) {
        return FileOwner::Other(need);
    }

    let is_main = path == ctx.main_path.as_path();
    if is_main || is_satellite_name(basename, ctx.root_stem) {
        return FileOwner::Current;
    }

    if let Some(parent) = ctx.include_parent.get(path) {
        let parent_owner = classify_file(parent, ctx, cache, stack);
        if parent_owner == FileOwner::Current {
            return FileOwner::Current;
        }
    }

    if in_system_include_search(path, ctx.system_paths) {
        return FileOwner::Skip;
    }

    FileOwner::Current
}

fn other_library_linker(basename: &str, current_linker: &str) -> Option<String> {
    let cands = derive_linker_candidates(basename);
    let mut found = Vec::new();
    for c in cands {
        if c == "c" || c == "m" || c == current_linker {
            continue;
        }
        if find_library_file(&c).is_some() {
            found.push(c);
        }
    }
    if found.iter().any(|x| x == "z") {
        Some("z".to_string())
    } else {
        found.into_iter().next()
    }
}

fn in_system_include_search(path: &Path, system_paths: &[String]) -> bool {
    let s = path.to_string_lossy();
    for p in system_paths {
        let p = p.trim_end_matches('/');
        if s == p || s.starts_with(&format!("{p}/")) {
            return true;
        }
    }
    false
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn cursor_file_path(cursor: CXCursor) -> Option<PathBuf> {
    unsafe {
        let loc = clang_getCursorLocation(cursor);
        let mut file: CXFile = core::ptr::null_mut();
        clang_getSpellingLocation(
            loc,
            &mut file,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        if file.is_null() {
            return None;
        }
        let name = cxstring_to_string(clang_getFileName(file));
        if name.is_empty() {
            None
        } else {
            Some(PathBuf::from(name))
        }
    }
}

unsafe fn extract_function_decl_raw(
    cursor: CXCursor,
    collected: &mut ClangCollect,
) -> Result<CFunctionDecl, String> {
    let name = cxstring_to_string(unsafe { clang_getCursorSpelling(cursor) });

    let mut stack = HashSet::new();
    let mut ctx = MapCtx {
        types: &mut collected.types,
        in_field: false,
        stack: &mut stack,
    };

    let return_type = unsafe { clang_getCursorResultType(cursor) };
    let return_type_str = unsafe { coffee_from_cx(return_type, &mut ctx, 0) }?;
    drop(ctx);

    let is_variadic = unsafe { clang_Cursor_isVariadic(cursor) } != 0;
    let mut args = Vec::new();
    let n = unsafe { clang_Cursor_getNumArguments(cursor) };
    if n >= 0 {
        for i in 0..n as u32 {
            let arg_cursor = unsafe { clang_Cursor_getArgument(cursor, i) };
            let arg_name = cxstring_to_string(unsafe { clang_getCursorSpelling(arg_cursor) });
            let arg_type = unsafe { clang_getCursorType(arg_cursor) };
            let mut stack = HashSet::new();
            let mut ctx = MapCtx {
                types: &mut collected.types,
                in_field: false,
                stack: &mut stack,
            };
            let arg_type_str = unsafe { coffee_from_cx(arg_type, &mut ctx, 0) }
                .unwrap_or_else(|_| "object".to_string());
            let arg_name_final = if arg_name.is_empty() {
                format!("arg{i}")
            } else {
                arg_name
            };
            args.push((arg_name_final, arg_type_str));
        }
    }

    Ok(CFunctionDecl {
        name,
        parameters: args,
        return_type: return_type_str,
        is_variadic,
    })
}

unsafe fn coffee_from_cx(ctype: CXType, ctx: &mut MapCtx<'_>, depth: u8) -> Result<String, String> {
    if depth > 12 {
        return Err("C type nesting too deep".to_string());
    }
    let kind = ctype.kind;
    if kind == CXType_Typedef || kind == CXType_Elaborated || kind == CXType_Auto {
        let next = if kind == CXType_Elaborated {
            let named = clang_Type_getNamedType(ctype);
            if named.kind != CXType_Invalid && named.kind != kind {
                named
            } else {
                clang_getCanonicalType(ctype)
            }
        } else {
            clang_getCanonicalType(ctype)
        };
        if next.kind != kind && next.kind != CXType_Invalid {
            return coffee_from_cx(next, ctx, depth + 1);
        }
    }

    if kind == CXType_Void {
        return Ok("void".to_string());
    }
    if kind == CXType_Bool {
        return Ok("bool".to_string());
    }
    if kind == CXType_Float {
        return Ok(float_from_sizeof(ctype, 4));
    }
    if kind == CXType_Double || kind == CXType_LongDouble {
        return Ok(float_from_sizeof(ctype, 8));
    }
    if is_unsigned_int_kind(kind) {
        return Ok(int_from_sizeof(ctype, false));
    }
    if is_signed_int_kind(kind) {
        return Ok(int_from_sizeof(ctype, true));
    }
    if kind == CXType_Enum {
        return ensure_enum(ctype, ctx);
    }
    if kind == CXType_Record {
        return ensure_record(ctype, ctx);
    }
    if kind == CXType_FunctionProto || kind == CXType_FunctionNoProto {
        return map_function_type(ctype, ctx, depth);
    }
    if kind == CXType_Pointer || kind == CXType_BlockPointer || kind == CXType_ObjCObjectPointer {
        return map_pointer_like(unsafe { clang_getPointeeType(ctype) }, ctx, depth);
    }
    if kind == CXType_NullPtr {
        return Ok("object".to_string());
    }
    if kind == CXType_ConstantArray || kind == CXType_IncompleteArray || kind == CXType_VariableArray
    {
        let elem = unsafe { clang_getArrayElementType(ctype) };
        if ctx.in_field {
            let n = unsafe { clang_getArraySize(ctype) };
            let inner = coffee_from_cx(elem, ctx, depth + 1)?;
            if n > 0 {
                return Ok(format!("[{inner}; {n}]"));
            }
            return Ok("object".to_string());
        }
        return map_pointer_like(elem, ctx, depth);
    }

    let type_name = type_display_name(ctype, std::ptr::null_mut());
    Err(format!("unsupported C type: {} (kind: {})", type_name, kind))
}

fn is_signed_int_kind(kind: CXTypeKind) -> bool {
    kind == CXType_Char_S
        || kind == CXType_SChar
        || kind == CXType_Short
        || kind == CXType_Int
        || kind == CXType_Long
        || kind == CXType_LongLong
        || kind == CXType_Char16
        || kind == CXType_Char32
        || kind == CXType_WChar
}

fn is_unsigned_int_kind(kind: CXTypeKind) -> bool {
    kind == CXType_Char_U
        || kind == CXType_UChar
        || kind == CXType_UShort
        || kind == CXType_UInt
        || kind == CXType_ULong
        || kind == CXType_ULongLong
}

fn is_char_type(ctype: CXType) -> bool {
    let k = ctype.kind;
    k == CXType_Char_S || k == CXType_Char_U || k == CXType_SChar || k == CXType_UChar
}

fn int_from_sizeof(ctype: CXType, signed: bool) -> String {
    let n = unsafe { clang_Type_getSizeOf(ctype) };
    let bytes = if n > 0 && n <= 16 { n as u8 } else { 8 };
    format!("int({}){}", bytes, if signed { "+" } else { "-" })
}

fn float_from_sizeof(ctype: CXType, fallback: u8) -> String {
    let n = unsafe { clang_Type_getSizeOf(ctype) };
    let bytes = if n == 4 || n == 8 { n as u8 } else { fallback };
    if bytes == 8 {
        "float".to_string()
    } else {
        format!("float({bytes})")
    }
}

unsafe fn map_pointer_like(pointee: CXType, ctx: &mut MapCtx<'_>, depth: u8) -> Result<String, String> {
    if is_char_type(pointee) {
        return Ok(if ctx.in_field {
            "object".to_string()
        } else {
            "string".to_string()
        });
    }
    if pointee.kind == CXType_FunctionProto || pointee.kind == CXType_FunctionNoProto {
        return map_function_type(pointee, ctx, depth + 1);
    }
    if pointee.kind == CXType_Record
        || pointee.kind == CXType_Elaborated
        || pointee.kind == CXType_Typedef
    {
        let name = coffee_from_cx(pointee, ctx, depth + 1)?;
        if ctx
            .types
            .get(&name)
            .map(|d| matches!(d, CTypeDef::Newtype { .. }))
            .unwrap_or(false)
        {
            return Ok(name);
        }
        return Ok("object".to_string());
    }
    Ok("object".to_string())
}

unsafe fn map_function_type(ctype: CXType, ctx: &mut MapCtx<'_>, depth: u8) -> Result<String, String> {
    let ret = clang_getResultType(ctype);
    let ret_s = coffee_from_cx(ret, ctx, depth + 1)?;
    let n = clang_getNumArgTypes(ctype);
    let mut params = Vec::new();
    if n >= 0 {
        for i in 0..n {
            let arg = clang_getArgType(ctype, i as u32);
            params.push(coffee_from_cx(arg, ctx, depth + 1)?);
        }
    }
    Ok(format!("fn({}) => {}", params.join(", "), ret_s))
}

unsafe fn ensure_enum(ctype: CXType, ctx: &mut MapCtx<'_>) -> Result<String, String> {
    let decl = clang_getTypeDeclaration(ctype);
    let name = record_coffee_name(decl, ctype);
    if name.is_empty() {
        return Ok(int_from_sizeof(ctype, true));
    }
    if ctx.types.contains_key(&name) {
        return Ok(name);
    }
    let mut variants = Vec::new();
    let mut data = &mut variants as *mut Vec<(String, i64)>;
    clang_visitChildren(decl, enum_const_callback, &mut data as *mut _ as CXClientData);
    ctx.types.insert(
        name.clone(),
        CTypeDef::Enum {
            name: name.clone(),
            variants,
        },
    );
    Ok(name)
}

extern "C" fn enum_const_callback(cursor: CXCursor, _parent: CXCursor, client_data: CXClientData) -> i32 {
    unsafe {
        let variants = &mut **(client_data as *mut *mut Vec<(String, i64)>);
        if clang_getCursorKind(cursor) == CXCursor_EnumConstantDecl {
            let n = cxstring_to_string(clang_getCursorSpelling(cursor));
            let v = clang_getEnumConstantDeclValue(cursor);
            variants.push((n, v));
        }
    }
    CXChildVisit_Continue
}

unsafe fn ensure_record(ctype: CXType, ctx: &mut MapCtx<'_>) -> Result<String, String> {
    let decl = clang_getTypeDeclaration(ctype);
    let name = record_coffee_name(decl, ctype);
    if name.is_empty() {
        return Err("anonymous C record is not supported".to_string());
    }
    if ctx.types.contains_key(&name) {
        return Ok(name);
    }
    if !ctx.stack.insert(name.clone()) {
        return Ok(name);
    }
    let size = clang_Type_getSizeOf(ctype);
    let align = clang_Type_getAlignOf(ctype);
    let align = if align > 0 { align as u64 } else { 0 };
    if size < 0 {
        ctx.types.insert(
            name.clone(),
            CTypeDef::Newtype {
                name: name.clone(),
                source: "object".to_string(),
            },
        );
        ctx.stack.remove(&name);
        return Ok(name);
    }

    let mut fields: Vec<(String, String)> = Vec::new();
    let mut bitfield = false;
    let mut field_data = FieldVisit {
        fields: &mut fields as *mut _,
        types: ctx.types as *mut _,
        bitfield: &mut bitfield as *mut _,
        stack: ctx.stack as *mut _,
    };
    clang_visitChildren(decl, field_callback, &mut field_data as *mut _ as CXClientData);

    let def = if bitfield {
        CTypeDef::Newtype {
            name: name.clone(),
            source: "object".to_string(),
        }
    } else if clang_getCursorKind(decl) == CXCursor_UnionDecl {
        CTypeDef::Union {
            name: name.clone(),
            fields,
            size: size as u64,
            align,
        }
    } else {
        CTypeDef::Class {
            name: name.clone(),
            fields,
            size: size as u64,
            align,
        }
    };
    ctx.types.insert(name.clone(), def);
    ctx.stack.remove(&name);
    Ok(name)
}

struct FieldVisit {
    fields: *mut Vec<(String, String)>,
    types: *mut HashMap<String, CTypeDef>,
    bitfield: *mut bool,
    stack: *mut HashSet<String>,
}

extern "C" fn field_callback(cursor: CXCursor, _parent: CXCursor, client_data: CXClientData) -> i32 {
    unsafe {
        let data = &mut *(client_data as *mut FieldVisit);
        if clang_getCursorKind(cursor) != CXCursor_FieldDecl {
            return CXChildVisit_Continue;
        }
        if clang_Cursor_isBitField(cursor) != 0 {
            *data.bitfield = true;
            return CXChildVisit_Continue;
        }
        let fname = cxstring_to_string(clang_getCursorSpelling(cursor));
        let fty = clang_getCursorType(cursor);
        let mut ctx = MapCtx {
            types: &mut *data.types,
            in_field: true,
            stack: &mut *data.stack,
        };
        let mapped = coffee_from_cx(fty, &mut ctx, 0).unwrap_or_else(|_| "object".to_string());
        (*data.fields).push((fname, mapped));
    }
    CXChildVisit_Continue
}

fn record_coffee_name(decl: CXCursor, ctype: CXType) -> String {
    let spelling = cxstring_to_string(unsafe { clang_getCursorSpelling(decl) });
    if !spelling.is_empty() {
        return spelling;
    }
    let t = type_display_name(ctype, std::ptr::null_mut());
    t.trim()
        .trim_start_matches("const ")
        .trim()
        .trim_start_matches("struct ")
        .trim()
        .trim_start_matches("union ")
        .trim()
        .trim_start_matches("enum ")
        .trim()
        .to_string()
}

fn type_display_name(ctype: CXType, policy: CXPrintingPolicy) -> String {
    if !policy.is_null() {
        let pretty = cxstring_to_string(unsafe { clang_getTypePrettyPrinted(ctype, policy) });
        if !pretty.is_empty() {
            return pretty;
        }
    }
    cxstring_to_string(unsafe { clang_getTypeSpelling(ctype) })
}

fn cxstring_to_string(cxstr: CXString) -> String {
    // SAFETY: This unsafe block is safe because:
    // 1. We first check if cxstr.data is null before dereferencing
    // 2. clang_getCString is called with a valid CXString
    // 3. We check if the returned C string pointer is null before using it
    // 4. We properly convert the C string to a Rust string using CStr::from_ptr
    // 5. We call clang_disposeString to free the memory allocated by clang
    unsafe {
        if cxstr.data.is_null() {
            return String::new();
        }
        let c_str = clang_getCString(cxstr);
        if c_str.is_null() {
            return String::new();
        }
        let slice = std::ffi::CStr::from_ptr(c_str);
        let result = slice.to_string_lossy().to_string();
        clang_disposeString(cxstr);
        result
    }
}

/// Format .cfc file content
fn format_cfc_content(library_name: &str, collected: &ClangCollect) -> String {
    let mut content = format!(
        "// Auto-generated .cfc file for {library}
// Generated by Coffee compiler using clang-sys
//
// Type mappings follow Coffee's type system conventions.
",
        library = library_name
    );

    let mut headers: Vec<String> = collected.owned_headers.iter().cloned().collect();
    headers.sort();
    let mut needs = collected.needs.clone();
    needs.sort();
    needs.dedup();
    let meta = CfcFileMeta {
        version: 1,
        module: Some(library_name.to_string()),
        linker: Some(library_name.to_string()),
        headers,
        needs,
    };
    content.push_str(&format_cfc_meta(&meta));

    let mut type_names: Vec<_> = collected.types.keys().cloned().collect();
    type_names.sort();
    if !type_names.is_empty() {
        content.push('\n');
        for name in type_names {
            if let Some(td) = collected.types.get(&name) {
                content.push_str(&format_type_def(td));
                content.push('\n');
            }
        }
    }

    if collected.functions.is_empty() && collected.types.is_empty() {
        content.push_str("\n// No declarations found\n");
    } else {
        content.push('\n');
        for decl in &collected.functions {
            content.push_str(&format_cfc_decl(decl));
            content.push('\n');
        }
    }

    content
}

fn format_type_def(td: &CTypeDef) -> String {
    match td {
        CTypeDef::Newtype { name, source } => format!("type {name}: {source}\n"),
        CTypeDef::Class {
            name,
            fields,
            size,
            align,
        } => {
            let mut s = format!("c class {name} sizeof={size} align={align}:\n");
            for (n, t) in fields {
                s.push_str(&format!("    {n}: {t}\n"));
            }
            s
        }
        CTypeDef::Union {
            name,
            fields,
            size,
            align,
        } => {
            let mut s = format!("c union {name} sizeof={size} align={align}:\n");
            for (n, t) in fields {
                s.push_str(&format!("    {n}: {t}\n"));
            }
            s
        }
        CTypeDef::Enum { name, variants } => {
            let mut s = format!("c enum {name}:\n");
            for (n, v) in variants {
                s.push_str(&format!("    {n} = {v}\n"));
            }
            s
        }
    }
}

/// Format a single function declaration in .cfc syntax
fn format_cfc_decl(decl: &CFunctionDecl) -> String {
    let mut params: Vec<String> = decl.parameters.iter()
        .map(|(name, typ)| {
            format!("{}: {}", name, typ)
        })
        .collect();

    if decl.is_variadic {
        params.push("args: object".to_string());
    }

    format!("c fn {}({}) => {}:", decl.name, params.join(", "), decl.return_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_clang_is_crate_private() {
        let src = include_str!("generator.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(
            prod.contains("pub(crate) fn parse_with_clang"),
            "parse_with_clang must be pub(crate); CFunctionDecl is crate-private"
        );
        assert!(
            !prod.contains("pub fn parse_with_clang"),
            "parse_with_clang must not be a public crate API"
        );
    }

    #[test]
    fn test_cfc_input_hint_mentions_h_c_and_dash_i() {
        let hint = cfc_input_hint();
        assert!(hint.contains(".h") && hint.contains(".c"), "{hint}");
        assert!(hint.contains("-I"), "{hint}");
    }

    #[test]
    fn test_extract_library_name() {
        assert_eq!(
            extract_library_name_from_path(Path::new("test.h")),
            "test"
        );
        assert_eq!(
            extract_library_name_from_path(Path::new("libhello.h")),
            "libhello"
        );
        assert_eq!(extract_library_name_from_path(Path::new("zlib.h")), "z");
    }

    #[test]
    fn test_format_cfc_decl_emits_c_fn() {
        let decl = CFunctionDecl {
            name: "add_one".to_string(),
            parameters: vec![("x".to_string(), "int(4)+".to_string())],
            return_type: "int(4)+".to_string(),
            is_variadic: false,
        };
        let line = format_cfc_decl(&decl);
        assert!(line.contains("c fn"), "expected c fn in {line}");
        assert!(line.contains("add_one"), "expected add_one in {line}");
        let collected = ClangCollect {
            functions: vec![decl],
            types: HashMap::new(),
            ..Default::default()
        };
        let content = format_cfc_content("foo", &collected);
        assert!(content.contains("c fn"));
        assert!(content.contains("add_one"));
    }
}
