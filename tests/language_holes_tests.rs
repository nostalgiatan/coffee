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

#[test]
fn test_buf_read_write_and_len() {
    let src = "fn main() => int:\n    let b: buf<int> = new_buf(int, 4)\n    b[0] = 5\n    b[3] = 10\n    return b[0] + b[3] + b[1] + len(b) - 4\n";
    assert_exit_code(src, 15).unwrap();
}

#[test]
fn test_buf_is_zero_initialised() {
    let src = "fn main() => int:\n    let b: buf<float> = new_buf(float, 3)\n    return int(b[2])\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_buf_index_out_of_bounds_stops_the_program() {
    for index in ["2", "0 - 1"] {
        let src = format!("fn main() => int:\n    let b: buf<int> = new_buf(int, 2)\n    let i = {}\n    return b[i]\n", index);
        let r = compile_and_run_coffee(&src).unwrap();
        assert_eq!(r.exit_code, 1, "index {index}");
        assert!(r.stdout.contains("out of bounds"), "index {index}: {:?}", r.stdout);
    }
}

#[test]
fn test_buf_negative_length_stops_the_program() {
    let src = "fn main() => int:\n    let n = 0 - 5\n    let b: buf<int> = new_buf(int, n)\n    return 0\n";
    let r = compile_and_run_coffee(src).unwrap();
    assert_eq!(r.exit_code, 1);
    assert!(r.stdout.contains("negative buffer length"), "{:?}", r.stdout);
}

#[test]
fn test_buf_resize_keeps_contents_and_zero_fills() {
    let src = "fn main() => int:\n    let b: buf<int> = new_buf(int, 2)\n    b[1] = 7\n    b = buf_resize(b, 10)\n    return b[1] + b[9] + len(b) - 10\n";
    assert_exit_code(src, 7).unwrap();
}

#[test]
fn test_buf_moves_through_calls_and_is_freed_once() {
    let src = "fn fill(b: buf<int>) => buf<int>:\n    b[0] = 9\n    return b\n\nfn main() => int:\n    let b: buf<int> = new_buf(int, 3)\n    let c: buf<int> = fill(b)\n    return c[0]\n";
    assert_exit_code(src, 9).unwrap();
    assert_balanced(src);
}

#[test]
fn test_buf_is_freed_at_scope_end_and_after_resize() {
    assert_balanced("fn main() => int:\n    let b: buf<int> = new_buf(int, 4)\n    b = buf_resize(b, 8)\n    b[7] = 1\n    return 0\n");
    assert_balanced("fn main() => int:\n    let b: buf<int> = new_buf(int, 4)\n    rm b\n    return 0\n");
}

#[test]
fn test_buf_in_a_class_field_is_dropped_with_the_class() {
    let src = "class V:\n    data: buf<int>\n\nfn main() => int:\n    let v = V { data: new_buf(int, 4) }\n    v.data[2] = 4\n    return v.data[2]\n";
    assert_exit_code(src, 4).unwrap();
    assert_balanced(src);
}

#[test]
fn test_buf_cannot_be_copied() {
    assert_compile_error("fn main() => int:\n    let b: buf<int> = new_buf(int, 2)\n    let c = b\n    return b[0]\n", "cannot copy resource").unwrap();
}

#[test]
fn test_buf_elements_are_plain_values() {
    assert_compile_error("fn main() => int:\n    let b: buf<str> = new_buf(str, 2)\n    return 0\n", "buf<T> needs one element type").unwrap();
}

#[test]
fn test_buf_sized_element_and_raw_pointer_escape() {
    assert_exit_code("fn main() => int:\n    let b: buf<u8> = new_buf(u8, 4)\n    b[1] = 200\n    return int(b[1])\n", 200).unwrap();
    let src = "use memset in libc of c\n\nfn main() => int:\n    let b: buf<int> = new_buf(int, 4)\n    memset(buf_ptr(b), 255, 8)\n    if b[0] == 0 - 1:\n        return 0\n    return 1\n";
    assert_exit_code(src, 0).unwrap();
}

const SUM_SLICE: &str = "fn sum(xs: [int]) => int:\n    let t = 0\n    for x in xs:\n        t = t + x\n    return t\n\n";

