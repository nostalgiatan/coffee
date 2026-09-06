// Package fetch: path XOR url+hash, cache, coffee fetch --save.

include!("common/mod.rs");

use std::path::Path;

fn pkg_cache_dir() -> PathBuf {
    std::env::temp_dir().join(format!("coffee_test_cache_{}", unique_temp_id()))
}

fn fixture_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("coffee_pkg_{}_{}", tag, unique_temp_id()))
}

fn coffee_bin_path() -> PathBuf {
    coffee_bin(&std::env::current_dir().expect("cwd"))
}

fn run_pkg_coffee(cwd: &Path, cache: &Path, args: &[&str]) -> TestResult {
    let mut cmd = Command::new(coffee_bin_path());
    cmd.current_dir(cwd);
    cmd.env("COFFEE_CACHE", cache);
    cmd.arg("--test-mode");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("run coffee");
    test_result_from_output(&output)
}

fn write_lib_pkg(dir: &Path, name: &str, extra_toml: &str, cf_body: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("coffee.toml"),
        format!("[package]\nname = \"{name}\"\n{extra_toml}"),
    )
    .unwrap();
    fs::write(dir.join("src").join(format!("{name}.cf")), cf_body).unwrap();
}

fn write_app(dir: &Path, packages_toml: &str, main_cf: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("coffee.toml"),
        format!(
            r#"[package]
name = "app"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"

{packages_toml}
"#
        ),
    )
    .unwrap();
    fs::write(dir.join("src/main.cf"), main_cf).unwrap();
}

fn tar_gz_dir(src_dir: &Path, tar_path: &Path) {
    if let Some(p) = tar_path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    let parent = src_dir.parent().unwrap();
    let name = src_dir.file_name().unwrap();
    let status = Command::new("tar")
        .current_dir(parent)
        .args(["-czf", tar_path.to_str().unwrap(), name.to_str().unwrap()])
        .status()
        .expect("tar");
    assert!(status.success(), "tar failed");
}

const HELPER_CF: &str = r#"fn helper_val() => int:
    return 42
"#;

const APP_MAIN: &str = r#"use helper_val in helper
use printf in libc of c

fn main() => int:
    printf("ok")
    return helper_val()
"#;

#[test]
fn path_dep_compiles_with_packages_entry() {
    let base = fixture_dir("path_ok");
    let cache = pkg_cache_dir();
    let app = base.join("app");
    let helper = base.join("helper_pkg");
    write_lib_pkg(&helper, "helper", "", HELPER_CF);
    write_app(
        &app,
        "[dependencies.packages.helper]\npath = \"../helper_pkg\"\n",
        APP_MAIN,
    );
    let r = run_pkg_coffee(&app, &cache, &["--emit-llvm"]);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
}

#[test]
fn path_dep_fails_without_packages_entry() {
    let base = fixture_dir("path_no");
    let cache = pkg_cache_dir();
    let app = base.join("app");
    let helper = base.join("helper_pkg");
    write_lib_pkg(&helper, "helper", "", HELPER_CF);
    write_app(&app, "", APP_MAIN);
    let r = run_pkg_coffee(&app, &cache, &["--emit-llvm"]);
    assert_ne!(r.exit_code, 0, "expected import failure");
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
}

#[test]
fn file_url_tarball_compiles() {
    let base = fixture_dir("tarball");
    let cache = pkg_cache_dir();
    let helper = base.join("helper_pkg");
    write_lib_pkg(&helper, "helper", "", HELPER_CF);
    let tar = base.join("helper.tar.gz");
    tar_gz_dir(&helper, &tar);

    let app = base.join("app");
    write_app(&app, "", APP_MAIN);
    let url = format!("file://{}", tar.display());
    let r = run_pkg_coffee(&app, &cache, &["fetch", &url, "--save", "helper"]);
    assert_eq!(r.exit_code, 0, "fetch --save: {}", r.stderr);
    let toml = fs::read_to_string(app.join("coffee.toml")).unwrap();
    assert!(toml.contains("url"), "{toml}");
    assert!(toml.contains("hash"), "{toml}");

    let r = run_pkg_coffee(&app, &cache, &["--emit-llvm"]);
    assert_eq!(r.exit_code, 0, "compile: stderr={} stdout={}", r.stderr, r.stdout);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
}

#[test]
fn wrong_hash_fails_and_leaves_no_cache() {
    let base = fixture_dir("wrong");
    let cache = pkg_cache_dir();
    let helper = base.join("helper_pkg");
    write_lib_pkg(&helper, "helper", "", HELPER_CF);
    let tar = base.join("helper.tar.gz");
    tar_gz_dir(&helper, &tar);
    let app = base.join("app");
    let wrong = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    write_app(
        &app,
        &format!(
            "[dependencies.packages.helper]\nurl = \"file://{}\"\nhash = \"{wrong}\"\n",
            tar.display()
        ),
        APP_MAIN,
    );
    let r = run_pkg_coffee(&app, &cache, &["--emit-llvm"]);
    assert_ne!(r.exit_code, 0);
    assert!(
        r.stderr.contains("hash mismatch") || r.stdout.contains("hash mismatch"),
        "stderr={} stdout={}",
        r.stderr,
        r.stdout
    );
    assert!(!cache.join("p").join(wrong).exists());
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
}

#[test]
fn transitive_path_deps_compile() {
    let base = fixture_dir("trans");
    let cache = pkg_cache_dir();
    let leaf = base.join("leaf_pkg");
    let mid = base.join("mid_pkg");
    let app = base.join("app");
    write_lib_pkg(
        &leaf,
        "leaf",
        "",
        "fn leaf_val() => int:\n    return 3\n",
    );
    write_lib_pkg(
        &mid,
        "mid",
        "[dependencies.packages.leaf]\npath = \"../leaf_pkg\"\n",
        "use leaf_val in leaf\n\nfn mid_val() => int:\n    return leaf_val()\n",
    );
    write_app(
        &app,
        "[dependencies.packages.mid]\npath = \"../mid_pkg\"\n",
        "use mid_val in mid\n\nfn main() => int:\n    return mid_val()\n",
    );
    let r = run_pkg_coffee(&app, &cache, &["--emit-llvm"]);
    assert_eq!(r.exit_code, 0, "stderr={} stdout={}", r.stderr, r.stdout);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
}

#[test]
fn fetch_save_writes_url_and_hash() {
    let base = fixture_dir("save");
    let cache = pkg_cache_dir();
    let helper = base.join("helper_pkg");
    write_lib_pkg(&helper, "helper", "", HELPER_CF);
    let tar = base.join("helper.tar.gz");
    tar_gz_dir(&helper, &tar);
    let app = base.join("app");
    write_app(
        &app,
        "",
        "fn main() => int:\n    return 0\n",
    );
    let url = format!("file://{}", tar.display());
    let r = run_pkg_coffee(&app, &cache, &["fetch", &url, "--save"]);
    assert_eq!(r.exit_code, 0, "{}", r.stderr);
    let toml = fs::read_to_string(app.join("coffee.toml")).unwrap();
    assert!(toml.contains("[dependencies.packages.helper]"), "{toml}");
    assert!(toml.contains(&url), "{toml}");
    assert!(toml.contains("hash"), "{toml}");
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&cache);
}
