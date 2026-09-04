//! Compiler trace logging. Silent unless `COFFEE_DEBUG` is set.

#[macro_export]
macro_rules! coffee_debug {
    ($($arg:tt)*) => {{
        if std::env::var_os("COFFEE_DEBUG").is_some() {
            eprintln!($($arg)*);
        }
    }};
}
