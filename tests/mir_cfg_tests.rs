#[path = "common/mod.rs"]
mod common;
use common::*;

// CFG MIR codegen: if/while with complete MIR must compile via MIR, not AST.

fn emit_llvm(source: &str) -> String {
    let test_id = format!(
        "{:?}_{:?}",
        std::thread::current().id(),
        std::time::SystemTime::now()
    );
    let ll = format!(
        "test_mir_cfg_{}.ll",
        test_id.replace(['(', ')', ':', ' '], "_")
    );
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let ir = std::fs::read_to_string(&ll).unwrap_or_default();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(
        result.exit_code, 0,
        "emit-llvm failed:\n{}",
        result.stderr
    );
    ir
}

fn assert_mir_cfg(source: &str) {
    assert_compiles(source).unwrap();
    let ir = emit_llvm(source);
    assert!(
        ir.contains("mir0") || ir.contains("mir1"),
        "expected MIR basic blocks (mirN) so CFG uses MIR not AST:\n{}",
        ir
    );
}

#[test]
fn test_if_else_returns_compiles_via_mir() {
    let source = r#"
fn main() => int:
    if true:
        return 1
    else:
        return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_while_assign_then_return_compiles_via_mir() {
    // Condition is a bool literal so HIR lowering can type it without
    // re-checking locals (the type checker restores `values` after each fn).
    let source = r#"
fn main() => int:
    let i: int = 0
    while false:
        i = 1
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_range_for_compiles_via_mir() {
    let source = r#"
fn main() => int:
    let s: int = 0
    for i in 0..3:
        s = s + i
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_borrow_ends_with_if_scope_compiles() {
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

/// Then-branch locals must drop at `ScopeExit` without `memory_ctx.enter_scope`
/// while scanning MIR blocks in index order (that would nest then/else).
#[test]
fn test_if_then_local_then_assign_after_compiles_via_mir() {
    let source = r#"
fn main() => int:
    let x: int = 10
    if true:
        let y: int = 1
    x = 11
    return 0

"#;
    // `return 0` (not `return x`) so HIR lowering can type the body from
    // literals after the type checker restores `values`.
    assert_mir_cfg(source);
    let ir = emit_llvm(source);
    // Zero-init is one `store i64 0` next to `%y = alloca`. ScopeExit must
    // emit another in that same MIR block (not only at function return).
    let y_block_zeros = zero_stores_in_alloca_block(&ir, "%y");
    assert!(
        y_block_zeros >= 2,
        "expected ScopeExit drop of then-local y in its block, found {} zero-stores:\n{}",
        y_block_zeros,
        ir
    );
}

fn zero_stores_in_alloca_block(ir: &str, name: &str) -> usize {
    let alloca = format!("{} = alloca", name);
    let mut in_block = false;
    let mut count = 0;
    for line in ir.lines() {
        let t = line.trim();
        if t.starts_with("mir") && t.ends_with(':') {
            if in_block {
                break;
            }
        }
        if t.contains(&alloca) {
            in_block = true;
        }
        if in_block && t.contains("store i64 0") && t.contains(name) {
            count += 1;
        }
    }
    count
}

/// `rm` must stay on complete MIR. Marking it Unsupported used to omit the
/// function from `hir_fns` (now a hard `missing MIR for function` error).
#[test]
fn test_rm_local_compiles_via_mir() {
    let source = r#"
fn main() => int:
    let x: int = 1
    rm x
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_match_int_wildcard_compiles_via_mir() {
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
    assert_mir_cfg(source);
}

/// `match` as the last statement must terminate `matchend` (implicit return).
#[test]
fn test_match_as_function_body_compiles_via_mir() {
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
    assert_mir_cfg(source);
}

#[test]
fn test_nested_result_ok_option_some_match_compiles_via_mir() {
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
    assert_mir_cfg(source);
}

#[test]
fn test_match_or_literals_compiles_via_mir() {
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
    assert_mir_cfg(source);
}

#[test]
fn test_collection_for_in_compiles_via_mir() {
    let source = r#"
fn main() => int:
    let xs: [int; 2] = [1, 2]
    for x in xs:
        let n: int = x
    rm xs
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_collection_for_in_array_literal_compiles_via_mir() {
    let source = r#"
fn main() => int:
    for x in [1, 2]:
        let n: int = x
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_collection_for_in_array_call_compiles_via_mir() {
    let source = r#"
fn make_xs() => [int; 2]:
    return [1, 2]

fn main() => int:
    for x in make_xs():
        let n: int = x
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_collection_for_in_slice_call_compiles_via_mir() {
    let source = r#"
fn make_xs() => [int]:
    let xs: [int] = []
    return xs

fn main() => int:
    for x in make_xs():
        let n: int = x
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_collection_for_in_tuple_compiles_via_mir() {
    let source = r#"
fn main() => int:
    let t: (int, int) = (1, 2)
    for x in t:
        let n: int = x
    rm t
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_raise_after_let_compiles_via_mir() {
    let source = r#"
use fprintf, exit in libc of c

class TestError of Error:
    message: str

fn main() => int:
    let a: int = 10
    raise TestError { message: "Error after computation", code: 1, note: "n", e: 0 }

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_raise_call_form_compiles_via_mir() {
    let source = r#"
use fprintf, exit in libc of c

class TestError of Error:
    message: str

fn main() => int:
    raise TestError("An error occurred")

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_nested_fn_in_main_compiles_via_mir() {
    let source = r#"
fn main() => int:
    fn helper() => int:
        return 1
    return 0

"#;
    assert_mir_cfg(source);
}

#[test]
fn test_nested_class_and_comment_in_main_compiles_via_mir() {
    let source = r#"
fn main() => int:
    /#/ nested type stays on MIR
    class Point:
        x: int
    let p: Point = Point { x: 1 }
    rm p
    return 0

"#;
    assert_mir_cfg(source);
}

/// Debug dump for remaining while+break MIR order (must keep compiling).
#[test]
fn test_while_two_breaks_compiles_via_mir() {
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
    assert_mir_cfg(source);
}

#[test]
fn test_mul_in_if_then_does_not_drop_later_stmts() {
    let source = r#"
fn main() => int:
    let a: int = 1
    if a == 1:
        let n: int = a * 2
        let m: int = n + 1
        return m
    return 0

"#;
    let ir = emit_llvm(source);
    assert!(
        ir.contains("mul i64") && ir.contains("add i64") && ir.contains("ret i64 %m"),
        "stmts after overflow-checked mul in an if-then must still compile:\n{}",
        ir
    );
}
