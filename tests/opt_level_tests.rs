include!("common/mod.rs");

use std::path::Path;

fn tiny_add() -> &'static str {
    r#"
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    return add(1, 2)
"#
}

/// Drop filename / ident noise so -O0 vs -O3 can be compared for real codegen.
fn normalize_ir(ir: &str) -> String {
    ir.lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with("source_filename")
                && !t.starts_with("; ModuleID")
                && !t.contains("test_temp_")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Drop filename / ident noise so -O0 vs -O3 can be compared for real codegen.
fn normalize_asm(asm: &str) -> String {
    asm.lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with(".file")
                && !t.starts_with(".ident")
                && !t.contains("test_temp_")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn unique_out(prefix: &str, ext: &str) -> String {
    format!("{}_{}.{}", prefix, unique_temp_id(), ext)
}

#[test]
fn compile_coffee_without_dash_o_does_not_leave_default_ll() {
    let r = compile_coffee(tiny_add(), &["--emit-llvm"]).unwrap();
    assert_eq!(r.exit_code, 0, "--emit-llvm: {}", r.stderr);
    let Some(path) = r.stdout.split("-> ").nth(1).map(str::trim) else {
        panic!("expected Compiled … -> path, got stdout={}", r.stdout);
    };
    assert!(
        !Path::new(path).exists(),
        "compile_coffee must remove default artifact {path}"
    );
}

#[test]
fn test_o0_and_o3_emit_llvm_succeed() {
    let o0_name = unique_out("opt_o0", "ll");
    let o3_name = unique_out("opt_o3", "ll");

    let o0 = compile_coffee(tiny_add(), &["-O0", "--emit-llvm", "-o", &o0_name]).unwrap();
    let o3 = compile_coffee(tiny_add(), &["-O3", "--emit-llvm", "-o", &o3_name]).unwrap();

    let o0_ir = fs::read_to_string(&o0_name);
    let o3_ir = fs::read_to_string(&o3_name);
    let _ = fs::remove_file(&o0_name);
    let _ = fs::remove_file(&o3_name);

    assert_eq!(o0.exit_code, 0, "-O0 --emit-llvm: {}", o0.stderr);
    assert_eq!(o3.exit_code, 0, "-O3 --emit-llvm: {}", o3.stderr);
    assert!(
        o0_ir.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false),
        "-O0 IR missing\nstdout={}\nstderr={}",
        o0.stdout,
        o0.stderr
    );
    assert!(
        o3_ir.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false),
        "-O3 IR missing\nstdout={}\nstderr={}",
        o3.stdout,
        o3.stderr
    );

    let o0_ir = normalize_ir(&o0_ir.expect("-O0 IR file"));
    let o3_ir = normalize_ir(&o3_ir.expect("-O3 IR file"));
    assert_ne!(
        o0_ir, o3_ir,
        "-O0 (no-op) and -O3 (default<O3>) LLVM IR should differ after inlining\n--- O0 ---\n{o0_ir}\n--- O3 ---\n{o3_ir}"
    );
}

#[test]
fn test_o0_and_o1_emit_llvm_differ() {
    let o0_name = unique_out("opt_o0_vs_o1", "ll");
    let o1_name = unique_out("opt_o1", "ll");

    let o0 = compile_coffee(tiny_add(), &["-O0", "--emit-llvm", "-o", &o0_name]).unwrap();
    let o1 = compile_coffee(tiny_add(), &["-O1", "--emit-llvm", "-o", &o1_name]).unwrap();

    let o0_ir = fs::read_to_string(&o0_name);
    let o1_ir = fs::read_to_string(&o1_name);
    let _ = fs::remove_file(&o0_name);
    let _ = fs::remove_file(&o1_name);

    assert_eq!(o0.exit_code, 0, "-O0 --emit-llvm: {}", o0.stderr);
    assert_eq!(o1.exit_code, 0, "-O1 --emit-llvm: {}", o1.stderr);

    let o0_ir = normalize_ir(&o0_ir.expect("-O0 IR file"));
    let o1_ir = normalize_ir(&o1_ir.expect("-O1 IR file"));
    assert_ne!(
        o0_ir, o1_ir,
        "-O0 must stay unoptimized; -O1 must run default<O1>\n--- O0 ---\n{o0_ir}\n--- O1 ---\n{o1_ir}"
    );
}

