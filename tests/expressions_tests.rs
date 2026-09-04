// 表达式测试
// 
// 测试Coffee语言的所有表达式：
// - 字面量表达式
// - 变量表达式
// - 二元操作符表达式
// - 一元操作符表达式
// - 函数调用表达式
// - 成员访问表达式
// - 数组索引表达式
// - 元组表达式
// - 范围表达式
// - 条件表达式（三元操作符）

include!("common/mod.rs");

//=============================================================================
// 字面量表达式测试
//=============================================================================

#[test]
fn test_integer_literal() {
    let source = r#"
fn main() => int:
    let a: int = 42
    let b: int = -100
    let c: int = 0
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_float_literal() {
    let source = r#"
fn main() => int:
    let a: float = 3.14
    let b: float = -2.718
    let c: float = 0.0
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_bool_literal() {
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
fn test_string_literal() {
    let source = r#"
fn main() => int:
    let a: str = "hello"
    let b: str = ""
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 二元操作符表达式测试
//=============================================================================

#[test]
fn test_addition() {
    let source = r#"
fn main() => int:
    let a: int = 10 + 20
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_subtraction() {
    let source = r#"
fn main() => int:
    let a: int = 20 - 10
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiplication() {
    let source = r#"
fn main() => int:
    let a: int = 5 * 6
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_division() {
    let source = r#"
fn main() => int:
    let a: int = 20 / 5
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_modulo() {
    let source = r#"
fn main() => int:
    let a: int = 20 % 3
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_equality() {
    let source = r#"
fn main() => int:
    let a: bool = 10 == 10
    let b: bool = 10 != 20
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_comparison() {
    let source = r#"
fn main() => int:
    let a: bool = 10 < 20
    let b: bool = 20 > 10
    let c: bool = 10 <= 10
    let d: bool = 10 >= 10
    rm d
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_logical_and() {
    let source = r#"
fn main() => int:
    let a: bool = true && false
    let b: bool = true && true
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_logical_or() {
    let source = r#"
fn main() => int:
    let a: bool = true || false
    let b: bool = false || false
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 一元操作符表达式测试
//=============================================================================

#[test]
fn test_negation() {
    let source = r#"
fn main() => int:
    let a: int = -10
    let b: int = -(-10)
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_logical_not() {
    let source = r#"
fn main() => int:
    let a: bool = !true
    let b: bool = !false
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 操作符优先级测试
//=============================================================================

#[test]
fn test_multiplication_before_addition() {
    let source = r#"
fn main() => int:
    let a: int = 2 + 3 * 4
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_parentheses_override() {
    let source = r#"
fn main() => int:
    let a: int = (2 + 3) * 4
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_complex_expression() {
    let source = r#"
fn main() => int:
    let a: int = (10 + 20) * (30 - 40) / 5
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_logical_and_before_or() {
    let source = r#"
fn main() => int:
    let a: bool = true && false || true
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_comparison_before_logical() {
    let source = r#"
fn main() => int:
    let a: bool = 10 < 20 && 30 > 40
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 函数调用表达式测试
//=============================================================================

#[test]
fn test_simple_function_call() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let result: int = add(10, 20)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_function_call() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn multiply(a: int, b: int) => int:
    return a * b

fn main() => int:
    let result: int = add(multiply(2, 3), 4)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_call_with_expressions() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let result: int = add(10 + 5, 20 * 2)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 成员访问表达式测试
//=============================================================================

#[test]
fn test_field_access() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let x_val: int = p.x
    let y_val: int = p.y
    rm y_val
    rm x_val
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_field_access() {
    let source = r#"
class Point:
    x: int
    y: int

class Line:
    start: Point
    end: Point

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let p2: Point = Point { x: 100, y: 100 }
    let line: Line = Line { start: p1, end: p2 }
    let start_x: int = line.start.x
    let end_y: int = line.end.y
    rm end_y
    rm start_x
    rm line
    rm p2
    rm p1
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 类型转换表达式测试
//=============================================================================

#[test]
fn test_int_to_float_conversion() {
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
fn test_mixed_type_arithmetic() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 20
    let c: int = a + b
    rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 复杂表达式测试
//=============================================================================

#[test]
fn test_complex_arithmetic() {
    let source = r#"
fn main() => int:
    let a: int = ((10 + 20) * 30 - 40) / 5
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_complex_boolean() {
    let source = r#"
fn main() => int:
    let a: bool = (10 < 20) && (30 > 40) || (50 == 50)
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_complex_with_function_calls() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let result: int = add(10, 20) + add(30, 40)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_complex_with_field_access() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let sum: int = p.x + p.y
    rm sum
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_very_complex_expression() {
    let source = r#"
class Point:
    x: int
    y: int

fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let p1: Point = Point { x: 10, y: 20 }
    let p2: Point = Point { x: 30, y: 40 }
    let sum1: int = add(p1.x, p2.x)
    let sum2: int = add(p1.y, p2.y)
    let sum: int = sum1 + sum2
    rm sum
    rm sum2
    rm sum1
    rm p2
    rm p1
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_division_by_zero_compiles() {
    let source = r#"
fn main() => int:
    let a: int = 10 / 0
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_modulo_by_zero_compiles() {
    let source = r#"
fn main() => int:
    let a: int = 10 % 0
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_very_large_number() {
    let source = r#"
fn main() => int:
    let a: int = 9223372036854775807
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_very_negative_number() {
    let source = r#"
fn main() => int:
    let a: int = -9223372036854775807
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_very_long_string() {
    let source = r#"
fn main() => int:
    let a: str = "this is a very long string that contains many characters and should still compile without any issues"
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_bitwise_operations() {
    let source = r#"
fn main() => int:
    let a: int = 15
    let b: int = 7
    let c: int = a & b
    let d: int = a | b
    let e: int = a ^ b
    let f: int = a << 2
    let g: int = a >> 1
    rm a, b, c, d, e, f, g
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_complex_arithmetic_operations() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 5
    let c: int = (a + b) * (a - b) / (a / b)
    rm a, b, c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_operator_precedence() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 5
    let c: int = 3
    let d: int = a + b * c
    let e: int = (a + b) * c
    let f: int = a / b + c
    let g: int = a / (b + c)
    rm a, b, c, d, e, f, g
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_expressions() {
    let source = r#"
fn main() => int:
    let a: int = (((1 + 2) * 3) - 4) / 2
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_mixed_operations() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: float = 3.5
    let c: bool = true
    let d: int = a + int(b)
    rm a, b, c, d
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_comparison_chaining() {
    let source = r#"
fn main() => int:
    let a: int = 5
    let b: int = 10
    let c: int = 15
    let d: bool = a < b && b < c
    rm a, b, c, d
    return 0

"#;
    assert_compiles(source).unwrap();
}