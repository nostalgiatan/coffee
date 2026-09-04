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
}

#[test]
fn missing_fn_colon() {
    let source = r#"
fn main() => int
    return 0
"#;
    assert_compile_error(source, "colon").unwrap();
}