#[test]
fn test_borrowed_buf_reads_and_writes() {
    let read = "fn first(b: &buf<int>) => int:\n    return b[0] + len(b)\n\nfn main() => int:\n    let b: buf<int> = new_buf(int, 2)\n    b[0] = 5\n    return first(&b)\n";
    assert_exit_code(read, 7).unwrap();
    assert_balanced(read);
    let write = "fn set(b: &mut buf<int>) => void:\n    b[1] = 8\n\nfn main() => int:\n    let b: buf<int> = new_buf(int, 2)\n    set(&mut b)\n    return b[1]\n";
    assert_exit_code(write, 8).unwrap();
}

#[test]
fn test_borrowed_buf_is_a_slice_view() {
    let src = format!("{SUM_SLICE}fn main() => int:\n    let b: buf<int> = new_buf(int, 3)\n    b[0] = 1\n    b[1] = 2\n    b[2] = 3\n    return sum(&b)\n");
    assert_exit_code(&src, 6).unwrap();
    assert_balanced(&src);
}

#[test]
fn test_subslice_ranges() {
    let main = |tail: &str| format!("{SUM_SLICE}fn main() => int:\n    let b: buf<int> = new_buf(int, 4)\n    b[0] = 1\n    b[1] = 2\n    b[2] = 3\n    b[3] = 4\n    return {tail}\n");
    assert_exit_code(&main("sum(b[1..3])"), 5).unwrap();
    assert_exit_code(&main("sum(b[2..])"), 7).unwrap();
    assert_exit_code(&main("sum(b[..2])"), 3).unwrap();
    assert_exit_code(&main("len(b[1..3])"), 2).unwrap();
    assert_balanced(&main("sum(b[1..3])"));
}

#[test]
fn test_subslice_out_of_range_stops_the_program() {
    let src = "fn n(xs: [int]) => int:\n    return len(xs)\n\nfn main() => int:\n    let b: buf<int> = new_buf(int, 4)\n    let i = 5\n    return n(b[1..i])\n";
    let r = compile_and_run_coffee(src).unwrap();
    assert_eq!(r.exit_code, 1);
    assert!(r.stdout.contains("Slice range out of bounds"), "{:?}", r.stdout);
}

#[test]
fn test_subslice_cannot_be_stored() {
    assert_compile_error("fn main() => int:\n    let b: buf<int> = new_buf(int, 4)\n    let s = b[1..3]\n    return 0\n", "a subslice cannot be stored").unwrap();
}

#[test]
fn test_reassigning_a_moved_variable_rearms_its_drop() {
    assert_balanced("fn id(s: str) => str:\n    return s\n\nfn main() => int:\n    let s: str = \"a\" + \"b\"\n    s = id(s)\n    return 0\n");
    let src = "fn grow(toks: buf<int>, need: int) => buf<int>:\n    let t = toks\n    if need > len(t):\n        t = buf_resize(t, need * 2)\n    return t\n\nfn main() => int:\n    let b: buf<int> = new_buf(int, 2)\n    b = grow(b, 5)\n    b = grow(b, 1)\n    return len(b)\n";
    assert_exit_code(src, 10).unwrap();
    assert_balanced(src);
}

/// A lexer written in Coffee (`tests/coffee_programs/lexer.cf`): bytes of a `str`, a growing
/// `buf<int>` of token triples, and no leaks. 13 = the sum of the token kinds in the input.
#[test]
fn test_lexer_written_in_coffee() {
    let src = include_str!("coffee_programs/lexer.cf");
    assert_exit_code(src, 13).unwrap();
    assert_balanced(src);
}

/// Keywords, `==`, strings, comments and a token class that grows its own `buf<int>`:
/// `&str` parameters, string literals lent to them, and methods that resize a buffer field.
#[test]
fn test_token_lexer_written_in_coffee() {
    let src = include_str!("coffee_programs/lexer_tokens.cf");
    assert_exit_code(src, 85).unwrap();
    assert_balanced(src);
}

