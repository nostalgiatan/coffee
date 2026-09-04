//! .cfc File Parser
//! 
//! Parses C Function Declaration (.cfc) files which contain `c fn` declarations.
//! The .cfc format is a Coffee-specific file format that describes C library
//! function signatures using Coffee syntax. This allows Coffee programs to
//! interface with C libraries without needing the original C header files.
//! 
//! The parser supports standard Coffee function syntax but with the `c fn`
//! prefix to indicate that these functions are external C functions. The
//! parser extracts function signatures including parameter types, return
//! types, and variadic function indicators.

use std::path::Path;
use std::fs;
use crate::parser::function::{parse_function, FunctionBody};
use crate::c::{CSymbolTable, CSymbol};
// Note: Diagnostic imports are reserved for future error reporting
#[allow(unused_imports)]
use crate::diagnostics::{Diagnostic, Severity, ErrorKind};

/// Errors that can occur during .cfc parsing
/// 
/// This enumeration represents all possible error conditions that can occur
/// when parsing .cfc (C Function Declaration) files. Each error variant
/// provides specific information about what went wrong during the parsing
/// process, making it easier to diagnose and fix issues with .cfc files.
#[derive(Debug, Clone)]
pub enum CFCParseError {
    /// File not found error
    /// 
    /// This error occurs when the specified .cfc file does not exist on the filesystem.
    /// The String contains the path that was attempted to be accessed.
    FileNotFound(String),
    /// IO error during file operations
    /// 
    /// This error occurs during file reading operations, such as when the file
    /// exists but cannot be read due to permissions or other system-level issues.
    /// The String contains a description of the IO error.
    IoError(String),
    /// Parse error during function parsing
    /// 
    /// This error occurs when the content of the .cfc file cannot be parsed
    /// as valid Coffee function declarations. This might be due to syntax errors
    /// or unsupported constructs. The String contains a description of the parse error.
    ParseError(String),
    /// Function is not a C function error
    /// 
    /// This error occurs when a function declaration in the .cfc file does not
    /// have the required `c fn` prefix, indicating it's not an external C function.
    /// The String contains the name of the problematic function.
    NotCFunction(String),
    /// Empty file error
    /// 
    /// This error occurs when the .cfc file is completely empty or contains only
    /// whitespace and comments, with no actual function declarations.
    EmptyFile,
}

impl std::fmt::Display for CFCParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CFCParseError::FileNotFound(path) => write!(f, "file not found: {}", path),
            CFCParseError::IoError(msg) => write!(f, "IO error: {}", msg),
            CFCParseError::ParseError(msg) => write!(f, "parse error: {}", msg),
            CFCParseError::NotCFunction(func) => write!(f, "not a C function: {}", func),
            CFCParseError::EmptyFile => write!(f, "empty file"),
        }
    }
}

impl std::error::Error for CFCParseError {}

/// Parse a .cfc file and return a symbol table
/// 
/// This function reads and parses a C Function Declaration (.cfc) file,
/// extracting all function declarations and creating a symbol table that
/// can be used by the Coffee compiler for type checking and code generation.
/// 
/// The function performs several validation steps:
/// 1. Verifies the file exists and is readable
/// 2. Confirms the file has the correct .cfc extension
/// 3. Ensures the file is not empty
/// 4. Extracts the library name from the filename
/// 5. Parses each function declaration in the file
/// 
/// The library name is extracted from the filename by removing the "lib" prefix
/// if present (e.g., "libhello.cfc" becomes "hello", "math.cfc" becomes "math").
/// 
/// # Arguments
/// 
/// * `path` - Path to the .cfc file to parse. Can be any type that implements
///            `AsRef<Path>`, such as `&str`, `String`, `Path`, or `PathBuf`.
/// 
/// # Returns
/// 
/// * `Ok(CSymbolTable)` - A symbol table containing all parsed C function declarations
/// * `Err(CFCParseError)` - An error describing what went wrong during parsing
/// 
/// # Errors
/// 
/// This function can return several types of errors:
/// * `CFCParseError::FileNotFound` - If the specified file does not exist
/// * `CFCParseError::IoError` - If there's an error reading the file
/// * `CFCParseError::ParseError` - If the file content cannot be parsed
/// * `CFCParseError::NotCFunction` - If a function is not declared with `c fn`
/// * `CFCParseError::EmptyFile` - If the file is empty or contains no functions
/// 
/// # Examples
/// 
/// ```
/// use coffee::c::parse_cfc_file;
/// 
/// // Parse a system library file (this would fail in a test environment)
/// // let table = parse_cfc_file("libc.cfc");
/// 
/// // Parse a custom library file (this would also fail in a test environment)
/// // let table = parse_cfc_file("libmymath.cfc");
/// ```
pub fn parse_cfc_file<P: AsRef<Path>>(path: P) -> Result<CSymbolTable, CFCParseError> {
    let path = path.as_ref();

    // Check file exists
    if !path.exists() {
        return Err(CFCParseError::FileNotFound(path.display().to_string()));
    }

    // Check extension
    if path.extension().and_then(|s| s.to_str()) != Some("cfc") {
        return Err(CFCParseError::ParseError(format!(
            "expected .cfc file, got: {}",
            path.display()
        )));
    }

    // Read file
    let content = fs::read_to_string(path)
        .map_err(|e| CFCParseError::IoError(format!("failed to read file: {}", e)))?;

    if content.trim().is_empty() {
        return Err(CFCParseError::EmptyFile);
    }

    // Extract library name from filename
    // e.g., "libhello.cfc" -> "hello", "math.cfc" -> "math"
    let library = extract_library_name(path);

    // Parse content
    parse_cfc_content(&content, &library)
}

