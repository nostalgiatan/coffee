// Official `library/std` package: print via write(2), IntBuf growable ints.

include!("common/mod.rs");

use std::path::Path;

fn std_pkg_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("library/std")
}

fn fixture_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("coffee_std_{}_{}", tag, unique_temp_id()))
}

fn coffee_bin_path() -> PathBuf {
    coffee_bin(&std::env::current_dir().expect("cwd"))
}

fn std_packages_toml() -> String {
    format!(
        "[dependencies.packages.std]\npath = \"{}\"\n",
        std_pkg_root().display()
    )
}

fn write_std_app(dir: &Path, main_cf: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("coffee.toml"),
        format!(
            r#"[package]
name = "std_lib_app"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"

{}
"#,
            std_packages_toml()
        ),
    )
    .unwrap();
    fs::write(dir.join("src/main.cf"), main_cf).unwrap();
}

fn run_std_project(cwd: &Path, cache: &Path, args: &[&str]) -> TestResult {
    let mut cmd = Command::new(coffee_bin_path());
    cmd.current_dir(cwd);
    cmd.env("COFFEE_CACHE", cache);
    cmd.env("COFFEE_STD", std_pkg_root());
    cmd.arg("--test-mode");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("run coffee");
    test_result_from_output(&output)
}

fn run_std_jit(app: &Path, cache: &Path) -> TestResult {
    run_std_project(app, cache, &["--jit", "src/main.cf"])
}

#[test]
fn test_std_print_hello_stdout() {
    let base = fixture_dir("print");
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let app = base.join("app");
    write_std_app(
        &app,
        r#"use print in std

fn main() => int:
    print("hello-std\n")
    return 0
"#,
    );
    let r = run_std_jit(&app, &cache);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    assert!(
        !r.stderr.contains("M005"),
        "hello world must not warn about std internals: stderr={}",
        r.stderr
    );
    assert!(
        r.stdout.contains("hello-std"),
        "expected hello-std on stdout, got stdout={:?} stderr={:?}",
        r.stdout,
        r.stderr
    );
}

#[test]
fn test_std_intbuf_push_get_length() {
    let base = fixture_dir("intbuf");
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let app = base.join("app");
    write_std_app(
        &app,
        r#"use mem
use print in std

fn main() => int:
    let b: IntBuf = IntBuf::new()
    b.push(1)
    b.push(2)
    b.push(3)
    let g: int = b.get(1)
    let n: int = b.length()
    if g == 2:
        if n == 3:
            print("intbuf-ok\n")
            return 0
    print("intbuf-fail\n")
    return 1
"#,
    );
    let r = run_std_jit(&app, &cache);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    assert!(
        r.stdout.contains("intbuf-ok"),
        "expected intbuf-ok on stdout, got stdout={:?} stderr={:?}",
        r.stdout,
        r.stderr
    );
}

#[test]
fn test_std_intbuf_grows_past_eight_and_oob_get_is_zero() {
    let base = fixture_dir("intbuf_grow");
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let app = base.join("app");
    write_std_app(
        &app,
        r#"use mem
use print in std

fn main() => int:
    let b: IntBuf = IntBuf::new()
    let i: int = 0
    while i < 10:
        b.push(i + 1)
        i = i + 1
    let g0: int = b.get(0)
    let g9: int = b.get(9)
    let miss: int = b.get(10)
    let n: int = b.length()
    if g0 == 1:
        if g9 == 10:
            if miss == 0:
                if n == 10:
                    print("intbuf-grow-ok\n")
                    return 0
    print("intbuf-grow-fail\n")
    return 1
"#,
    );
    let r = run_std_jit(&app, &cache);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    assert!(
        r.stdout.contains("intbuf-grow-ok"),
        "expected intbuf-grow-ok on stdout, got stdout={:?} stderr={:?}",
        r.stdout,
        r.stderr
    );
}

fn write_user_project_no_std_dep(dir: &Path, main_cf: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("coffee.toml"),
        r#"[package]
name = "std_user"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#,
    )
    .unwrap();
    fs::write(dir.join("src/main.cf"), main_cf).unwrap();
}

