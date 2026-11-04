// 已知泄漏缺口：枚举载荷里的资源、对拥有资源的变量重新赋值。

include!("common/mod.rs");

const ENUMS: &str = r#"
class Box:
    n: int

enum E:
    A
    S(s: str)
    B(b: Box)
"#;

// match 绑定只借用载荷，Enum__drop 是载荷的唯一所有者。
#[test]
fn test_enum_str_payload_is_balanced() {
    assert_balanced(&format!("{ENUMS}\nfn main() => int:\n    let e: E = E.S(\"hi\")\n    return 0\n"));
}

// match 绑定只借用载荷，Enum__drop 是载荷的唯一所有者。
#[test]
fn test_enum_class_payload_is_balanced() {
    assert_balanced(&format!("{ENUMS}\nfn main() => int:\n    let b: Box = Box {{ n: 1 }}\n    let e: E = E.B(b)\n    return 0\n"));
}

#[test]
fn test_enum_unit_variant_is_balanced() {
    assert_balanced(&format!("{ENUMS}\nfn main() => int:\n    let e: E = E.A\n    return 0\n"));
}

#[test]
fn test_enum_payload_matched_and_read_is_balanced() {
    assert_balanced(&format!(
        "{ENUMS}\nfn len_of(e: E) => int:\n    match e:\n        E.A => 0\n        E.S(s) => 1\n        E.B(b) => b.n\n\nfn main() => int:\n    let e: E = E.S(\"hi\")\n    return len_of(e)\n"
    ));
}

#[test]
fn test_reassign_str_local_frees_old_value() {
    assert_balanced("fn main() => int:\n    let s: str = \"a\"\n    s = \"b\"\n    return 0\n");
}

#[test]
fn test_reassign_str_in_loop_frees_each_old_value() {
    assert_balanced("fn main() => int:\n    let s: str = \"a\"\n    let i: int = 0\n    while i < 3:\n        s = \"b\" + \"c\"\n        i = i + 1\n    return 0\n");
}

#[test]
fn test_reassign_class_local_frees_old_value() {
    assert_balanced("class Box:\n    n: int\n\nfn main() => int:\n    let b: Box = Box { n: 1 }\n    b = Box { n: 2 }\n    return b.n\n");
}

#[test]
fn test_reassign_class_field_frees_old_value() {
    assert_balanced("class In:\n    s: str\n\nclass Out:\n    i: In\n\nfn main() => int:\n    let o: Out = Out { i: In { s: \"a\" } }\n    o.i = In { s: \"b\" }\n    return 0\n");
}

#[test]
fn test_reassign_str_field_frees_old_value() {
    assert_balanced("class N:\n    s: str\n\nfn main() => int:\n    let n: N = N { s: \"a\" }\n    n.s = \"b\"\n    return 0\n");
}

#[test]
fn test_enum_payload_dropped_when_arm_does_not_bind_it() {
    assert_balanced(&format!(
        "{ENUMS}\nfn f(e: E) => int:\n    match e:\n        E.S(s) => 1\n        _ => 0\n\nfn main() => int:\n    let a: E = E.S(\"x\")\n    let n: int = f(a)\n    let b: Box = Box {{ n: 2 }}\n    let c: E = E.B(b)\n    return f(c) + n\n"
    ));
}

#[test]
fn test_enum_payload_moved_out_by_binding_into_callee() {
    assert_balanced(&format!(
        "{ENUMS}\nfn eat(s: str) => int:\n    return 1\n\nfn f(e: E) => int:\n    match e:\n        E.S(s) => eat(s)\n        _ => 0\n\nfn main() => int:\n    let a: E = E.S(\"x\")\n    return f(a)\n"
    ));
}
