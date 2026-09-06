// Expected-failure mutations for illegal control flow / match,
// plus a few tiny compile-ok cases not covered by control_flow_tests.

include!("common/mod.rs");

#[test]
fn continue_outside_loop_is_e111() {
    let source = r#"
fn main() => int:
    continue
    return 0

"#;
    assert_compile_error(source, "E111").unwrap();
}

#[test]
fn continue_in_if_not_loop_is_e111() {
    let source = r#"
fn main() => int:
    if true:
        continue
    return 0

"#;
    assert_compile_error(source, "E111").unwrap();
}

#[test]
fn break_in_function_with_no_loop_is_e111() {
    let source = r#"
fn helper() => int:
    break
    return 0

fn main() => int:
    return helper()

"#;
    assert_compile_error(source, "E111").unwrap();
}

#[test]
fn match_enum_missing_variant_is_e112() {
    let source = r#"
enum Color:
    Red
    Green
    Blue

fn main() => int:
    let c: Color = Color.Red
    match c:
        Color.Red => 1
        Color.Green => 2
    rm c
    return 0

"#;
    assert_compile_error(source, "E112").unwrap();
}

#[test]
fn for_range_end_bool_is_e100() {
    let source = r#"
fn main() => int:
    for i in 1..true:
        i = i
    return 0

"#;
    assert_compile_error(source, "E100").unwrap();
}

#[test]
fn duplicate_fn_main_is_e001() {
    let source = r#"
fn main() => int:
    return 0

fn main() => int:
    return 1

"#;
    assert_compile_error(source, "E001").unwrap();
}

#[test]
fn while_false_increment_only_compiles() {
    let source = r#"
fn main() => int:
    let i: int = 0
    while false:
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn while_zero_iterations_increment_only_compiles() {
    let source = r#"
fn main() => int:
    let i: int = 0
    while i < 0:
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}
