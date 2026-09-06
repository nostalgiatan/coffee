// Type diagnostics must keep distinct ErrorKind codes (not E001).

include!("common/mod.rs");

#[test]
fn type_mismatch_is_e100() {
    let source = r#"
fn main() => int:
    let x: int = true
    rm x
    return 0

"#;
    assert_compile_error(source, "E100").unwrap();
}

#[test]
fn invalid_unary_is_e104() {
    let source = r#"
fn main() => int:
    let x: int = -true
    rm x
    return 0

"#;
    assert_compile_error(source, "E104").unwrap();
}

#[test]
fn undefined_variable_is_e200() {
    let source = r#"
fn main() => int:
    let x: int = missing
    rm x
    return 0

"#;
    assert_compile_error(source, "E200").unwrap();
}

#[test]
fn missing_method_is_e107() {
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
    assert_compile_error(source, "E107").unwrap();
}

#[test]
fn break_outside_loop_is_e111() {
    let source = r#"
fn main() => int:
    break
    return 0

"#;
    assert_compile_error(source, "E111").unwrap();
}

#[test]
fn incomplete_match_is_e112() {
    let source = r#"
enum Flag:
    A
    B

fn main() => int:
    let f: Flag = Flag.A
    match f:
        Flag.A => 1
    rm f
    return 0

"#;
    assert_compile_error(source, "E112").unwrap();
}

#[test]
fn undefined_variable_suggests_similar_name() {
    let source = r#"
fn main() => int:
    let count: int = 1
    let x: int = coutn
    return 0

"#;
    assert_compile_error(source, "E200").unwrap();
    assert_compile_error(source, "did you mean").unwrap();
    assert_compile_error(source, "count").unwrap();
}

#[test]
fn int_float_mismatch_suggests_cast() {
    let source = r#"
fn main() => int:
    let x: int = 1.5
    return 0

"#;
    assert_compile_error(source, "E100").unwrap();
    assert_compile_error(source, "int(").unwrap();
}

#[test]
fn unknown_print_suggests_libc_printf() {
    let source = r#"
fn main() => int:
    print("hi")
    return 0

"#;
    assert_compile_error(source, "printf").unwrap();
    assert_compile_error(source, "libc").unwrap();
    assert_compile_error(source, "use print in std").unwrap();
}
