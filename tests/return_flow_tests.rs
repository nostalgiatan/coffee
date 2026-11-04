// 返回路径与 `match` 取值测试
//
// 这些测试**运行**程序并比较退出码，不只是检查能否编译。
// 背景见 docs/superpowers/specs/2026-11-04-embedded-vm-direction.md。

include!("common/mod.rs");

// ---- 非 void 函数的所有路径必须返回 ----

#[test]
fn test_missing_return_is_error() {
    let source = r#"
fn f(x: int) => int:
    let y: int = x + 1

fn main() => int:
    return f(1)
"#;
    assert_compile_error(source, "missing return").unwrap();
}

#[test]
fn test_missing_return_on_one_branch_is_error() {
    let source = r#"
fn f(x: int) => int:
    if x > 0:
        return 1

fn main() => int:
    return f(1)
"#;
    assert_compile_error(source, "missing return").unwrap();
}

#[test]
fn test_return_on_every_branch_is_ok() {
    let source = r#"
fn f(x: int) => int:
    if x > 0:
        return 3
    elif x < 0:
        return 4
    else:
        return 5

fn main() => int:
    return f(1)
"#;
    assert_exit_code(source, 3).unwrap();
}

#[test]
fn test_void_function_needs_no_return() {
    let source = r#"
fn f() => void:
    let y: int = 1
    return

fn g() => void:
    let z: int = 2

fn main() => int:
    g()
    return 7
"#;
    assert_exit_code(source, 7).unwrap();
}

#[test]
fn test_raise_ends_a_path() {
    let source = r#"
use fprintf, exit in libc of c

fn f(x: int) => int:
    if x > 0:
        return 6
    raise Error("bad")

fn main() => int:
    return f(1)
"#;
    assert_exit_code(source, 6).unwrap();
}

// ---- 函数尾部的 match：分支值就是返回值 ----

#[test]
fn test_tail_match_int_returns_arm_value() {
    let source = r#"
fn f(x: int) => int:
    match x:
        0 => 10
        _ => 20

fn main() => int:
    return f(0) + f(5)
"#;
    assert_exit_code(source, 30).unwrap();
}

#[test]
fn test_tail_match_selects_the_right_arm() {
    let source = r#"
fn f(x: int) => int:
    match x:
        0 => 10
        _ => 20

fn main() => int:
    return f(5)
"#;
    assert_exit_code(source, 20).unwrap();
}

#[test]
fn test_tail_match_enum_unit_variants() {
    let source = r#"
enum Color:
    Red
    Green
    Blue

fn name(c: Color) => int:
    match c:
        Color.Red => 1
        Color.Green => 2
        Color.Blue => 3

fn main() => int:
    return name(Color.Green)
"#;
    assert_exit_code(source, 2).unwrap();
}

#[test]
fn test_tail_match_wrong_arm_type_is_error() {
    let source = r#"
fn f(x: int) => int:
    match x:
        0 => "zero"
        _ => 1

fn main() => int:
    return f(0)
"#;
    assert!(assert_compile_error(source, "").is_ok());
}

// ---- 类方法遵循同一规则 ----

#[test]
fn test_method_missing_return_is_error() {
    let source = r#"
class A:
    v: int

    fn get(self) => int:
        let y: int = self.v

fn main() => int:
    let a: A = A { v: 1 }
    return a.get()
"#;
    assert_compile_error(source, "missing return").unwrap();
}

#[test]
fn test_method_tail_match_returns_arm_value() {
    let source = r#"
class A:
    v: int

    fn pick(self) => int:
        match self.v:
            1 => 40
            _ => 50

fn main() => int:
    let a: A = A { v: 1 }
    return a.pick()
"#;
    assert_exit_code(source, 40).unwrap();
}

#[test]
fn test_method_trailing_expression_still_returns() {
    let source = r#"
class A:
    v: int

    fn plus(self, n: int) => int:
        self.v + n

fn main() => int:
    let a: A = A { v: 1 }
    return a.plus(4)
"#;
    assert_exit_code(source, 5).unwrap();
}
