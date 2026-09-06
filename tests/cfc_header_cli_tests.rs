// CLI tests for `-c`/`--gen-cfc` (header → .cfc) and `--have-c` (Coffee → .h).

include!("common/mod.rs");

use std::path::Path;

fn run_coffee_args(args: &[&str]) -> Result<TestResult, String> {
    run_coffee_args_in(None, args, true)
}

fn run_coffee_args_in(
    cwd: Option<&Path>,
    args: &[&str],
    test_mode: bool,
) -> Result<TestResult, String> {
    let current_dir =
        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?;
    let compiler_path = coffee_bin(&current_dir);
    let mut cmd = Command::new(&compiler_path);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    if test_mode {
        cmd.arg("--test-mode");
    }
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run compiler: {e}"))?;
    Ok(test_result_from_output(&output))
}

#[test]
fn test_gen_cfc_from_header() {
    let id = unique_temp_id();
    let header = format!("cfc_cli_{id}.h");
    let cfc = format!("cfc_cli_{id}.cfc");

    fs::write(
        &header,
        "int add_one(int x);\n",
    )
    .expect("write header");

    let result = run_coffee_args(&["-c", &header, "-o", &cfc]);

    let cfc_text = fs::read_to_string(&cfc).ok();
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&cfc);

    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "gen-cfc should succeed (libclang parse)\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = cfc_text.expect(".cfc should exist after -c");
    assert!(
        content.contains("c fn"),
        "expected `c fn` in .cfc:\n{content}"
    );
    assert!(
        content.contains("add_one"),
        "expected add_one in .cfc:\n{content}"
    );
}

#[test]
fn test_have_c_emits_header_with_add_prototype() {
    let id = unique_temp_id();
    let cf = format!("have_c_cli_{id}.cf");
    let ll = format!("have_c_cli_{id}.ll");
    let header = format!("have_c_cli_{id}.h");

    let source = r#"
fn add(a: int, b: int) => int:
    return a + b
fn main() => int:
    return add(1, 2)
"#;
    fs::write(&cf, source).expect("write .cf");

    let result = run_coffee_args(&["--have-c", "--emit-llvm", "-o", &ll, &cf]);

    let header_text = fs::read_to_string(&header).ok();
    let _ = fs::remove_file(&cf);
    let _ = fs::remove_file(&ll);
    let _ = fs::remove_file(&header);

    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "--have-c --emit-llvm should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = header_text.expect("{stem}.h should exist next to the .cf");
    // header_gen maps Coffee `int` (64-bit) to C `long long`
    assert!(
        content.contains("long long add(long long a, long long b);"),
        "expected C prototype for add in header:\n{content}"
    );
}

#[test]
fn test_project_have_c_emits_include_header() {
    let dir = std::env::temp_dir().join(format!("coffee_have_c_proj_{}", unique_temp_id()));
    fs::create_dir_all(dir.join("src")).expect("mkdir src");
    fs::write(
        dir.join("coffee.toml"),
        r#"[package]
name = "have_c_proj"
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
fn add(a: int, b: int) => int:
    return a + b

fn main() => int:
    return add(1, 2)
"#,
    )
    .expect("write main.cf");

    let result = run_coffee_args_in(
        Some(&dir),
        &["--have-c", "--emit-llvm", "src/main.cf"],
        true,
    );
    let header = dir.join("target/debug/include/main.h");
    let header_text = fs::read_to_string(&header).ok();
    let _ = fs::remove_dir_all(&dir);

    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "project --have-c --emit-llvm should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = header_text.expect("target/debug/include/main.h after project --have-c");
    assert!(
        content.contains("long long add(long long a, long long b);"),
        "expected C prototype for add in project header:\n{content}"
    );
}

