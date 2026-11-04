// `mv a b` / `clone a b`：新名字沿用源的类型，字段访问和释放都要正确。

include!("common/mod.rs");

const BOX: &str = "class Box:\n    n: int\n    tag: str\n\n";

#[test]
fn test_mv_target_field_is_readable() {
    assert_exit_code(&format!("{BOX}fn main() => int:\n    let a: Box = Box {{ n: 4, tag: \"x\" }}\n    mv a b\n    return b.n\n"), 4).unwrap();
}

#[test]
fn test_mv_is_balanced() {
    assert_balanced(&format!("{BOX}fn main() => int:\n    let a: Box = Box {{ n: 4, tag: \"x\" }}\n    mv a b\n    return b.n\n"));
}

#[test]
fn test_clone_target_is_independent_and_balanced() {
    assert_balanced(&format!(
        "{BOX}fn main() => int:\n    let a: Box = Box {{ n: 4, tag: \"x\" }}\n    clone a b\n    return a.n + b.n\n"
    ));
}

#[test]
fn test_clone_target_field_is_readable() {
    assert_exit_code(&format!("{BOX}fn main() => int:\n    let a: Box = Box {{ n: 4, tag: \"x\" }}\n    clone a b\n    return a.n + b.n\n"), 8).unwrap();
}
