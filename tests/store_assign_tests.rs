// `a[i] = v` 与 `*r = v`：通过元素或 `&mut` 引用写入。

include!("common/mod.rs");

fn run(body: &str, expected: i32) {
    assert_exit_code(body, expected).unwrap();
}

#[test]
fn test_index_assign_then_read() {
    run("fn main() => int:\n    let a = [1, 2, 3]\n    a[1] = 9\n    return a[1]\n", 9);
}

#[test]
fn test_index_assign_does_not_touch_neighbours() {
    run("fn main() => int:\n    let a = [1, 2, 3]\n    a[1] = 9\n    return a[0] + a[2]\n", 4);
}

#[test]
fn test_index_assign_with_computed_index_and_value() {
    run(
        "fn main() => int:\n    let a = [0, 0, 0, 0]\n    let i = 0\n    while i < 4:\n        a[i] = i * 2\n        i = i + 1\n    return a[3]\n",
        6,
    );
}

#[test]
fn test_deref_assign_through_mut_ref() {
    run(
        "fn set(r: &mut int) => void:\n    *r = 7\n\nfn main() => int:\n    let x = 1\n    set(&mut x)\n    return x\n",
        7,
    );
}

#[test]
fn test_deref_assign_uses_old_value() {
    run(
        "fn bump(r: &mut int) => void:\n    *r = *r + 1\n\nfn main() => int:\n    let x = 4\n    bump(&mut x)\n    bump(&mut x)\n    return x\n",
        6,
    );
}

#[test]
fn test_assign_through_shared_ref_is_an_error() {
    assert_compile_error(
        "fn set(r: &int) => void:\n    *r = 7\n\nfn main() => int:\n    let x = 1\n    set(&x)\n    return x\n",
        "&mut",
    )
    .unwrap();
}

#[test]
fn test_index_assign_type_mismatch_is_an_error() {
    assert_compile_error(
        "fn main() => int:\n    let a = [1, 2]\n    a[0] = \"x\"\n    return 0\n",
        "mismatch",
    )
    .unwrap();
}

#[test]
fn test_index_assign_of_resource_is_an_error() {
    assert_compile_error(
        "fn main() => int:\n    let a = [\"x\", \"y\"]\n    a[0] = \"z\"\n    return 0\n",
        "not supported",
    )
    .unwrap();
}
