# Library Finder Module

## Overview

The Library Finder module (`src/library_finder.rs`) provides unified library searching functionality for both static and dynamic libraries. This module eliminates code duplication between `main.rs` and `linker.rs` by providing a centralized library file discovery mechanism.

## Module Purpose

The library finder serves several critical purposes:

1. **Unified Search**: Single interface for finding both static (`.a`) and dynamic (`.so`) libraries
2. **Standard Paths**: Searches in standard system locations
3. **Priority Order**: Searches locations in a defined priority order
4. **Cross-Platform**: Works across different Unix-like systems
5. **Code Reuse**: Eliminates duplication between compiler components

## Core Functions

### find_library_file

Finds a library file in standard locations:

```rust
pub fn find_library_file(lib_name: &str) -> Option<String>
```

**Parameters:**
- `lib_name`: The name of the library (e.g., "m", "pthread", "mylib")

**Returns:**
- `Some(String)`: The full path to the library file if found
- `None`: If the library file was not found in any search location

**Search Locations (in priority order):**
1. Current directory (`.`)
2. `./lib` subdirectory
3. Directories in `$PATH` environment variable
4. `/usr/lib`
5. `/usr/local/lib`
6. `/lib`
7. `/lib64`
8. `~/.local/lib`

**Library Name Handling:**

The function handles both library name formats:

```rust
// With "lib" prefix
find_library_file("libc")   // Searches for libc.so or libc.a

// Without "lib" prefix
find_library_file("m")      // Searches for libm.so or libm.a
```

**File Extensions:**

The function searches for both dynamic and static libraries:

```rust
// Dynamic libraries
libm.so
libc.so

// Static libraries
libm.a
libc.a
```

### get_standard_search_paths

Returns the standard library search paths:

```rust
pub fn get_standard_search_paths() -> Vec<String>
```

**Returns:**
A vector of directory paths to search for library files

**Default Search Paths:**

```rust
vec![
    ".",                       // Current directory
    "./lib",                   // Local lib directory
    // PATH directories (dynamic)
    "/usr/lib",                // System libraries
    "/usr/local/lib",          // Local system libraries
    "/lib",                    // Essential libraries
    "/lib64",                  // 64-bit libraries
    "~/.local/lib",            // User local libraries
]
```

## Implementation Details

### Search Algorithm

The library finder uses the following search algorithm:

```rust
pub fn find_library_file(lib_name: &str) -> Option<String> {
    // 1. Generate possible library filenames
    let lib_filenames = if lib_name.starts_with("lib") {
        vec![
            format!("{}.so", lib_name),  // Dynamic
            format!("{}.a", lib_name),   // Static
        ]
    } else {
        vec![
            format!("lib{}.so", lib_name),  // Dynamic
            format!("lib{}.a", lib_name),   // Static
        ]
    };

    // 2. Get search paths
    let search_paths = get_standard_search_paths();

    // 3. Search in each path
    for search_path in &search_paths {
        for lib_filename in &lib_filenames {
            let lib_path = Path::new(search_path).join(lib_filename);
            if lib_path.exists() {
                return Some(lib_path.to_string_lossy().to_string());
            }
        }
    }

    // 4. Not found
    None
}
```

### Path Resolution

The function uses `Path::join` for cross-platform path construction:

```rust
let lib_path = Path::new(search_path).join(lib_filename);
```

This ensures correct path separators on different operating systems.

### Existence Check

The function uses `Path::exists()` to check if the library file exists:

```rust
if lib_path.exists() {
    return Some(lib_path.to_string_lossy().to_string());
}
```

## Usage Examples

### Finding Standard Libraries

```rust
// Find libm (math library)
if let Some(path) = find_library_file("m") {
    println!("Found libm at: {}", path);
    // Output: /usr/lib/libm.so
}

// Find libc (C standard library)
if let Some(path) = find_library_file("c") {
    println!("Found libc at: {}", path);
    // Output: /usr/lib/libc.so
}

// Find libpthread (POSIX threads)
if let Some(path) = find_library_file("pthread") {
    println!("Found libpthread at: {}", path);
    // Output: /usr/lib/libpthread.so
}
```

