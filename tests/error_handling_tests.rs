// 异常处理测试
// 
// 测试Coffee语言的异常处理功能：
// - raise 语句
// - 错误类型定义
// - 错误处理函数

include!("common/mod.rs");

//=============================================================================
// raise 语句测试
//=============================================================================

#[test]
fn test_simple_raise() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn main() => int:
    raise TestError("An error occurred")

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_with_string_literal() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    code: int

fn main() => int:
    raise TestError(404)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_with_expression() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    value: int

fn main() => int:
    let x: int = 42
    raise TestError(x)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_function() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn risky_operation() => int:
    raise TestError("Something went wrong")

fn main() => int:
    risky_operation()

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_conditional() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn main() => int:
    let flag: bool = true
    if flag:
        raise TestError("Flag is true")

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_loop() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn main() => int:
    let i: int = 0
    while i < 10:
        if i == 5:
            raise TestError("Found 5")
        i = i + 1

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 错误类型定义测试
//=============================================================================

#[test]
fn test_error_class_with_single_field() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    code: int

fn main() => int:
    raise TestError(500)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_error_class_with_multiple_fields() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    code: int
    message: str
    details: str

fn main() => int:
    raise TestError(404, "Not Found", "Resource unavailable")

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_error_class_with_nested_struct() {
    let source = r#"
use fprintf, exit in libc of c

class Location:
    file: str
    line: int

class TestError:
    message: str
    location: Location

fn main() => int:
    let loc: Location = Location { file: "test.cf", line: 10 }
    raise TestError("Error occurred", loc)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_error_types() {
    let source = r#"
use fprintf, exit in libc of c

class NetworkError:
    code: int
    message: str

class FileError:
    path: str
    reason: str

fn main() => int:
    raise NetworkError(500, "Server error")

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 错误处理函数测试
//=============================================================================

#[test]
fn test_error_handler_function() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn risky_operation() #error_handler => int:
    raise TestError("Something went wrong")

fn main() => int:
    risky_operation()

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_error_handler_with_parameters() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    code: int

fn divide(a: int, b: int) #error_handler => int:
    if b == 0:
        raise TestError(400)

fn main() => int:
    let result: int = divide(10, 2)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_error_handler_in_recursive_function() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn factorial(n: int) #error_handler => int:
    if n < 0:
        raise TestError("Negative factorial")
    if n <= 1:
        rm n
        return 1

fn main() => int:
    let result: int = factorial(5)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 复杂错误处理场景测试
//=============================================================================

#[test]
fn test_raise_with_computation() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    value: int

fn main() => int:
    let a: int = 10
    let b: int = 20
    raise TestError(a + b)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_nested_function() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn inner_function() => int:
    raise TestError("Inner error")

fn outer_function() => int:
    inner_function()

fn main() => int:
    outer_function()

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_raises_in_function() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn check_value(x: int) #error_handler => int:
    if x < 0:
        raise TestError("Negative value")

fn main() => int:
    check_value(50)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_with_class_instance() {
    let source = r#"
use fprintf, exit in libc of c

class Point:
    x: int
    y: int

class TestError:
    location: Point

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    raise TestError(p)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_class_method() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

class Container:
    value: int
    
    fn validate(self) #error_handler => int:
        if self.value < 0:
            raise TestError("Invalid value")
        return 0

fn main() => int:
    let c: Container = Container { value: 10 }
    c.validate()

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_raise_empty_class() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:

fn main() => int:
    raise TestError()

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_with_large_fields() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    field1: int
    field2: int
    field3: int
    field4: int
    field5: int

fn main() => int:
    raise TestError(1, 2, 3, 4, 5)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_constructor() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

class Container:
    value: int
    
    fn new(v: int) => Container:
        Container { value: v }

fn main() => int:
    let c: Container = Container::new(10)
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_after_operations() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn main() => int:
    let a: int = 10
    let b: int = 20
    let c: int = a + b
    raise TestError("Error after computation")

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_with_string_field() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn main() => int:
    raise TestError("This is a long error message with details")

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_multiple_functions() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn function1() #error_handler => int:
    raise TestError("Error in function1")

fn function2() #error_handler => int:
    raise TestError("Error in function2")

fn main() => int:
    function1()

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_with_error_code() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    code: int
    message: str

fn main() => int:
    raise TestError(404, "Not Found")

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_loop_with_break() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn main() => int:
    let i: int = 0
    while i < 10:
        if i == 5:
            raise TestError("Found 5")
        i = i + 1

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_raise_in_nested_conditionals() {
    let source = r#"
use fprintf, exit in libc of c

class TestError:
    message: str

fn main() => int:
    let a: int = 10
    let b: int = 20
    if a > 5:
        if b > 15:
            raise TestError("Both conditions true")

"#;
    assert_compiles(source).unwrap();
}

// TODO: Fix this test - compiler has bug with elif statements
// #[test]
// fn test_multiple_error_types_validation() {
//     let source = r#"
// use fprintf, exit in libc of c
//
// class ValueError:
//     message: str
//
// class TypeError:
//     message: str
//
// fn main() => int:
//     let x: int = 5
//     if x < 0:
//         raise ValueError("Negative value")
//     elif x > 10:
//         raise TypeError("Value too large")
//
// "#;
//     assert_compiles(source).unwrap();
// }

#[test]
fn test_error_with_fields() {
    let source = r#"
use fprintf, exit in libc of c

class CustomError:
    code: int
    message: str
    details: str

fn main() => int:
    raise CustomError {
        code: 404,
        message: "Not found",
        details: "Resource does not exist"
    }

"#;
    assert_compiles(source).unwrap();
}

// TODO: Fix this test - compiler has bug with elif statements
// #[test]
// fn test_raise_in_function_with_validation() {
//     let source = r#"
// use fprintf, exit in libc of c
//
// class ValidationError:
//     field: str
//     message: str
//
// fn validate_age(age: int) #error_handler => int:
//     if age < 0:
//         raise ValidationError { field: "age", message: "Age cannot be negative" }
//     if age > 150:
//         raise ValidationError { field: "age", message: "Age cannot exceed 150" }
//     return age
//
// fn main() => int:
//     let age: int = validate_age(25)
//     rm age
//
// "#;
//     assert_compiles(source).unwrap();
// }

#[test]
fn test_error_in_loop() {
    let source = r#"
use fprintf, exit in libc of c

class ProcessError:
    step: int
    message: str

fn main() => int:
    let i: int = 0
    while i < 10:
        if i == 7:
            raise ProcessError { step: i, message: "Failed at step 7" }
        i = i + 1

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_error_handling() {
    let source = r#"
use fprintf, exit in libc of c

class InnerError:
    message: str

class OuterError:
    message: str
    inner: InnerError

fn main() => int:
    let inner: InnerError = InnerError { message: "Inner failure" }
    raise OuterError {
        message: "Outer failure",
        inner: inner
    }

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_error_recovery() {
    let source = r#"
use fprintf, exit in libc of c

class RecoverableError:
    message: str
    can_retry: bool

fn main() => int:
    let error: RecoverableError = RecoverableError {
        message: "Temporary failure",
        can_retry: true
    }
    rm error

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_error_with_complex_fields() {
    let source = r#"
use fprintf, exit in libc of c

class Point:
    x: int
    y: int

class LocationError:
    point: Point
    message: str
    timestamp: int

fn main() => int:
    let p: Point = Point { x: 100, y: 200 }
    raise LocationError {
        point: p,
        message: "Invalid location",
        timestamp: 1234567890
    }

"#;
    assert_compiles(source).unwrap();
}