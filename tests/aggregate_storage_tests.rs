// 聚合值（class 实例、带 payload 的枚举）的存储测试：运行程序并比较退出码。
// 设计见 docs/superpowers/specs/2026-11-04-aggregate-storage.md。
// 返回的聚合值必须在调用者里仍然有效；之前是指向已返回栈帧的 `alloca`，
// 同一程序连续运行会得到不同的退出码。

include!("common/mod.rs");

/// 连续运行同一可执行文件，确认结果稳定（悬空栈指针会随运行变化）。
fn assert_stable_exit_code(source: &str, expected: i32) {
    for _ in 0..4 {
        assert_exit_code(source, expected).unwrap();
    }
}

#[test]
fn test_return_class_literal_without_constructor() {
    let source = r#"
class P:
    x: int
    y: int

fn mk() => P:
    return P { x: 1, y: 2 }

fn sum(p: P) => int:
    return p.x + p.y

fn main() => int:
    let a: P = mk()
    return sum(a)
"#;
    assert_stable_exit_code(source, 3);
}

#[test]
fn test_return_class_through_two_calls() {
    let source = r#"
class P:
    x: int
    y: int

fn inner() => P:
    return P { x: 10, y: 20 }

fn outer() => P:
    return inner()

fn main() => int:
    let a: P = outer()
    return a.x + a.y
"#;
    assert_stable_exit_code(source, 30);
}

#[test]
fn test_return_class_with_class_field() {
    let source = r#"
class In:
    v: int

class Out:
    i: In
    n: int

fn mk() => Out:
    let i: In = In { v: 7 }
    return Out { i: i, n: 5 }

fn main() => int:
    let o: Out = mk()
    return o.i.v + o.n
"#;
    assert_stable_exit_code(source, 12);
}

#[test]
fn test_return_class_with_constructor_still_works() {
    let source = r#"
class P:
    x: int
    y: int

    fn new(x: int, y: int) => P:
        return P { x: x, y: y }

fn mk() => P:
    return P::new(1, 2)

fn main() => int:
    let a: P = mk()
    return a.x + a.y
"#;
    assert_stable_exit_code(source, 3);
}

const SHAPE: &str = r#"
enum Shape:
    Circle(r: int)
    Rect(w: int, h: int)
    Dot

fn area(s: Shape) => int:
    match s:
        Shape.Circle(r) => r * 3
        Shape.Rect(w, h) => w * h
        Shape.Dot => 0
"#;

#[test]
fn test_return_payload_enum_from_function() {
    let source = format!(
        r#"{SHAPE}
fn make_rect() => Shape:
    return Shape.Rect(3, 5)

fn make_dot() => Shape:
    return Shape.Dot

fn main() => int:
    return area(make_rect()) + area(make_dot())
"#
    );
    assert_stable_exit_code(&source, 15);
}
