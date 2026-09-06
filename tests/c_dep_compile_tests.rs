// Project compile auto-generates target/cfc from c_libraries.headers.

include!("common/mod.rs");

use coffee::compiler::{ProjectBuilder, ProjectConfig};
use std::path::Path;

fn zlib_header() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(prefix) = std::env::var("PREFIX") {
        candidates.push(PathBuf::from(prefix).join("include").join("zlib.h"));
    }
    candidates.push(PathBuf::from("/usr/include/zlib.h"));
    candidates.into_iter().find(|p| p.exists())
}

#[test]
fn project_compile_autogens_libz_cfc_from_headers_key() {
    let Some(_) = zlib_header() else {
        eprintln!("skip: zlib.h not in $PREFIX/include or /usr/include");
        return;
    };

    let dir = std::env::temp_dir().join(format!("coffee_cdep_z_{}", unique_temp_id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("coffee.toml"),
        r#"
[package]
name = "cdep_z"
version = "0.0.1"

[build]
main = "src/main"

[dependencies.c_libraries.z]
headers = ["zlib.h"]
"#,
    )
    .unwrap();
    fs::write(
        dir.join("src/main.cf"),
        r#"
use zlibVersion in z of c
use puts in libc of c

fn main() => int:
    puts(zlibVersion())
    return 0
"#,
    )
    .unwrap();

    let cfg = ProjectConfig::from_file(&dir.join("coffee.toml")).expect("load toml");
    let builder = ProjectBuilder::new(cfg);
    builder
        .ensure_c_library_cfcs()
        .expect("auto-generate libz.cfc from zlib.h");

    let cfc = dir.join("target").join("cfc").join("libz.cfc");
    assert!(
        cfc.exists(),
        "expected {} after ensure_c_library_cfcs",
        cfc.display()
    );

    let compiler = coffee_bin(&std::env::current_dir().unwrap());
    let output = Command::new(&compiler)
        .current_dir(&dir)
        .arg("--test-mode")
        .arg("--bin")
        .arg("src/main.cf")
        .output()
        .expect("run coffee --bin");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "project --bin should compile zlibVersion via auto .cfc\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(Path::new(&cfc).exists(), "libz.cfc must remain after compile");

    let _ = fs::remove_dir_all(&dir);
}
