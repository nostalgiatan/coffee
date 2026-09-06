// 控制流测试
// 
// 测试Coffee语言的所有控制流语句：
// - if/elif/else 语句
// - while 循环
// - for 循环
// - break 语句
// - continue 语句
// - match 模式匹配
// - return 语句

include!("common/mod.rs");

//=============================================================================
// if/elif/else 语句测试
//=============================================================================

#[test]
fn test_simple_if() {
    let source = r#"
fn main() => int:
    let a: int = 10
    if a > 5:
        let b: int = 20
        rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_if_else() {
    let source = r#"
fn main() => int:
    let a: int = 10
    if a > 5:
        let b: int = 20
        rm b
    else:
        let c: int = 30
        rm c
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_if_elif_else() {
    let source = r#"
fn main() => int:
    let a: int = 10
    if a > 20:
        let b: int = 30
        rm b
    elif a > 10:
        let c: int = 40
        rm c
    else:
        let d: int = 50
        rm d
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_elif() {
    let source = r#"
fn main() => int:
    let a: int = 10
    if a > 40:
        let b: int = 50
        rm b
    elif a > 30:
        let c: int = 60
        rm c
    elif a > 20:
        let d: int = 70
        rm d
    elif a > 10:
        let e: int = 80
        rm e
    else:
        let f: int = 90
        rm f
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_if() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 20
    if a > 5:
        if b > 15:
            let c: int = 30
            rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_if_with_and_condition() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 20
    if a > 5 && b > 15:
        let c: int = 30
        rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_if_with_or_condition() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 20
    if a > 15 || b > 15:
        let c: int = 30
        rm c
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// while 循环测试
//=============================================================================

#[test]
fn test_simple_while() {
    let source = r#"
fn main() => int:
    let i: int = 0
    while i < 5:
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_while_with_break() {
    let source = r#"
fn main() => int:
    let i: int = 0
    while i < 10:
        if i == 5:
            break
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_while_with_continue() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let sum: int = 0
    while i < 10:
        if i % 2 == 0:
            i = i + 1
            continue
        sum = sum + i
        i = i + 1
    rm sum
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_while() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let j: int = 0
    while i < 3:
        j = 0
        while j < 3:
            j = j + 1
        i = i + 1
    rm j
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_while_with_complex_condition() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let j: int = 0
    while i < 10 && j < 5:
        i = i + 1
        j = j + 1
    rm j
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// for 循环测试
//=============================================================================

#[test]
fn test_for_loop_range() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_loop_with_break() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        if i == 5:
            break
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_loop_with_continue() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        if i % 2 == 0:
            continue
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_for_loop() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..3:
        for j in 0..3:
            sum = sum + i * j
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_loop_with_start_end() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 5..10:
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_array_literal() {
    let source = r#"
fn main() => int:
    for x in [1, 2, 3]:
        let n: int = x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_array_variable() {
    let source = r#"
fn main() => int:
    let xs: [int; 2] = [1, 2]
    for x in xs:
        let n: int = x
    rm xs
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_array_call() {
    let source = r#"
fn make_arr() => [int; 2]:
    return [1, 2]

fn main() => int:
    for x in make_arr():
        let n: int = x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_slice_variable() {
    let source = r#"
fn main() => int:
    let xs: [int] = []
    for x in xs:
        let n: int = x
    rm xs
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_slice_call() {
    let source = r#"
fn make_xs() => [int]:
    let xs: [int] = []
    return xs

fn main() => int:
    for x in make_xs():
        let n: int = x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_tuple_variable() {
    let source = r#"
fn main() => int:
    let t: (int, int) = (1, 2)
    for x in t:
        let n: int = x
    rm t
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_tuple_literal() {
    let source = r#"
fn main() => int:
    for x in (1, 2):
        let n: int = x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_tuple_expr() {
    let source = r#"
fn main() => int:
    let a: int = 1
    let b: int = 2
    for x in (a, b):
        let n: int = x
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_in_body_let_field_access() {
    let source = r#"
class Point:
    x: int

fn main() => int:
    let xs: [int; 1] = [1]
    for x in xs:
        let p: Point = Point { x: x }
        if p.x > 0:
            let n: int = p.x
        rm p
    rm xs
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// break 语句测试
//=============================================================================

#[test]
fn test_break_in_while() {
    let source = r#"
fn main() => int:
    let i: int = 0
    while i < 10:
        if i == 5:
            break
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_break_in_for() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        if i == 5:
            break
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_break_in_nested_loops() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let j: int = 0
    while i < 10:
        j = 0
        while j < 10:
            if j == 5:
                break
            j = j + 1
        if i == 3:
            break
        i = i + 1
    rm j
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// continue 语句测试
//=============================================================================

#[test]
fn test_continue_in_while() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let sum: int = 0
    while i < 10:
        i = i + 1
        if i % 2 == 0:
            continue
        sum = sum + i
    rm sum
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_continue_in_for() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        if i % 2 == 0:
            continue
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_continue_in_nested_loops() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let j: int = 0
    let sum: int = 0
    while i < 5:
        j = 0
        while j < 5:
            j = j + 1
            if j == 3:
                continue
            sum = sum + 1
        i = i + 1
    rm sum
    rm j
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// return 语句测试
//=============================================================================

#[test]
fn test_simple_return() {
    let source = r#"
fn get_value() => int:
    return 42

fn main() => int:
    let value: int = get_value()
    rm value
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_return_with_expression() {
    let source = r#"
fn add(a: int, b: int) => int:
    return a + b
    rm b
    rm a

fn main() => int:
    let result: int = add(10, 20)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_return_points() {
    let source = r#"
fn get_value(flag: bool) => int:
    if flag:
        return 1
    else:
        return 0

fn main() => int:
    let a: int = get_value(true)
    let b: int = get_value(false)
    rm b
    rm a
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_return_in_loop() {
    let source = r#"
fn find_value() => int:
    let i: int = 0
    while i < 10:
        if i == 5:
            return i
        i = i + 1
    rm i
    return -1

fn main() => int:
    let result: int = find_value()
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 复杂控制流测试
//=============================================================================

#[test]
fn test_complex_nested_control_flow() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let j: int = 0
    while i < 5:
        if i == 2:
            i = i + 1
            continue
        j = 0
        while j < 5:
            if j == 3:
                j = j + 1
                break
            for k in 0..3:
                if k == 1:
                    continue
        i = i + 1
    rm j
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_fizzbuzz() {
    let source = r#"
fn main() => int:
    let i: int = 1
    while i <= 15:
        if i % 15 == 0:
            let s: str = "fizzbuzz"
            rm s
        elif i % 3 == 0:
            let s: str = "fizz"
            rm s
        elif i % 5 == 0:
            let s: str = "buzz"
            rm s
        else:
            let s: str = "number"
            rm s
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_factorial() {
    let source = r#"
fn factorial(n: int) => int:
    if n <= 1:
        return 1
    else:
        let result: int = n * factorial(n - 1)
        rm n
        return result

fn main() => int:
    let result: int = factorial(5)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_fibonacci() {
    let source = r#"
fn fibonacci(n: int) => int:
    if n <= 1:
        return n
    else:
        let a: int = fibonacci(n - 1)
        let b: int = fibonacci(n - 2)
        let result: int = a + b
        rm n
        rm b
        rm a
        return result

fn main() => int:
    let result: int = fibonacci(10)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_prime_check() {
    let source = r#"
fn is_prime(n: int) => bool:
    if n <= 1:
        return false
    let i: int = 2
    while i * i <= n:
        if n % i == 0:
            return false
        i = i + 1
    rm i
    rm n
    return true

fn main() => int:
    let result: bool = is_prime(17)
    rm result
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_infinite_loop_with_break() {
    let source = r#"
fn main() => int:
    let i: int = 0
    while true:
        if i == 5:
            break
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_empty_while() {
    let source = r#"
fn main() => int:
    let flag: bool = false
    while flag:
        let a: int = 10
        rm a
    rm flag
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_empty_for() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 10..5:
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_single_iteration_loop() {
    let source = r#"
fn main() => int:
    let i: int = 0
    while i < 1:
        i = i + 1
    rm i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_loops_with_break() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        for j in 0..10:
            if i == 5 && j == 5:
                break
            sum = sum + 1
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_loops_with_continue() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 0..10:
        for j in 0..10:
            if j == 5:
                continue
            sum = sum + 1
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_complex_nested_if() {
    let source = r#"
fn main() => int:
    let a: int = 10
    let b: int = 20
    let c: int = 30
    let result: int = 0
    
    if a > b:
        if b > c:
            result = 1
        else:
            result = 2
    elif a > c:
        result = 3
    else:
        result = 4
    
    rm a, b, c, result
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_while_with_break_and_continue() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let sum: int = 0
    while i < 10:
        i = i + 1
        if i == 5:
            continue
        if i == 8:
            break
        sum = sum + i
    rm i, sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_for_loop_with_range() {
    let source = r#"
fn main() => int:
    let sum: int = 0
    for i in 1..100:
        sum = sum + i
    rm sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_breaks() {
    let source = r#"
fn main() => int:
    let i: int = 0
    let count: int = 0
    while i < 100:
        i = i + 1
        if i == 10:
            break
        if i == 5:
            break
        count = count + 1
    rm i, count
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_early_return_in_loop() {
    let source = r#"
fn find_value(arr: [int; 5], target: int) => int:
    for i in range(0, 5):
        if arr[i] == target:
            return i
    return -1

fn main() => int:
    let arr: [int; 5] = [1, 2, 3, 4, 5]
    let index: int = find_value(arr, 3)
    rm arr, index
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// Short-circuit && / || (right-hand call must not run when skipped)
//=============================================================================

#[test]
fn test_and_short_circuit_does_not_call_right() {
    let source = r#"
use exit in libc of c

fn boom() => bool:
    exit(99)
    return true

fn main() => int:
    let x: bool = false && boom()
    rm x
    return 0

"#;
    assert_exit_code(source, 0).unwrap();
}

#[test]
fn test_or_short_circuit_does_not_call_right() {
    let source = r#"
use exit in libc of c

fn boom() => bool:
    exit(99)
    return false

fn main() => int:
    let x: bool = true || boom()
    rm x
    return 0

"#;
    assert_exit_code(source, 0).unwrap();
}

#[test]
fn test_bitwise_and_still_evaluates_right() {
    let source = r#"
use exit in libc of c

fn boom() => int:
    exit(99)
    return 1

fn main() => int:
    let x: int = 0 & boom()
    rm x
    return 0

"#;
    assert_exit_code(source, 99).unwrap();
}