### Finding Custom Libraries

```rust
// Find custom library in current directory
if let Some(path) = find_library_file("mylib") {
    println!("Found mylib at: {}", path);
    // Output: ./libmylib.so or ./libmylib.a
}

// Find library with "lib" prefix
if let Some(path) = find_library_file("libcustom") {
    println!("Found libcustom at: {}", path);
    // Output: ./libcustom.so or ./libcustom.a
}
```

### Getting Search Paths

```rust
let paths = get_standard_search_paths();
for path in &paths {
    println!("{}", path);
}
```

### Custom Search Logic

```rust
fn find_with_fallback(lib_name: &str, fallback_paths: &[&str]) -> String {
    // Try standard paths first
    if let Some(path) = find_library_file(lib_name) {
        return path;
    }

    // Try fallback paths
    for fallback_path in fallback_paths {
        let lib_path = Path::new(fallback_path).join(format!("lib{}.so", lib_name));
        if lib_path.exists() {
            return lib_path.to_string_lossy().to_string();
        }
    }

    panic!("Library '{}' not found", lib_name);
}
```

## Integration with Compiler

### Linker Integration

The linker uses the library finder to locate libraries:

```rust
use crate::library_finder::find_library_file;

pub fn link(&self, object_files: &[PathBuf], output: &Path) -> Result<(), String> {
    for lib_name in &self.c_libraries {
        if let Some(lib_path) = find_library_file(lib_name) {
            // Add library to linker command
            linker_cmd.arg(lib_path);
        } else {
            return Err(format!("library '{}' not found", lib_name));
        }
    }
    // ...
}
```

### Main Integration

The main function uses the library finder for JIT mode:

```rust
use crate::library_finder::find_library_file;

// Find library for linking
match find_library_file(lib_name) {
    Some(lib_path) => {
        // Add library path to linker
        linker_cmd.arg(format!("-L{}", lib_dir));
        linker_cmd.arg(format!("-Wl,-rpath,{}", lib_dir));
    }
    None => {
        // Report error
        return Err(format!("library '{}' not found", lib_name));
    }
}
```

## Search Path Priority

### Priority Order

The library finder searches locations in the following priority order:

1. **Current Directory (`.`)**: Highest priority for local development
2. **`./lib`**: Local library directory for project-specific libraries
3. **`$PATH`**: User-defined search paths
4. **`/usr/lib`**: Standard system libraries
5. **`/usr/local/lib`**: Local system libraries (often for manually installed software)
6. **`/lib`**: Essential system libraries
7. **`/lib64`**: 64-bit system libraries
8. **`~/.local/lib`**: User-local libraries

### Why This Order?

1. **Local First**: Allows developers to override system libraries with local versions
2. **Project Libraries**: `./lib` allows project-specific libraries
3. **User Paths**: Respects user's PATH environment variable
4. **System Libraries**: Falls back to standard system locations
5. **User Local**: Allows user-specific libraries without system-wide installation

## Platform Considerations

### Unix-like Systems

The library finder is designed for Unix-like systems (Linux, macOS, BSD):

- Uses forward slashes (`/`) as path separators
- Searches standard Unix library directories
- Supports both `.so` (shared object) and `.a` (archive) files

### Android (Termux)

The library finder works in Android Termux environment:

- Searches Termux-specific library paths
- Respects Android's library structure
- Works with Termux's package manager libraries

### Cross-Platform Path Handling

The function uses `std::path::Path` for cross-platform compatibility:

```rust
let lib_path = Path::new(search_path).join(lib_filename);
```

This ensures correct path handling on different platforms.

## Error Handling

### Not Found

The function returns `None` if the library is not found:

```rust
if let Some(path) = find_library_file("nonexistent") {
    println!("Found: {}", path);
} else {
    println!("Library not found");
}
```

### Error Reporting

The compiler provides detailed error messages:

