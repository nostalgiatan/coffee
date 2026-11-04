// C 返回的 `char*` 不是 Coffee 拥有的 `str`：不能被悄悄绑成 owned `str` 然后 `free`。

include!("common/mod.rs");

#[test]
fn test_c_object_result_cannot_be_bound_to_str() {
    assert_compile_error(
        "use getenv in libc of c\n\nfn main() => int:\n    let s: str = getenv(\"PATH\")\n    return 0\n",
        "mismatch",
    )
    .unwrap();
}

#[test]
fn test_object_from_c_is_not_freed_at_scope_end() {
    assert_exit_code(
        "use getenv in libc of c\n\nfn main() => int:\n    let p: object = getenv(\"PATH\")\n    return 0\n",
        0,
    )
    .unwrap();
}

#[test]
fn test_strdup_result_is_object_and_freed_explicitly() {
    assert_balanced(
        "use strdup, free in libc of c\n\nfn main() => int:\n    let p: object = strdup(\"abc\")\n    free(p)\n    return 0\n",
    );
}
