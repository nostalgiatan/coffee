// 语法错误要指到出错的那一行，而不是所在 `fn` 的第一行。

include!("common/mod.rs");

fn error_line(source: &str) -> String {
    match compile_coffee(source, &["--emit-llvm", "-o", "/dev/null"]) {
        Ok(r) => r.stderr,
        Err(e) => e,
    }
}

#[test]
fn test_unclosed_paren_points_at_its_line() {
    let out = error_line("fn main() => int:\n    let a: int = 1\n    let b: int = 2\n    let c: int = (3 +\n    return a\n");
    assert!(out.contains("line 4"), "{out}");
}

#[test]
fn test_bad_statement_in_second_function_points_at_its_line() {
    let out = error_line("fn f() => int:\n    return 1\n\nfn main() => int:\n    let a: int = 1\n    let b: int = * *\n    return a\n");
    assert!(out.contains("line 6"), "{out}");
}

#[test]
fn test_valid_program_is_not_flagged() {
    assert_exit_code("fn main() => int:\n    let a: int = (1 +\n        2)\n    return a\n", 3).unwrap();
}
