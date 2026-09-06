//! User-facing C interop diagnostics (missing `.cfc`, headers, installed libs).
//! Callers attach these as `Diagnostic` kind + suggestions. Do not silently use `object`.

use crate::diagnostics::{Diagnostic, ErrorKind, Severity, Suggestion};

fn format_searched(searched: &[String]) -> String {
    if searched.is_empty() {
        "(none listed)".to_string()
    } else {
        searched.join(", ")
    }
}

/// Prefer dedicated C interop kinds when `diagnostics.rs` has landed them; otherwise
/// compile against the stable import/link variants (parallel wave).
fn cfc_kind(library: &str) -> ErrorKind {
    ErrorKind::CfcNotFound {
        library: library.to_string(),
    }
}

/// Missing `.cfc` for a C library (searched `.`, `lib/`, `target/cfc`, etc.).
pub fn cfc_not_found(library: &str, searched: &[String]) -> Diagnostic {
    let searched_s = format_searched(searched);
    Diagnostic::new(
        Severity::Error,
        cfc_kind(library),
        format!(".cfc not found for C library '{library}'. Searched: {searched_s}."),
    )
    .with_suggestion(Suggestion::new(format!(
        "generate one with `coffee -c <header.h> [-I path] -o lib{library}.cfc`, \
         or declare `[dependencies.c_libraries.{library}]` with `headers = [...]` in coffee.toml"
    )))
}

/// Root or include header could not be opened.
pub fn header_not_found(header: &str, searched: &[String]) -> Diagnostic {
    let searched_s = format_searched(searched);
    Diagnostic::new(
        Severity::Error,
            ErrorKind::CHeaderNotFound {
                header: header.to_string(),
                searched: searched_s.clone(),
            },
        format!("cannot find C header '{header}'. Searched: {searched_s}."),
    )
    .with_suggestion(Suggestion::new(format!(
        "pass `-I <include-dir>` or set `include_paths` / `$PREFIX/include` so `{header}` can be found"
    )))
}

/// Linker cannot find `lib{linker_name}` (`.so` / `.a`).
pub fn library_not_installed(linker_name: &str, searched: &[String]) -> Diagnostic {
    let searched_s = format_searched(searched);
    Diagnostic::new(
        Severity::Error,
        ErrorKind::CLibraryNotInstalled {
            name: linker_name.to_string(),
            searched: searched_s.clone(),
        },
        format!(
            "C library '{linker_name}' is not installed \
             (lib{linker_name}.so / lib{linker_name}.a). Searched: {searched_s}."
        ),
    )
    .with_suggestion(Suggestion::new(format!(
        "install the package that provides lib{linker_name} (e.g. `pkg install lib{linker_name}`), \
         or set PREFIX so the linker can find it"
    )))
}

/// `needs` / `c_libraries` graph contains a cycle.
pub fn c_dep_cycle(path: &[String]) -> Diagnostic {
    let cycle = if path.is_empty() {
        "(empty cycle)".to_string()
    } else {
        path.join(" -> ")
    };
    Diagnostic::new(
        Severity::Error,
        ErrorKind::CDepCycle {
            path: path.to_vec(),
        },
        format!("C library dependency cycle: {cycle}"),
    )
    .with_suggestion(Suggestion::new(format!(
        "break the cycle in `[dependencies.c_libraries]` `needs` (cycle: {cycle})"
    )))
}
