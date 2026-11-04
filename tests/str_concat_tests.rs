// `str + str` 拼接：结果是新分配的 owned `str`，操作数只是借用。
// 链式 `a + b + c` 只分配一次；调用结果、f-string 这类临时操作数用完即释放。

include!("common/mod.rs");

const LIBC: &str = "use strlen, strcmp in libc of c\n\n";

fn run(body: &str, expected: i32) {
    assert_exit_code(&format!("{LIBC}{body}"), expected).unwrap();
}

fn balanced(body: &str) {
    assert_balanced(&format!("{LIBC}{body}"));
}

#[test]
fn test_concat_two_variables_has_right_length_and_content() {
    run(
        "fn main() => int:\n    let a: str = \"ab\"\n    let b: str = \"cde\"\n    let c: str = a + b\n    if strcmp(c, \"abcde\") == 0:\n        return strlen(c)\n    else:\n        return 99\n",
        5,
    );
}

#[test]
fn test_concat_chain_of_three() {
    run(
        "fn main() => int:\n    let a: str = \"x\"\n    let b: str = \"yy\"\n    let c: str = a + b + \"zzz\"\n    if strcmp(c, \"xyyzzz\") == 0:\n        return strlen(c)\n    else:\n        return 99\n",
        6,
    );
}

#[test]
fn test_concat_of_two_literals() {
    run(
        "fn main() => int:\n    let c: str = \"ab\" + \"cd\"\n    if strcmp(c, \"abcd\") == 0:\n        return 4\n    else:\n        return 99\n",
        4,
    );
}

#[test]
fn test_concat_with_empty_string() {
    run(
        "fn main() => int:\n    let a: str = \"\"\n    let c: str = a + \"q\"\n    if strcmp(c, \"q\") == 0:\n        return strlen(c)\n    else:\n        return 99\n",
        1,
    );
}

#[test]
fn test_operands_are_borrowed_and_still_usable() {
    run(
        "fn main() => int:\n    let a: str = \"ab\"\n    let b: str = \"cd\"\n    let c: str = a + b\n    return strlen(a) + strlen(b) + strlen(c)\n",
        8,
    );
}

#[test]
fn test_concat_is_balanced() {
    balanced("fn main() => int:\n    let a: str = \"ab\"\n    let b: str = \"cd\"\n    let c: str = a + b\n    return 0\n");
}

#[test]
fn test_concat_chain_is_balanced() {
    balanced("fn main() => int:\n    let a: str = \"ab\"\n    let c: str = a + \"-\" + a + \"-\" + a\n    return 0\n");
}

#[test]
fn test_call_result_operand_is_freed() {
    balanced(
        "fn mk() => str:\n    return \"tmp\"\n\nfn main() => int:\n    let a: str = \"ab\"\n    let c: str = a + mk()\n    return 0\n",
    );
}

#[test]
fn test_fstring_operand_is_freed() {
    balanced("fn main() => int:\n    let n: int = 7\n    let c: str = \"n=\" + f\"{n}\"\n    return 0\n");
}

#[test]
fn test_concat_passed_directly_to_function() {
    balanced(
        "fn take(s: str) => int:\n    return strlen(s)\n\nfn main() => int:\n    let a: str = \"ab\"\n    return take(a + \"cd\")\n",
    );
}

#[test]
fn test_concat_returned_from_function() {
    balanced(
        "fn join(a: str, b: str) => str:\n    return a + b\n\nfn main() => int:\n    let r: str = join(\"x\", \"y\")\n    return 0\n",
    );
}

#[test]
fn test_concat_in_a_loop_does_not_leak() {
    balanced(
        "fn step(n: int) => int:\n    let s: str = \"a\" + \"b\"\n    return strlen(s) + n\n\nfn main() => int:\n    let i = 0\n    let acc = 0\n    while i < 5:\n        acc = acc + step(i)\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn test_concat_stored_in_class_field() {
    balanced(
        "class N:\n    name: str\n\nfn main() => int:\n    let a: str = \"ab\"\n    let n: N = N { name: a + \"cd\" }\n    return 0\n",
    );
}

#[test]
fn test_str_plus_int_is_still_an_error() {
    assert_compile_error(
        "fn main() => int:\n    let a: str = \"ab\"\n    let b = a + 1\n    return 0\n",
        "invalid operation",
    )
    .unwrap();
}
