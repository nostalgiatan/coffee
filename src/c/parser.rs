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
pub fn parse_cfc_content(content: &str, library: &str) -> Result<CSymbolTable, CFCParseError> {
    let mut table = CSymbolTable::new(library.to_string());
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line_num = i + 1;
        let line = lines[i].trim();

        if line.is_empty() || line.starts_with("/#/") {
            i += 1;
            continue;
        }

        if line.starts_with("//") {
            apply_cfc_meta_comment(&mut table, library, line);
            i += 1;
            continue;
        }

        if line.starts_with("type ") {
            match parse_type_line(line) {
                Ok(type_def) => {
                    table.type_defs.insert(type_def.name().to_string(), type_def);
                }
                Err(msg) => {
                    return Err(CFCParseError::ParseError(format!("line {}: {}", line_num, msg)));
                }
            }
            i += 1;
            continue;
        }

        if line.starts_with("c class ") || line.starts_with("c union ") || line.starts_with("c enum ") {
            match parse_c_record(&lines, i) {
                Ok((type_def, n)) => {
                    table.type_defs.insert(type_def.name().to_string(), type_def);
                    i += n;
                }
                Err(msg) => {
                    return Err(CFCParseError::ParseError(format!("line {}: {}", line_num, msg)));
                }
            }
            continue;
        }

        match parse_function(line) {
            Ok((_, func)) => {
                if !func.is_c {
                    return Err(CFCParseError::NotCFunction(func.name.clone()));
                }
                if func.body != FunctionBody::External {
                    return Err(CFCParseError::ParseError(format!(
                        "line {}: function '{}' should not have a body in .cfc file",
                        line_num,
                        func.name
                    )));
                }
                table.add(CSymbol::from_function(&func));
            }
            Err(_) => {
                return Err(CFCParseError::ParseError(format!(
                    "line {}: failed to parse '{}'",
                    line_num, line
                )));
            }
        }
        i += 1;
    }

    if table.is_empty() {
        return Err(CFCParseError::EmptyFile);
    }

    Ok(table)
}

/// Parse `// coffee-cfc` / `// module:` / `// linker:` / `// headers:` / `// needs:` into `table`.
/// Unknown `// coffee-cfc` keys are ignored. `library` stays the map key unless it is empty.
fn apply_cfc_meta_comment(table: &mut CSymbolTable, library_arg: &str, line: &str) {
    let body = if let Some(rest) = line.strip_prefix("// coffee-cfc ") {
        let rest = rest.trim();
        if rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit()) {
            return;
        }
        if rest.contains(':') {
            rest
        } else {
            return;
        }
    } else if let Some(rest) = line.strip_prefix("// coffee-cfc") {
        let rest = rest.trim();
        if rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit()) {
            return;
        }
        if rest.contains(':') {
            rest
        } else {
            return;
        }
    } else if let Some(rest) = line.strip_prefix("//") {
        rest.trim()
    } else {
        return;
    };

    if let Some(v) = body.strip_prefix("module:") {
        let name = v.trim();
        if !name.is_empty() && library_arg.is_empty() {
            table.library = name.to_string();
        }
        return;
    }
    if let Some(v) = body.strip_prefix("linker:") {
        let name = v.trim();
        if !name.is_empty() {
            table.linker = name.to_string();
        }
        return;
    }
    if let Some(v) = body.strip_prefix("headers:") {
        table.owned_headers = v.split_whitespace().map(|s| s.to_string()).collect();
        return;
    }
    if let Some(v) = body.strip_prefix("needs:") {
        table.needs = v.split_whitespace().map(|s| s.to_string()).collect();
        return;
    }
    // Unknown keys (including extra `// coffee-cfc` keys): ignore.
}

