//! Bundled libc/libm `.cfc` tables.
//!
//! Keys are always `libc` and `libm`. Do not run `extract_library_name` on
//! `libc.cfc` (that yields `"c"`).

use std::collections::HashMap;

use super::parser::parse_cfc_content;
use super::CSymbolTable;

/// Parse the bundled `library/cfc/{libc,libm}.cfc` files shipped in the binary.
pub fn load_bundled_c_tables() -> HashMap<String, CSymbolTable> {
    let mut tables = HashMap::new();
    let libc = parse_cfc_content(
        include_str!("../../library/cfc/libc.cfc"),
        "libc",
    )
    .expect("bundled library/cfc/libc.cfc must parse");
    let libm = parse_cfc_content(
        include_str!("../../library/cfc/libm.cfc"),
        "libm",
    )
    .expect("bundled library/cfc/libm.cfc must parse");
    tables.insert("libc".to_string(), libc);
    tables.insert("libm".to_string(), libm);
    for table in tables.values_mut() {
        strip_cfc_variadic_args_param(table);
    }
    tables
}

/// Bundled true-variadic lines use a trailing `args: object` so `parse_cfc_content`
/// sets `is_variadic`. Drop that sentinel so typecheck matches the old `c_builtins`
/// tables (fixed params only; extra args are untyped C varargs).
fn strip_cfc_variadic_args_param(table: &mut CSymbolTable) {
    for symbol in table.symbols.values_mut() {
        let strip = symbol.parameters.last().is_some_and(|p| {
            p.name == "args" && p.param_type == "object"
        });
        if strip {
            symbol.parameters.pop();
            symbol.is_variadic = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::load_bundled_c_tables;

    #[test]
    fn bundled_keys_are_libc_and_libm() {
        let tables = load_bundled_c_tables();
        assert!(tables["libc"].get("write").is_some());
        assert!(tables["libc"].get("printf").is_some());
        assert!(tables["libm"].get("sin").is_some());
        assert!(!tables.contains_key("c"));
    }

    #[test]
    fn printf_signature_is_variadic_with_format_then_args() {
        let tables = load_bundled_c_tables();
        let p = tables["libc"].get("printf").expect("printf");
        assert!(p.is_variadic, "printf must be variadic");
        assert_eq!(p.parameters.len(), 1, "params={:?}", p.parameters);
        assert_eq!(p.parameters[0].param_type, "string");
    }
}
