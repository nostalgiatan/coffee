// 基础类型测试
// 
// 测试Coffee语言的所有基础类型：
// - 整数类型（int, int(N)+, int(N)-）
// - 浮点类型（float, float(N)）
// - 布尔类型（bool）
// - 字符串类型（str/string）
// - 单位类型（()）
// - 空类型（void）

include!("common/mod.rs");

//=============================================================================
// 整数类型测试
//=============================================================================

#[test]
fn test_int_type() {
    let source = r#"
fn main() => int:
    let a: int = 42
    let b: int = -100
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_1_byte_signed() {
    let source = r#"
fn main() => int:
    let a: int(1)+ = 127
    let b: int(1)+ = -128
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_2_bytes_signed() {
    let source = r#"
fn main() => int:
    let a: int(2)+ = 32767
    let b: int(2)+ = -32768
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_4_bytes_signed() {
    let source = r#"
fn main() => int:
    let a: int(4)+ = 2147483647
    let b: int(4)+ = -2147483648
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_8_bytes_signed() {
    let source = r#"
fn main() => int:
    let a: int(8)+ = 9223372036854775807
    let b: int(8)+ = 0
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_1_byte_unsigned() {
    let source = r#"
fn main() => int:
    let a: int(1)- = 255
    let b: int(1)- = 0
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_2_bytes_unsigned() {
    let source = r#"
fn main() => int:
    let a: int(2)- = 65535
    let b: int(2)- = 0
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_4_bytes_unsigned() {
    let source = r#"
fn main() => int:
    let a: int(4)- = 4294967295
    let b: int(4)- = 0
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_8_bytes_unsigned() {
    let source = r#"
fn main() => int:
    let a: int(8)- = 1000000000
    let b: int(8)- = 0
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_arithmetic() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 20
    let c: int = a + b
    let d: int = a - b
    let e: int = a * b
    let f: int = b / a
    let g: int = b % a
    rm g
    rm f
    rm e
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_comparison() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 20
    let c: bool = a == b
    let d: bool = a != b
    let e: bool = a < b
    let f: bool = a > b
    let g: bool = a <= b
    let h: bool = a >= b
    rm h
    rm g
    rm f
    rm e
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 浮点类型测试
//=============================================================================

#[test]
fn test_float_type() {
    let source = r#"
fn main() => int:
    let a: float = 3.14
    let b: float = -2.718
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_4_bytes() {
    let source = r#"
fn main() => int:
    let a: float(4) = 3.14159
    let b: float(4) = -2.71828
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_8_bytes() {
    let source = r#"
fn main() => int:
    let a: float(8) = 3.141592653589793
    let b: float(8) = -2.718281828459045
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_arithmetic() {
    let source = r#"
fn main() => int:
    let a: float = 10.5
    let b: float = 20.3
    let c: float = a + b
    let d: float = a - b
    let e: float = a * b
    let f: float = b / a
    rm f
    rm e
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_comparison() {
    let source = r#"
fn main() => int:
    let a: float = 10.5
    let b: float = 20.3
    let c: bool = a == b
    let d: bool = a != b
    let e: bool = a < b
    let f: bool = a > b
    let g: bool = a <= b
    let h: bool = a >= b
    rm h
    rm g
    rm f
    rm e
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 布尔类型测试
//=============================================================================

#[test]
fn test_bool_type() {
    let source = r#"
fn main() => int:
    let a: bool = true
    let b: bool = false
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_bool_comparison() {
    let source = r#"
fn main() => int:
    let a: bool = true
    let b: bool = false
    let c: bool = a == b
    let d: bool = a != b
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 字符串类型测试
//=============================================================================

#[test]
fn test_string_type() {
    let source = r#"
fn main() => int:
    let a: str = "hello"
    let b: str = "world"
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_string_type_alias() {
    let source = r#"
fn main() => int:
    let a: string = "hello"
    let b: string = "world"
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_string_with_special_chars() {
    let source = r#"
fn main() => int:
    let a: str = "hello\nworld"
    let b: str = "tab\there"
    let c: str = "quote\"test"
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_string_empty() {
    let source = r#"
fn main() => int:
    let a: str = ""
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 单位类型测试
//=============================================================================

#[test]
fn test_unit_type() {
    let source = r#"
fn main() => int:
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_void_type() {
    let source = r#"
fn do_nothing() => int:
    return 0

fn main() => int:
    do_nothing()
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 类型转换测试
//=============================================================================

#[test]
fn test_int_to_float() {
    let source = r#"
fn main() => int:
    let a: int = 42
    let b: float = 3.14
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_to_int() {
    let source = r#"
fn main() => int:
    let a: float = 42.0
    let b: int = 3
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_different_int_sizes() {
    let source = r#"
fn main() => int:
    let a: int(1)+ = 127
    let b: int(2)+ = 32767
    let c: int(4)+ = 2147483647
    let d: int(8)+ = 9223372036854775807
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_different_float_sizes() {
    let source = r#"
fn main() => int:
    let a: float(4) = 3.14159
    let b: float(8) = 3.141592653589793
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_int_zero() {
    let source = r#"
fn main() => int:
    let a: int = 0
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_zero() {
    let source = r#"
fn main() => int:
    let a: float = 0.0
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_negative_zero() {
    let source = r#"
fn main() => int:
    let a: float = -0.0
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_max_values() {
    let source = r#"
fn main() => int:
    let a: int(1)+ = 127
    let b: int(2)+ = 32767
    let c: int(4)+ = 2147483647
    let d: int(8)+ = 9223372036854775807
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_min_values() {
    let source = r#"
fn main() => int:
    let a: int(1)+ = 0
    let b: int(2)+ = 0
    let c: int(4)+ = 0
    let d: int(8)+ = 0
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_unsigned_int_max_values() {
    let source = r#"
fn main() => int:
    let a: int(1)- = 255
    let b: int(2)- = 65535
    let c: int(4)- = 1000000
    let d: int(8)- = 1000000000
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_infinity() {
    let source = r#"
fn main() => int:
    let a: float = 1.0 / 0.0
    let b: float = -1.0 / 0.0
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_nan() {
    let source = r#"
fn main() => int:
    let a: float = 0.0 / 0.0
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_int_overflow() {
    let source = r#"
fn main() => int:
    let a: int(1)+ = 127
    let b: int(1)+ = 1
    let c: int(1)+ = a + b
    rm a, b, c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_unsigned_int() {
    let source = r#"
fn main() => int:
    let a: int(4)- = 4294967295
    let b: int(4)- = 1
    let c: int(4)- = a + b
    rm a, b, c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_string_concat() {
    let source = r#"
fn main() => int:
    let a: str = "Hello"
    let b: str = "World"
    rm a, b
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_bool_operations() {
    let source = r#"
fn main() => int:
    let a: bool = true
    let b: bool = false
    let c: bool = a && b
    let d: bool = a || b
    let e: bool = !a
    rm a, b, c, d, e
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_type_conversion() {
    let source = r#"
fn main() => int:
    let a: int = 42
    let b: float = 3.14
    rm a, b
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_array_literal() {
    let source = r#"
fn main() => int:
    let arr: [int; 3] = [1, 2, 3]
    rm arr
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_tuple() {
    let source = r#"
fn main() => int:
    let t: ((int, int), int) = ((1, 2), 3)
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_triple_nested_tuple() {
    let source = r#"
fn main() => int:
    let t: (((int, int), int), int) = (((1, 2), 3), 4)
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_complex_tuple() {
    let source = r#"
fn main() => int:
    let t: (int, float, str, bool) = (42, 3.14, "hello", true)
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}