#[test]
fn test_each_opt_level_object_and_asm_codegen_succeeds() {
    for flag in ["-O0", "-O1", "-O2", "-O3"] {
        let obj = unique_out(&format!("opt_obj_{}", &flag[2..]), "o");
        let asm = unique_out(&format!("opt_asm_{}", &flag[2..]), "s");

        let obj_r = compile_coffee(tiny_add(), &[flag, "-o", &obj]).unwrap();
        let asm_r = compile_coffee(tiny_add(), &[flag, "--emit-asm", "-o", &asm]).unwrap();

        let obj_ok = Path::new(&obj).is_file();
        let asm_ok = Path::new(&asm).is_file();
        let _ = fs::remove_file(&obj);
        let _ = fs::remove_file(&asm);

        assert_eq!(
            obj_r.exit_code, 0,
            "{flag} object codegen failed: {}",
            obj_r.stderr
        );
        assert!(obj_ok, "{flag} did not write object file");
        assert_eq!(
            asm_r.exit_code, 0,
            "{flag} asm codegen failed: {}",
            asm_r.stderr
        );
        assert!(asm_ok, "{flag} did not write assembly file");
    }
}

#[test]
fn test_o0_and_o3_assembly_may_differ() {
    let o0_name = unique_out("opt_asm_o0", "s");
    let o3_name = unique_out("opt_asm_o3", "s");

    let o0 = compile_coffee(tiny_add(), &["-O0", "--emit-asm", "-o", &o0_name]).unwrap();
    let o3 = compile_coffee(tiny_add(), &["-O3", "--emit-asm", "-o", &o3_name]).unwrap();

    let o0_asm = fs::read_to_string(&o0_name);
    let o3_asm = fs::read_to_string(&o3_name);
    let _ = fs::remove_file(&o0_name);
    let _ = fs::remove_file(&o3_name);

    assert_eq!(o0.exit_code, 0, "-O0 --emit-asm: {}", o0.stderr);
    assert_eq!(o3.exit_code, 0, "-O3 --emit-asm: {}", o3.stderr);

    let o0_asm = normalize_asm(&o0_asm.expect("-O0 assembly file"));
    let o3_asm = normalize_asm(&o3_asm.expect("-O3 assembly file"));
    assert_ne!(
        o0_asm, o3_asm,
        "-O0 and -O3 assembly should differ once TargetMachine sees opt level\n--- O0 ---\n{o0_asm}\n--- O3 ---\n{o3_asm}"
    );
}

#[test]
fn test_project_cli_opt_overrides_toml() {
    let dir = std::env::temp_dir().join(format!("coffee_opt_proj_{}", unique_temp_id()));
    fs::create_dir_all(dir.join("src")).expect("mkdir src");
    fs::write(
        dir.join("coffee.toml"),
        r#"[package]
name = "opt_proj"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"

[target]
opt_level = 2
"#,
    )
    .expect("write coffee.toml");
    fs::write(dir.join("src/main.cf"), tiny_add()).expect("write main.cf");

    let current_dir = std::env::current_dir().expect("cwd");
    let compiler = coffee_bin(&current_dir);

    let run = |args: &[&str]| {
        let output = Command::new(&compiler)
            .current_dir(&dir)
            .arg("--test-mode")
            .args(args)
            .arg("src/main.cf")
            .output()
            .expect("run coffee");
        test_result_from_output(&output)
    };

    let toml_only = run(&["--emit-llvm", "-o", "from_toml.ll"]);
    let cli_o0 = run(&["-O0", "--emit-llvm", "-o", "from_cli.ll"]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(toml_only.exit_code, 0, "toml opt_level=2: {}", toml_only.stderr);
    assert!(
        toml_only.stdout.contains("Opt level: O2"),
        "project mode should use coffee.toml opt_level without CLI -O:\n{}",
        toml_only.stdout
    );
    assert_eq!(cli_o0.exit_code, 0, "CLI -O0: {}", cli_o0.stderr);
    assert!(
        cli_o0.stdout.contains("Opt level: O0"),
        "CLI -O0 should override toml opt_level=2:\n{}",
        cli_o0.stdout
    );
}

#[test]
fn test_project_detected_from_nested_cwd() {
    let dir = std::env::temp_dir().join(format!("coffee_walkup_{}", unique_temp_id()));
    fs::create_dir_all(dir.join("src")).expect("mkdir src");
    fs::create_dir_all(dir.join("nested")).expect("mkdir nested");
    fs::write(
        dir.join("coffee.toml"),
        r#"[package]
name = "walkup"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#,
    )
    .expect("write coffee.toml");
    fs::write(dir.join("src/main.cf"), tiny_add()).expect("write main.cf");

    let current_dir = std::env::current_dir().expect("cwd");
    let compiler = coffee_bin(&current_dir);
    let output = Command::new(&compiler)
        .current_dir(dir.join("nested"))
        .arg("--test-mode")
        .arg("--emit-llvm")
        .arg("-o")
        .arg("walkup.ll")
        .output()
        .expect("run coffee");
    let result = test_result_from_output(&output);
    let wrote = dir.join("nested/walkup.ll").is_file() || dir.join("walkup.ll").is_file();
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(result.exit_code, 0, "walk-up project: {}", result.stderr);
    assert!(
        result.stdout.contains("Coffee project detected"),
        "running from a nested dir should still find coffee.toml:\n{}",
        result.stdout
    );
    assert!(wrote, "expected LLVM IR output from nested cwd");
}

fn write_tiny_project(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("src")).expect("mkdir src");
    fs::write(
        dir.join("coffee.toml"),
        r#"[package]
name = "cli_flags"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#,
    )
    .expect("write coffee.toml");
    fs::write(dir.join("src/main.cf"), tiny_add()).expect("write main.cf");
}

