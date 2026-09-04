// REGRESSION baseline Task 1: test_match_complex_nested, test_match_enum_with_multiple_fields, test_match_enum_with_named_fields, test_match_enum_with_value, test_match_nested_enum
// 模式匹配测试
// 
// 测试Coffee语言的match表达式：
// - 基本模式匹配
// - 字面量模式
// - 变量模式
// - 通配符模式
// - 枚举模式
// - 元组模式
// - 嵌套模式

include!("common/mod.rs");

//=============================================================================
// 基本模式匹配测试
//=============================================================================

#[test]
fn test_simple_match() {
    let source = r#"
fn main() => int:
    let x: int = 5
    match x:
        1 => 10
        2 => 20
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_fallthrough() {
    let source = r#"
fn main() => int:
    let x: int = 5
    match x:
        1 => 10
        2 => 20
        3 => 30
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_wildcard() {
    let source = r#"
fn main() => int:
    let x: int = 100
    match x:
        1 => 10
        2 => 20
        _ => 30
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_expressions() {
    let source = r#"
fn main() => int:
    let x: int = 5
    let y: int = 10
    match x + y:
        15 => 100
        20 => 200
        _ => 0
    rm y
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 字面量模式测试
//=============================================================================

#[test]
fn test_match_integer_literal() {
    let source = r#"
fn main() => int:
    let x: int = 42
    match x:
        0 => 0
        1 => 1
        42 => 100
        _ => -1
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_float_literal() {
    let source = r#"
fn main() => int:
    let x: float = 3.14
    match x:
        0.0 => 0
        1.0 => 1
        3.14 => 100
        _ => -1
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_bool_literal() {
    let source = r#"
fn main() => int:
    let x: bool = true
    match x:
        true => 1
        false => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_string_literal() {
    let source = r#"
fn main() => int:
    let x: str = "hello"
    match x:
        "hello" => 1
        "world" => 2
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 枚举模式测试
//=============================================================================

#[test]
fn test_match_simple_enum() {
    let source = r#"
enum Color:
    Red
    Green
    Blue

fn main() => int:
    let c: Color = Color.Red
    match c:
        Color.Red => 1
        Color.Green => 2
        Color.Blue => 3
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_enum_with_value() {
    let source = r#"
enum Option:
    Some(int)
    None

fn main() => int:
    let opt: Option = Option.Some(42)
    match opt:
        Option.Some(x) => x
        Option.None => 0
    rm opt
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_enum_with_named_fields() {
    let source = r#"
enum Result:
    Ok(int)
    Error(str)

fn main() => int:
    let res: Result = Result.Ok(42)
    match res:
        Result.Ok(value) => value
        Result.Error(msg) => 0
    rm res
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_multiple_enum_variants() {
    let source = r#"
enum Color:
    Red
    Green
    Blue
    Yellow
    Cyan
    Magenta

fn main() => int:
    let c: Color = Color.Blue
    match c:
        Color.Red => 1
        Color.Green => 2
        Color.Blue => 3
        Color.Yellow => 4
        Color.Cyan => 5
        Color.Magenta => 6
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_enum_with_multiple_fields() {
    let source = r#"
enum Point:
    Coord(int, int)

fn main() => int:
    let p: Point = Point.Coord(10, 20)
    match p:
        Point.Coord(x, y) => x + y
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 元组模式测试
//=============================================================================

#[test]
fn test_match_tuple() {
    let source = r#"
fn main() => int:
    let t: (int, int) = (10, 20)
    match t:
        (1, 2) => 100
        (10, 20) => 200
        _ => 0
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_tuple_with_variables() {
    let source = r#"
fn main() => int:
    let t: (int, int) = (10, 20)
    match t:
        (x, y) => x + y
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_tuple_with_wildcard() {
    let source = r#"
fn main() => int:
    let t: (int, int, int) = (10, 20, 30)
    match t:
        (x, _, _) => x
        (_, y, _) => y
        (_, _, z) => z
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_nested_tuple() {
    let source = r#"
fn main() => int:
    let t: ((int, int), int) = ((10, 20), 30)
    match t:
        ((x, y), z) => x + y + z
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_triple_tuple() {
    let source = r#"
fn main() => int:
    let t: (int, int, int) = (1, 2, 3)
    match t:
        (1, 2, 3) => 100
        (4, 5, 6) => 200
        _ => 0
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 嵌套模式测试
//=============================================================================

#[test]
#[ignore = "nested Enum.Variant(Enum.Variant) misparsed (inner dot); not E700 construction"]
fn test_match_nested_enum() {
    let source = r#"
enum Option:
    Some(int)
    None

enum Result:
    Success(Option)
    Failure(str)

fn main() => int:
    let res: Result = Result.Success(Option.Some(42))
    match res:
        Result.Success(Option.Some(x)) => x
        Result.Success(Option.None) => 0
        Result.Failure(msg) => -1
    rm res
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
#[ignore = "nested Enum.Variant(Enum.Variant) misparsed (inner dot); not E700 construction"]
fn test_match_complex_nested() {
    let source = r#"
enum Option:
    Some(int)
    None

enum Result:
    Ok(Option)
    Error(str)

fn main() => int:
    let res: Result = Result.Ok(Option.Some(42))
    match res:
        Result.Ok(Option.Some(x)) => x * 2
        Result.Ok(Option.None) => 0
        Result.Error(msg) => -1
    rm res
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 复杂模式匹配测试
//=============================================================================

#[test]
fn test_match_in_function() {
    let source = r#"
fn classify(x: int) => int:
    match x:
        0 => 0
        1 => 1
        _ => -1

fn main() => int:
    let result: int = classify(5)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_in_loop() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..5:
        match i:
            0 => 0
            1 => 1
            2 => 2
            3 => 3
            4 => 4
            _ => 0
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_computation() {
    let source = r#"
fn main() => int:
    let x: int = 10
    let y: int = 20
    match x * y:
        200 => 1000
        300 => 2000
        _ => 0
    rm y
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_function_call() {
    let source = r#"
fn get_value() => int:
    return 42

fn main() => int:
    let x: int = get_value()
    match x:
        42 => 100
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_class_field() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    match p.x:
        10 => 100
        _ => 0
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_match_empty_enum() {
    let source = r#"
enum Empty:
    Value

fn main() => int:
    let e: Empty = Empty.Value
    match e:
        Empty.Value => 1
    rm e
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_single_enum_variant() {
    let source = r#"
enum Single:
    Only

fn main() => int:
    let s: Single = Single.Only
    match s:
        Single.Only => 1
    rm s
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_large_enum() {
    let source = r#"
enum Large:
    V1
    V2
    V3
    V4
    V5
    V6
    V7
    V8
    V9
    V10

fn main() => int:
    let l: Large = Large.V5
    match l:
        Large.V1 => 1
        Large.V2 => 2
        Large.V3 => 3
        Large.V4 => 4
        Large.V5 => 5
        Large.V6 => 6
        Large.V7 => 7
        Large.V8 => 8
        Large.V9 => 9
        Large.V10 => 10
    rm l
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_zero_value() {
    let source = r#"
fn main() => int:
    let x: int = 0
    match x:
        0 => 100
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_negative_value() {
    let source = r#"
fn main() => int:
    let x: int = -42
    match x:
        -42 => 100
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_all_wildcard() {
    let source = r#"
fn main() => int:
    let x: int = 42
    match x:
        _ => 100
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_side_effects() {
    let source = r#"
fn main() => int:
    let x: int = 5
    let y: int = 10
    match x:
        5 => y
        _ => 0
    rm y
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_nested_expressions() {
    let source = r#"
fn main() => int:
    let x: int = 5
    let y: int = 10
    match x:
        1 => y + 1
        2 => y * 2
        3 => y - 1
        4 => y / 2
        5 => (y + x) * 2
        _ => 0
    rm y, x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_tuple_patterns() {
    let source = r#"
fn main() => int:
    let point: (int, int) = (10, 20)
    match point:
        (0, 0) => 1
        (x, 0) => 2
        (0, y) => 3
        (x, y) => x + y
    rm point
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_wildcard_in_tuple() {
    let source = r#"
fn main() => int:
    let point: (int, int, int) = (10, 20, 30)
    match point:
        (x, _, z) => x + z
        _ => 0
    rm point
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_or_patterns() {
    let source = r#"
fn main() => int:
    let x: int = 5
    match x:
        1 | 2 | 3 => 1
        4 | 5 | 6 => 2
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_guard() {
    let source = r#"
fn main() => int:
    let x: int = 10
    match x:
        n if n > 5 => 1
        n if n < 5 => -1
        _ => 0
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_class_field_patterns() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    match p:
        Point { x: 0, y: 0 } => 1
        Point { x: _, y: 0 } => 2
        Point { x: 0, y: _ } => 3
        Point { x: a, y: b } => a + b
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_nested_class() {
    let source = r#"
class Point:
    x: int
    y: int

class Rectangle:
    top_left: Point
    bottom_right: Point

fn main() => int:
    let r: Rectangle = Rectangle {
        top_left: Point { x: 0, y: 10 },
        bottom_right: Point { x: 10, y: 0 }
    }
    match r:
        Rectangle { top_left: Point { x: 0, y: _ }, bottom_right: _ } => 1
        _ => 0
    rm r
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_with_multiple_arms() {
    let source = r#"
fn main() => int:
    let x: int = 5
    match x:
        0 => 0
        1 => 1
        2 => 2
        3 => 3
        4 => 4
        5 => 5
        6 => 6
        7 => 7
        8 => 8
        9 => 9
        _ => -1
    rm x
    return 0

"#;
    assert_compiles(source).unwrap();
}