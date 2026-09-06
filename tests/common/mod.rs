// Coffee编译器测试框架
//
// 提供以下功能：
// - 测试用例编译和执行
// - 输出比较
// - 错误断言辅助函数
// - 测试项目生成
//
// Integration tests `include!` this file; do not put `#![...]` here
// (edition 2024 rejects inner attributes after other tokens).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 测试结果，包含输出和退出状态
#[derive(Debug, Clone)]
#[allow(dead_code)] // `include!` into every integration crate; fields used unevenly
pub struct TestResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

fn unique_temp_id() -> String {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tid = format!("{:?}", std::thread::current().id()).replace(['(', ')', ':', ' '], "_");
    format!("{}_{}_{}", std::process::id(), tid, n)
}

/// Default `coffee` outputs use the source *basename* in cwd (`foo.cf` → `foo.ll`).
/// `compile_coffee` deletes the `.cf`; it must also drop those sibling artifacts.
fn remove_default_stem_artifacts(source_cf: &std::path::Path) {
    let Some(stem) = source_cf.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let dir = source_cf.parent().unwrap_or(std::path::Path::new("."));
    for ext in ["ll", "o", "s", "bc", "bin"] {
        let _ = fs::remove_file(dir.join(format!("{stem}.{ext}")));
    }
    let _ = fs::remove_file(dir.join(stem));
}

fn remove_exe_and_object(exe: &str) {
    let _ = fs::remove_file(exe);
    let _ = fs::remove_file(format!("{exe}.o"));
}

fn coffee_bin(current_dir: &std::path::Path) -> PathBuf {
    option_env!("CARGO_BIN_EXE_coffee")
        .map(PathBuf::from)
        .unwrap_or_else(|| current_dir.join("target/debug/coffee"))
}

fn test_result_from_output(output: &std::process::Output) -> TestResult {
    TestResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

/// 编译Coffee源文件并返回结果
pub fn compile_coffee(source: &str, args: &[&str]) -> Result<TestResult, String> {
    let test_file = format!("test_temp_{}.cf", unique_temp_id());

    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let test_path = current_dir.join(&test_file);
    let compiler_path = coffee_bin(&current_dir);

    let mut file = fs::File::create(&test_path)
        .map_err(|e| format!("Failed to create test file: {}", e))?;
    file.write_all(source.as_bytes())
        .map_err(|e| format!("Failed to write test file: {}", e))?;

    let mut cmd = Command::new(&compiler_path);
    cmd.arg("--test-mode");
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(&test_path);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run compiler: {}", e))?;

    let _ = fs::remove_file(&test_path);
    remove_default_stem_artifacts(&test_path);

    Ok(test_result_from_output(&output))
}

/// Compile `source` as a one-file Coffee *project* (`coffee.toml` + `src/main.cf`).
/// Runs the compiler with cwd = the temp project dir so project mode is selected.
#[allow(dead_code)]
pub fn compile_project_fixture(source: &str, extra_args: &[&str]) -> Result<TestResult, String> {
    let dir = std::env::temp_dir().join(format!("coffee_proj_{}", unique_temp_id()));
    fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;

    let toml = r#"[package]
name = "parity_fixture"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#;
    fs::write(dir.join("coffee.toml"), toml).map_err(|e| e.to_string())?;
    fs::write(dir.join("src/main.cf"), source).map_err(|e| e.to_string())?;

    let compiler = coffee_bin(
        &std::env::current_dir().map_err(|e| e.to_string())?,
    );

    let mut cmd = Command::new(&compiler);
    cmd.current_dir(&dir);
    cmd.arg("--test-mode");
    for a in extra_args {
        cmd.arg(a);
    }
    // Relative to project cwd so EntryPointManager matches scanner paths (`src/main.cf`).
    cmd.arg("src/main.cf");

    let output = cmd.output().map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&dir);

    Ok(test_result_from_output(&output))
}

/// 编译并执行Coffee程序，成功时返回**运行**结果（stdout/stderr/exit_code）。
/// 编译失败时返回编译结果（非零 exit），与原先 compile-fail 路径一致。
#[allow(dead_code)]
pub fn compile_and_run_coffee(source: &str) -> Result<TestResult, String> {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    let test_id = unique_temp_id();
    let test_file = format!("test_temp_{}.cf", test_id);
    let exe_file = format!("test_temp_{}", test_id);

    fs::write(&test_file, source)
        .map_err(|e| format!("Failed to write test file: {}", e))?;

    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let compiler_path = coffee_bin(&current_dir);

    let mut cmd = Command::new(&compiler_path);
    cmd        .arg("--test-mode")
        .arg("-b")
        .arg("-o")
        .arg(&exe_file)
        .arg(&test_file);

    let compile_output = cmd.output().map_err(|e| {
        let _ = fs::remove_file(&test_file);
        remove_exe_and_object(&exe_file);
        format!("Failed to run compiler: {}", e)
    })?;

    if !compile_output.status.success() {
        let _ = fs::remove_file(&test_file);
        remove_exe_and_object(&exe_file);
        return Ok(test_result_from_output(&compile_output));
    }

    if !Path::new(&exe_file).exists() {
        let _ = fs::remove_file(&test_file);
        remove_exe_and_object(&exe_file);
        return Err(format!("Executable {} not created", exe_file));
    }

    let mut perms = fs::metadata(&exe_file)
        .map_err(|e| {
            let _ = fs::remove_file(&test_file);
            remove_exe_and_object(&exe_file);
            format!("Failed to get executable metadata: {}", e)
        })?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&exe_file, perms).map_err(|e| {
        let _ = fs::remove_file(&test_file);
        remove_exe_and_object(&exe_file);
        format!("Failed to set executable permissions: {}", e)
    })?;

    let run_output = Command::new(format!("./{}", exe_file))
        .output()
        .map_err(|e| {
            let _ = fs::remove_file(&test_file);
            remove_exe_and_object(&exe_file);
            format!("Failed to run executable: {}", e)
        })?;

    let _ = fs::remove_file(&test_file);
    remove_exe_and_object(&exe_file);

    Ok(test_result_from_output(&run_output))
}