/// Missing `.h` for `-c` must fail with a useful message (no panic).
#[test]
fn test_gen_cfc_missing_header_useful_error() {
    let id = unique_temp_id();
    let header = format!("missing_cfc_cli_{id}.h");
    assert!(
        !Path::new(&header).exists(),
        "test header must not exist: {header}"
    );

    let result = run_coffee_args(&["-c", &header]).expect("run coffee");
    assert_ne!(
        result.exit_code, 0,
        "missing header must fail\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    let lower = combined.to_lowercase();
    assert!(
        !lower.contains("panic") && !lower.contains("unwrap"),
        "missing header must not panic:\n{combined}"
    );
    assert!(
        lower.contains("not found") || lower.contains("no such file"),
        "expected a file-not-found message, got:\n{combined}"
    );
    assert!(
        combined.contains(".h") && combined.contains(".c"),
        "error should mention that .h and .c are accepted, got:\n{combined}"
    );
    assert!(
        combined.contains("-I"),
        "error should mention -I for extra include paths, got:\n{combined}"
    );
}

#[test]
fn test_gen_cfc_rejects_cf_input() {
    let source = r#"
fn main() => int:
    return 0
"#;
    let result = compile_coffee(source, &["-c"]).expect("run coffee");
    assert_ne!(
        result.exit_code, 0,
        "-c on a .cf file must fail\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains(".h or .c") || combined.contains("requires"),
        "expected input-type error, got:\n{combined}"
    );
}

#[test]
fn test_gen_cfc_long_flag_from_header() {
    let id = unique_temp_id();
    let header = format!("cfc_cli_long_{id}.h");
    let cfc = format!("cfc_cli_long_{id}.cfc");

    fs::write(&header, "int add_one(int x);\n").expect("write header");

    let result = run_coffee_args(&["--gen-cfc", &header, "-o", &cfc]);
    let exists = Path::new(&cfc).is_file();
    let content = fs::read_to_string(&cfc).ok();
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&cfc);

    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "--gen-cfc should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    assert!(exists, ".cfc missing");
    let content = content.unwrap();
    assert!(content.contains("c fn") && content.contains("add_one"), "{content}");
}

#[test]
fn test_gen_cfc_from_function_definition_and_variadic() {
    let id = unique_temp_id();
    let header = format!("cfc_cli_defn_{id}.h");
    let cfc = format!("cfc_cli_defn_{id}.cfc");

    fs::write(
        &header,
        "int add_one(int x) { return x + 1; }\nint printf(const char *, ...);\n",
    )
    .expect("write header");

    let result = run_coffee_args(&["-c", &header, "-o", &cfc]);
    let content = fs::read_to_string(&cfc).ok();
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&cfc);

    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "gen-cfc should extract decls with SkipFunctionBodies\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = content.expect(".cfc should exist after -c");
    assert!(
        content.contains("add_one"),
        "expected add_one from function definition:\n{content}"
    );
    assert!(
        content.contains("printf"),
        "expected variadic printf:\n{content}"
    );
    assert!(
        content.contains("c fn"),
        "expected `c fn` in .cfc:\n{content}"
    );
}

/// `--have-c` emits a C `struct` for a Coffee class with fields.
#[test]
fn test_have_c_class_struct() {
    let id = unique_temp_id();
    let cf = format!("have_c_class_{id}.cf");
    let ll = format!("have_c_class_{id}.ll");
    let header = format!("have_c_class_{id}.h");

    let source = r#"
class Point:
    x: int(4)+
    y: int(4)+

fn origin() => int:
    return 0

fn main() => int:
    return origin()
"#;
    fs::write(&cf, source).expect("write .cf");

    let result = run_coffee_args(&["--have-c", "--emit-llvm", "-o", &ll, &cf]);

    let header_text = fs::read_to_string(&header).ok();
    let _ = fs::remove_file(&cf);
    let _ = fs::remove_file(&ll);
    let _ = fs::remove_file(&header);

    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "--have-c with class should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = header_text.expect("{stem}.h should exist next to the .cf");
    assert!(
        content.contains("struct Point"),
        "expected C struct for class Point in header:\n{content}"
    );
    assert!(
        content.contains("int x;") && content.contains("int y;"),
        "expected int(4)+ fields mapped to C int:\n{content}"
    );
}

