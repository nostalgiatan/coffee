// 嵌套块（if / while / for / match）后面同缩进的语句不能丢。
// 块的收集只在 `return`、`raise`、内存操作和顶层关键字处才结束，
// 紧跟其后的赋值、调用语句、`break` / `continue` 被吞进块里，再被解析器静默丢弃。

include!("common/mod.rs");

fn run(source: &str, expected: i32) {
    assert_exit_code(source, expected).unwrap();
}

#[test]
fn test_assignment_after_if() {
    run("fn main() => int:\n    let acc = 0\n    if true:\n        acc = 1\n    acc = acc + 10\n    return acc\n", 11);
}

#[test]
fn test_call_statement_after_if() {
    run(
        "fn bump(n: int) => int:\n    return n\n\nfn main() => int:\n    let acc = 0\n    if true:\n        acc = 1\n    bump(acc)\n    acc = acc + 10\n    return acc\n",
        11,
    );
}

#[test]
fn test_assignment_after_if_else() {
    run("fn main() => int:\n    let a = 0\n    if false:\n        a = 1\n    else:\n        a = 2\n    a = a + 10\n    return a\n", 12);
}

#[test]
fn test_assignment_after_while() {
    run("fn main() => int:\n    let i = 0\n    while i < 3:\n        i = i + 1\n    i = i + 10\n    return i\n", 13);
}

#[test]
fn test_assignment_after_for() {
    run("fn main() => int:\n    let acc = 0\n    for i in 0..3:\n        acc = acc + i\n    acc = acc + 10\n    return acc\n", 13);
}

#[test]
fn test_increment_after_if_inside_while() {
    run(
        "fn main() => int:\n    let i = 0\n    let hits = 0\n    while i < 5:\n        if i == 2:\n            hits = hits + 1\n        i = i + 1\n    return hits * 10 + i\n",
        15,
    );
}

#[test]
fn test_statements_after_nested_blocks_two_deep() {
    run(
        "fn main() => int:\n    let i = 0\n    let acc = 0\n    while i < 3:\n        let j = 0\n        while j < 2:\n            acc = acc + 1\n            j = j + 1\n        acc = acc + 10\n        i = i + 1\n    return acc\n",
        36,
    );
}

#[test]
fn test_assignment_after_match() {
    run(
        "enum E:\n    A\n    B(n: int)\n\nfn main() => int:\n    let acc = 0\n    let e: E = E.B(2)\n    match e:\n        E.A => acc = 100\n        E.B(n) => acc = n\n    acc = acc + 1\n    return acc\n",
        3,
    );
}

#[test]
fn test_break_and_continue_after_if() {
    run(
        "fn main() => int:\n    let i = 0\n    let acc = 0\n    while i < 10:\n        i = i + 1\n        if i == 3:\n            continue\n        acc = acc + 1\n        if i == 6:\n            break\n    return acc\n",
        5,
    );
}

#[test]
fn test_multiline_struct_literal_still_collected() {
    run(
        "class P:\n    x: int\n    y: int\n\nfn main() => int:\n    let p: P = P {\n        x: 1,\n        y: 2\n    }\n    return p.x + p.y\n",
        3,
    );
}
