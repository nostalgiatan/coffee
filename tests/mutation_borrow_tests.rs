include!("common/mod.rs");

#[test]
fn test_shared_and_mut_on_same_var_nested() {
    let source = r#"
fn main() => int:
    let val: int = 42
    if true:
        let shared: &int = &val
        let exclusive: &mut int = &mut val
        let n: int = *shared
    return 0

"#;
    assert_compile_error(source, "[E401]").unwrap();
}

#[test]
fn test_two_mut_borrows() {
    let source = r#"
fn main() => int:
    let k: int = 3
    let r1: &mut int = &mut k
    let r2: &mut int = &mut k
    return *r1

"#;
    assert_compile_error(source, "[E401]").unwrap();
}

#[test]
fn test_mv_while_borrowed_in_helper() {
    let source = r#"
fn helper() => int:
    let num: int = 7
    let ptr: &int = &num
    let unused: int = 0
    mv num dest
    return *ptr

fn main() => int:
    return helper()

"#;
    assert_compile_error(source, "[E110]").unwrap();
}

#[test]
fn test_rm_while_borrowed() {
    let source = r#"
fn main() => int:
    let item: int = 5
    let ref_item: &int = &item
    rm item
    return *ref_item

"#;
    assert_compile_error(source, "[E110]").unwrap();
}

#[test]
fn test_assign_while_borrowed_extra_let() {
    let source = r#"
fn main() => int:
    let count: int = 1
    let view: &int = &count
    let dummy: int = 0
    count = 2
    return *view

"#;
    assert_compile_error(source, "[E303]").unwrap();
}

#[test]
fn test_return_ref_to_tmp() {
    let source = r#"
fn peek() => &int:
    let tmp: int = 99
    return &tmp

fn main() => int:
    let z: int = 1
    return z

"#;
    assert_compile_error(source, "[E400]").unwrap();
}

#[test]
fn test_let_copy_class_resource() {
    let source = r#"
class Holder:
    v: int

fn main() => int:
    let src: Holder = Holder { v: 8 }
    let dst: Holder = src
    rm src
    rm dst
    return 0

"#;
    assert_compile_error(source, "[E302]").unwrap();
}

#[test]
fn test_let_copy_str_resource() {
    let source = r#"
fn main() => int:
    let s: str = "hi"
    let t: str = s
    return 0

"#;
    assert_compile_error(source, "[E302]").unwrap();
}

#[test]
fn test_copy_keyword_is_type_error() {
    let source = r#"
fn main() => int:
    let n: int = 4
    let dummy: int = 1
    copy n m
    return 0

"#;
    assert_compile_error(source, "[E110]").unwrap();
}

#[test]
fn test_clean_out_keyword_is_type_error() {
    let source = r#"
fn main() => int:
    let n: int = 4
    if true:
        clean out
    return 0

"#;
    assert_compile_error(source, "[E110]").unwrap();
}