/// Extract library name from .cfc file path
/// 
/// This internal function extracts the library name from a .cfc file path by:
/// 1. Getting the file stem (filename without extension)
/// 2. Removing the "lib" prefix if present
/// 
/// This function is used to determine which C library a .cfc file describes,
/// which is important for organizing and referencing C function symbols.
/// 
/// # Arguments
/// 
/// * `path` - Reference to the file path from which to extract the library name
/// 
/// # Returns
/// 
/// A String containing the extracted library name:
/// - "libhello.cfc" -> "hello"
/// - "math.cfc" -> "math"
/// - "libc.cfc" -> "c"
/// - If the path has no valid filename, returns "unknown"
/// 
/// # Examples
/// 
/// ```
/// use std::path::Path;
/// use coffee::c::parser::extract_library_name;
/// 
/// let path1 = Path::new("libhello.cfc");
/// assert_eq!(extract_library_name(path1), "hello");
/// 
/// let path2 = Path::new("math.cfc");
/// assert_eq!(extract_library_name(path2), "math");
/// 
/// let path3 = Path::new("libc.cfc");
/// assert_eq!(extract_library_name(path3), "c");
/// ```
fn extract_library_name(path: &Path) -> String {
    let file_name = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    // Remove "lib" prefix if present
    if file_name.starts_with("lib") {
        file_name[3..].to_string()
    } else {
        file_name.to_string()
    }
}

/// Parse .cfc content string
/// 
/// This internal function parses the content of a .cfc file, extracting
/// function declarations and building a symbol table. It processes the
/// content line by line, skipping empty lines and comments, and parsing
/// each function declaration using the main Coffee parser.
/// 
/// The function enforces several rules specific to .cfc files:
/// - All functions must be declared with the `c fn` prefix
/// - All functions must be external (no function body in .cfc files)
/// - The file must contain at least one valid function declaration
/// 
/// # Arguments
/// 
/// * `content` - The string content of the .cfc file to parse
/// * `library` - The name of the library this file describes
/// 
/// # Returns
/// 
/// * `Ok(CSymbolTable)` - A symbol table containing all parsed C function declarations
/// * `Err(CFCParseError)` - An error describing what went wrong during parsing
/// 
/// # Errors
/// 
/// This function returns errors for:
/// - Functions not declared with `c fn` prefix
/// - Functions with bodies (only external declarations allowed)
/// - Malformed function declarations
/// - Empty files with no functions
/// 
/// # Examples
/// 
/// ```
/// use coffee::c::parser::parse_cfc_content;
/// 
/// let content = r#"
/// // Math library functions
/// c fn sqrt(x: float): float
/// c fn sin(x: float): float
/// "#;
/// 
/// let result = parse_cfc_content(content, "math");
/// assert!(result.is_ok());
/// 
/// let table = result.unwrap();
/// assert!(table.contains("sqrt"));
/// assert!(table.contains("sin"));
/// ```
fn parse_cfc_content(content: &str, library: &str) -> Result<CSymbolTable, CFCParseError> {
    let mut table = CSymbolTable::new(library.to_string());

    // Split into lines and parse each function declaration
    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        // Try to parse as a function
        match parse_function(line) {
            Ok((_, func)) => {
                // Must be a C function (c fn)
                if !func.is_c {
                    return Err(CFCParseError::NotCFunction(func.name.clone()));
                }

                // Must have a body (we'll use empty body for declarations)
                if func.body != FunctionBody::External {
                    return Err(CFCParseError::ParseError(format!(
                        "line {}: function '{}' should not have a body in .cfc file",
                        line_num + 1,
                        func.name
                    )));
                }

                let symbol = CSymbol::from_function(&func);
                table.add(symbol);
            }
            Err(_) => {
                return Err(CFCParseError::ParseError(format!(
                    "line {}: failed to parse '{}'",
                    line_num + 1,
                    line
                )));
            }
        }
    }

    if table.is_empty() {
        return Err(CFCParseError::EmptyFile);
    }

    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_library_name() {
        assert_eq!(extract_library_name(Path::new("libhello.cfc")), "hello");
        assert_eq!(extract_library_name(Path::new("math.cfc")), "math");
        assert_eq!(extract_library_name(Path::new("libc.cfc")), "c");
    }

    #[test]
    fn test_parse_simple_function() {
        let content = r#"
c fn hello_world() => void:
"#;

        let result = parse_cfc_content(content, "test");
        assert!(result.is_ok());

        let table = result.unwrap();
        assert!(table.contains("hello_world"));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_parse_multiple_functions() {
        let content = r#"
c fn add(a: int(4), b: int(4)) => int(4):
c fn multiply(x: int, y: int) => int:
"#;

        let result = parse_cfc_content(content, "math");
        assert!(result.is_ok());

        let table = result.unwrap();
        assert_eq!(table.len(), 2);
        assert!(table.contains("add"));
        assert!(table.contains("multiply"));
    }

    #[test]
    fn test_parse_with_comments() {
        let content = r#"
// Math library functions
c fn sqrt(x: float) => float:
// Trigonometric functions
c fn sin(x: float) => float:
"#;

        let result = parse_cfc_content(content, "math");
        assert!(result.is_ok());

        let table = result.unwrap();
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_parse_variadic_function() {
        let content = r#"
c fn printf(fmt: string, args: object) => int(4):
"#;

        let result = parse_cfc_content(content, "libc");
        assert!(result.is_ok());

        let table = result.unwrap();
        assert!(table.contains("printf"));

        let symbol = table.get("printf").unwrap();
        assert!(symbol.is_variadic);
    }
}