/// `--have-c` on a module with only an external `c fn` does not write a header;
/// CLI warns (`no exportable functions`) and still exits 0.
#[test]
fn test_have_c_rejects_empty_exports() {
    let id = unique_temp_id();
    let cf = format!("have_c_empty_{id}.cf");
    let ll = format!("have_c_empty_{id}.ll");
    let header = format!("have_c_empty_{id}.h");

    // External `c fn` must include `=>` return type; `c fn foo():` is a parse error.
    fs::write(&cf, "c fn foo() => void:\n").expect("write .cf");

    let result = run_coffee_args(&["--have-c", "--emit-llvm", "-o", &ll, &cf]);

    let header_exists = Path::new(&header).is_file();
    let header_text = fs::read_to_string(&header).ok();
    let _ = fs::remove_file(&cf);
    let _ = fs::remove_file(&ll);
    let _ = fs::remove_file(&header);

    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "compile of external-only module should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    assert!(
        !header_exists,
        "no .h should be written when generate_header finds no exports; got:\n{}",
        header_text.unwrap_or_default()
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("no exportable functions")
            || combined.contains("Failed to generate C header"),
        "expected generate_header warning, got:\n{combined}"
    );
}

fn cargo_toml_package_version() -> String {
    let text = fs::read_to_string("Cargo.toml").expect("Cargo.toml");
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("version") {
            let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
            if let Some(v) = rest.strip_prefix('"').and_then(|s| s.split('"').next()) {
                return v.to_string();
            }
        }
    }
    panic!("package version not found in Cargo.toml");
}

#[test]
fn test_help_lists_have_c_and_test_mode() {
    let result = run_coffee_args(&["--help"]).expect("run coffee --help");
    assert_eq!(result.exit_code, 0, "stderr={}", result.stderr);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("--have-c"),
        "--help should list --have-c:\n{hay}"
    );
    assert!(
        hay.contains("--test-mode"),
        "--help should list --test-mode:\n{hay}"
    );
    let ver = cargo_toml_package_version();
    assert!(
        hay.contains(&ver),
        "--help should show crate version {ver}:\n{hay}"
    );
}

#[test]
fn test_version_matches_cargo_toml() {
    let result = run_coffee_args(&["--version"]).expect("run coffee --version");
    assert_eq!(result.exit_code, 0, "stderr={}", result.stderr);
    let ver = cargo_toml_package_version();
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains(&ver),
        "--version should match Cargo.toml ({ver}):\n{hay}"
    );
    let plus = hay.find('+');
    assert!(
        plus.is_some(),
        "--version should suffix crate version with compiler binary hash: {hay}"
    );
}

#[test]
fn test_unknown_flag_mentions_help() {
    let result = run_coffee_args(&["--not-a-real-flag"]).expect("run coffee");
    assert_ne!(result.exit_code, 0);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("--help"),
        "unknown flag should mention --help:\n{hay}"
    );
}

#[test]
fn test_missing_dash_o_arg_errors() {
    let result = run_coffee_args(&["-o"]).expect("run coffee");
    assert_ne!(result.exit_code, 0);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("-o") && (hay.contains("--help") || hay.contains("Usage:")),
        "missing -o argument should error with short help:\n{hay}"
    );
}

#[test]
fn test_init_main_cf_has_exactly_one_entry() {
    let dir = std::env::temp_dir().join(format!("coffee_init_one_entry_{}", unique_temp_id()));
    fs::create_dir_all(&dir).expect("temp dir");

    let init = run_coffee_args_in(Some(&dir), &["init"], true).expect("run coffee init");
    assert_eq!(
        init.exit_code, 0,
        "coffee init should succeed in empty dir\nstdout={}\nstderr={}",
        init.stdout, init.stderr
    );

    let main_cf = fs::read_to_string(dir.join("src/main.cf")).expect("src/main.cf after init");
    let has_fn_main = main_cf.lines().any(|l| l.trim_start().starts_with("fn main"));
    let has_bare_main = main_cf.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("main(") && !t.starts_with("fn main")
    });
    assert_eq!(
        (has_fn_main as u8) + (has_bare_main as u8),
        1,
        "src/main.cf must have exactly one entry (fn main or main(...), not both):\n{main_cf}"
    );

    let compile = run_coffee_args_in(Some(&dir), &["--emit-llvm"], true).expect("compile init project");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        compile.exit_code, 0,
        "project from coffee init should compile\nstdout={}\nstderr={}",
        compile.stdout, compile.stderr
    );
}

