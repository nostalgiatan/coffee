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
    assert_crashes_with(source, "");
}

fn assert_crashes_with(source: &str, message: &str) {
    let r = compile_and_run_coffee(source).unwrap();
    assert!(r.stdout.contains(message), "expected {:?} in stdout, got {:?}", message, r.stdout);
    assert!(
        r.exit_code != 9 && r.exit_code != 0,
        "expected a crash, got exit {} stdout {:?}",
        r.exit_code,
        r.stdout
    );
}

#[test]
fn test_integer_division_by_zero_traps() {
    assert_crashes_with("fn main() => int:\n    let z: int = 0\n    let r: int = 7 // z\n    return 9\n", "division by zero");
}

#[test]
fn test_modulo_by_zero_traps() {
    assert_crashes_with("fn main() => int:\n    let z: int = 0\n    let r: int = 7 % z\n    return 9\n", "modulo by zero");
}

#[test]
fn test_signed_overflow_traps() {
    assert_crashes_with("fn main() => int:\n    let a: int = 9223372036854775807\n    let b: int = a + 1\n    return 9\n", "integer overflow in addition");
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

#[test]
fn test_bitwise_and_binds_tighter_than_equality() {
    assert_exit_code("fn main() => int:\n    let x: int = 5\n    if x & 1 == 1:\n        return 1\n    return 0\n", 1).unwrap();
}

#[test]
fn test_hex_binary_octal_and_underscore_literals() {
    assert_exit_code("fn main() => int:\n    return 0xF + 0b11 + 0o7 + 1_0\n", 35).unwrap();
}

#[test]
fn test_character_literals_are_integers() {
    assert_exit_code("fn is_digit(c: int) => bool:\n    return c >= '0' && c <= '9'\n\nfn main() => int:\n    let n: int = 'a' + '\\n'\n    if is_digit('7'):\n        return n\n    return 0\n", 107).unwrap();
}

#[test]
fn test_character_literal_fits_a_byte() {
    assert_exit_code("fn main() => int:\n    let b: int(1)- = 'A'\n    return int(b)\n", 65).unwrap();
}

#[test]
fn test_syntax_error_names_the_unexpected_character() {
    assert_compile_error("fn main() => int:\n    let x: int = 1 $ 2\n    return 0\n", "unexpected character '$'").unwrap();
}

#[test]
fn test_tuple_index_error_says_what_is_unsupported() {
    assert_compile_error("fn main() => int:\n    let t: (int, int) = (1, 2)\n    return t.0\n", "tuple element access").unwrap();
}

#[test]
fn test_untyped_parameter_is_reported_as_such() {
    assert_compile_error("fn f(a, b: int) => int:\n    return 0\n\nfn main() => int:\n    return 0\n", "Missing type annotation for parameter 'a'").unwrap();
}

#[test]
fn test_unclosed_bracket_is_reported_as_such() {
    assert_compile_error("fn main() => int:\n    let x: int = (1 + 2\n    return 0\n", "unclosed bracket").unwrap();
}

#[test]
fn test_len_of_str_and_array() {
    assert_exit_code("fn main() => int:\n    let s = \"hello\"\n    let a = [1, 2, 3]\n    return len(s) + len(a)\n", 8).unwrap();
}

#[test]
fn test_len_rejects_other_types() {
    assert_compile_error("fn main() => int:\n    let x = 5\n    return len(x)\n", "len() takes").unwrap();
}

#[test]
fn test_str_byte_index() {
    assert_exit_code("fn main() => int:\n    let s = \"hello\"\n    return int(s[1])\n", 101).unwrap();
}

#[test]
fn test_str_byte_loop_counts_letters() {
    let src = "fn main() => int:\n    let s = \"banana\"\n    let n = 0\n    for i in 0..len(s):\n        if int(s[i]) == 97:\n            n = n + 1\n    return n\n";
    assert_exit_code(src, 3).unwrap();
}

#[test]
fn test_sizeof_primitive_and_class() {
    let src = "class P:\n    x: int\n\nfn main() => int:\n    return sizeof(int) + sizeof(bool) + sizeof(P)\n";
    assert_exit_code(src, 17).unwrap();
}

#[test]
fn test_sizeof_rejects_non_types() {
    assert_compile_error("fn main() => int:\n    return sizeof(nothing)\n", "sizeof()").unwrap();
}

#[test]
fn test_field_read_through_reference() {
    let src = "class C:\n    x: int\n\nfn get(c: &C) => int:\n    return c.x\n\nfn main() => int:\n    let c = C { x: 7 }\n    return get(&c)\n";
    assert_exit_code(src, 7).unwrap();
}

#[test]
fn test_field_assign_through_reference_is_a_clear_error() {
    let src = "class C:\n    x: int\n\nfn set(c: &mut C) => void:\n    c.x = 9\n\nfn main() => int:\n    let c = C { x: 7 }\n    set(&mut c)\n    return 0\n";
    assert_compile_error(src, "through a reference is not supported").unwrap();
}

#[test]
fn test_struct_literal_accepts_narrow_int_literal() {
    let src = "class P:\n    x: int(4)+\n\nfn main() => int:\n    let p = P { x: 3 }\n    return int(p.x)\n";
    assert_exit_code(src, 3).unwrap();
}

#[test]
fn test_else_if_is_elif() {
    let src = "fn main() => int:\n    let a = 5\n    if a == 1:\n        return 1\n    else if a == 5:\n        return 2\n    else:\n        return 3\n";
    assert_exit_code(src, 2).unwrap();
}

#[test]
fn test_void_main_exits_zero() {
    assert_exit_code("fn main() => void:\n    let a = 1\n    return\n", 0).unwrap();
}

#[test]
fn test_pointer_int_casts_are_explicit() {
    let src = "use free in libc of c\nuse malloc in libc of c\n\nfn main() => int:\n    let p = malloc(8)\n    let n: int = p as int\n    let q: object = n as object\n    free(q)\n    return 0\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_null_literal_is_the_only_int_that_is_an_object() {
    let ok = "use free in libc of c\n\nfn main() => int:\n    free(0)\n    return 0\n";
    assert_exit_code(ok, 0).unwrap();
    let bad = "use free in libc of c\n\nfn main() => int:\n    free(5)\n    return 0\n";
    assert_compile_error(bad, "expected 'object', found 'int'").unwrap();
}

#[test]
fn test_str_index_out_of_bounds_stops_the_program() {
    let src = "fn main() => int:\n    let s = \"abc\"\n    let i = 9\n    return int(s[i])\n";
    assert_exit_code(src, 1).unwrap();
}

#[test]
fn test_wrapping_add_does_not_trap() {
    let src = "fn main() => int:\n    let a: int = 9223372036854775807\n    let b = wrapping_add(a, 1)\n    if b < 0:\n        return 0\n    return 1\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_wrapping_op_rejects_mixed_types() {
    assert_compile_error("fn main() => int:\n    return wrapping_add(1, 2.0)\n", "wrapping_add() takes two integers").unwrap();
}