/// 编译并执行Coffee程序（`compile_and_run_coffee` 的别名）
#[allow(dead_code)]
pub fn run_coffee(source: &str) -> Result<TestResult, String> {
    compile_and_run_coffee(source)
}

/// 断言编译成功
#[allow(dead_code)]
pub fn assert_compiles(source: &str) -> Result<(), String> {
    let result = compile_coffee(source, &[])?;
    if result.exit_code != 0 {
        return Err(format!("Compilation failed:\n{}", result.stderr));
    }
    Ok(())
}

/// 断言编译失败并包含预期错误
#[allow(dead_code)]
pub fn assert_compile_error(source: &str, expected_error: &str) -> Result<(), String> {
    let result = compile_coffee(source, &[])?;
    if result.exit_code == 0 {
        return Err("Expected compilation error but succeeded".to_string());
    }
    if !result.stderr.contains(expected_error) {
        return Err(format!(
            "Expected error containing '{}', got:\n{}",
            expected_error, result.stderr
        ));
    }
    Ok(())
}

/// 断言程序运行并返回预期的退出代码
#[allow(dead_code)]
pub fn assert_exit_code(source: &str, expected: i32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let test_id = unique_temp_id();
    let test_file = format!("test_temp_{}.cf", test_id);
    let exe_file = format!("test_temp_{}", test_id);

    fs::write(&test_file, source)
        .map_err(|e| format!("Failed to write test file: {}", e))?;

    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let compiler_path = coffee_bin(&current_dir);

    let mut compile_cmd = Command::new(&compiler_path);
    compile_cmd.arg("--test-mode");
    compile_cmd.arg("-b").arg("-o").arg(&exe_file).arg(&test_file);

    let compile_output = compile_cmd.output().map_err(|e| {
        let _ = fs::remove_file(&test_file);
        remove_exe_and_object(&exe_file);
        format!("Failed to run compiler: {}", e)
    })?;

    if !compile_output.status.success() {
        let _ = fs::remove_file(&test_file);
        remove_exe_and_object(&exe_file);
        return Err(format!(
            "Compilation failed: {}",
            String::from_utf8_lossy(&compile_output.stderr)
        ));
    }

    let exit_code = if std::path::Path::new(&exe_file).exists() {
        let mut perms = fs::metadata(&exe_file)
            .map_err(|e| {
                let _ = fs::remove_file(&test_file);
                remove_exe_and_object(&exe_file);
                format!("Failed to get metadata: {}", e)
            })?
            .permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(&exe_file, perms);

        let run_output = Command::new(format!("./{}", exe_file))
            .output()
            .map_err(|e| {
                let _ = fs::remove_file(&test_file);
                remove_exe_and_object(&exe_file);
                format!("Failed to run executable: {}", e)
            })?;

        remove_exe_and_object(&exe_file);
        run_output.status.code().unwrap_or(-1)
    } else {
        let _ = fs::remove_file(&test_file);
        remove_exe_and_object(&exe_file);
        return Err(format!("Executable {} not created", exe_file));
    };

    let _ = fs::remove_file(&test_file);
    remove_exe_and_object(&exe_file);

    if exit_code != expected {
        return Err(format!("Expected exit code {}, got {}", expected, exit_code));
    }

    Ok(())
}

/// 创建临时测试项目
#[allow(dead_code)]
pub struct TestProject {
    pub dir: PathBuf,
}

#[allow(dead_code)]
impl TestProject {
    pub fn new(name: &str) -> Result<Self, String> {
        let dir = PathBuf::from(format!("test_{}_{}_{}", name, std::process::id(), unique_temp_id()));
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create test project dir: {}", e))?;

        let toml = format!(
            r#"[package]
name = "{}"
version = "0.1.0"

[build]
main = "src/main"

[target]
opt_level = 0
"#,
            name
        );
        fs::write(dir.join("coffee.toml"), toml)
            .map_err(|e| format!("Failed to write coffee.toml: {}", e))?;

        fs::create_dir_all(dir.join("src"))
            .map_err(|e| format!("Failed to create src dir: {}", e))?;

        Ok(TestProject { dir })
    }

    pub fn add_file(&self, path: &str, content: &str) -> Result<(), String> {
        let file_path = self.dir.join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }
        fs::write(&file_path, content)
            .map_err(|e| format!("Failed to write file: {}", e))
    }

    pub fn compile(&self) -> Result<TestResult, String> {
        let compiler = coffee_bin(
            &std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?,
        );
        let mut cmd = Command::new(&compiler);
        cmd.current_dir(&self.dir);
        cmd.arg("--test-mode");
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run compiler: {}", e))?;
        Ok(test_result_from_output(&output))
    }

    pub fn cleanup(self) {
        let _ = fs::remove_dir_all(&self.dir);
        std::mem::forget(self);
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
