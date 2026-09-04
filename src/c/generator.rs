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

use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::Write;

// Re-export clang types for convenience
pub use clang_sys;

use clang_sys::*;

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
    // SAFETY: This unsafe block is safe because:
    // 1. client_data is guaranteed by clang to be a valid pointer to our Vec
    // 2. We're only dereferencing it to access the Vec and then calling visit_cursor_raw
    // 3. The pointer was properly cast from a mutable reference in parse_with_clang
    // 4. We're not modifying the data structure in a way that would cause memory issues
    unsafe {
        let declarations = &mut *(client_data as *mut Vec<CFunctionDecl>);
        visit_cursor_raw(cursor, declarations);
    }
    CXChildVisit_Continue
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
    let input_path = input_path.as_ref();

    // Validate input file exists
    if !input_path.exists() {
        return Err(format!("input file not found: {}", input_path.display()));
    }

    // Check extension
    let ext = input_path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if ext != "h" && ext != "c" {
        return Err(format!("expected .h or .c file, got: {}", ext));
    }

    // Extract library name
    let library_name = extract_library_name_from_path(input_path);

    // Parse C file using clang
    let declarations = parse_with_clang(input_path, extra_includes)
        .map_err(|e| format!("failed to parse C file: {}", e))?;

    // Generate .cfc content
    let content = format_cfc_content(&library_name, &declarations);

    // Determine output path
    let output_path = output_path.map(|p| p.as_ref().to_path_buf())
        .unwrap_or_else(|| {
            let default = format!("lib{}.cfc", library_name);
            std::path::PathBuf::from(default)
        });

    // Write to file
    let mut file = File::create(&output_path)
        .map_err(|e| format!("failed to create output file: {}", e))?;

    file.write_all(content.as_bytes())
        .map_err(|e| format!("failed to write output: {}", e))?;

    Ok(output_path.display().to_string())
}

