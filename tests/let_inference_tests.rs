// `let x = value`：类型省略时从值推断。
// 之前每个 `let` 都必须写类型标注，没有标注是语法错误。

include!("common/mod.rs");

fn run(body: &str, expected: i32) {
    assert_exit_code(body, expected).unwrap();
}

#[test]
fn test_infer_int_literal() {
    run("fn main() => int:\n    let x = 42\n    return x\n", 42);
}

#[test]
fn test_infer_float_literal() {
    run(
        "fn main() => int:\n    let x = 1.5\n    let y = x * 2.0\n    if y > 2.5:\n        return 1\n    else:\n        return 0\n",
        1,
    );
}

#[test]
fn test_infer_bool_literal() {
    run(
        "fn main() => int:\n    let b = true\n    if b:\n        return 3\n    else:\n        return 4\n",
        3,
    );
}

#[test]
fn test_infer_str_literal_is_balanced() {
    assert_balanced("fn main() => int:\n    let s = \"hi\"\n    return 0\n");
}

#[test]
fn test_infer_from_binary_expression() {
    run("fn main() => int:\n    let a: int = 3\n    let b = a + 4\n    return b\n", 7);
}

#[test]
fn test_infer_from_call() {
    run("fn f() => int:\n    return 5\n\nfn main() => int:\n    let v = f()\n    return v\n", 5);
}

#[test]
fn test_infer_chain_of_inferred_lets() {
    run("fn main() => int:\n    let a = 2\n    let b = a * 3\n    let c = b + a\n    return c\n", 8);
}

#[test]
fn test_infer_inferred_value_passed_to_function() {
    run(
        "fn double(n: int) => int:\n    return n * 2\n\nfn main() => int:\n    let n = 5\n    return double(n)\n",
        10,
    );
}

#[test]
fn test_infer_inside_loop_body() {
    run(
        "fn main() => int:\n    let i = 0\n    let acc = 0\n    while i < 4:\n        let j = i * 2\n        acc = acc + j\n        i = i + 1\n    return acc\n",
        12,
    );
}

const POINT: &str = r#"
class P:
    x: int

    fn new(x: int) => P:
        return P { x: x }

fn get(p: P) => int:
    return p.x

fn mk() => P:
    return P { x: 9 }
"#;

#[test]
fn test_infer_struct_literal() {
    run(&format!("{POINT}\nfn main() => int:\n    let p = P {{ x: 3 }}\n    return p.x\n"), 3);
}

#[test]
fn test_infer_constructor_call() {
    run(&format!("{POINT}\nfn main() => int:\n    let p = P::new(4)\n    return p.x\n"), 4);
}

#[test]
fn test_infer_class_from_call_then_pass_to_function() {
    run(&format!("{POINT}\nfn main() => int:\n    let p = mk()\n    return get(p)\n"), 9);
}

#[test]
fn test_infer_class_is_balanced() {
    assert_balanced(&format!("{POINT}\nfn main() => int:\n    let p = mk()\n    return 0\n"));
}

#[test]
fn test_infer_field_read() {
    run(&format!("{POINT}\nfn main() => int:\n    let p: P = P {{ x: 6 }}\n    let q = p.x\n    return q\n"), 6);
}

#[test]
fn test_infer_enum_variant_and_match() {
    run(
        r#"
enum E:
    A
    B(n: int)

fn main() => int:
    let e = E.B(2)
    match e:
        E.A => 0
        E.B(n) => n
"#,
        2,
    );
}

#[test]
fn test_infer_array_literal() {
    run("fn main() => int:\n    let a = [1, 2, 3]\n    return a[1]\n", 2);
}

#[test]
fn test_infer_from_parameter_reached_through_match_binding() {
    run(
        r#"
enum E:
    A
    B(n: int)

fn inc(n: int) => int:
    let m = n + 1
    return m

fn main() => int:
    let e: E = E.B(5)
    match e:
        E.A => 0
        E.B(n) => inc(n)
"#,
        6,
    );
}

#[test]
fn test_annotated_let_still_works() {
    run("fn main() => int:\n    let x: int = 8\n    return x\n", 8);
}

#[test]
fn test_infer_empty_array_is_an_error() {
    assert_compile_error("fn main() => int:\n    let a = []\n    return 0\n", "infer").unwrap();
}

#[test]
fn test_infer_from_void_call_is_an_error() {
    assert_compile_error(
        "fn nothing() => void:\n    return\n\nfn main() => int:\n    let x = nothing()\n    return 0\n",
        "infer",
    )
    .unwrap();
}

#[test]
fn test_infer_type_is_enforced_on_later_use() {
    assert_compile_error(
        "fn main() => int:\n    let x = 1\n    let y: str = x\n    return 0\n",
        "mismatch",
    )
    .unwrap();
}