#[test]
fn test_init_fails_when_directory_not_empty() {
    let dir = Path::new(".").join(format!("init_cli_not_empty_{}", unique_temp_id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let toml = dir.join("coffee.toml");
    fs::write(&toml, "[package]\nname = \"already\"\n").expect("write coffee.toml");

    let result = run_coffee_args_in(Some(&dir), &["init"], true).expect("run coffee init");
    let _ = fs::remove_dir_all(&dir);

    assert_ne!(result.exit_code, 0);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("not empty") && hay.contains("coffee.toml"),
        "init on existing project should say directory is not empty:\n{hay}"
    );
}

#[test]
fn test_init_write_failure_is_actionable() {
    let dir = Path::new(".").join(format!("init_cli_nowrite_{}", unique_temp_id()));
    fs::create_dir_all(&dir).expect("temp dir");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o555);
        fs::set_permissions(&dir, perms).expect("chmod");
    }

    let result = run_coffee_args_in(Some(&dir), &["init"], true).expect("run coffee init");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
    }
    let _ = fs::remove_dir_all(&dir);

    if cfg!(unix) {
        assert_ne!(result.exit_code, 0, "init in unwritable dir should fail");
        let hay = format!("{}{}", result.stdout, result.stderr);
        assert!(
            hay.contains("failed to write") || hay.contains("Permission"),
            "init write failure should mention path/write:\n{hay}"
        );
        assert!(
            hay.contains("hint") || hay.contains("permission"),
            "init write failure should be actionable:\n{hay}"
        );
    }
}

#[test]
fn test_lock_busy_mentions_path_compiling_and_test_mode() {
    let dir = Path::new(".").join(format!("lock_cli_{}", unique_temp_id()));
    fs::create_dir_all(&dir).expect("temp dir");
    fs::write(
        dir.join("coffee.toml"),
        "[package]\nname = \"lock_cli\"\nversion = \"0.0.0\"\n",
    )
    .expect("coffee.toml so lock stays in this dir");
    let lock_path = dir.join(".coffee_compiler.lock");
    fs::write(&lock_path, "").expect("lock file");

    let mut holder = match Command::new("flock")
        .arg("-x")
        .arg(&lock_path)
        .arg("sleep")
        .arg("20")
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            let _ = fs::remove_dir_all(&dir);
            panic!("flock is required to hold .coffee_compiler.lock in this test");
        }
    };
    std::thread::sleep(std::time::Duration::from_millis(400));

    let result = run_coffee_args_in(Some(&dir), &["--version"], false);
    let _ = holder.kill();
    let _ = holder.wait();
    let _ = fs::remove_dir_all(&dir);

    let result = result.expect("run coffee without --test-mode");
    assert_ne!(result.exit_code, 0, "busy lock should fail the compiler");
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains(".coffee_compiler.lock"),
        "lock error should include the lock path:\n{hay}"
    );
    assert!(
        hay.contains("compiling") || hay.to_lowercase().contains("another"),
        "lock error should say another coffee is compiling:\n{hay}"
    );
    assert!(
        hay.contains("--test-mode"),
        "lock error should mention --test-mode for tests:\n{hay}"
    );
}

#[test]
fn test_missing_input_file_errors() {
    let result = run_coffee_args(&["--bin"]).expect("run coffee");
    assert_ne!(result.exit_code, 0);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("missing input") || hay.contains(".cf") || hay.contains("help"),
        "expected a missing-input or project message, got:\n{hay}"
    );
}

#[test]
fn test_conflicting_emit_flags_error() {
    let src = format!("emit_conflict_{}.cf", unique_temp_id());
    fs::write(&src, "fn main() => int:\n    return 0\n").expect("write");
    let result = run_coffee_args(&["--emit-llvm", "--emit-bc", &src]).expect("run coffee");
    let _ = fs::remove_file(&src);
    assert_ne!(result.exit_code, 0);
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("only one") || hay.contains("emit-llvm"),
        "expected emit conflict help, got:\n{hay}"
    );
}

