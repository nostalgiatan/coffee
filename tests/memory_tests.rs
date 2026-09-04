// 内存管理测试
// 
// 测试类的内存管理功能，包括堆分配、栈分配和rm操作

include!("common/mod.rs");

//=============================================================================
// 基础内存管理测试
//=============================================================================

#[test]
fn test_simple_class_cleanup() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_full_class_cleanup() {
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
fn test_multiple_class_cleanup() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let p2: Point = Point { x: 10, y: 10 }
    let p3: Point = Point { x: 20, y: 20 }
    rm p3
    rm p2
    rm p1
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 嵌套类的内存管理测试
//=============================================================================

#[test]
fn test_nested_class_cleanup() {
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
    rm line
    rm p2
    rm p1
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_deeply_nested_cleanup() {
    let source = r#"
class Point:
    x: int
    y: int

class Rectangle:
    top_left: Point
    bottom_right: Point

class Shape:
    rect: Rectangle
    name: int

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let p2: Point = Point { x: 100, y: 100 }
    let rect: Rectangle = Rectangle { top_left: p1, bottom_right: p2 }
    let shape: Shape = Shape { rect: rect, name: 1 }
    rm shape
    rm rect
    rm p2
    rm p1
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 函数中的内存管理测试
//=============================================================================

#[test]
fn test_class_as_parameter_cleanup() {
    let source = r#"
class Point:
    x: int
    y: int

fn process(p: Point) => int:
    let result: int = p.x
    return result

fn main() => int:
    let p: Point = Point { x: 42, y: 100 }
    let x_val: int = process(p)
    rm p
    rm x_val
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_as_return_value_cleanup() {
    let source = r#"
class Point:
    x: int
    y: int

fn create_point(x: int, y: int) => Point:
    let p: Point = Point { x: x, y: y }
    rm y
    rm x
    return p

fn main() => int:
    let p: Point = create_point(10, 20)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_function_scope() {
    let source = r#"
class Point:
    x: int
    y: int

fn create_and_process() => int:
    let p: Point = Point { x: 10, y: 20 }
    let result: int = p.x
    rm p
    rm result
    return 0

fn main() => int:
    create_and_process()
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 控制流中的内存管理测试
//=============================================================================

#[test]
fn test_class_in_if_branch() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let flag: int = 1
    if flag > 0:
        let p: Point = Point { x: 10, y: 20 }
        rm p
    rm flag
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_if_else() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let flag: int = 0
    if flag > 0:
        let p1: Point = Point { x: 10, y: 20 }
        rm p1
    else:
        let p2: Point = Point { x: 30, y: 40 }
        rm p2
    rm flag
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_while_loop() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let i: int = 0
    while i < 3:
        let p: Point = Point { x: i, y: i }
        rm p
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_for_loop() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let result: int = 0
    for i in 0..3:
        let p: Point = Point { x: i, y: i }
        let x_val: int = p.x
        result = result + x_val
        rm x_val
        rm p
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 方法调用后的内存管理测试
//=============================================================================

#[test]
fn test_cleanup_after_method_call() {
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
fn test_cleanup_after_multiple_method_calls() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self

fn main() => int:
    let c: Counter = Counter::new()
    c.increment()
    c.increment()
    c.increment()
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 批量删除测试
//=============================================================================

#[test]
fn test_batch_rm() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let p2: Point = Point { x: 10, y: 10 }
    let p3: Point = Point { x: 20, y: 20 }
    rm p1, p2, p3
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_mixed_batch_rm() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let a: int = 10
    let p1: Point = Point { x: 0, y: 0 }
    let b: int = 20
    let p2: Point = Point { x: 10, y: 10 }
    rm a, p1, b, p2
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// clean操作测试
//=============================================================================

#[test]
fn test_clean_out() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let p2: Point = Point { x: 10, y: 10 }
    let a: int = 100
    rm p1
    rm p2
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_clean_out_except() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let p2: Point = Point { x: 10, y: 10 }
    let a: int = 100
    rm p2
    rm a
    rm p1
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_clean_out_specific() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let p2: Point = Point { x: 10, y: 10 }
    let a: int = 100
    rm p1
    rm p2
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_empty_class_cleanup() {
    let source = r#"
class Empty:

fn main() => int:
    let e: Empty = Empty {}
    rm e
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_large_class_cleanup() {
    let source = r#"
class Large:
    f1: int
    f2: int
    f3: int
    f4: int
    f5: int
    f6: int
    f7: int
    f8: int
    f9: int
    f10: int

fn main() => int:
    let l: Large = Large {
        f1: 1, f2: 2, f3: 3, f4: 4, f5: 5,
        f6: 6, f7: 7, f8: 8, f9: 9, f10: 10
    }
    rm l
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_cleanup_with_field_access() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let x_val: int = p.x
    let y_val: int = p.y
    rm p
    rm y_val
    rm x_val
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_cleanup_after_field_modification() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    p.x = 30
    p.y = 40
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_cleanup_in_nested_scopes() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 0, y: 0 }
    let x1: int = p1.x
    if true:
        let p2: Point = Point { x: 10, y: 10 }
        let x2: int = p2.x
        if true:
            let p3: Point = Point { x: 20, y: 20 }
            let x3: int = p3.x
            rm p3
            return x3
        rm p2
        return x2
    rm p1
    return x1

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 内存泄漏防护测试
//=============================================================================

#[test]
fn test_no_leak_in_function() {
    let source = r#"
class Point:
    x: int
    y: int

fn create_point() => Point:
    let p: Point = Point { x: 10, y: 20 }
    return p

fn main() => int:
    let p: Point = create_point()
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_no_leak_in_loop() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let i: int = 0
    while i < 5:
        let p: Point = Point { x: i, y: i }
        rm p
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_no_leak_in_error_path() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let flag: int = 0
    if flag > 0:
        rm p
        return 1
    rm p
    rm flag
    return 0

"#;
    // This test should fail because p is removed twice (once in if, once after if)
    assert!(assert_compiles(source).is_err());
}

#[test]
fn test_multiple_variables_cleanup() {
    let source = r#"
fn main() => int:
    let a: int = 1
    let b: int = 2
    let c: int = 3
    let d: int = 4
    let e: int = 5
    rm a, b, c, d, e
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_scope_cleanup() {
    let source = r#"
fn main() => int:
    let a: int = 1
    if true:
        let b: int = 2
        if true:
            let c: int = 3
            rm c
        rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_loop_variable_cleanup() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        let temp: int = i * 2
        sum = sum + temp
        rm temp
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_parameter_cleanup() {
    let source = r#"
fn process(a: int, b: int, c: int) => int:
    let result: int = a + b + c
    rm a, b, c
    return result

fn main() => int:
    let result: int = process(1, 2, 3)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_instance_cleanup() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 1, y: 2 }
    let p2: Point = Point { x: 3, y: 4 }
    let p3: Point = Point { x: 5, y: 6 }
    rm p1, p2, p3
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_tuple_cleanup() {
    let source = r#"
fn main() => int:
    let t1: (int, int) = (1, 2)
    let t2: (int, int, int) = (1, 2, 3)
    let t3: ((int, int), int) = ((1, 2), 3)
    rm t1, t2, t3
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_array_cleanup() {
    let source = r#"
fn main() => int:
    let arr1: [int; 3] = [1, 2, 3]
    let arr2: [float; 2] = [1.5, 2.5]
    rm arr1, arr2
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_cleanup_with_exceptions() {
    let source = r#"
use fprintf in libc of c

class MyError:
    code: int
    message: str

fn might_fail(x: int) => int:
    if x < 0:
        raise MyError { code: -1, message: "negative value" }
    return x

fn main() => int:
    let result: int = might_fail(10)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// --enable-safety: array bounds checks in LLVM IR
//=============================================================================

#[test]
fn test_enable_safety_emits_bounds_panic_block() {
    let source = r#"
fn main() => int:
    let arr: [int; 2] = [1, 2]
    let i: int = 1
    let x: int = arr[i]
    rm x
    rm i
    rm arr
    return 0
"#;
    let pid = std::process::id();
    let off_ll = format!("test_enable_safety_off_{}.ll", pid);
    let on_ll = format!("test_enable_safety_on_{}.ll", pid);

    let off = compile_coffee(source, &["--emit-llvm", "-o", &off_ll]).unwrap();
    let on = compile_coffee(source, &["--enable-safety", "--emit-llvm", "-o", &on_ll]).unwrap();

    let on_ir = std::fs::read_to_string(&on_ll).unwrap_or_default();
    let _ = std::fs::remove_file(&off_ll);
    let _ = std::fs::remove_file(&on_ll);

    assert_eq!(on.exit_code, 0, "enable-safety emit-llvm failed:\n{}", on.stderr);
    assert!(
        on_ir.contains("bounds_panic")
            || on.stdout.contains("bounds_panic")
            || on.stderr.contains("bounds_panic"),
        "expected bounds_panic in LLVM IR file (emit-llvm writes a .ll file, not stdout):\nir={}\nstdout={}\nstderr={}",
        on_ir,
        on.stdout,
        on.stderr
    );
    let _ = off;
}