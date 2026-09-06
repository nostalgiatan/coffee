// Parser robustness / error recovery tests (binary crate — compile via `coffee`)

include!("common/mod.rs");

#[test]
fn long_file_still_parses() {
    let mut source = String::from("fn main() => int:\n");
    for i in 0..120 {
        source.push_str(&format!("    let x{}: int = {}\n", i, i));
    }
    for i in 0..120 {
        source.push_str(&format!("    rm x{}\n", i));
    }
    source.push_str("    return 0\n");
    assert_compiles(&source).unwrap();
}

#[test]
fn long_file_several_functions_still_parses() {
    let mut source = String::new();
    for f in 0..6 {
        source.push_str(&format!("fn helper{}(n: int) => int:\n", f));
        for i in 0..15 {
            source.push_str(&format!("    let x{}: int = {}\n", i, i));
        }
        for i in 0..15 {
            source.push_str(&format!("    rm x{}\n", i));
        }
        source.push_str("    rm n\n    return 0\n\n");
    }
    source.push_str("fn main() => int:\n    return 0\n");
    assert!(source.lines().count() >= 150);
    assert_compiles(&source).unwrap();
}

#[test]
fn assignment_without_spaces() {
    let source = r#"
fn main() => int:
    let x: int = 0
    x=1
    rm x
    return 0
"#;
    assert_compiles(source).unwrap();
}

#[test]
fn broken_if_does_not_parse_body_as_top_level() {
    let source = r#"
if
    let x: int = 1
"#;
    assert_compile_error(source, "if").unwrap();
}

#[test]
fn slash_slash_comment_is_syntax_error() {
    let source = r#"
// not a coffee comment
fn main() => int:
    return 0
"#;
    assert_compile_error(source, "comment").unwrap();
    // InvalidSyntax / E001 — distinct from unexpected-token and incomplete-input
    assert_compile_error(source, "E001").unwrap();
    assert_compile_error(source, "/#/").unwrap();
}

#[test]
fn missing_fn_colon() {
    let source = r#"
fn main() => int
    return 0
"#;
    assert_compile_error(source, "colon").unwrap();
    // IncompleteInput / E003 — missing ':' on the fn header
    assert_compile_error(source, "E003").unwrap();
}

#[test]
fn python_def_is_unexpected_token() {
    let source = r#"
def foo() => int:
    return 0
"#;
    assert_compile_error(source, "def").unwrap();
    // UnexpectedToken / E002 — `def` is not Coffee (`fn` is)
    assert_compile_error(source, "E002").unwrap();
    assert_compile_error(source, "fn").unwrap();
}

#[test]
fn parse_error_kinds_are_distinguishable() {
    let def_src = "def foo() => int:\n    return 0\n";
    let comment_src = "// not coffee\nfn main() => int:\n    return 0\n";
    let colon_src = "fn main() => int\n    return 0\n";

    let def_err = compile_coffee(def_src, &[]).unwrap();
    let comment_err = compile_coffee(comment_src, &[]).unwrap();
    let colon_err = compile_coffee(colon_src, &[]).unwrap();

    assert_ne!(def_err.exit_code, 0, "def should fail: {}", def_err.stderr);
    assert_ne!(comment_err.exit_code, 0, "// should fail: {}", comment_err.stderr);
    assert_ne!(colon_err.exit_code, 0, "missing colon should fail: {}", colon_err.stderr);

    assert!(def_err.stderr.contains("E002"), "def → UnexpectedToken: {}", def_err.stderr);
    assert!(comment_err.stderr.contains("E001"), "// → InvalidSyntax: {}", comment_err.stderr);
    assert!(colon_err.stderr.contains("E003"), "missing colon → IncompleteInput: {}", colon_err.stderr);

    assert!(!def_err.stderr.contains("E001") || def_err.stderr.contains("E002"));
    assert!(!comment_err.stderr.contains("E002"), "comment must not look like UnexpectedToken: {}", comment_err.stderr);
    assert!(!colon_err.stderr.contains("E002"), "colon must not look like UnexpectedToken: {}", colon_err.stderr);
}

#[test]
fn python_import_suggests_use() {
    let source = r#"
import math
fn main() => int:
    return 0
"#;
    assert_compile_error(source, "use").unwrap();
}

#[test]
fn try_keyword_suggests_raise() {
    let source = r#"
fn main() => int:
    try:
        return 0
"#;
    assert_compile_error(source, "raise").unwrap();
}

#[test]
fn c_style_if_brace_is_rejected_with_indent_help() {
    let source = r#"
fn main() => int:
    if true {
        return 1
    return 0
"#;
    assert_compile_error(source, "indent").unwrap();
}
