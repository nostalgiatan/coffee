// Bundled libc/libm `.cfc` tables and LLVM declare from `CSymbol`.

include!("common/mod.rs");

#[test]
fn bundled_tables_have_libc_write_printf_and_libm_sin() {
    let tables = coffee::c::load_bundled_c_tables();
    assert!(
        tables.get("libc").and_then(|t| t.get("write")).is_some(),
        "write must be in bundled libc"
    );
    assert!(
        tables.get("libc").and_then(|t| t.get("printf")).is_some(),
        "printf must be in bundled libc"
    );
    assert!(
        tables.get("libm").and_then(|t| t.get("sin")).is_some(),
        "sin must be in bundled libm"
    );
    assert!(
        !tables.contains_key("c"),
        "bundled keys must be libc/libm, not extract_library_name(\"libc.cfc\") == c"
    );
}

#[test]
fn write_from_libc_appears_in_llvm_ir() {
    let source = r#"
use write, malloc, free in libc of c
fn main() => int:
    let p: object = malloc(1)
    write(1, p, 1)
    free(p)
    return 0
"#;
    let ll = format!("cfc_write_{}.ll", unique_temp_id());
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let ir = std::fs::read_to_string(&ll).unwrap_or_default();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(
        result.exit_code, 0,
        "compile write() should succeed, not unknown C function:\n{}",
        result.stderr
    );
    assert!(
        ir.contains("write"),
        "IR must mention write:\n{ir}"
    );
    let write_decl = ir
        .lines()
        .find(|l| l.contains("declare") && l.contains("@write"))
        .unwrap_or("");
    assert!(
        !write_decl.contains("..."),
        "write is not varargs:\n{write_decl}\nfull:\n{ir}"
    );
    assert!(
        write_decl.contains("ptr") || ir.contains("ptr"),
        "object param/return must be LLVM ptr:\n{ir}"
    );
}

#[test]
fn write_accepts_str_and_memcpy_accepts_buf() {
    let source = r#"
use write, memcpy, malloc, free in libc of c
fn main() => int:
    write(1, "x", 1)
    let a: buf = malloc(8)
    let b: buf = malloc(8)
    memcpy(a, b, 8)
    rm a
    rm b
    return 0
"#;
    let ll = format!("cfc_buf_{}.ll", unique_temp_id());
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(
        result.exit_code, 0,
        "str/buf must pass as C object (ptr):\n{}",
        result.stderr
    );
}

fn collect_src_rs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "testdata") {
                continue;
            }
            collect_src_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn src_has_no_deleted_c_builtin_identifiers() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_src_rs(&src, &mut files);
    assert!(!files.is_empty(), "expected .rs files under src/");
    let forbidden = [
        "builtin_cfc_tables",
        "is_builtin_c_function",
        "get_builtin_c_function_signature",
    ];
    let mut hits = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).unwrap();
        for needle in forbidden {
            if text.contains(needle) {
                hits.push(format!("{}: {needle}", path.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "deleted C builtin identifiers must not appear in src/**/*.rs: {hits:?}"
    );
}

#[test]
fn bundled_libc_has_file_type_and_fopen_signature() {
    use coffee::c::CTypeDef;

    let tables = coffee::c::load_bundled_c_tables();
    let libc = tables.get("libc").expect("bundled libc");

    let file_def = libc.type_defs.get("FILE").expect("FILE type def");
    assert_eq!(
        file_def,
        &CTypeDef::Newtype {
            name: "FILE".to_string(),
            source: "object".to_string(),
        }
    );

    let fopen = libc.get("fopen").expect("fopen");
    assert_eq!(fopen.return_type, "FILE");

    let fclose = libc.get("fclose").expect("fclose");
    assert_eq!(fclose.parameters.len(), 1);
    assert_eq!(fclose.parameters[0].param_type, "FILE");
}
