include!("common/mod.rs");

#[test]
fn test_let_last_use_compiles() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    let b: Box = a
    return b.n

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_let_use_after_still_errors() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    let b: Box = a
    return a.n

"#;
    assert_compile_error(source, "cannot copy").unwrap();
}

#[test]
fn test_call_last_use_compiles() {
    let source = r#"
class Box:
    n: int

fn take(p: Box) => int:
    return p.n

fn main() => int:
    let a: Box = Box { n: 1 }
    return take(a)

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_return_last_use_compiles() {
    let source = r#"
class Box:
    n: int

fn give() => Box:
    let a: Box = Box { n: 1 }
    return a

fn main() => int:
    let b: Box = give()
    return b.n

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_assign_use_after_still_errors() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    let b: Box = Box { n: 0 }
    b = a
    return a.n

"#;
    assert_compile_error(source, "cannot copy").unwrap();
}

#[test]
fn test_borrowed_then_let_copy_errors() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    let r: &Box = &a
    let b: Box = a
    return r.n

"#;
    assert_compile_error(source, "cannot move").unwrap();
}

#[test]
fn test_explicit_mv_still_works() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    mv a b
    return b.n

"#;
    assert_compiles(source).unwrap();
}
