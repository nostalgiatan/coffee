//! C interop diagnostic helpers (`coffee::c::diag`).

use coffee::c::diag::{c_dep_cycle, cfc_not_found, header_not_found, library_not_installed};
use coffee::diagnostics::Severity;

#[test]
fn cfc_not_found_names_library_and_help() {
    let d = cfc_not_found("z", &[".".into(), "lib/".into(), "target/cfc".into()]);
    assert_eq!(d.severity, Severity::Error);
    assert!(d.similar_names.is_empty());
    let plain = d.format_plain();
    assert!(plain.contains("z"), "{plain}");
    assert!(
        plain.contains("help:") || plain.to_lowercase().contains("hint"),
        "{plain}"
    );
    assert!(
        plain.contains("coffee -c") || plain.contains("c_libraries"),
        "{plain}"
    );
}

#[test]
fn header_not_found_names_header_and_help() {
    let d = header_not_found("zlib.h", &["$PREFIX/include".into(), "/usr/include".into()]);
    assert_eq!(d.severity, Severity::Error);
    assert!(d.similar_names.is_empty());
    let plain = d.format_plain();
    assert!(plain.contains("zlib.h"), "{plain}");
    assert!(
        plain.contains("help:") || plain.to_lowercase().contains("hint"),
        "{plain}"
    );
    assert!(
        plain.contains("-I") || plain.contains("PREFIX") || plain.contains("include_paths"),
        "{plain}"
    );
}

#[test]
fn library_not_installed_names_lib_and_help() {
    let d = library_not_installed("png", &[".".into(), "/usr/lib".into()]);
    assert_eq!(d.severity, Severity::Error);
    assert!(d.similar_names.is_empty());
    let plain = d.format_plain();
    assert!(plain.contains("png"), "{plain}");
    assert!(
        plain.contains("help:") || plain.to_lowercase().contains("hint"),
        "{plain}"
    );
    assert!(
        plain.contains("install") || plain.contains("PREFIX") || plain.contains("pkg"),
        "{plain}"
    );
}

#[test]
fn c_dep_cycle_names_path_and_help() {
    let d = c_dep_cycle(&["z".into(), "png".into(), "z".into()]);
    assert_eq!(d.severity, Severity::Error);
    assert!(d.similar_names.is_empty());
    let plain = d.format_plain();
    assert!(plain.contains("z") && plain.contains("png"), "{plain}");
    assert!(
        plain.contains("help:") || plain.to_lowercase().contains("hint"),
        "{plain}"
    );
    assert!(
        plain.contains("c_libraries") || plain.contains("needs") || plain.contains("cycle"),
        "{plain}"
    );
}