#[test]
fn test_gen_cfc_emits_c_class_file_newtype_and_fn_pointer() {
    let id = unique_temp_id();
    let header = format!("cfc_types_{id}.h");
    let cfc = format!("cfc_types_{id}.cfc");
    fs::write(
        &header,
        r#"
struct Point { int x; int y; };
typedef struct FILE FILE;
FILE *fopen_demo(const char *filename, const char *mode);
struct Box { char *name; };
void take_cb(int (*cb)(int));
"#,
    )
    .expect("write header");

    let result = run_coffee_args(&["-c", &header, "-o", &cfc]);
    let cfc_text = fs::read_to_string(&cfc).ok();
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&cfc);
    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "gen-cfc types should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = cfc_text.expect(".cfc should exist");
    assert!(
        content.contains("c class Point"),
        "expected c class Point:\n{content}"
    );
    assert!(
        content.contains("type FILE: object"),
        "expected FILE newtype:\n{content}"
    );
    assert!(
        content.contains("filename: string") || content.contains("filename: str"),
        "char* param should be string:\n{content}"
    );
    assert!(
        content.contains("name: object"),
        "char* field should be object:\n{content}"
    );
    assert!(
        content.contains("fn(") && content.contains("take_cb"),
        "function pointer param:\n{content}"
    );
    assert!(
        !content.contains("*int") && !content.contains("*Point"),
        "must not emit Coffee *T:\n{content}"
    );
}

#[test]
fn test_gen_cfc_keeps_functions_from_same_library_split_headers() {
    let id = unique_temp_id();
    let extra = format!("cfc_inc_{id}_extra.h");
    let header = format!("cfc_inc_{id}.h");
    let cfc = format!("cfc_inc_{id}.cfc");
    fs::write(&extra, "int leak_fn(int y);\n").expect("write extra header");
    fs::write(
        &header,
        format!("#include \"{extra}\"\nint keep_fn(int x);\n"),
    )
    .expect("write main header");

    let result = run_coffee_args(&["-c", &header, "-I", ".", "-o", &cfc]);
    let cfc_text = fs::read_to_string(&cfc).ok();
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&extra);
    let _ = fs::remove_file(&cfc);
    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "gen-cfc satellite header should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = cfc_text.expect(".cfc should exist");
    assert!(
        content.contains("keep_fn"),
        "expected keep_fn from the main header:\n{content}"
    );
    assert!(
        content.contains("leak_fn"),
        "split-header functions without a distinct .so are satellites and must be kept:\n{content}"
    );
}

#[test]
fn test_gen_cfc_skips_posix_sys_unistd_include() {
    let id = unique_temp_id();
    let sys_dir = format!("cfc_posix_{id}_sys");
    fs::create_dir_all(&sys_dir).expect("mkdir sys");
    let extra = format!("{sys_dir}/unistd.h");
    let header = format!("cfc_posix_{id}.h");
    let cfc = format!("cfc_posix_{id}.cfc");
    fs::write(&extra, "int posix_skip_fn(int y);\n").expect("write sys/unistd.h");
    fs::write(
        &header,
        format!("#include \"{extra}\"\nint keep_fn(int x);\n"),
    )
    .expect("write main header");

    let result = run_coffee_args(&["-c", &header, "-I", ".", "-o", &cfc]);
    let cfc_text = fs::read_to_string(&cfc).ok();
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&extra);
    let _ = fs::remove_file(&cfc);
    let _ = fs::remove_dir(&sys_dir);
    let result = result.expect("run coffee");
    assert_eq!(
        result.exit_code, 0,
        "gen-cfc posix cut should succeed\nstdout={}\nstderr={}",
        result.stdout, result.stderr
    );
    let content = cfc_text.expect(".cfc should exist");
    assert!(
        content.contains("keep_fn"),
        "expected keep_fn from the main header:\n{content}"
    );
    assert!(
        !content.contains("posix_skip_fn"),
        "sys/unistd.h decls must be libc-cut:\n{content}"
    );
}

#[test]
fn parse_cfc_content_reads_owned_headers_and_needs_metadata() {
    let content = r#"
// coffee-cfc 1
// module: z
// linker: z
// headers: zlib.h zconf.h
// needs: libc
c fn foo() => void:
"#;
    let table = coffee::c::parse_cfc_content(content, "z").expect("parse metadata .cfc");
    assert_eq!(
        table.owned_headers,
        vec!["zlib.h".to_string(), "zconf.h".to_string()]
    );
    assert_eq!(table.needs, vec!["libc".to_string()]);
    assert_eq!(table.linker, "z");
    assert_eq!(table.library, "z");
    assert!(table.get("foo").is_some());
}
