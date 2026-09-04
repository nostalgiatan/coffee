include!("common/mod.rs");

fn tiny_program() -> &'static str {
    r#"
fn main() => int:
    let x: int = 1
    rm x
    return 0
"#
}

#[test]
fn test_single_file_compiles_tiny_program() {
    assert_compiles(tiny_program()).unwrap();
}

#[test]
fn test_project_mode_compiles_tiny_program() {
    let r = compile_project_fixture(tiny_program(), &["--emit-llvm"]).unwrap();
    assert_eq!(r.exit_code, 0, "project compile failed: {}", r.stderr);
}

#[test]
fn test_single_and_project_emit_llvm_both_succeed() {
    let single = compile_coffee(tiny_program(), &["--emit-llvm"]).unwrap();
    let project = compile_project_fixture(tiny_program(), &["--emit-llvm"]).unwrap();
    assert_eq!(single.exit_code, 0, "single: {}", single.stderr);
    assert_eq!(project.exit_code, 0, "project: {}", project.stderr);
}
