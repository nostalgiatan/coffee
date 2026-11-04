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

// 未解决：match 绑定把载荷当借用传给按值参数（`len(rest)`），被调用方会 drop 它；
// 若 Enum__drop 也释放载荷就会 double free。需要先定“match 绑定的所有权”规则。
#[test]
#[ignore = "needs a match-binding ownership rule"]
fn test_enum_str_payload_is_balanced() {
    assert_balanced(&format!("{ENUMS}\nfn main() => int:\n    let e: E = E.S(\"hi\")\n    return 0\n"));
}

// 未解决：match 绑定把载荷当借用传给按值参数（`len(rest)`），被调用方会 drop 它；
// 若 Enum__drop 也释放载荷就会 double free。需要先定“match 绑定的所有权”规则。
#[test]
#[ignore = "needs a match-binding ownership rule"]
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
