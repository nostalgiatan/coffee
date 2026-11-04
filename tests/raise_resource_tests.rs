// `raise` 与资源：监听器路径和中止路径都要释放作用域里仍然拥有的值。

include!("common/mod.rs");

const LISTENER: &str = r#"
class Box:
    n: int

fn on_err(err: Error) => int:
    return 7
"#;

#[test]
fn test_raise_calls_listener_and_returns_its_value() {
    assert_exit_code(
        &format!("{LISTENER}\nfn f() #on_err => int:\n    raise Error(1)\n\nfn main() => int:\n    return f()\n"),
        7,
    )
    .unwrap();
}

#[test]
fn test_raise_frees_str_and_class_locals() {
    assert_balanced(&format!(
        "{LISTENER}\nfn f() #on_err => int:\n    let s: str = \"ab\"\n    let b: Box = Box {{ n: 1 }}\n    raise Error(1)\n\nfn main() => int:\n    return f()\n"
    ));
}

#[test]
fn test_raise_after_local_was_moved_does_not_double_free() {
    assert_balanced(&format!(
        "{LISTENER}\nfn eat(b: Box) => int:\n    return b.n\n\nfn f() #on_err => int:\n    let b: Box = Box {{ n: 1 }}\n    let k: int = eat(b)\n    raise Error(k)\n\nfn main() => int:\n    return f()\n"
    ));
}

#[test]
fn test_raise_in_a_branch_frees_only_what_exists_there() {
    assert_balanced(&format!(
        "{LISTENER}\nfn f(x: int) #on_err => int:\n    let b: Box = Box {{ n: x }}\n    if x > 0:\n        raise Error(x)\n    return b.n\n\nfn main() => int:\n    let a: int = f(1)\n    let c: int = f(0)\n    return a + c\n"
    ));
}

#[test]
fn test_raise_with_heap_note_is_balanced() {
    assert_balanced(&format!(
        "{LISTENER}\nfn f() #on_err => int:\n    let note: str = \"a\" + \"b\"\n    raise Error {{ code: 1, note: note, e: 0 }}\n\nfn main() => int:\n    return f()\n"
    ));
}

#[test]
fn test_raise_without_listener_aborts_with_status_one() {
    assert_exit_code(
        "use fprintf, exit in libc of c\n\nfn main() => int:\n    raise Error(1)\n",
        1,
    )
    .unwrap();
}

#[test]
fn test_raise_without_listener_needs_no_libc_import() {
    assert_exit_code("fn main() => int:\n    raise Error(1)\n", 1).unwrap();
}

const CODE_LISTENER: &str = r#"
fn on_code(err: Error) => int:
    return err.code
"#;

#[test]
fn test_listener_sees_runtime_error_code() {
    assert_exit_code(
        &format!("{CODE_LISTENER}\nfn f(k: int) #on_code => int:\n    raise Error(k)\n\nfn main() => int:\n    return f(9)\n"),
        9,
    )
    .unwrap();
}

#[test]
fn test_listener_sees_literal_error_code() {
    assert_exit_code(
        &format!("{CODE_LISTENER}\nfn f() #on_code => int:\n    raise Error(4)\n\nfn main() => int:\n    return f()\n"),
        4,
    )
    .unwrap();
}

#[test]
fn test_listener_can_read_heap_note() {
    assert_exit_code(
        "fn on_note(err: Error) => int:\n    if err.note == \"ab\":\n        return 1\n    return 0\n\nfn f() #on_note => int:\n    let note: str = \"a\" + \"b\"\n    raise Error { code: 1, note: note, e: 0 }\n\nfn main() => int:\n    return f()\n",
        1,
    )
    .unwrap();
}

#[test]
fn test_raise_with_temporary_note_is_balanced() {
    assert_balanced(&format!(
        "{LISTENER}\nfn f() #on_err => int:\n    raise Error {{ code: 1, note: \"x\" + \"y\", e: 0 }}\n\nfn main() => int:\n    return f()\n"
    ));
}
