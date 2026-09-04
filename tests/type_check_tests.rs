// Type checker coverage: nested lets, unary ops, method lookup.

include!("common/mod.rs");

#[test]
fn test_let_int_from_bool_is_type_error() {
    let source = r#"
fn main() => int:
    let x: int = true
    rm x
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_nested_let_in_if_is_type_error() {
    let source = r#"
fn main() => int:
    if true:
        let x: int = true
        rm x
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_unary_not_on_int_is_error() {
    let source = r#"
fn main() => int:
    if !42:
        return 1
    return 0

"#;
    assert_compile_error(source, "invalid operation").unwrap();
}

#[test]
fn test_unary_neg_on_bool_is_error() {
    let source = r#"
fn main() => int:
    let x: int = -true
    rm x
    return 0

"#;
    assert_compile_error(source, "invalid operation").unwrap();
}

#[test]
fn test_missing_method_is_error() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

fn main() => int:
    let b: Box = Box::new(1)
    b.nope(1)
    rm b
    return 0

"#;
    assert_compile_error(source, "method").unwrap();
}
