// Coffee编译器测试框架
//
// 提供以下功能：
// - 测试用例编译和执行
// - 输出比较
// - 错误断言辅助函数
// - 测试项目生成

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::{self, Write};

/// 测试结果，包含输出和退出状态
#[derive(Debug, Clone)]
pub struct TestResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// 编译Coffee源文件并返回结果
pub fn compile_coffee(source: &str, args: &[&str]) -> Result<TestResult, String> {
    // 使用线程ID和时间戳生成唯一的测试文件名
    let test_id = format!("{:?}_{:?}", std::thread::current().id(), std::time::SystemTime::now());
    let test_file = format!("test_temp_{}.cf", test_id);

    // 获取当前目录并创建绝对路径
    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let test_path = current_dir.join(&test_file);
    let compiler_path = current_dir.join("target/debug/coffee");

    // 将源代码写入临时文件
    let mut file = fs::File::create(&test_path)
        .map_err(|e| format!("Failed to create test file: {}", e))?;
    file.write_all(source.as_bytes())
        .map_err(|e| format!("Failed to write test file: {}", e))?;

    // 构建编译器命令，使用绝对路径
    let mut cmd = Command::new(&compiler_path);
    cmd.arg("--test-mode");  // 测试模式：跳过文件锁
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(&test_path);

    // 运行编译器
    let output = cmd.output()
        .map_err(|e| format!("Failed to run compiler: {}", e))?;

    // 清理
    let _ = fs::remove_file(&test_path);

    Ok(TestResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Compile `source` as a one-file Coffee *project* (`coffee.toml` + `src/main.cf`).
/// Runs the compiler with cwd = the temp project dir so project mode is selected.
pub fn compile_project_fixture(source: &str, extra_args: &[&str]) -> Result<TestResult, String> {
    use std::process::Command;

    let test_id = format!(
        "{:?}_{:?}",
        std::thread::current().id(),
        std::time::SystemTime::now()
    );
    let dir = std::env::temp_dir().join(format!("coffee_proj_{}", test_id.replace(['(', ')', ':', ' '], "_")));
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

    let compiler = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join("target/debug/coffee");

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

    Ok(TestResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// 编译并执行Coffee程序
pub fn run_coffee(source: &str) -> Result<TestResult, String> {
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::os::unix::fs::PermissionsExt;

    // 使用线程ID和时间戳生成唯一的测试文件名
    let test_id = format!("{:?}_{:?}", std::thread::current().id(), std::time::SystemTime::now());
    let test_file = format!("test_temp_{}.cf", test_id);
    let exe_file = format!("test_temp_{}", test_id);

    // 写入源文件
    fs::write(&test_file, source)
        .map_err(|e| format!("Failed to write test file: {}", e))?;

    // 获取编译器绝对路径
    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let compiler_path = current_dir.join("target/debug/coffee");

    // 使用-b标志编译以创建可执行文件
    let mut cmd = Command::new(&compiler_path);
    cmd.arg("-b")
        .arg("-o")
        .arg(&test_file)  // 输出基础名称（编译器会移除.cf）
        .arg(&test_file);

    let compile_output = cmd.output()
        .map_err(|e| {
            let _ = fs::remove_file(&test_file);
            format!("Failed to run compiler: {}", e)
        })?;

    if !compile_output.status.success() {
        let _ = fs::remove_file(&test_file);
        return Ok(TestResult {
            stdout: String::from_utf8_lossy(&compile_output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&compile_output.stderr).to_string(),
            exit_code: compile_output.status.code().unwrap_or(-1),
        });
    }

    // -b标志创建的可执行文件与输入同名但没有.cf扩展名
    // 确保它是可执行的
    if Path::new(&exe_file).exists() {
        let mut perms = fs::metadata(&exe_file)
            .map_err(|e| {
                let _ = fs::remove_file(&test_file);
                format!("Failed to get executable metadata: {}", e)
            })?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exe_file, perms)
            .map_err(|e| {
                let _ = fs::remove_file(&test_file);
                let _ = fs::remove_file(&exe_file);
                format!("Failed to set executable permissions: {}", e)
            })?;

        // 运行可执行文件
        let run_output = Command::new(format!("./{}", exe_file))
            .output()
            .map_err(|e| {
                let _ = fs::remove_file(&test_file);
                let _ = fs::remove_file(&exe_file);
                format!("Failed to run executable: {}", e)
            })?;

        // 清理
        let _ = fs::remove_file(&exe_file);
    }

    let _ = fs::remove_file(&test_file);

    // 返回结果（应该已创建并运行可执行文件）
    Ok(TestResult {
        stdout: String::from_utf8_lossy(&compile_output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&compile_output.stderr).to_string(),
        exit_code: 0,  // 编译成功
    })
}

/// 断言编译成功
pub fn assert_compiles(source: &str) -> Result<(), String> {
    let result = compile_coffee(source, &[])?;
    if result.exit_code != 0 {
        return Err(format!("Compilation failed:\n{}", result.stderr));
    }
    Ok(())
}

/// 断言编译失败并包含预期错误
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
pub fn assert_exit_code(source: &str, expected: i32) -> Result<(), String> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let test_id = format!("{:?}_{:?}", std::thread::current().id(), std::time::SystemTime::now());
    let test_file = format!("test_temp_{}.cf", test_id);
    let exe_file = format!("test_temp_{}", test_id);

    // 写入源代码
    fs::write(&test_file, source)
        .map_err(|e| format!("Failed to write test file: {}", e))?;

    // 获取编译器绝对路径
    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let compiler_path = current_dir.join("target/debug/coffee");

    // 编译
    // 添加--test-mode参数以跳过文件锁
    let mut compile_cmd = Command::new(&compiler_path);
    compile_cmd.arg("--test-mode");  // 测试模式：跳过文件锁
    compile_cmd.arg("-b").arg("-o").arg(&test_file).arg(&test_file);

    let compile_output = compile_cmd.output()
        .map_err(|e| {
            let _ = fs::remove_file(&test_file);
            format!("Failed to run compiler: {}", e)
        })?;

    if !compile_output.status.success() {
        let _ = fs::remove_file(&test_file);
        return Err(format!("Compilation failed: {}",
            String::from_utf8_lossy(&compile_output.stderr)));
    }

    // 使其可执行并运行
    let exit_code = if std::path::Path::new(&exe_file).exists() {
        let mut perms = fs::metadata(&exe_file)
            .map_err(|e| {
                let _ = fs::remove_file(&test_file);
                format!("Failed to get metadata: {}", e)
            })?
            .permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(&exe_file, perms);

        let run_output = Command::new(format!("./{}", exe_file))
            .output()
            .map_err(|e| {
                let _ = fs::remove_file(&test_file);
                let _ = fs::remove_file(&exe_file);
                format!("Failed to run executable: {}", e)
            })?;

        let _ = fs::remove_file(&exe_file);
        run_output.status.code().unwrap_or(-1)
    } else {
        // 未创建可执行文件
        let _ = fs::remove_file(&test_file);
        return Err(format!("Executable {} not created", exe_file));
    };

    let _ = fs::remove_file(&test_file);

    if exit_code != expected {
        return Err(format!("Expected exit code {}, got {}", expected, exit_code));
    }

    Ok(())
}

/// 创建临时测试项目
pub struct TestProject {
    pub dir: PathBuf,
}

impl TestProject {
    pub fn new(name: &str) -> Result<Self, String> {
        let dir = PathBuf::from(format!("test_{}_{}", name, std::process::id()));
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create test project dir: {}", e))?;

        // 创建coffee.toml
        let toml = format!(r#"[package]
name = "{}"
version = "0.1.0"

[build]
main = "src/main"

[target]
opt_level = 0
"#, name);
        fs::write(dir.join("coffee.toml"), toml)
            .map_err(|e| format!("Failed to write coffee.toml: {}", e))?;

        // 创建src目录
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
        let mut cmd = Command::new("./target/debug/coffee");
        cmd.current_dir(&self.dir);
        let output = cmd.output()
            .map_err(|e| format!("Failed to run compiler: {}", e))?;
        Ok(TestResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
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