#[test]
fn test_project_jit_runs_source_instead_of_linking() {
    let dir = std::env::temp_dir().join(format!("coffee_jit_proj_{}", unique_temp_id()));
    write_tiny_project(&dir);
    let current_dir = std::env::current_dir().expect("cwd");
    let compiler = coffee_bin(&current_dir);
    let output = Command::new(&compiler)
        .current_dir(&dir)
        .arg("--test-mode")
        .arg("--jit")
        .arg("src/main.cf")
        .output()
        .expect("run coffee");
    let result = test_result_from_output(&output);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(result.exit_code, 3, "jit add(1,2): stdout={} stderr={}", result.stdout, result.stderr);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("exited") || hay.contains("code"),
        "JIT should run in-process, not project-link:\n{hay}"
    );
    assert!(
        !result.stdout.contains("Linking..."),
        "--jit must not enter project link:\n{}",
        result.stdout
    );
}

#[test]
fn test_project_emit_ast_dumps_ast_instead_of_linking() {
    let dir = std::env::temp_dir().join(format!("coffee_ast_proj_{}", unique_temp_id()));
    write_tiny_project(&dir);
    let current_dir = std::env::current_dir().expect("cwd");
    let compiler = coffee_bin(&current_dir);
    let output = Command::new(&compiler)
        .current_dir(&dir)
        .arg("--test-mode")
        .arg("--emit-ast")
        .arg("src/main.cf")
        .output()
        .expect("run coffee");
    let result = test_result_from_output(&output);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(result.exit_code, 0, "emit-ast: {}", result.stderr);
    assert!(
        result.stdout.contains("// AST"),
        "--emit-ast should dump AST:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("Linking..."),
        "--emit-ast must not enter project link:\n{}",
        result.stdout
    );
}

#[test]
fn test_project_show_memory_prints_layout_report() {
    let dir = std::env::temp_dir().join(format!("coffee_showmem_proj_{}", unique_temp_id()));
    fs::create_dir_all(dir.join("src")).expect("mkdir src");
    fs::write(
        dir.join("coffee.toml"),
        r#"[package]
name = "showmem"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#,
    )
    .expect("write coffee.toml");
    fs::write(
        dir.join("src/main.cf"),
        r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 1, y: 2 }
    rm p
    return 0
"#,
    )
    .expect("write main.cf");

    let current_dir = std::env::current_dir().expect("cwd");
    let compiler = coffee_bin(&current_dir);
    let output = Command::new(&compiler)
        .current_dir(&dir)
        .arg("--test-mode")
        .arg("--show-memory")
        .arg("--emit-llvm")
        .output()
        .expect("run coffee");
    let result = test_result_from_output(&output);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        result.exit_code, 0,
        "project --show-memory --emit-llvm: stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(
        result.stdout.contains("Memory Layout Analysis Report"),
        "project --show-memory should print layout report:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Point"),
        "layout report should mention class Point:\n{}",
        result.stdout
    );
}
