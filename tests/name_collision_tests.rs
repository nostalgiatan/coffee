// 用户函数与 libc 同名（read / round / write ...）时，没有 `use ... in libc` 就不该串台。

include!("common/mod.rs");

#[test]
fn test_user_fn_named_like_libc_read() {
    assert_exit_code("fn read(x: int) => int:\n    return x + 1\n\nfn main() => int:\n    return read(4)\n", 5).unwrap();
}

#[test]
fn test_user_fn_named_like_libc_round() {
    assert_exit_code("fn round(x: int) => int:\n    return x * 2\n\nfn main() => int:\n    return round(4)\n", 8).unwrap();
}

#[test]
fn test_user_fn_named_like_libc_write_and_exit() {
    assert_exit_code(
        "fn write(a: int, b: int) => int:\n    return a - b\n\nfn exit(x: int) => int:\n    return x\n\nfn main() => int:\n    return write(9, 3) + exit(1)\n",
        7,
    )
    .unwrap();
}

#[test]
fn test_user_fn_named_like_libc_free_and_malloc() {
    assert_exit_code(
        "fn malloc(n: int) => int:\n    return n\n\nfn free(n: int) => int:\n    return n + 1\n\nfn main() => int:\n    return malloc(2) + free(2)\n",
        5,
    )
    .unwrap();
}
