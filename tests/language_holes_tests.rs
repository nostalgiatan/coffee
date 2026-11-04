// 语言审查发现的漏洞：编译通过却在运行时 double free / 静默出错。

include!("common/mod.rs");

const C_NAME: &str = "class C:\n    name: str\n\n";

#[test]
fn test_let_from_field_is_rejected() {
    assert_compile_error(&format!("{C_NAME}fn main() => int:\n    let c: C = C {{ name: \"n\" }}\n    let s: str = c.name\n    return 0\n"), "E302").unwrap();
}

#[test]
fn test_return_field_is_rejected() {
    assert_compile_error(&format!("{C_NAME}fn get(c: C) => str:\n    return c.name\n\nfn main() => int:\n    return 0\n"), "E302").unwrap();
}

#[test]
fn test_call_arg_from_field_is_rejected() {
    assert_compile_error(&format!("{C_NAME}fn eat(s: str) => int:\n    return 1\n\nfn main() => int:\n    let c: C = C {{ name: \"n\" }}\n    return eat(c.name)\n"), "E302").unwrap();
}

#[test]
fn test_let_from_array_element_is_rejected() {
    assert_compile_error("fn main() => int:\n    let a: [str; 1] = [\"x\" + \"y\"]\n    let s: str = a[0]\n    return 0\n", "E302").unwrap();
}

#[test]
fn test_struct_field_from_field_is_rejected() {
    assert_compile_error(&format!("{C_NAME}class D:\n    n: str\n\nfn main() => int:\n    let c: C = C {{ name: \"n\" }}\n    let d: D = D {{ n: c.name }}\n    return 0\n"), "E302").unwrap();
}

#[test]
fn test_clone_of_field_is_balanced() {
    assert_balanced(&format!("{C_NAME}fn main() => int:\n    let c: C = C {{ name: \"a\" + \"b\" }}\n    let s: str = clone c.name\n    return 0\n"));
}

#[test]
fn test_value_field_read_is_allowed() {
    assert_exit_code("class C:\n    n: int\n\nfn main() => int:\n    let c: C = C { n: 3 }\n    let a: int = c.n\n    return a\n", 3).unwrap();
}

#[test]
fn test_str_array_of_literals_is_balanced() {
    assert_balanced("fn main() => int:\n    let a: [str; 2] = [\"a\", \"b\"]\n    return 0\n");
}

#[test]
fn test_str_tuple_of_literals_is_balanced() {
    assert_balanced("fn main() => int:\n    let t: (str, int) = (\"a\", 1)\n    return 0\n");
}

fn assert_crashes(source: &str) {
    let r = compile_and_run_coffee(source).unwrap();
    assert!(
        r.exit_code != 9 && r.exit_code != 0,
        "expected a crash, got exit {} stdout {:?}",
        r.exit_code,
        r.stdout
    );
}

#[test]
fn test_integer_division_by_zero_traps() {
    assert_crashes("fn main() => int:\n    let z: int = 0\n    let r: int = 7 // z\n    return 9\n");
}

#[test]
fn test_modulo_by_zero_traps() {
    assert_crashes("fn main() => int:\n    let z: int = 0\n    let r: int = 7 % z\n    return 9\n");
}

#[test]
fn test_signed_overflow_traps() {
    assert_crashes("fn main() => int:\n    let a: int = 9223372036854775807\n    let b: int = a + 1\n    return 9\n");
}

#[test]
fn test_duplicate_function_is_an_error() {
    assert_compile_error("fn f() => int:\n    return 1\n\nfn f() => int:\n    return 2\n\nfn main() => int:\n    return f()\n", "E201").unwrap();
}

#[test]
fn test_dedent_to_unknown_level_is_an_error() {
    assert_compile_error("fn main() => int:\n    if true:\n        return 1\n      return 2\n", "inconsistent indentation").unwrap();
}

#[test]
fn test_mixed_int_float_arithmetic_is_a_type_error() {
    assert_compile_error("fn main() => int:\n    let a: int = 1\n    let b: float = 2.0\n    let c: float = a + b\n    return 0\n", "E104").unwrap();
}

#[test]
fn test_float_cast_makes_mixed_arithmetic_explicit() {
    assert_exit_code("fn main() => int:\n    let a: int = 1\n    let b: float = 2.0\n    let c: float = float(a) + b\n    return int(c)\n", 3).unwrap();
}

#[test]
fn test_payload_free_enum_equality() {
    assert_exit_code("enum E:\n    A\n    B\n\nfn main() => int:\n    let a: E = E.A\n    if a == E.A:\n        return 1\n    return 0\n", 1).unwrap();
}

const CHAIN: &str = "class C:\n    n: int\n    fn me(self) => C:\n        return C { n: self.n }\n    fn get(self) => int:\n        return self.n\n\n";

#[test]
fn test_method_chain_on_call_result() {
    assert_exit_code(&format!("{CHAIN}fn main() => int:\n    let c: C = C {{ n: 4 }}\n    return c.me().get()\n"), 4).unwrap();
}

#[test]
fn test_method_chain_temporary_is_freed() {
    assert_balanced(&format!("{CHAIN}fn main() => int:\n    let c: C = C {{ n: 4 }}\n    return c.me().get()\n"));
}

#[test]
fn test_field_of_indexed_element() {
    assert_exit_code("class P:\n    x: int\n\nfn main() => int:\n    let a: [P; 2] = [P { x: 1 }, P { x: 2 }]\n    return a[1].x\n", 2).unwrap();
}

#[test]
fn test_index_of_field_and_call_result() {
    assert_exit_code("class B:\n    xs: [int; 3]\n\nfn mk() => [int; 3]:\n    return [1, 2, 3]\n\nfn main() => int:\n    let b: B = B { xs: [3, 4, 5] }\n    return b.xs[2] + mk()[0]\n", 6).unwrap();
}

#[test]
fn test_return_with_multiline_call_arguments() {
    assert_exit_code("fn add3(a: int, b: int, c: int) => int:\n    return a + b + c\n\nfn main() => int:\n    return add3(\n        1,\n        2,\n        3\n    )\n", 6).unwrap();
}

#[test]
fn test_multiline_parameter_list() {
    assert_exit_code("fn add3(\n    a: int,\n    b: int,\n    c: int\n) => int:\n    return a + b + c\n\nfn main() => int:\n    return add3(1, 2, 3)\n", 6).unwrap();
}

#[test]
fn test_trailing_operator_continues_the_statement() {
    assert_exit_code("fn main() => int:\n    let x: int = 0\n    x = 1 +\n        2\n    return x\n", 3).unwrap();
}

#[test]
fn test_comment_inside_multiline_call_is_ignored() {
    assert_exit_code("fn add(a: int, b: int) => int:\n    return a + b\n\nfn main() => int:\n    return add(\n        1,  /#/ first\n        2   /#/ second\n    )\n", 3).unwrap();
}

#[test]
fn test_error_after_joined_lines_keeps_original_line_number() {
    assert_compile_error("fn add(a: int, b: int) => int:\n    return add(\n        1,\n        2\n    )\n\nfn main() => int:\n    let x: int = 1 $ 2\n    return 0\n", "line 8").unwrap();
}

#[test]
fn test_sized_integer_type_suffix_is_not_a_continuation() {
    assert_compiles("class P:\n    x: int(4)+\n    y: int(4)-\n\nfn main() => int:\n    return 0\n").unwrap();
}
