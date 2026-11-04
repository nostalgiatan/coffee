// 泛型函数按实参推断 `T` 并真正运行。

include!("common/mod.rs");

#[test]
fn test_inferred_generic_fn_runs() {
    assert_exit_code("fn id<T>(x: T) => T:\n    return x\n\nfn main() => int:\n    let y: int = id(5)\n    return y\n", 5).unwrap();
}

#[test]
fn test_inferred_generic_fn_with_let_inference() {
    assert_exit_code("fn id<T>(x: T) => T:\n    return x\n\nfn main() => int:\n    let y = id(6)\n    return y\n", 6).unwrap();
}

#[test]
fn test_generic_fn_instantiated_at_two_types() {
    assert_exit_code(
        "fn id<T>(x: T) => T:\n    return x\n\nfn main() => int:\n    let a: int = id(3)\n    let b: float = id(1.5)\n    if b > 1.0:\n        return a\n    return 0\n",
        3,
    )
    .unwrap();
}