#[test]
fn test_str_borrow_reads_bytes_and_length() {
    let src = "fn at(s: &str, i: int) => int:\n    return int(s[i]) + len(s)\n\nfn main() => int:\n    let s: str = \"hel\" + \"lo\"\n    let a = at(&s, 1)\n    let b = at(&s, 0)\n    return b - a\n";
    assert_exit_code(src, 3).unwrap();
    assert_balanced(src);
}

#[test]
fn test_string_literal_can_be_lent_to_a_str_borrow() {
    assert_exit_code("fn n(s: &str) => int:\n    return len(s)\n\nfn main() => int:\n    return n(\"hello\")\n", 5).unwrap();
}

#[test]
fn test_subtracting_a_negative_is_not_an_overflow() {
    let src = "fn n(x: int) => int:\n    return 0 - x\n\nfn main() => int:\n    let a = 3 - n(5)\n    let b = n(5) - n(3)\n    return a + b - 6\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_subtraction_that_really_overflows_still_traps() {
    assert_crashes_with("fn big() => int:\n    return 9223372036854775807\n\nfn main() => int:\n    let b = 0 - 5\n    return big() - b\n", "integer overflow in subtraction");
}

#[test]
fn test_else_belongs_to_the_if_nested_in_the_body() {
    let src = "fn h(c: int) => int:\n    let j = 0\n    if c == 2:\n        let l = 2\n        if l == 0:\n            j = 1\n        else:\n            j = 5\n    else:\n        j = 9\n    return j\n\nfn main() => int:\n    return h(2) * 10 + h(3) - 59\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_buf_resize_argument_declared_in_a_branch() {
    let src = "fn f(b: buf<int>) => int:\n    if 1 > len(b):\n        let cap = 4\n        b = buf_resize(b, cap)\n    return len(b)\n\nfn main() => int:\n    return f(new_buf(int, 0)) - 4\n";
    assert_exit_code(src, 0).unwrap();
    assert_balanced(src);
}

#[test]
fn test_buf_parameter_is_freed_by_the_callee() {
    assert_balanced("fn f(b: buf<int>) => int:\n    return len(b)\n\nfn main() => int:\n    return f(new_buf(int, 1)) - 1\n");
    assert_balanced("fn f(b: buf<int>) => int:\n    if 1 > len(b):\n        b = buf_resize(b, 4)\n    return len(b)\n\nfn main() => int:\n    return f(new_buf(int, 0)) - 4\n");
}

#[test]
fn test_buf_parameter_returned_is_not_freed_twice() {
    assert_balanced("fn id(b: buf<int>) => buf<int>:\n    return b\n\nfn main() => int:\n    let b = id(new_buf(int, 2))\n    return len(b) - 2\n");
}

