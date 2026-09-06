// Mutation tests: valid snippets tweaked so the frontend must fail
// with a stable diagnostic code (not ICE / empty / E700 codegen).

include!("common/mod.rs");

// -----------------------------------------------------------------------------
// Type mutations (not the exact sources in type_error_kind_tests.rs)
// -----------------------------------------------------------------------------

#[test]
fn mutate_int_let_float_literal_is_e100() {
    // Valid: `let x: int = 1` → mutate RHS to float
    let source = r#"
fn main() => int:
    let x: int = 1.0
    return 0

"#;
    assert_compile_error(source, "E100").unwrap();
}

#[test]
fn mutate_int_let_string_literal_is_e100() {
    let source = r#"
fn main() => int:
    let x: int = "nope"
    return 0

"#;
    assert_compile_error(source, "E100").unwrap();
}

#[test]
fn mutate_int_assign_float_is_e100() {
    let source = r#"
fn main() => int:
    let x: int = 1
    x = 2.5
    return 0

"#;
    assert_compile_error(source, "E100").unwrap();
}

#[test]
fn mutate_return_true_in_int_fn_is_e100() {
    // Valid: `return 0` in `=> int` → mutate to `return true`
    let source = r#"
fn main() => int:
    return true

"#;
    assert_compile_error(source, "E100").unwrap();
}

#[test]
fn mutate_unary_not_on_int_is_e104() {
    // type_error_kind uses `-true`; this is `!` on an int
    let source = r#"
fn main() => int:
    if !0:
        return 1
    return 0

"#;
    assert_compile_error(source, "E104").unwrap();
}

#[test]
fn mutate_unary_neg_on_false_is_e104() {
    // type_error_kind uses `-true`
    let source = r#"
fn main() => int:
    let x: int = -false
    return 0

"#;
    assert_compile_error(source, "E104").unwrap();
}

#[test]
fn mutate_undefined_ident_ghost_is_e200() {
    // type_error_kind uses `missing`
    let source = r#"
fn main() => int:
    let x: int = ghost
    return 0

"#;
    assert_compile_error(source, "E200").unwrap();
}

#[test]
fn mutate_undefined_in_return_is_e200() {
    let source = r#"
fn main() => int:
    return nowhere

"#;
    assert_compile_error(source, "E200").unwrap();
}

#[test]
fn mutate_unknown_type_annotation_is_e101() {
    let source = r#"
fn main() => int:
    let x: NoSuchType = 1
    return 0

"#;
    assert_compile_error(source, "E101").unwrap();
}

#[test]
fn mutate_call_arity_is_e103() {
    let source = r#"
fn id(n: int) => int:
    return n

fn main() => int:
    return id(1, 2)

"#;
    assert_compile_error(source, "E103").unwrap();
}

#[test]
fn mutate_missing_class_field_is_e106() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let b: Box = Box { n: 1 }
    let x: int = b.nope
    return 0

"#;
    assert_compile_error(source, "E106").unwrap();
}

#[test]
fn mutate_continue_outside_loop_is_e111() {
    // type_error_kind covers `break`
    let source = r#"
fn main() => int:
    continue
    return 0

"#;
    assert_compile_error(source, "E111").unwrap();
}

#[test]
fn mutate_duplicate_let_is_e201() {
    // Duplicate `fn` names are currently accepted; duplicate `let` is E201.
    let source = r#"
fn main() => int:
    let x: int = 1
    let x: int = 2
    return 0

"#;
    assert_compile_error(source, "E201").unwrap();
}

// -----------------------------------------------------------------------------
// Multiple entry points
// -----------------------------------------------------------------------------

#[test]
fn mutate_extra_main_is_e001() {
    let source = r#"
fn main() => int:
    return 0

fn main() => int:
    return 1

"#;
    assert_compile_error(source, "E001").unwrap();
}

// -----------------------------------------------------------------------------
// Parser mutations (not the exact sources in parser_error_tests.rs)
// -----------------------------------------------------------------------------

#[test]
fn mutate_python_def_colon_is_e002() {
    // parser_error_tests uses `def foo() => int:`; Python-style `def foo():`
    let source = r#"
def foo():
    return 0

fn main() => int:
    return 0

"#;
    assert_compile_error(source, "E002").unwrap();
}

#[test]
fn mutate_hash_comment_is_e001() {
    // parser_error_tests uses `//`; Coffee also rejects `#`
    let source = r#"
# not a coffee comment
fn main() => int:
    return 0

"#;
    assert_compile_error(source, "E001").unwrap();
}

#[test]
fn mutate_slash_slash_inside_fn_is_e001() {
    let source = r#"
fn main() => int:
    // trailing style comment
    return 0

"#;
    assert_compile_error(source, "E001").unwrap();
}

#[test]
fn mutate_unclosed_if_indent_is_e003() {
    let source = r#"
fn main() => int:
    if true:
"#;
    assert_compile_error(source, "E003").unwrap();
}

#[test]
fn mutate_fn_missing_body_indent_is_e003() {
    let source = r#"
fn main() => int:
"#;
    assert_compile_error(source, "E003").unwrap();
}

// -----------------------------------------------------------------------------
// Memory contract: copy / clean out (type checker InvalidType)
// -----------------------------------------------------------------------------

#[test]
fn mutate_copy_keyword_is_e110() {
    let source = r#"
fn main() => int:
    let a: int = 1
    copy a b
    return 0

"#;
    assert_compile_error(source, "E110").unwrap();
}

#[test]
fn mutate_clean_out_keyword_is_e110() {
    let source = r#"
fn main() => int:
    let a: int = 1
    clean out
    return 0

"#;
    assert_compile_error(source, "E110").unwrap();
}
