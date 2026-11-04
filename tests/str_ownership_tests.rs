// `str` 的所有权：owned `str` 一律在堆上，字面量用作 owned `str` 时复制进堆，
// 局部、参数、字段在作用域结束时各释放一次。设计见
// docs/superpowers/specs/2026-11-04-memory-system-and-bytecode.md（未决项 `str`）。
// 之前字面量是静态数据，class 字段 drop 时 `free` 它会 abort，而局部 `str` 从不释放。

include!("common/mod.rs");

#[test]
fn test_local_literal_is_balanced() {
    assert_balanced("fn main() => int:\n    let s: str = \"abc\"\n    return 0\n");
}

#[test]
fn test_rm_literal_local_is_balanced() {
    assert_balanced("fn main() => int:\n    let s: str = \"abc\"\n    rm s\n    return 0\n");
}

#[test]
fn test_literal_in_class_field_without_constructor() {
    assert_balanced(
        r#"
class N:
    name: str
    v: int

fn main() => int:
    let n: N = N { name: "hello", v: 1 }
    return n.v
"#,
    );
}

#[test]
fn test_literal_in_class_field_through_constructor() {
    assert_balanced(
        r#"
class N:
    name: str
    v: int

    fn new(v: int) => N:
        return N { name: "hello", v: v }

fn main() => int:
    let n: N = N::new(1)
    return n.v
"#,
    );
}

#[test]
fn test_return_literal_from_function() {
    assert_balanced(
        r#"
fn mk() => str:
    return "abc"

fn main() => int:
    let s: str = mk()
    return 0
"#,
    );
}

#[test]
fn test_pass_str_by_value_to_callee() {
    assert_balanced(
        r#"
fn take(s: str) => int:
    return 1

fn main() => int:
    let s: str = "abc"
    return take(s)
"#,
    );
}

#[test]
fn test_literal_passed_directly_to_coffee_function() {
    assert_balanced(
        r#"
fn take(s: str) => int:
    return 1

fn main() => int:
    return take("abc")
"#,
    );
}

#[test]
fn test_last_use_move_between_str_locals() {
    assert_balanced(
        r#"
fn main() => int:
    let a: str = "abc"
    let b: str = a
    return 0
"#,
    );
}

#[test]
fn test_str_returned_through_two_calls() {
    assert_balanced(
        r#"
fn inner() => str:
    let s: str = "abc"
    return s

fn outer() => str:
    return inner()

fn main() => int:
    let s: str = outer()
    return 0
"#,
    );
}

#[test]
fn test_fstring_local_is_freed() {
    assert_balanced(
        r#"
fn main() => int:
    let a: int = 3
    let s: str = f"v={a}"
    return 0
"#,
    );
}

#[test]
fn test_literal_in_nested_class_field() {
    assert_balanced(
        r#"
class In:
    name: str

class Out:
    inner: In
    tag: str

fn main() => int:
    let i: In = In { name: "a" }
    let o: Out = Out { inner: i, tag: "b" }
    return 0
"#,
    );
}

/// 字面量传给 C 函数是借用，不能为每次调用复制一份。
#[test]
fn test_literal_passed_to_c_function_is_not_copied() {
    let source = r#"
use printf in libc of c

fn main() => int:
    printf("a\n")
    printf("b\n")
    printf("c\n")
    return 0
"#;
    if let Some((m, _)) = count_allocations(source) {
        // libc 自己给 stdout 分配一次缓冲，不会释放；字面量复制会让这里变成 4。
        assert!(m <= 1, "string literals passed to C must not be copied (malloc={m})");
    }
}
