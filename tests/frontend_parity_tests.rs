include!("common/mod.rs");

fn tiny_program() -> &'static str {
    r#"
fn main() => int:
    let x: int = 1
    rm x
    return 0
"#
}

fn unique_ll(prefix: &str) -> String {
    format!(
        "{}_{:?}_{:?}.ll",
        prefix,
        std::thread::current().id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
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
    let cwd = std::env::current_dir().unwrap();
    let single_name = unique_ll("parity_single");
    let project_name = unique_ll("parity_project");
    let single_ll = cwd.join(&single_name);
    let project_ll = cwd.join(&project_name);
    let project_ll_str = project_ll.to_string_lossy().to_string();

    let single = compile_coffee(tiny_program(), &["--emit-llvm", "-o", &single_name]).unwrap();
    let project = compile_project_fixture(
        tiny_program(),
        &["--emit-llvm", "-o", &project_ll_str],
    )
    .unwrap();

    let single_ir = fs::read_to_string(&single_ll);
    let project_ir = fs::read_to_string(&project_ll);
    let _ = fs::remove_file(&single_ll);
    let _ = fs::remove_file(&project_ll);

    assert_eq!(single.exit_code, 0, "single: {}", single.stderr);
    assert_eq!(project.exit_code, 0, "project: {}", project.stderr);

    let single_ir = single_ir.expect("single-file --emit-llvm must write a .ll file");
    let project_ir = project_ir.expect("project --emit-llvm must write a .ll file");
    assert!(
        !single_ir.trim().is_empty(),
        "single-file LLVM IR was empty\nstdout={}\nstderr={}",
        single.stdout,
        single.stderr
    );
    assert!(
        !project_ir.trim().is_empty(),
        "project LLVM IR was empty (entry module .ll only)\nstdout={}\nstderr={}",
        project.stdout,
        project.stderr
    );
}
