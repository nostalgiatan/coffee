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

// ---- 配平：构造次数等于释放次数 ----
//
// 用 LD_PRELOAD 的小垫片统计 malloc/free。双重释放会被 glibc 直接中止，
// 所以这些测试同时覆盖"泄漏"和"重复释放"。

#[test]
fn test_balanced_local_class() {
    assert_balanced("class P:\n    x: int\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    return 0\n");
}

#[test]
fn test_balanced_explicit_rm() {
    assert_balanced("class P:\n    x: int\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    rm a\n    return 0\n");
}

#[test]
fn test_balanced_class_with_constructor() {
    assert_balanced(
        "class P:\n    x: int\n\n    fn new(x: int) => P:\n        return P { x: x }\n\nfn main() => int:\n    let a: P = P::new(1)\n    return 0\n",
    );
}

#[test]
fn test_balanced_returned_class() {
    assert_balanced(
        "class P:\n    x: int\n\nfn mk() => P:\n    return P { x: 1 }\n\nfn main() => int:\n    let a: P = mk()\n    return a.x\n",
    );
}

#[test]
fn test_balanced_class_moved_into_field() {
    assert_balanced(
        "class In:\n    v: int\n\nclass Out:\n    i: In\n    n: int\n\nfn main() => int:\n    let i: In = In { v: 7 }\n    let o: Out = Out { i: i, n: 5 }\n    return 0\n",
    );
}

#[test]
fn test_balanced_clone() {
    assert_balanced(
        "class P:\n    x: int\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = clone a\n    return b.x\n",
    );
}

#[test]
fn test_balanced_class_built_in_a_loop() {
    assert_balanced(
        "class P:\n    x: int\n\nfn main() => int:\n    let i: int = 0\n    while i < 3:\n        let a: P = P { x: i }\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn test_balanced_payload_enum_locals() {
    assert_balanced(
        "enum S:\n    A(r: int)\n    B\n\nfn main() => int:\n    let s: S = S.A(1)\n    let t: S = S.B\n    return 0\n",
    );
}

#[test]
fn test_balanced_class_consumed_by_callee() {
    assert_balanced(
        "class P:\n    x: int\n\nfn eat(p: P) => int:\n    return 1\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    return eat(a)\n",
    );
}

#[test]
fn test_balanced_class_passed_through_callee() {
    assert_balanced(
        "class P:\n    x: int\n\nfn id(p: P) => P:\n    return p\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = id(a)\n    return b.x\n",
    );
}

#[test]
fn test_balanced_method_does_not_consume_receiver() {
    assert_balanced(
        "class P:\n    x: int\n\n    fn get(self) => int:\n        return self.x\n\nfn main() => int:\n    let a: P = P { x: 4 }\n    let n: int = a.get()\n    return n + a.get()\n",
    );
}

#[test]
fn test_balanced_method_consumes_own_class_parameter() {
    assert_balanced(
        "class P:\n    x: int\n\n    fn add(self, o: P) => int:\n        return self.x + o.x\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = P { x: 2 }\n    return a.add(b)\n",
    );
}

#[test]
fn test_balanced_two_class_parameters() {
    assert_balanced(
        "class P:\n    x: int\n\nfn two(a: P, b: P) => int:\n    return a.x + b.x\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = P { x: 2 }\n    return two(a, b)\n",
    );
}

#[test]
fn test_balanced_recursion_with_class_parameter() {
    assert_balanced(
        "class N:\n    v: int\n\nfn depth(n: N, k: int) => int:\n    if k == 0:\n        return n.v\n    let m: N = N { v: n.v + 1 }\n    return depth(m, k - 1)\n\nfn main() => int:\n    let s: N = N { v: 0 }\n    return depth(s, 5)\n",
    );
}

#[test]
fn test_balanced_payload_enum_parameter() {
    assert_balanced(
        "enum E:\n    A(r: int)\n    B\n\nfn val(e: E) => int:\n    match e:\n        E.A(r) => r\n        E.B => 0\n\nfn main() => int:\n    return val(E.A(5)) + val(E.B)\n",
    );
}
