// match 分支可以是多语句块：`=>` 后换行，下面是更深一层缩进的语句。
// 之前分支只能是同一行里的一条语句，要写多条只能拆成辅助函数。

include!("common/mod.rs");

const E: &str = r#"
enum E:
    A
    B(n: int)
    C(n: int)
"#;

#[test]
fn test_block_arm_with_let_and_return() {
    let source = format!(
        "{E}\nfn f(e: E) => int:\n    match e:\n        E.A => return 0\n        E.B(n) =>\n            let m = n + 1\n            return m * 2\n        E.C(n) => return n\n\nfn main() => int:\n    return f(E.B(4))\n"
    );
    assert_exit_code(&source, 10).unwrap();
}

#[test]
fn test_block_arm_is_not_the_last_arm() {
    let source = format!(
        "{E}\nfn f(e: E) => int:\n    match e:\n        E.B(n) =>\n            let m = n + 1\n            return m\n        E.A => return 0\n        E.C(n) => return n\n\nfn main() => int:\n    return f(E.C(7))\n"
    );
    assert_exit_code(&source, 7).unwrap();
}

#[test]
fn test_block_arm_with_nested_if() {
    let source = format!(
        "{E}\nfn f(e: E) => int:\n    match e:\n        E.A => return 0\n        E.B(n) =>\n            if n > 3:\n                return 1\n            else:\n                return 2\n        E.C(n) => return n\n\nfn main() => int:\n    return f(E.B(9))\n"
    );
    assert_exit_code(&source, 1).unwrap();
}

#[test]
fn test_block_arm_statements_run_in_order_and_fall_through_to_the_code_after() {
    let source = format!(
        "{E}\nfn f(e: E) => int:\n    let total = 0\n    match e:\n        E.A => total = 1\n        E.B(n) =>\n            total = n\n            total = total + 10\n        E.C(n) => total = n\n    return total\n\nfn main() => int:\n    return f(E.B(5))\n"
    );
    assert_exit_code(&source, 15).unwrap();
}

#[test]
fn test_block_arm_in_loop_with_blank_line() {
    let source = format!(
        "{E}\nfn main() => int:\n    let i = 0\n    let acc = 0\n    while i < 3:\n        let e: E = E.B(i)\n        match e:\n            E.A => acc = acc + 100\n            E.B(n) =>\n                let k = n + 1\n\n                acc = acc + k\n            E.C(n) => acc = acc + n\n        i = i + 1\n    return acc\n"
    );
    assert_exit_code(&source, 6).unwrap();
}

#[test]
fn test_single_line_arms_still_work() {
    let source = format!(
        "{E}\nfn f(e: E) => int:\n    match e:\n        E.A => 0\n        E.B(n) => n\n        E.C(n) => n + 1\n\nfn main() => int:\n    return f(E.C(4))\n"
    );
    assert_exit_code(&source, 5).unwrap();
}
