//! Embedded Coffee standard runtime library
//!
//! This runtime is automatically linked when user code uses `raise` or other runtime features.

/// Embedded runtime source code
/// This will be automatically compiled and linked when needed
pub const RUNTIME_SOURCE: &str = r#"
/# Coffee Standard Runtime Library #/

use puts, exit in libc of c

c fn coffee_panic(msg: string, len: int) => ():
    puts("PANIC: ")
    exit(1)
"#;

/// Runtime symbols that should be automatically declared
pub const RUNTIME_SYMBOLS: &[(&str, &str)] = &[
    ("coffee_panic", "c fn coffee_panic(msg: string, len: int) => ()"),
];