#[test]
fn test_grouping_parens_may_contain_a_call_with_commas() {
    let src = "fn add(a: int, b: int) => int:\n    return a + b\n\nfn main() => int:\n    return 10 - (add(1, 2) + 1) - 6\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_use_in_module_brings_the_types_along() {
    let dir = std::env::temp_dir().join(format!("coffee_use_types_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kinds.cf"), "enum K:\n    A\n    B\n\nfn is_b(k: K) => bool:\n    return k == K.B\n").unwrap();
    std::fs::write(dir.join("main.cf"), "use is_b in kinds\n\nfn main() => int:\n    let k = K.B\n    if is_b(k):\n        return 0\n    return 1\n").unwrap();
    let out = std::process::Command::new(coffee_bin(&std::env::current_dir().unwrap()))
        .current_dir(&dir)
        .args(["--test-mode", "--jit", "main.cf"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(text.contains("exited with code: 0"), "{}", text);
}

#[test]
fn test_non_ascii_string_literals_keep_their_utf8_bytes() {
    // "单行" is 6 bytes; the first is 0xE5, which `int()` must widen as 229, not -27.
    let src = "fn main() => int:\n    let s = \"\u{5355}\u{884c}\"\n    return len(&s) * 1000 + int(s[0]) - 6229\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_non_ascii_text_in_a_let_does_not_panic_the_parser() {
    assert_exit_code("fn main() => int:\n    let s = \"x /#/ \u{5355}\"\n    return len(&s) - 9\n", 0).unwrap();
}

#[test]
fn test_comment_marker_after_an_escaped_quote_in_a_string_is_text() {
    let src = "fn show(s: &str) => int:\n    return len(s)\n\nfn main() => int:\n    let n = show(\"{ \\\" /#/\")\n    return n - 7\n";
    assert_exit_code(src, 0).unwrap();
}

#[test]
fn test_int_cast_widens_u8_without_sign_extension() {
    assert_exit_code("fn main() => int:\n    let s = \"\\xffA\"\n    return int(s[0]) - 255 + int(s[1]) - 65\n", 0).unwrap();
}

#[test]
fn test_bool_assigned_from_comparison_negates_correctly() {
    // an i1 widened to the bool slot must be 0/1, not -1, or `!b` is true for both
    assert_exit_code("fn pick(n: int, k: int) => int:\n    let has = n >= 0\n    if k == 1:\n        has = true\n    if !has:\n        return 0\n    return 1\n\nfn main() => int:\n    return pick(2, 5) + pick(2, 1) * 10 + pick(-2, 5) * 100\n", 11).unwrap();
}

#[test]
fn test_index_store_evaluates_the_value_before_the_buffer_address() {
    // the value reallocates the buffer the element lives in; the store must hit the new one
    assert_exit_code("class H:\n    a: buf<int>\n\n    fn grow(self) => int:\n        self.a = buf_resize(self.a, 100)\n        return 7\n\n    fn put(self) => void:\n        self.a[0] = self.grow()\n\nfn main() => int:\n    let h = H { a: new_buf(int, 2) }\n    h.put()\n    return h.a[0]\n", 7).unwrap();
}

#[test]
fn test_string_literal_argument_to_a_str_ref_method_is_lent() {
    assert_exit_code("class W:\n    s: str\n\n    fn is_word(self, at: int, w: &str) => int:\n        return len(w) + at\n\nfn main() => int:\n    let w = W { s: \"x\" + \"y\" }\n    return w.is_word(4, \"self\")\n", 8).unwrap();
}

#[test]
fn test_enum_is_usable_inside_a_class_method() {
    assert_exit_code("enum K:\n    A\n    B\n\nclass P:\n    n: int\n\n    fn kind(self) => int:\n        return int(K.B) + self.n\n\nfn main() => int:\n    let p = P { n: 1 }\n    return p.kind()\n", 2).unwrap();
}

#[test]
fn test_trailing_whitespace_after_an_if_header_keeps_the_next_statement() {
    assert_exit_code("fn main() => int:\n    let x = 1\n    if x == 1:  \n        x = 5\n    return x\n", 5).unwrap();
}

#[test]
fn test_borrowing_a_string_before_moving_it_is_balanced() {
    assert_balanced("fn eat(s: str) => int:\n    return len(s)\n\nfn main() => int:\n    let s = \"a\" + \"b\"\n    let n = len(&s)\n    return eat(s) + n - 4\n");
}

#[test]
fn test_method_may_take_its_own_class_by_reference() {
    assert_exit_code("class A:\n    n: int\n\n    fn same(self, o: &A) => bool:\n        return self.n == o.n\n\nfn main() => int:\n    let a = A { n: 1 }\n    let b = A { n: 1 }\n    if a.same(&b):\n        return 0\n    return 1\n", 0).unwrap();
}

#[test]
fn test_function_declared_after_use_may_take_a_class() {
    assert_exit_code("fn main() => int:\n    let b = B { n: 3 }\n    return get(&b)\n\nfn get(b: &B) => int:\n    return b.n\n\nclass B:\n    n: int\n", 3).unwrap();
}

#[test]
fn test_method_called_through_a_class_reference_with_a_loop_variable() {
    assert_exit_code("class S:\n    t: str\n\n    fn at(self, i: int) => int:\n        return byte(&self.t, i)\n\nfn byte(s: &str, i: int) => int:\n    return int(s[i])\n\nfn same(a: &S, w: &str) => bool:\n    for k in 0..len(w):\n        if a.at(k) != byte(w, k):\n            return false\n    return true\n\nfn main() => int:\n    let a = S { t: \"ab\" + \"c\" }\n    if same(&a, \"abc\"):\n        return 0\n    return 1\n", 0).unwrap();
}

#[test]
fn test_comparison_passed_as_a_bool_argument_to_a_method() {
    // the comparison is an i1; the method's `bool` parameter is a byte
    assert_exit_code("fn f(x: int, b: bool) => int:\n    if b:\n        return x\n    return 0\n\nclass C:\n    n: int\n\n    fn g(self, b: bool) => int:\n        if b:\n            return 1\n        return 0\n\nfn mk() => buf<int>:\n    let b: buf<int> = new_buf(int, 2)\n    b[0] = 1\n    return b\n\nfn main() => int:\n    let v = mk()\n    let c = C { n: 1 }\n    return f(5, v[0] == 1) + c.g(v[0] == 1) * 10\n", 15).unwrap();
}

#[test]
fn test_class_may_define_a_method_named_assign() {
    // `assign` is built in for `p.assign("field", v)`; a method of the class wins
    assert_exit_code("class A:\n    n: int\n\n    fn assign(self, a: int, b: int, c: int, d: int, e: int) => int:\n        return a + b + c + d + e + self.n\n\n    fn go(self) => int:\n        return self.assign(1, 2, 3, 4, 5)\n\nfn main() => int:\n    let a = A { n: 1 }\n    return a.go()\n", 16).unwrap();
}

#[test]
fn test_field_read_wins_over_a_method_of_the_same_name() {
    // `f.ret` reads the field; `f.ret(4)` calls the method
    assert_exit_code("class F:\n    ret: int\n\n    fn ret(self, v: int) => int:\n        return v + self.ret\n\nfn make() => F:\n    return F { ret: 3 }\n\nfn main() => int:\n    let f = F { ret: 5 }\n    return f.ret + f.ret(4) + make().ret\n", 17).unwrap();
}

#[test]
fn test_early_return_of_a_class_does_not_move_it_for_later_code() {
    assert_exit_code("class D:\n    n: int\n\nfn make(nb: int) => D:\n    let d = D { n: 1 }\n    if nb == 0:\n        return d\n    d.n = 4\n    return d\n\nfn main() => int:\n    let a = make(0)\n    let b = make(3)\n    return a.n * 10 + b.n\n", 14).unwrap();
}

#[test]
fn test_early_return_of_a_string_is_balanced() {
    assert_balanced("fn pick(n: int) => str:\n    let s = \"a\" + \"b\"\n    if n == 0:\n        return s\n    let t = s + \"c\"\n    return t\n\nfn main() => int:\n    let x = pick(0)\n    let y = pick(1)\n    return len(&x) + len(&y) - 5\n");
}

#[test]
fn test_buf_stored_into_a_field_is_moved_not_freed_twice() {
    assert_balanced("class H:\n    nm: buf<int>\n\n    fn set(self, k: int) => void:\n        let b: buf<int> = new_buf(int, k)\n        for i in 0..k:\n            b[i] = i\n        self.nm = b\n\nfn main() => int:\n    let h = H { nm: new_buf(int, 2) }\n    h.set(5)\n    return len(h.nm) - 5\n");
}

#[test]
fn test_buf_declared_in_a_branch_is_not_freed_when_the_branch_is_skipped() {
    assert_balanced("fn f(k: int, v: int) => int:\n    if k == 2:\n        if v < 0:\n            return 1\n        let offs: buf<int> = new_buf(int, 4)\n        offs[0] = 3\n        return offs[0]\n    return 0\n\nfn main() => int:\n    return f(1, 1) + f(2, -1) + f(2, 1) - 4\n");
}

#[test]
fn test_same_name_declared_with_another_type_in_a_disjoint_scope_gets_its_own_slot() {
    assert_balanced("class T:\n    n: int\n    b: buf<int>\n\nfn mk(k: int) => T:\n    return T { n: k, b: new_buf(int, 2) }\n\nfn f(k: int) => int:\n    if k == 1:\n        let x = 5\n        return x\n    if k == 2:\n        let x = mk(3)\n        return x.n\n    let x = \"ab\" + \"cd\"\n    return len(x)\n\nfn main() => int:\n    return f(1) + f(2) + f(3) - 12\n");
}