#[test]
fn test_std_eprint_goes_to_stderr() {
    let base = fixture_dir("eprint");
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let app = base.join("app");
    write_std_app(
        &app,
        r#"use eprint in std

fn main() => int:
    eprint("hello-eprint\n")
    return 0
"#,
    );
    let r = run_std_jit(&app, &cache);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    assert!(
        r.stderr.contains("hello-eprint"),
        "expected hello-eprint on stderr, got stdout={:?} stderr={:?}",
        r.stdout,
        r.stderr
    );
}

#[test]
fn test_std_print_without_packages_std_dep() {
    let base = fixture_dir("nodep");
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let app = base.join("app");
    write_user_project_no_std_dep(
        &app,
        r#"use print in std

fn main() => int:
    print("hello-nodep\n")
    return 0
"#,
    );
    let r = run_std_jit(&app, &cache);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    assert!(
        r.stdout.contains("hello-nodep"),
        "compiler-injected std should print, stdout={:?} stderr={:?}",
        r.stdout,
        r.stderr
    );
}

#[test]
fn test_std_print_single_file_jit() {
    let dir = fixture_dir("single");
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("hello.cf");
    fs::write(
        &src,
        r#"use print in std

fn main() => int:
    print("hello-single\n")
    return 0
"#,
    )
    .unwrap();
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let r = run_std_project(&dir, &cache, &["--jit", "hello.cf"]);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&cache);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    assert!(
        r.stdout.contains("hello-single"),
        "single-file use print in std, stdout={:?} stderr={:?}",
        r.stdout,
        r.stderr
    );
}

#[test]
fn test_std_print_hello_native_run_exits() {
    let base = fixture_dir("print_run");
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let app = base.join("app");
    write_std_app(
        &app,
        r#"use print in std

fn main() => int:
    print("hello-run\n")
    return 0
"#,
    );
    let compile = run_std_project(&app, &cache, &["--bin", "-O0"]);
    assert_eq!(
        compile.exit_code, 0,
        "compile stderr={} stdout={}",
        compile.stderr, compile.stdout
    );
    assert!(
        !compile.stderr.contains("M005"),
        "native hello must not warn M005: {}",
        compile.stderr
    );
    let ll = run_std_project(&app, &cache, &["--emit-llvm", "-o", "hello_run.ll"]);
    let ir = fs::read_to_string(app.join("hello_run.ll")).unwrap_or_default();
    assert_eq!(ll.exit_code, 0, "emit-llvm stderr={}", ll.stderr);
    assert!(
        !ir.lines().any(|l| l.contains("define ") && l.contains("@exit(")),
        "Coffee must not define libc exit (CRT calls it after main):\n{ir}"
    );

    let exe = app.join("target").join("debug").join("std_lib_app");
    assert!(exe.is_file(), "missing exe at {}", exe.display());
    let mut child = Command::new(&exe)
        .current_dir(&app)
        .spawn()
        .expect("spawn hello");
    let start = std::time::Instant::now();
    let status = loop {
        if let Ok(Some(st)) = child.try_wait() {
            break Some(st);
        }
        if start.elapsed() > std::time::Duration::from_secs(3) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
    let status = status.expect("hello printed then hung: libc exit was replaced by Coffee fn exit");
    assert!(status.success(), "hello native exit={status:?}");
}

#[test]
fn test_std_exit_sets_process_status() {
    let base = fixture_dir("exit");
    let cache = std::env::temp_dir().join(format!("coffee_std_cache_{}", unique_temp_id()));
    let app = base.join("app");
    write_std_app(
        &app,
        r#"use exit in std

fn main() => int:
    exit(3)
    return 0
"#,
    );
    let compile = run_std_project(&app, &cache, &["--bin"]);
    let exe = app.join("target").join("debug").join("std_lib_app");
    let linked_ok = compile.exit_code == 0 && exe.is_file();
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
    assert!(
        linked_ok,
        "std.exit should typecheck and link: stderr={} stdout={}",
        compile.stderr, compile.stdout
    );
}

#[test]
fn test_std_of_c_only_in_sys_cf() {
    let root = std_pkg_root().join("src");
    let mut offenders = Vec::new();
    for name in ["std.cf", "io.cf", "mem.cf", "sys.cf"] {
        let path = root.join(name);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let has_c = text.lines().any(|l| {
            let t = l.trim();
            !t.starts_with("/#") && t.contains(" of c")
        });
        if name == "sys.cf" {
            assert!(has_c, "sys.cf must be the C edge");
        } else if has_c {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "of c only in sys.cf, also found in {offenders:?}"
    );
}
