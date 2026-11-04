// 循环体里声明的变量，在同一次迭代里的最后一次使用可以隐式移动。

include!("common/mod.rs");

#[test]
fn test_local_declared_in_loop_body_moves_into_call() {
    assert_balanced(
        "fn take(s: str) => int:\n    return 1\n\nfn main() => int:\n    let i: int = 0\n    while i < 3:\n        let s: str = \"a\" + \"b\"\n        let n: int = take(s)\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn test_local_declared_in_loop_body_moves_into_let() {
    assert_balanced(
        "fn main() => int:\n    let i: int = 0\n    while i < 3:\n        let a: str = \"a\" + \"b\"\n        let b: str = a\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn test_local_in_for_body_moves_into_class_field() {
    assert_balanced(
        "class N:\n    s: str\n\nfn main() => int:\n    let i: int = 0\n    while i < 3:\n        let s: str = \"a\" + \"b\"\n        let n: N = N { s: s }\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn test_outer_variable_moved_inside_loop_is_still_an_error() {
    assert_compile_error(
        "fn take(s: str) => int:\n    return 1\n\nfn main() => int:\n    let s: str = \"a\"\n    let i: int = 0\n    while i < 2:\n        let n: int = take(s)\n        i = i + 1\n    return 0\n",
        "E302",
    )
    .unwrap();
}

#[test]
fn test_two_uses_in_loop_body_do_not_move_at_the_first() {
    assert_compile_error(
        "fn take(s: str) => int:\n    return 1\n\nfn main() => int:\n    let i: int = 0\n    while i < 2:\n        let s: str = \"a\"\n        let n: int = take(s)\n        let m: int = take(s)\n        i = i + 1\n    return 0\n",
        "E302",
    )
    .unwrap();
}
