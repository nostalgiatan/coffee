// Task C: C library toml name default, include_paths are not linker -L,
// and undeclared `use` still needs a .cfc.

include!("common/mod.rs");

use coffee::compiler::ProjectConfig;
use std::path::Path;

fn write_toml(dir: &Path, body: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("coffee.toml"), body).unwrap();
}

#[test]
fn c_library_name_defaults_to_table_key_when_omitted() {
    let dir = std::env::temp_dir().join(format!("coffee_cdep_name_{}", unique_temp_id()));
    write_toml(
        &dir,
        r#"
[package]
name = "cdep_name"
version = "0.0.1"

[build]
main = "src/main"

[dependencies.c_libraries.z]
headers = ["zlib.h"]
"#,
    );
    let cfg = ProjectConfig::from_file(&dir.join("coffee.toml"))
        .expect("toml without name field should load");
    let lib = cfg
        .dependencies
        .c_libraries
        .get("z")
        .expect("c_libraries.z");
    assert_eq!(lib.name, "z");
    assert_eq!(lib.headers, vec!["zlib.h".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn include_paths_are_not_library_link_names() {
    let dir = std::env::temp_dir().join(format!("coffee_cdep_inc_{}", unique_temp_id()));
    write_toml(
        &dir,
        r#"
[package]
name = "cdep_inc"
version = "0.0.1"

[build]
main = "src/main"

[dependencies.c_libraries.z]
headers = ["zlib.h"]
include_paths = ["/usr/include"]
"#,
    );
    let cfg = ProjectConfig::from_file(&dir.join("coffee.toml")).expect("load");
    let includes = cfg.get_c_include_paths();
    assert!(
        includes.iter().any(|p| p == "/usr/include"),
        "include_paths stay on get_c_include_paths: {includes:?}"
    );
    let names: Vec<String> = cfg
        .get_c_library_link_types()
        .into_iter()
        .map(|(n, _, _)| n)
        .collect();
    assert!(
        !names.iter().any(|n| n.contains("/usr/include") || n == "/usr/include"),
        "include_paths must not be mixed into linker library names: {names:?}"
    );
    assert_eq!(names, vec!["z".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn undeclared_c_use_without_cfc_still_fails() {
    let source = r#"
use zlibVersion in z of c

fn main() => int:
    return 0
"#;
    assert_compile_error(source, ".cfc").unwrap();
}

#[test]
fn missing_cfc_error_mentions_coffee_c_or_c_libraries() {
    let source = r#"
use zlibVersion in z of c

fn main() => int:
    return 0
"#;
    let result = compile_coffee(source, &[]).expect("run compiler");
    assert_ne!(result.exit_code, 0, "undeclared C use must fail");
    let text = format!("{}\n{}", result.stderr, result.stdout);
    assert!(
        text.contains("coffee -c") || text.contains("c_libraries"),
        "missing .cfc diagnostic must hint coffee -c or c_libraries, got:\n{text}"
    );
}

#[test]
fn empty_headers_does_not_auto_generate_cfc() {
    use coffee::compiler::ProjectBuilder;

    let dir = std::env::temp_dir().join(format!("coffee_cdep_empty_{}", unique_temp_id()));
    write_toml(
        &dir,
        r#"
[package]
name = "cdep_empty"
version = "0.0.1"

[build]
main = "src/main"

[dependencies.c_libraries.foo]
"#,
    );
    let cfg = ProjectConfig::from_file(&dir.join("coffee.toml")).expect("load");
    let builder = ProjectBuilder::new(cfg);
    builder
        .ensure_c_library_cfcs()
        .expect("empty headers must skip generate, not error");
    assert!(
        !dir.join("target").join("cfc").join("libfoo.cfc").exists(),
        "empty headers must not invent libfoo.cfc"
    );
    let _ = fs::remove_dir_all(&dir);
}
