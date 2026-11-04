// 所有权漏洞回归测试：资源（class 实例等）按值传给调用时，
// 不是最后一次使用就必须 `mv` / `clone`，否则就是悄悄复制。
// 背景见 docs/superpowers/specs/2026-11-04-embedded-vm-direction.md。

include!("common/mod.rs");

const HEAD: &str = "class P:\n    n: int\n\nfn take(p: P) => int:\n    return 1\n\n";

fn with_main(body: &str) -> String {
    format!("{HEAD}fn main() => int:\n{body}")
}

// ---- 按值传资源：不是最后一次使用 ----

#[test]
fn test_by_value_call_then_read_field_is_error() {
    let src = with_main("    let a: P = P { n: 1 }\n    let r: int = take(a)\n    return a.n\n");
    assert_compile_error(&src, "cannot copy resource").unwrap();
}

#[test]
fn test_by_value_call_twice_is_error() {
    let src = with_main("    let a: P = P { n: 1 }\n    let r: int = take(a)\n    let s: int = take(a)\n    return 0\n");
    assert_compile_error(&src, "cannot copy resource").unwrap();
}

#[test]
fn test_by_value_method_argument_then_use_is_error() {
    let src = "class P:\n    n: int\n\n    fn add(self, o: P) => int:\n        return self.n + o.n\n\nfn main() => int:\n    let a: P = P { n: 1 }\n    let b: P = P { n: 2 }\n    let r: int = a.add(b)\n    return b.n\n";
    assert_compile_error(src, "cannot copy resource").unwrap();
}

// ---- 合法写法仍然通过 ----

#[test]
fn test_by_value_call_as_last_use_is_a_move() {
    let src = with_main("    let a: P = P { n: 1 }\n    return take(a)\n");
    assert_exit_code(&src, 1).unwrap();
}

#[test]
fn test_by_value_call_with_clone_keeps_original() {
    let src = with_main("    let a: P = P { n: 5 }\n    let r: int = take(clone a)\n    return a.n + r\n");
    assert_exit_code(&src, 6).unwrap();
}

#[test]
fn test_by_value_call_with_explicit_mv_expression() {
    let src = "class P:\n    n: int\n\nfn take(p: P) => int:\n    return p.n\n\nfn main() => int:\n    let a: P = P { n: 7 }\n    mv a b\n    return take(b)\n";
    assert_exit_code(src, 7).unwrap();
}

#[test]
fn test_borrowed_argument_does_not_move() {
    let src = "class P:\n    n: int\n\nfn peek(p: &P) => int:\n    return 1\n\nfn main() => int:\n    let a: P = P { n: 3 }\n    let r: int = peek(&a)\n    return a.n + r\n";
    assert_exit_code(src, 4).unwrap();
}

// ---- 分支里的 rm / 移动：发散的分支不影响之后的代码 ----

#[test]
fn test_rm_in_returning_branch_does_not_poison_later_use() {
    let src = r#"
class P:
    n: int

fn f(p: P, c: bool) => int:
    if c:
        rm p
        return 1
    return p.n

fn main() => int:
    let a: P = P { n: 4 }
    return f(a, false)
"#;
    assert_exit_code(src, 4).unwrap();
}

#[test]
fn test_rm_of_value_type_in_returning_branch() {
    let src = "fn f(n: int) => int:\n    if n < 0:\n        rm n\n        return 1\n    return n\n\nfn main() => int:\n    return f(2)\n";
    assert_exit_code(src, 2).unwrap();
}

#[test]
fn test_rm_in_non_returning_branch_still_poisons_later_use() {
    let src = r#"
class P:
    n: int

fn f(p: P, c: bool) => int:
    if c:
        rm p
    return p.n

fn main() => int:
    let a: P = P { n: 4 }
    return f(a, false)
"#;
    assert_compile_error(src, "dropped").unwrap();
}

#[test]
fn test_move_in_returning_branch_does_not_poison_later_use() {
    let src = r#"
class P:
    n: int

fn eat(p: P) => int:
    return 9

fn f(p: P, c: bool) => int:
    if c:
        return eat(p)
    return p.n

fn main() => int:
    let a: P = P { n: 4 }
    return f(a, false)
"#;
    assert_exit_code(src, 4).unwrap();
}
