// Official std embed, `coffee std install`, compile without packages.std path.

include!("common/mod.rs");

use std::path::Path;

fn run_coffee_env(
    cwd: Option<&Path>,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Result<TestResult, String> {
    let current_dir =
        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?;
    let compiler_path = coffee_bin(&current_dir);
    let mut cmd = Command::new(&compiler_path);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.arg("--test-mode");
    for arg in args {
        cmd.arg(arg);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run compiler: {e}"))?;
    Ok(test_result_from_output(&output))
}

#[test]
fn test_print_usage_lists_std_install() {
    let result = run_coffee_env(None, &["--help"], &[]).expect("coffee --help");
    assert_eq!(result.exit_code, 0, "stderr={}", result.stderr);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("std install"),
        "usage should mention std install:\n{hay}"
    );
}

#[test]
fn test_std_install_tree_and_compile_without_packages_std() {
    let dest = std::env::temp_dir().join(format!("coffee_std_install_{}", unique_temp_id()));
    fs::create_dir_all(&dest).expect("temp COFFEE_STD");

    let dest_s = dest.to_string_lossy().into_owned();
    let install = run_coffee_env(None, &["std", "install"], &[("COFFEE_STD", &dest_s)])
        .expect("coffee std install");
    assert_eq!(
        install.exit_code, 0,
        "std install should succeed\nstdout={}\nstderr={}",
        install.stdout, install.stderr
    );
    assert!(
        dest.join("src").join("std.cf").is_file(),
        "installed tree should contain src/std.cf under {}\nstdout={}",
        dest.display(),
        install.stdout
    );
    assert!(
        dest.join("coffee.toml").is_file(),
        "installed tree should contain coffee.toml"
    );
    let printed = install.stdout.trim();
    assert!(
        !printed.is_empty(),
        "std install should print the installed root"
    );

    // Compile against COFFEE_STD without `[dependencies.packages.std]`.
    // A no-op `print` isolates Task 2 (lookup/prepend) from Task 1 wrapper types.
    fs::write(
        dest.join("src").join("std.cf"),
        "fn print(s: str) => void:\n    let n: int = 0\n",
    )
    .expect("stub print for compile");

    let proj = std::env::temp_dir().join(format!("coffee_std_user_{}", unique_temp_id()));
    fs::create_dir_all(proj.join("src")).expect("user project src");
    fs::write(
        proj.join("coffee.toml"),
        r#"[package]
name = "std_install_user"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#,
    )
    .expect("write coffee.toml");
    fs::write(
        proj.join("src/main.cf"),
        r#"use print in std

fn main() => int:
    print("hello-std\n")
    return 0
"#,
    )
    .expect("write main.cf");

    let compile = run_coffee_env(
        Some(&proj),
        &["--emit-llvm"],
        &[("COFFEE_STD", &dest_s)],
    )
    .expect("compile project");
    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&dest);
    assert_eq!(
        compile.exit_code, 0,
        "project with use print in std and no packages.std should compile\nstdout={}\nstderr={}",
        compile.stdout, compile.stderr
    );
}