fn parse_c_record(lines: &[&str], start: usize) -> Result<(crate::c::CTypeDef, usize), String> {
    let header = lines[start].trim();
    let (kind, rest) = if let Some(r) = header.strip_prefix("c class ") {
        ("class", r)
    } else if let Some(r) = header.strip_prefix("c union ") {
        ("union", r)
    } else if let Some(r) = header.strip_prefix("c enum ") {
        ("enum", r)
    } else {
        return Err("expected c class, c union, or c enum".to_string());
    };
    let rest = rest.trim().trim_end_matches(':').trim();
    let (name, size, align) = parse_record_header_name(rest)?;
    if name.is_empty() {
        return Err("missing type name".to_string());
    }

    let mut i = start + 1;
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut variants: Vec<(String, i64)> = Vec::new();
    while i < lines.len() {
        let raw = lines[i];
        if raw.trim().is_empty() || raw.trim().starts_with("//") || raw.trim().starts_with("/#/") {
            i += 1;
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        if indent == 0 {
            break;
        }
        let body = raw.trim();
        if kind == "enum" {
            variants.push(parse_enum_variant_line(body)?);
        } else {
            fields.push(parse_field_line(body)?);
        }
        i += 1;
    }
    let n = i - start;
    let def = match kind {
        "class" => crate::c::CTypeDef::Class {
            name,
            fields,
            size,
            align,
        },
        "union" => crate::c::CTypeDef::Union {
            name,
            fields,
            size,
            align,
        },
        _ => crate::c::CTypeDef::Enum { name, variants },
    };
    Ok((def, n))
}

fn parse_record_header_name(rest: &str) -> Result<(String, u64, u64), String> {
    let mut parts = rest.split_whitespace();
    let name = parts.next().unwrap_or("").to_string();
    let mut size = 0u64;
    let mut align = 0u64;
    for p in parts {
        if let Some(v) = p.strip_prefix("sizeof=") {
            size = v.parse().map_err(|_| format!("bad sizeof in '{rest}'"))?;
        } else if let Some(v) = p.strip_prefix("align=") {
            align = v.parse().map_err(|_| format!("bad align in '{rest}'"))?;
        }
    }
    Ok((name, size, align))
}

fn parse_field_line(line: &str) -> Result<(String, String), String> {
    let (name, ty) = line
        .split_once(':')
        .ok_or_else(|| format!("expected 'name: type', got '{line}'"))?;
    let name = name.trim();
    let ty = ty.trim();
    if name.is_empty() || ty.is_empty() {
        return Err(format!("empty field in '{line}'"));
    }
    Ok((name.to_string(), ty.to_string()))
}

fn parse_enum_variant_line(line: &str) -> Result<(String, i64), String> {
    if let Some((name, val)) = line.split_once('=') {
        let name = name.trim();
        let val = val
            .trim()
            .parse::<i64>()
            .map_err(|_| format!("bad enum value in '{line}'"))?;
        if name.is_empty() {
            return Err(format!("empty enum name in '{line}'"));
        }
        Ok((name.to_string(), val))
    } else {
        let name = line.trim();
        if name.is_empty() {
            return Err("empty enum variant".to_string());
        }
        Ok((name.to_string(), 0))
    }
}

/// Parse `type NAME: object` from a single trimmed line.
fn parse_type_line(line: &str) -> Result<crate::c::CTypeDef, String> {
    let rest = line
        .strip_prefix("type ")
        .ok_or_else(|| "expected line to start with 'type '".to_string())?
        .trim();

    let colon = rest
        .find(':')
        .ok_or_else(|| "expected ':' after type name".to_string())?;

    let name = rest[..colon].trim();
    if name.is_empty() {
        return Err("type name must not be empty".to_string());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("invalid type name '{}'", name));
    }

    let after_colon = rest[colon + 1..].trim();
    let source = after_colon
        .split_whitespace()
        .next()
        .ok_or_else(|| "expected source type after ':'".to_string())?;

    if source != "object" {
        return Err(format!(
            "unsupported type source '{}'; only 'object' is supported",
            source
        ));
    }

    let trailing = after_colon[source.len()..].trim();
    if !trailing.is_empty() {
        return Err(format!("unexpected trailing input '{}'", trailing));
    }

    Ok(crate::c::CTypeDef::Newtype {
        name: name.to_string(),
        source: source.to_string(),
    })
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
        assert_eq!(symbol.parameters.len(), 1);
        assert_eq!(symbol.parameters[0].param_type, "string");
    }

    #[test]
    fn object_params_are_pointers_not_varargs() {
        let content = r#"
c fn write(fd: int, buf: object, count: int) => int(4)+:
"#;
        let table = parse_cfc_content(content, "libc").unwrap();
        let write = table.get("write").unwrap();
        assert!(!write.is_variadic);
        assert_eq!(write.parameters.len(), 3);
        assert_eq!(write.parameters[1].param_type, "object");
        assert!(!write.parameters[1].is_variadic);
    }

    #[test]
    fn parse_type_line_and_fopen_return_file() {
        use crate::c::CTypeDef;

        let content = "type FILE: object\nc fn fopen(filename: string, mode: string) => FILE:\n";
        let table = parse_cfc_content(content, "libc").unwrap();

        let file_def = table.type_defs.get("FILE").expect("FILE type def");
        assert_eq!(
            file_def,
            &CTypeDef::Newtype {
                name: "FILE".to_string(),
                source: "object".to_string(),
            }
        );

        let fopen = table.get("fopen").expect("fopen symbol");
        assert_eq!(fopen.return_type, "FILE");
    }

    #[test]
    fn parse_type_only_is_not_empty_file() {
        let content = "type FILE: object\n";
        let table = parse_cfc_content(content, "test").unwrap();
        assert!(table.type_defs.contains_key("FILE"));
        assert!(table.symbols.is_empty());
        assert!(!table.is_empty());
    }

    #[test]
    fn reject_type_with_non_object_source() {
        let content = "type FILE: int\nc fn f() => void:\n";
        let err = parse_cfc_content(content, "test").unwrap_err();
        assert!(matches!(err, CFCParseError::ParseError(_)));
    }

    #[test]
    fn skip_hash_comment_lines() {
        use crate::c::CTypeDef;

        let content = "/#/ header comment\ntype FILE: object\n/#/ another\nc fn f() => void:\n";
        let table = parse_cfc_content(content, "test").unwrap();
        assert_eq!(
            table.type_defs.get("FILE"),
            Some(&CTypeDef::Newtype {
                name: "FILE".to_string(),
                source: "object".to_string(),
            })
        );
        assert!(table.contains("f"));
    }

    #[test]
    fn parse_c_class_union_enum() {
        use crate::c::CTypeDef;
        let content = r#"
c class Point sizeof=8 align=4:
    x: int(4)+
    y: int(4)+
c union Num sizeof=4 align=4:
    i: int(4)+
    f: float(4)
c enum Color:
    Red = 1
    Green = 2
c fn orig(p: Point) => Point:
"#;
        let table = parse_cfc_content(content, "t").unwrap();
        match table.type_defs.get("Point").unwrap() {
            CTypeDef::Class { fields, size, align, .. } => {
                assert_eq!(fields, &[("x".into(), "int(4)+".into()), ("y".into(), "int(4)+".into())]);
                assert_eq!((*size, *align), (8, 4));
            }
            other => panic!("{other:?}"),
        }
        match table.type_defs.get("Num").unwrap() {
            CTypeDef::Union { fields, .. } => {
                assert_eq!(fields.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        match table.type_defs.get("Color").unwrap() {
            CTypeDef::Enum { variants, .. } => {
                assert_eq!(variants, &[("Red".into(), 1), ("Green".into(), 2)]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(table.get("orig").unwrap().parameters[0].param_type, "Point");
    }
}
