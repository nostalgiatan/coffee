// Generics v1: monomorphized class must reach LLVM without backend changes.

include!("common/mod.rs");

#[test]
fn test_generic_box_emit_llvm() {
    let source = r#"
class Box<T>:
    v: T

fn main() => int:
    let b: Box<int> = Box { v: 1 }
    rm b
    return 0

"#;
    let ll = format!("test_generic_box_{}.ll", std::process::id());
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let ir = std::fs::read_to_string(&ll).unwrap_or_default();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(result.exit_code, 0, "emit-llvm failed:\n{}", result.stderr);
    assert!(
        ir.contains("Box__int") || ir.contains("%Box__int"),
        "expected monomorphized Box__int in LLVM IR:\n{ir}"
    );
}
