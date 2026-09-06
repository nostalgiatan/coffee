// CLI tests for `coffee run` and `coffee test`.

include!("common/mod.rs");

use std::path::Path;

fn coffee_cmd() -> Command {
    let cwd = std::env::current_dir().expect("cwd");
    let mut cmd = Command::new(coffee_bin(&cwd));
    cmd.arg("--test-mode");
    cmd
}

fn tiny_return(code: i32) -> String {
    format!(
        "fn main() => int:\n    return {code}\n"
    )
}


#[test]
fn test_run_returns_program_exit_code() {
    let id = unique_temp_id();
    let src = format!("run_exit_{id}.cf");
    fs::write(&src, tiny_return(42)).expect("write");

    let output = coffee_cmd()
        .arg("run")
        .arg(&src)
        .output()
        .expect("coffee run");

    let _ = fs::remove_file(&src);
    remove_default_stem_artifacts(Path::new(&src));

    let r = test_result_from_output(&output);
    assert_eq!(
        r.exit_code, 42,
        "coffee run should exit with the program status; stderr={}\nstdout={}",
        r.stderr, r.stdout
    );
}

#[test]
fn test_run_forwards_args_after_double_dash() {
    let id = unique_temp_id();
    let src = format!("run_args_{id}.cf");
    fs::write(&src, tiny_return(0)).expect("write");

    let output = coffee_cmd()
        .arg("run")
        .arg("-O0")
        .arg(&src)
        .arg("--")
        .arg("--not-a-real-flag")
        .output()
        .expect("coffee run -- args");

    let _ = fs::remove_file(&src);
    remove_default_stem_artifacts(Path::new(&src));

    let r = test_result_from_output(&output);
    assert_eq!(
        r.exit_code, 0,
        "tokens after -- must be program argv, not coffee flags; stderr={}\nstdout={}",
        r.stderr, r.stdout
    );
}

#[test]
fn test_run_project_entry_without_file() {
    let proj = TestProject::new("run_proj").expect("project");
    proj.add_file("src/main.cf", &tiny_return(3))
        .expect("main");

    let output = coffee_cmd()
        .current_dir(&proj.dir)
        .arg("run")
        .output()
        .expect("coffee run in project");

    let r = test_result_from_output(&output);
    proj.cleanup();
    assert_eq!(
        r.exit_code, 3,
        "coffee run with coffee.toml and no file should exec the project entry; stderr={}\nstdout={}",
        r.stderr, r.stdout
    );
}

#[test]
fn test_test_runs_single_file() {
    let id = unique_temp_id();
    let src = format!("test_one_{id}.cf");
    fs::write(&src, tiny_return(0)).expect("write");

    let output = coffee_cmd()
        .arg("test")
        .arg(&src)
        .output()
        .expect("coffee test file");

    let _ = fs::remove_file(&src);
    remove_default_stem_artifacts(Path::new(&src));

    let r = test_result_from_output(&output);
    assert_eq!(
        r.exit_code, 0,
        "coffee test <file.cf> should compile+run; stderr={}\nstdout={}",
        r.stderr, r.stdout
    );
    let hay = format!("{}\n{}", r.stdout, r.stderr);
    assert!(
        hay.contains("ok") || hay.contains("pass") || hay.contains("1"),
        "expected a test summary, got: {hay}"
    );
}

#[test]
fn test_test_project_tests_dir_summary() {
    let proj = TestProject::new("test_suite").expect("project");
    proj.add_file("src/main.cf", &tiny_return(0))
        .expect("main");
    proj.add_file("tests/a.cf", &tiny_return(0))
        .expect("a");
    proj.add_file("tests/b.cf", &tiny_return(0))
        .expect("b");

    let output = coffee_cmd()
        .current_dir(&proj.dir)
        .arg("test")
        .output()
        .expect("coffee test project");

    let r = test_result_from_output(&output);
    proj.cleanup();
    assert_eq!(
        r.exit_code, 0,
        "all tests/*.cf returning 0 should pass; stderr={}\nstdout={}",
        r.stderr, r.stdout
    );
    let hay = format!("{}\n{}", r.stdout, r.stderr);
    assert!(
        hay.contains("a.cf") && hay.contains("b.cf"),
        "summary should name both test files: {hay}"
    );
}

#[test]
fn test_test_nonzero_or_compile_error_fails() {
    let proj = TestProject::new("test_fail").expect("project");
    proj.add_file("src/main.cf", &tiny_return(0))
        .expect("main");
    proj.add_file("tests/ok.cf", &tiny_return(0))
        .expect("ok");
    proj.add_file("tests/bad.cf", &tiny_return(1))
        .expect("bad");

    let output = coffee_cmd()
        .current_dir(&proj.dir)
        .arg("test")
        .output()
        .expect("coffee test fail");

    let r = test_result_from_output(&output);
    proj.cleanup();
    assert_ne!(
        r.exit_code, 0,
        "nonzero program exit should fail coffee test; stdout={}\nstderr={}",
        r.stdout, r.stderr
    );
}

#[test]
fn test_run_unknown_flag_still_errors() {
    let output = coffee_cmd()
        .arg("run")
        .arg("--not-a-real-flag")
        .output()
        .expect("coffee run bad flag");
    let r = test_result_from_output(&output);
    assert_ne!(r.exit_code, 0);
    assert!(
        r.stderr.contains("unknown option") || r.stderr.contains("error"),
        "stderr={}",
        r.stderr
    );
}
