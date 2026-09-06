include!("common/mod.rs");

#[test]
fn test_shared_borrow_and_deref_compiles() {
    let source = r#"
fn main() => int:
    let x: int = 10
    let y: &int = &x
    return *y

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_cannot_move_while_borrowed() {
    let source = r#"
fn main() => int:
    let x: int = 10
    let y: &int = &x
    mv x z
    return *y

"#;
    assert_compile_error(source, "cannot move").unwrap();
}

#[test]
fn test_cannot_assign_while_borrowed() {
    let source = r#"
fn main() => int:
    let x: int = 10
    let y: &int = &x
    x = 11
    return *y

"#;
    assert_compile_error(source, "cannot assign").unwrap();
}

#[test]
fn test_shared_and_mut_borrow_conflict() {
    let source = r#"
fn main() => int:
    let x: int = 10
    let y: &mut int = &mut x
    let z: &int = &x
    return *z

"#;
    assert_compile_error(source, "already borrowed").unwrap();
}

#[test]
fn test_two_shared_borrows_ok() {
    let source = r#"
fn main() => int:
    let x: int = 10
    let y: &int = &x
    let z: &int = &x
    return *y

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_cannot_return_ref_to_local() {
    let source = r#"
fn get_ref() => &int:
    let x: int = 10
    return &x

fn main() => int:
    return 0

"#;
    assert_compile_error(source, "local variable").unwrap();
}

#[test]
fn test_return_ref_to_param_ok() {
    let source = r#"
fn get_ref(x: int) => &int:
    return &x

fn main() => int:
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_borrow_ends_with_if_scope() {
    let source = r#"
fn main() => int:
    let x: int = 10
    if true:
        let y: &int = &x
        let n: int = *y
    x = 11
    return x

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_field_borrow_compiles() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let r: &int = &p.x
    return *r

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_whole_and_field_mut_borrow_conflict() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let r: &Point = &p
    let q: &mut int = &mut p.x
    return *q

"#;
    assert_compile_error(source, "already borrowed").unwrap();
}

#[test]
fn test_cannot_move_while_field_borrowed() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let r: &int = &p.x
    mv p q
    return *r

"#;
    assert_compile_error(source, "cannot move").unwrap();
}

#[test]
fn test_inherited_field_borrow_compiles() {
    let source = r#"
class Base:
    x: int

class Child of Base:
    y: int

fn main() => int:
    let c: Child = Child { x: 7, y: 9 }
    let r: &int = &c.x
    return *r

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nll_assign_after_last_use_compiles() {
    let source = r#"
fn main() => int:
    let x: int = 10
    let y: &int = &x
    let n: int = *y
    x = 11
    return x

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_index_borrow_compiles() {
    let source = r#"
fn main() => int:
    let a: [int; 3] = [1, 2, 3]
    let i: int = 1
    let r: &int = &a[i]
    return *r

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_mut_index_borrow_compiles() {
    let source = r#"
fn main() => int:
    let a: [int; 3] = [1, 2, 3]
    let i: int = 0
    let r: &mut int = &mut a[i]
    return *r

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_whole_and_index_mut_borrow_conflict() {
    let source = r#"
fn main() => int:
    let a: [int; 3] = [1, 2, 3]
    let r: &[int; 3] = &a
    let q: &mut int = &mut a[0]
    return *q

"#;
    assert_compile_error(source, "already borrowed").unwrap();
}

#[test]
fn test_index_loans_conflict_conservatively() {
    let source = r#"
fn main() => int:
    let a: [int; 3] = [1, 2, 3]
    let r: &mut int = &mut a[0]
    let q: &int = &a[1]
    return *q

"#;
    assert_compile_error(source, "already borrowed").unwrap();
}

#[test]
fn test_ref_returning_call_keeps_arg_loan_until_result_last_use() {
    let source = r#"
fn foo(p: &int) => &int:
    return p

fn main() => int:
    let x: int = 10
    let r: &int = foo(&x)
    mv x z
    return *r

"#;
    let result = compile_coffee(source, &["--emit-ast"]).unwrap();
    assert_ne!(result.exit_code, 0, "expected borrow error, got success:\n{}", result.stdout);
    assert!(
        result.stderr.contains("cannot move"),
        "Expected error containing 'cannot move', got:\n{}",
        result.stderr
    );
}

#[test]
fn test_non_ref_returning_call_arg_loan_ends_at_statement() {
    let source = r#"
fn foo(p: &int) => int:
    return 0

fn main() => int:
    let x: int = 10
    foo(&x)
    mv x z
    return 0

"#;
    let result = compile_coffee(source, &["--emit-ast"]).unwrap();
    assert_eq!(
        result.exit_code, 0,
        "Compilation failed:\n{}",
        result.stderr
    );
}

#[test]
fn test_discarded_ref_returning_call_keeps_arg_loan_until_fn_end() {
    let source = r#"
fn foo(p: &int) => &int:
    return p

fn main() => int:
    let x: int = 10
    foo(&x)
    mv x z
    return 0

"#;
    let result = compile_coffee(source, &["--emit-ast"]).unwrap();
    assert_ne!(result.exit_code, 0, "expected borrow error, got success:\n{}", result.stdout);
    assert!(
        result.stderr.contains("cannot move"),
        "Expected error containing 'cannot move', got:\n{}",
        result.stderr
    );
}

#[test]
fn test_ref_return_aliases_only_returned_param() {
    let source = r#"
fn foo(p: &int, q: &int) => &int:
    return p

fn main() => int:
    let a: int = 1
    let b: int = 2
    let r: &int = foo(&a, &b)
    mv b z
    return *r

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_ref_return_alias_still_borrows_returned_arg() {
    let source = r#"
fn foo(p: &int, q: &int) => &int:
    return p

fn main() => int:
    let a: int = 1
    let b: int = 2
    let r: &int = foo(&a, &b)
    mv a z
    return *r

"#;
    assert_compile_error(source, "cannot move").unwrap();
}

#[test]
fn test_opaque_ref_returning_call_pins_all_ref_args() {
    let source = r#"
fn main() => int:
    let a: int = 1
    let b: int = 2
    let f: fn(&int, &int) => &int = fn(p: &int, q: &int) => &int:
        return p
    let r: &int = f(&a, &b)
    mv b z
    return *r

"#;
    assert_compile_error(source, "cannot move").unwrap();
}