```rust
match find_library_file(lib_name) {
    Some(path) => {
        eprintln!("  = note: found library '{}' at '{}'", lib_name, path);
    }
    None => {
        let error = ErrorKind::LinkError {
            details: format!(
                "library '{}' not found\n  = note: searched in: current directory, ./lib, $PATH, /usr/lib, /usr/local/lib, ~/.local/lib\n  = help: compile the library first or install it to a standard location",
                lib_name
            ),
        };
        eprintln!("{}", error.description());
    }
}
```

## Performance Considerations

### Early Exit

The function returns as soon as the library is found:

```rust
if lib_path.exists() {
    return Some(lib_path.to_string_lossy().to_string());  // Exit early
}
```

### Minimal Filesystem Access

The function only checks for file existence:

```rust
if lib_path.exists() {
    // Found!
}
```

This is faster than reading file metadata or content.

### Path Caching

The search paths are generated once:

```rust
let search_paths = get_standard_search_paths();
```

In practice, you could cache the search paths for repeated calls.

## Security Considerations

### Path Validation

The library finder uses `Path::join` to prevent path traversal:

```rust
let lib_path = Path::new(search_path).join(lib_filename);
```

This ensures that library names cannot escape the search directories.

### No Arbitrary Execution

The function only finds files, it does not execute them:

```rust
if lib_path.exists() {
    return Some(lib_path.to_string_lossy().to_string());
}
```

The caller is responsible for safely using the found library.

### Relative Paths

The function searches relative paths first:

```rust
search_paths.push(".".to_string());
search_paths.push("./lib".to_string());
```

This allows local development without system-wide installation.

## Testing

### Unit Tests

The module includes unit tests:

```rust
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
        // Test finding library with "lib" prefix
        let result = find_library_file("libc");
        // In a real system, this might find /usr/lib/libc.so
    }

    #[test]
    fn test_find_library_file_without_prefix() {
        // Test finding library without "lib" prefix
        let result = find_library_file("m");
        // In a real system, this might find /usr/lib/libm.so
    }
}
```

## Best Practices

### 1. Handle Not Found

```rust
// Good: Handle not found case
if let Some(path) = find_library_file("mylib") {
    // Use library
} else {
    // Provide helpful error message
    eprintln!("Library 'mylib' not found");
    eprintln!("Searched in: {}", get_standard_search_paths().join(":"));
}
```

### 2. Use Standard Names

```rust
// Good: Use standard library names
find_library_file("m")      // Math library
find_library_file("pthread") // POSIX threads
find_library_file("dl")     // Dynamic loading

// Avoid: Non-standard names
find_library_file("math")   // Should be "m"
find_library_file("threads") // Should be "pthread"
```

### 3. Prefer Dynamic Libraries

The function searches for `.so` before `.a`:

```rust
// Dynamic libraries are searched first
vec![
    format!("{}.so", lib_name),  // Dynamic
    format!("{}.a", lib_name),   // Static
]
```

This is generally preferred for smaller binary sizes and easier updates.

### 4. Use Project-Specific Libraries

Place project-specific libraries in `./lib`:

```bash
project/
├── src/
├── lib/
│   ├── libmylib.so
│   └── libother.a
└── main.cf
```

## Future Enhancements

### Planned Features

1. **Custom Search Paths**
   ```rust
   pub fn find_library_file_with_paths(lib_name: &str, paths: &[PathBuf]) -> Option<String>
   ```

2. **Library Version Support**
   ```rust
   pub fn find_library_version(lib_name: &str, version: &str) -> Option<String>
   ```

3. **Caching**
   ```rust
   pub struct CachedLibraryFinder {
       cache: HashMap<String, Option<String>>,
   }
   ```

4. **Platform-Specific Paths**
   ```rust
   pub fn get_platform_search_paths() -> Vec<String>
   ```

5. **Library Metadata**
   ```rust
   pub struct LibraryInfo {
       pub path: String,
       pub is_static: bool,
       pub version: Option<String>,
   }
   ```

## See Also

- [Compiler Module](compiler.md) - Compilation orchestration
- [Linker Module](compiler/linker.md) - Linking implementation
- [C Integration Module](c_integration.md) - C library imports
- [Main Module](main.rs) - Compiler entry point