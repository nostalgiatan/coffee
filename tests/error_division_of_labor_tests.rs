// 文档里的分工示例：可恢复错误走枚举返回，调用方 match。

include!("common/mod.rs");

const PARSE: &str = r#"
enum ParseResult:
    Ok(v: int)
    Bad(pos: int)

fn parse_digit(c: int, pos: int) => ParseResult:
    if c >= 48 && c <= 57:
        return ParseResult.Ok(c - 48)
    return ParseResult.Bad(pos)

fn code_of(c: int, pos: int) => int:
    let r: ParseResult = parse_digit(c, pos)
    match r:
        ParseResult.Ok(v) => v
        ParseResult.Bad(p) => 100 + p
"#;

#[test]
fn test_enum_result_ok_path() {
    assert_exit_code(&format!("{PARSE}\nfn main() => int:\n    return code_of(55, 3)\n"), 7).unwrap();
}

#[test]
fn test_enum_result_error_path_is_distinguishable_from_zero() {
    assert_exit_code(&format!("{PARSE}\nfn main() => int:\n    return code_of(120, 3)\n"), 103).unwrap();
}

#[test]
fn test_enum_result_is_balanced() {
    assert_balanced(&format!("{PARSE}\nfn main() => int:\n    return code_of(55, 3)\n"));
}
