// 函数测试
// 
// 测试Coffee语言的函数功能：
// - 函数定义
// - 函数参数
// - 函数返回值
// - 函数调用
// - 递归函数
// - 高阶函数（如果支持）
// - C ABI函数
// - 错误处理函数

include!("common/mod.rs");

//=============================================================================
// 基础函数测试
//=============================================================================

#[test]
fn test_simple_function() {
    let source = r#"
fn greet() => int:
    return 42

fn main() => int:
    let result: int = greet()
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_function_in_body_compiles() {
    let source = r#"
fn main() => int:
    fn helper() => int:
        return 1
    let result: int = helper()
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_same_nested_helper_name_in_two_functions_compiles() {
    let source = r#"
fn a() => int:
    fn helper() => int:
        return 1
    return helper()

fn b() => int:
    fn helper() => int:
        return 2
    return helper()

fn main() => int:
    return a() + b()

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_with_parameters() {
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
fn test_function_with_return_value() {
    let source = r#"
fn multiply(a: int, b: int) => int:
    return a * b

fn main() => int:
    let result: int = multiply(5, 6)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_no_return_value() {
    let source = r#"
fn do_nothing() => int:
    return 0

fn main() => int:
    do_nothing()
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_unit_return() {
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
// 函数参数测试
//=============================================================================

#[test]
fn test_single_parameter() {
    let source = r#"
fn square(x: int) => int:
    return x * x

fn main() => int:
    let result: int = square(5)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_parameters() {
    let source = r#"
fn add_three(a: int, b: int, c: int) => int:
    return a + b + c

fn main() => int:
    let result: int = add_three(10, 20, 30)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_different_type_parameters() {
    let source = r#"
fn combine(a: int, b: float, c: bool) => int:
    return 0

fn main() => int:
    let result: int = combine(10, 3.14, true)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_string_parameter() {
    let source = r#"
fn print_length(s: str) => int:
    return 0

fn main() => int:
    let result: int = print_length("hello")
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_bool_parameter() {
    let source = r#"
fn toggle(flag: bool) => bool:
    return !flag

fn main() => int:
    let result: bool = toggle(true)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 函数调用测试
//=============================================================================

#[test]
fn test_nested_function_calls() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let result: int = add(add(1, 2), add(3, 4))
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_call_in_expression() {
    let source = r#"
fn square(x: int) => int:
    return x * x

fn main() => int:
    let result: int = square(5) + square(10)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_call_in_condition() {
    let source = r#"
fn is_positive(x: int) => bool:
    return x > 0

fn main() => int:
    if is_positive(10):
        let a: int = 20
        rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_call_in_loop() {
    let source = r#"
fn square(x: int) => int:
    return x * x

fn main() => int:
    let sum: int = 0
    let i: int = 0
    while i < 5:
        let s: int = square(i)
        sum = sum + s
        rm s
        i = i + 1
    rm i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 递归函数测试
//=============================================================================

#[test]
fn test_simple_recursion() {
    let source = r#"
fn countdown(n: int) => int:
    if n <= 0:
        return 0
    let result: int = countdown(n - 1)
    return n + result

fn main() => int:
    let result: int = countdown(5)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_factorial_recursion() {
    let source = r#"
fn factorial(n: int) => int:
    if n <= 1:
        return 1
    let f: int = factorial(n - 1)
    let result: int = n * f
    rm f
    return result

fn main() => int:
    let result: int = factorial(5)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_fibonacci_recursion() {
    let source = r#"
fn fibonacci(n: int) => int:
    if n <= 1:
        return n
    let a: int = fibonacci(n - 1)
    let b: int = fibonacci(n - 2)
    let result: int = a + b
    rm b
    rm a
    return result

fn main() => int:
    let result: int = fibonacci(7)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_mutual_recursion() {
    let source = r#"
fn is_even(n: int) => bool:
    if n == 0:
        return true
    let result: bool = is_odd(n - 1)
    return result

fn is_odd(n: int) => bool:
    if n == 0:
        return false
    let result: bool = is_even(n - 1)
    return result

fn main() => int:
    let a: bool = is_even(10)
    let b: bool = is_odd(10)
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 高阶函数测试
//=============================================================================

#[test]
fn test_function_as_parameter() {
    let source = r#"
fn apply(f: int, x: int) => int:
    return f

fn main() => int:
    let result: int = apply(42, 10)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_returning_function() {
    let source = r#"
fn get_adder() => int:
    return 42

fn main() => int:
    let adder: int = get_adder()
    rm adder
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// C ABI函数测试
//=============================================================================

#[test]
fn test_c_abi_function() {
    let source = r#"
c fn c_function(x: int) => int:
    return x * 2

fn main() => int:
    let result: int = c_function(21)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_c_abi_with_multiple_params() {
    let source = r#"
c fn c_add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let result: int = c_add(10, 20)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 错误处理函数测试
//=============================================================================

#[test]
fn test_error_handler_function() {
    let source = r#"
fn on_err(err: Error) => int:
    return -1

fn risky_operation() #on_err => int:
    return 42

fn main() => int:
    let result: int = risky_operation()
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_error_handler_with_params() {
    let source = r#"
fn on_err(err: Error) => int:
    return -1

fn divide(a: int, b: int) #on_err => int:
    return 0

fn main() => int:
    let result: int = divide(10, 2)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 复杂函数测试
//=============================================================================

#[test]
fn test_function_with_local_variables() {
    let source = r#"
fn complex_calculation(x: int, y: int) => int:
    let temp1: int = x * 2
    let temp2: int = y * 3
    let result: int = temp1 + temp2
    rm temp1
    rm temp2
    return result

fn main() => int:
    let result: int = complex_calculation(5, 10)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_with_nested_scopes() {
    let source = r#"
fn nested_scopes(x: int) => int:
    let y: int = x * 2
    if x > 0:
        let z: int = y * 3
        rm z
    rm y
    return 0

fn main() => int:
    let result: int = nested_scopes(5)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_with_loops() {
    let source = r#"
fn sum_range(start: int, end: int) => int:
    let sum: int = 0
    for i in start..end:
        sum = sum + i
    return sum

fn main() => int:
    let result: int = sum_range(1, 11)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_with_conditionals() {
    let source = r#"
fn classify(n: int) => int:
    if n < 0:
        return -1
    elif n == 0:
        return 0
    else:
        return 1

fn main() => int:
    let a: int = classify(-5)
    let b: int = classify(0)
    let c: int = classify(5)
    rm c
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
fn test_function_with_no_parameters() {
    let source = r#"
fn get_value() => int:
    return 42

fn main() => int:
    let result: int = get_value()
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_with_many_parameters() {
    let source = r#"
fn many_params(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int) => int:
    return 0

fn main() => int:
    let result: int = many_params(1, 2, 3, 4, 5, 6, 7, 8)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_call_with_literals() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let result: int = add(100, 200)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_call_with_variables() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    let x: int = 10
    let y: int = 20
    let result: int = add(x, y)
    rm result
    rm y
    rm x
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
    let result: int = add(10 * 2, 20 * 3)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_deep_recursion() {
    let source = r#"
fn recursive_depth(n: int) => int:
    if n <= 0:
        return 0
    let r: int = recursive_depth(n - 1)
    let result: int = 1 + r
    rm r
    return result

fn main() => int:
    let result: int = recursive_depth(10)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_return_paths() {
    let source = r#"
fn conditional_return(x: int) => int:
    if x > 0:
        return 1
    elif x < 0:
        return -1
    else:
        return 0

fn main() => int:
    let a: int = conditional_return(5)
    let b: int = conditional_return(-5)
    let c: int = conditional_return(0)
    rm a, b, c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_overloading() {
    let source = r#"
fn add_int(a: int, b: int) => int:
    return a + b

fn add_float(a: float, b: float) => float:
    return a + b

fn main() => int:
    let a: int = add_int(1, 2)
    let b: float = add_float(1.5, 2.5)
    rm a, b
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_function_calls_operations() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn multiply(a: int, b: int) => int:
    return a * b

fn main() => int:
    let result: int = multiply(add(2, 3), 4)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_with_multiple_params() {
    let source = r#"
fn sum(a: int, b: int, c: int, d: int) => int:
    return a + b + c + d

fn main() => int:
    let result: int = sum(1, 2, 3, 4)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_tail_recursion() {
    let source = r#"
fn factorial_tail(n: int, acc: int) => int:
    if n <= 1:
        return acc
    return factorial_tail(n - 1, n * acc)

fn main() => int:
    let result: int = factorial_tail(5, 1)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_function_pointer() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b

fn subtract(a: int, b: int) => int:
    return a - b

fn main() => int:
    let a: int = add(10, 5)
    let b: int = subtract(10, 5)
    rm a, b
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// Anonymous functions (no capture)
//=============================================================================

#[test]
fn test_anonymous_fn_assigned_and_called() {
    let source = r#"
fn main() => int:
    let callback: fn(int) => int = fn(x: int) => int:
        return x * 2
    return callback(21)

"#;
    assert_exit_code(source, 42).unwrap();
}

#[test]
fn test_anonymous_fn_may_call_toplevel_function() {
    let source = r#"
fn double(x: int) => int:
    return x * 2

fn main() => int:
    let callback: fn(int) => int = fn(x: int) => int:
        return double(x)
    return callback(21)

"#;
    assert_exit_code(source, 42).unwrap();
}

#[test]
fn test_anonymous_fn_cannot_capture_enclosing_local() {
    let source = r#"
fn main() => int:
    let n: int = 1
    let callback: fn(int) => int = fn(x: int) => int:
        return x + n
    return callback(1)

"#;
    assert_compile_error(source, "capture").unwrap();
}

#[test]
fn test_anonymous_fn_cannot_capture_enclosing_param() {
    let source = r#"
fn apply(n: int) => int:
    let callback: fn(int) => int = fn(x: int) => int:
        return x + n
    return callback(1)

fn main() => int:
    return apply(1)

"#;
    assert_compile_error(source, "capture").unwrap();
}

#[test]
fn test_anonymous_fn_may_use_global() {
    let source = r#"
let n: int = 21

fn main() => int:
    let callback: fn(int) => int = fn(x: int) => int:
        return x * n
    return callback(2)

"#;
    assert_exit_code(source, 42).unwrap();
}

#[test]
fn test_anonymous_fn_may_call_c() {
    let source = r#"
use printf in libc of c

fn main() => int:
    let callback: fn(int) => int = fn(x: int) => int:
        printf("%d\n", x)
        return x
    return callback(42)

"#;
    assert_exit_code(source, 42).unwrap();
}