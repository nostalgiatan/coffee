// 构造函数测试
// 
// 测试完整类（堆分配）的构造函数调用

include!("common/mod.rs");

//=============================================================================
// 基础构造函数测试
//=============================================================================

#[test]
fn test_simple_constructor() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let p: Point = Point::new(10, 20)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_no_args() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }

fn main() => int:
    let c: Counter = Counter::new()
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_single_arg() {
    let source = r#"
class Wrapper:
    value: int
    
    fn new(v: int) => Wrapper:
        Wrapper { value: v }

fn main() => int:
    let w: Wrapper = Wrapper::new(42)
    rm w
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_multiple_args() {
    let source = r#"
class Rectangle:
    x: int
    y: int
    width: int
    height: int
    
    fn new(x: int, y: int, w: int, h: int) => Rectangle:
        Rectangle { x: x, y: y, width: w, height: h }

fn main() => int:
    let r: Rectangle = Rectangle::new(10, 20, 100, 50)
    rm r
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 构造函数逻辑测试
//=============================================================================

#[test]
fn test_constructor_with_computation() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(base: int) => Point:
        Point { x: base * 2, y: base * 3 }

fn main() => int:
    let p: Point = Point::new(10)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_condition() {
    let source = r#"
class Config:
    enabled: int
    
    fn new(flag: int) => Config:
        if flag > 0:
            return Config { enabled: 1 }
        else:
            return Config { enabled: 0 }

fn main() => int:
    let c: Config = Config::new(1)
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_local_vars() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(a: int, b: int) => Point:
        let x: int = a + b
        let y: int = a - b
        return Point::new(x, y)

fn main() => int:
    let p: Point = Point::new(10, 5)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 嵌套构造函数测试
//=============================================================================

#[test]
fn test_nested_constructor() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

class Line:
    start: Point
    end: Point
    
    fn new(x1: int, y1: int, x2: int, y2: int) => Line:
        Line { start: Point::new(x1, y1), end: Point::new(x2, y2) }

fn main() => int:
    let line: Line = Line::new(0, 0, 100, 100)
    rm line
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_returning_nested_class() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

class Rectangle:
    top_left: Point
    bottom_right: Point
    
    fn new(x1: int, y1: int, x2: int, y2: int) => Rectangle:
        Rectangle {
            top_left: Point::new(x1, y1),
            bottom_right: Point::new(x2, y2)
        }

fn main() => int:
    let r: Rectangle = Rectangle::new(0, 0, 100, 100)
    rm r
    return 0
"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 多个构造函数调用测试
//=============================================================================

#[test]
fn test_multiple_constructor_calls() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let p1: Point = Point::new(0, 0)
    let p2: Point = Point::new(10, 10)
    let p3: Point = Point::new(20, 20)
    rm p3
    rm p2
    rm p1
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_in_loop() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let i: int = 0
    while i < 3:
        let p: Point = Point::new(i, i)
        rm p
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_in_if_statement() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let flag: int = 1
    if flag > 0:
        let p: Point = Point::new(10, 20)
        rm p
    rm flag
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 构造函数参数类型测试
//=============================================================================

#[test]
fn test_constructor_with_int_params() {
    let source = r#"
class IntWrapper:
    value: int
    
    fn new(v: int) => IntWrapper:
        IntWrapper { value: v }

fn main() => int:
    let w: IntWrapper = IntWrapper::new(42)
    rm w
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_float_params() {
    let source = r#"
class FloatWrapper:
    value: float
    
    fn new(v: float) => FloatWrapper:
        FloatWrapper { value: v }

fn main() => int:
    let w: FloatWrapper = FloatWrapper::new(3.14)
    rm w
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_mixed_params() {
    let source = r#"
class Mixed:
    a: int
    b: float
    
    fn new(x: int, y: float) => Mixed:
        Mixed { a: x, b: y }

fn main() => int:
    let m: Mixed = Mixed::new(42, 3.0)
    rm m
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 构造函数和方法组合测试
//=============================================================================

#[test]
fn test_constructor_followed_by_method() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }
    
    fn move(self, dx: int, dy: int) => Point:
        self.x = self.x + dx
        self.y = self.y + dy
        return self

fn main() => int:
    let p: Point = Point::new(10, 20)
    p.move(5, 5)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_field_access() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let p: Point = Point::new(10, 20)
    let x_val: int = p.x
    let y_val: int = p.y
    rm y_val
    rm x_val
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_constructor_with_zero_values() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let p: Point = Point::new(0, 0)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_negative_values() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let p: Point = Point::new(-10, -20)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_large_values() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let p: Point = Point::new(1000000, 2000000)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_validation() {
    let source = r#"
class PositiveInt:
    value: int
    
    fn new(value: int) => PositiveInt:
        if value < 0:
            return PositiveInt { value: 0 }
        return PositiveInt { value: value }

fn main() => int:
    let p1: PositiveInt = PositiveInt::new(10)
    let p2: PositiveInt = PositiveInt::new(-5)
    rm p1, p2
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_area_computation() {
    let source = r#"
class Rectangle:
    width: int
    height: int
    area: int
    
    fn new(width: int, height: int) => Rectangle:
        Rectangle {
            width: width,
            height: height,
            area: width * height
        }

fn main() => int:
    let r: Rectangle = Rectangle::new(10, 20)
    rm r
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_nested_objects() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

class Rectangle:
    top_left: Point
    bottom_right: Point
    
    fn new(x1: int, y1: int, x2: int, y2: int) => Rectangle:
        Rectangle {
            top_left: Point::new(x1, y1),
            bottom_right: Point::new(x2, y2)
        }

fn main() => int:
    let r: Rectangle = Rectangle::new(0, 10, 10, 0)
    rm r
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_default_values() {
    let source = r#"
class Config:
    timeout: int
    retries: int
    
    fn new() => Config:
        Config {
            timeout: 30,
            retries: 3
        }
    
    fn custom(timeout: int, retries: int) => Config:
        Config {
            timeout: timeout,
            retries: retries
        }

fn main() => int:
    let c1: Config = Config::new()
    let c2: Config = Config::custom(60, 5)
    rm c1, c2
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_chaining() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn origin() => Point:
        Point::new(0, 0)
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }

fn main() => int:
    let p1: Point = Point::origin()
    let p2: Point = Point::new(10, 20)
    rm p1, p2
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_constructor_with_array() {
    let source = r#"
class Buffer:
    data: [int; 5]
    size: int
    
    fn new() => Buffer:
        Buffer {
            data: [0, 0, 0, 0, 0],
            size: 5
        }
    
    fn from_array(arr: [int; 5]) => Buffer:
        Buffer {
            data: arr,
            size: 5
        }

fn main() => int:
    let b1: Buffer = Buffer::new()
    let b2: Buffer = Buffer::from_array([1, 2, 3, 4, 5])
    rm b1, b2
    return 0

"#;
    assert_compiles(source).unwrap();
}