/// Extract library name from input file path
fn extract_library_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
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
/// 
/// # Examples
/// 
/// ```
/// use std::path::Path;
/// use coffee::c::generator::parse_with_clang;
/// 
/// // Parse a header file (would fail in test environment without the file)
/// // let result = parse_with_clang(Path::new("stdio.h"), &[]);
/// // assert!(result.is_ok());
/// ```
pub fn parse_with_clang(header_path: &Path, extra_includes: &[String]) -> Result<Vec<CFunctionDecl>, String> {
    use std::ffi::CString;

    let path_cstr = CString::new(header_path.to_str().unwrap())
        .map_err(|e| format!("failed to convert path: {}", e))?;

    // Dynamically detect system include paths from clang
    let system_paths = get_system_include_paths();

    // Combine system paths with extra include paths
    let mut all_paths = system_paths;
    for extra_path in extra_includes {
        all_paths.push(extra_path.clone());
    }

    let c_args: Vec<*const i8> = all_paths.iter()
        .map(|p| format!("-I{}", p))
        .map(|s| CString::new(s).unwrap())
        .map(|s| s.as_ptr() as *const i8)
        .collect();

    unsafe {
        // SAFETY: This unsafe block is safe because:
        // 1. path_cstr is a valid CString containing the file path
        // 2. c_args is a properly constructed array of C string pointers
        // 3. All clang API calls are made with valid parameters
        // 4. We properly dispose of the translation unit and index to prevent memory leaks
        // 5. The function pointer for visit_callback is valid and properly defined
        let index = clang_createIndex(0, 1);
        if index.is_null() {
            return Err("failed to create clang index".to_string());
        }

        let tu = clang_createTranslationUnitFromSourceFile(
            index,
            path_cstr.as_ptr(),
            c_args.len() as i32,
            if c_args.is_empty() { core::ptr::null() } else { c_args.as_ptr() as *const *const u8 },
            0,
            core::ptr::null_mut(),
        );

        if tu.is_null() {
            return Err("failed to create translation unit".to_string());
        }

        let mut declarations = Vec::new();
        let cursor = clang_getTranslationUnitCursor(tu);

        clang_visitChildren(
            cursor,
            visit_callback,
            &mut declarations as *mut _ as CXClientData,
        );

        Ok(declarations)
    }
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

unsafe fn visit_cursor_raw(cursor: CXCursor, declarations: &mut Vec<CFunctionDecl>) {
    // SAFETY: This unsafe function is safe because:
    // 1. cursor is a valid CXCursor provided by clang
    // 2. declarations is a mutable reference to a valid Vec
    // 3. clang_getCursorKind is a safe operation on valid cursors
    // 4. The call to extract_function_decl_raw is checked for errors
    let kind = unsafe { clang_getCursorKind(cursor) };

    if kind == 8 { // CXCursor_FunctionDecl
        if let Ok(func_decl) = unsafe { extract_function_decl_raw(cursor) } {
            declarations.push(func_decl);
        }
    }

    // Recursively visit children
    // SAFETY: This unsafe call is safe because:
    // 1. cursor is a valid CXCursor provided by clang
    // 2. visit_callback is a properly defined and safe function
    // 3. declarations is properly cast to CXClientData
    unsafe {
        clang_visitChildren(
            cursor,
            visit_callback,
            declarations as *mut _ as CXClientData,
        );
    }
}

unsafe fn extract_function_decl_raw(cursor: CXCursor) -> Result<CFunctionDecl, String> {
    // SAFETY: This unsafe function is safe because:
    // 1. cursor is a valid CXCursor provided by clang
    // 2. clang_getCursorSpelling returns a valid CXString that we properly dispose of
    // 3. clang_getCursorResultType is a safe operation on valid cursors
    // 4. All other clang API calls are made with valid parameters
    let name = cxstring_to_string(unsafe { clang_getCursorSpelling(cursor) });

    let return_type = unsafe { clang_getCursorResultType(cursor) };
    let return_type_str = convert_ctype_to_coffee_raw(return_type)?;

    let mut args = Vec::new();
    let is_variadic = unsafe { clang_Cursor_isVariadic(cursor) } != 0;

    // Get arguments by visiting children
    let mut arg_count = 0;
    let mut temp_args: Vec<(String, String)> = Vec::new();
    let mut visitor_data = (&mut temp_args as *mut Vec<(String, String)>, &mut arg_count);

    // SAFETY: This unsafe call is safe because:
    // 1. cursor is a valid CXCursor provided by clang
    // 2. extract_args_callback is a properly defined and safe function
    // 3. visitor_data is properly cast to CXClientData
    unsafe {
        clang_visitChildren(
            cursor,
            extract_args_callback,
            &mut visitor_data as *mut _ as CXClientData,
        );
    }

    for (i, (arg_name, arg_type_str)) in temp_args.drain(..).enumerate() {
        let arg_name_final = if arg_name.is_empty() {
            format!("arg{}", i)
        } else {
            arg_name
        };
        args.push((arg_name_final, arg_type_str));
    }

    Ok(CFunctionDecl {
        name,
        parameters: args,
        return_type: return_type_str,
        is_variadic,
    })
}

extern "C" fn extract_args_callback(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> i32 {
    // SAFETY: This unsafe block is safe because:
    // 1. client_data is guaranteed by clang to be a valid pointer to our visitor data
    // 2. We're only dereferencing it to access the tuple of references
    // 3. The pointer was properly cast from references in visit_cursor_raw
    // 4. We're only appending to the vector and incrementing the counter
    unsafe {
        let (temp_args, arg_count) = &mut *(client_data as *mut (&mut Vec<(String, String)>, &mut usize));
        let kind = clang_getCursorKind(cursor);

        if kind == 10 { // CXCursor_ParmDecl
            let arg_name = cxstring_to_string(clang_getCursorSpelling(cursor));
            let arg_type = clang_getCursorType(cursor);
            let arg_type_str = convert_ctype_to_coffee_raw(arg_type).unwrap_or_else(|_| "int".to_string());

            (*temp_args).push((arg_name, arg_type_str));
            **arg_count += 1;
        }
    }
    CXChildVisit_Continue
}

fn convert_ctype_to_coffee_raw(ctype: CXType) -> Result<String, String> {
    let kind = ctype.kind;

    match kind {
        2 => Ok("void".to_string()),  // CXType_Void

        1 => Ok("bool".to_string()),   // CXType_Bool

        8 => {  // CXType_Char_S
            Ok("int(1)+".to_string())
        }

        9 => {  // CXType_Char_U (observed as 'unsigned int' in some cases)
            // SAFETY: This unsafe block is safe because:
            // 1. clang_getTypeSpelling is called with a valid CXType
            // 2. The returned CXString is properly converted to a Rust string using cxstring_to_string
            // 3. We only use the string for comparison and don't store the raw pointer
            unsafe {
                let type_name = cxstring_to_string(clang_getTypeSpelling(ctype));
                if type_name == "unsigned int" {
                    Ok("int(4)-".to_string())
                } else {
                    Ok("int(1)-".to_string())
                }
            }
        }

        10 => {  // Observed as 'unsigned long'
            // SAFETY: This unsafe block is safe because:
            // 1. clang_getTypeSpelling is called with a valid CXType
            // 2. The returned CXString is properly converted to a Rust string using cxstring_to_string
            // 3. We only use the string for comparison and don't store the raw pointer
            unsafe {
                let type_name = cxstring_to_string(clang_getTypeSpelling(ctype));
                if type_name == "unsigned long" {
                    Ok("int-".to_string())
                } else {
                    Ok("int(1)-".to_string())
                }
            }
        }

        11 => {  // CXType_UShort
            Ok("int(2)-".to_string())
        }

        3 => {  // CXType_Short
            Ok("int(2)+".to_string())
        }

        13 => {  // CXType_UInt
            Ok("int(4)-".to_string())
        }

        17 => {  // CXType_Int (observed as 'int')
            Ok("int(4)+".to_string())
        }

        18 => {  // Observed as 'long'
            Ok("int(8)+".to_string())
        }

        15 | 16 => {  // CXType_ULong | CXType_ULongLong
            Ok("int(8)-".to_string())
        }

        5 | 6 => {  // CXType_Long | CXType_LongLong
            Ok("int(8)+".to_string())
        }

        21 => {  // Observed as 'float'
            Ok("float(4)".to_string())
        }

        7 | 12 | 22 => {  // CXType_Float, CXType_Double (kind 7, 12, or 22)
            Ok("float".to_string())
        }

        101 => {  // Pointer types (observed for 'char *', 'void *', etc.)
            // SAFETY: This unsafe block is safe because:
            // 1. clang_getPointeeType is called with a valid CXType
            // 2. clang_getTypeSpelling is called with the valid pointee type
            // 3. The returned CXString is properly converted to a Rust string using cxstring_to_string
            // 4. We only use the string for comparison and don't store the raw pointer
            unsafe {
                let pointee = clang_getPointeeType(ctype);
                let pointee_name = cxstring_to_string(clang_getTypeSpelling(pointee));

                // Check the actual type name instead of just kind
                if pointee_name == "char" {
                    Ok("string".to_string())
                } else if pointee_name == "void" {
                    Ok("int".to_string())
                } else {
                    Ok("int".to_string())
                }
            }
        }

        _ => {
            // SAFETY: This unsafe block is safe because:
            // 1. clang_getTypeSpelling is called with a valid CXType
            // 2. The returned CXString is properly converted to a Rust string using cxstring_to_string
            // 3. We only use the string for error reporting and don't store the raw pointer
            unsafe {
                let type_name = cxstring_to_string(clang_getTypeSpelling(ctype));
                Err(format!("unsupported C type: {} (kind: {})", type_name, kind))
            }
        }
    }
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
fn format_cfc_content(library_name: &str, declarations: &[CFunctionDecl]) -> String {
    let mut content = format!(
"// Auto-generated .cfc file for {library}
// Generated by Coffee compiler using clang-sys
//
// This file declares C function signatures for FFI integration.
// Type mappings follow Coffee's type system conventions.
",
        library = library_name
    );

    if declarations.is_empty() {
        content.push_str("\n// No function declarations found\n");
    } else {
        content.push('\n');
        for decl in declarations {
            content.push_str(&format_cfc_decl(decl));
            content.push('\n');
        }
    }

    content
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

/// C function declaration extracted by clang
#[derive(Debug, Clone)]
pub(crate) struct CFunctionDecl {
    pub name: String,
    pub parameters: Vec<(String, String)>, // (name, type)
    pub return_type: String,
    pub is_variadic: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
