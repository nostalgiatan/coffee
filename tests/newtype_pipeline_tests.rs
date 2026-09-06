include!("common/mod.rs");

#[test]
fn type_decl_as_object_compiles() {
    let source = r#"
use fopen in libc of c
fn main() => int:
    let f: FILE = fopen("a", "r")
    let q: object = f as object
    let f2: FILE = q as FILE
    return 0
"#;
    let ll = format!("newtype_as_{}.ll", unique_temp_id());
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(
        result.exit_code, 0,
        "type FILE + as should compile:\n{}",
        result.stderr
    );
}

#[test]
fn user_hwnd_newtype_as_compiles() {
    let source = r#"
type HWND: object
fn main() => int:
    let p: object = 0
    let h: HWND = p as HWND
    let q: object = h as object
    return 0
"#;
    let ll = format!("hwnd_as_{}.ll", unique_temp_id());
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(
        result.exit_code, 0,
        "user type HWND + as should compile:\n{}",
        result.stderr
    );
}

#[test]
fn fopen_binds_file_without_as() {
    let source = r#"
use fopen in libc of c
fn main() => int:
    let f: FILE = fopen("a", "r")
    return 0
"#;
    let ll = format!("fopen_file_{}.ll", unique_temp_id());
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(
        result.exit_code, 0,
        "fopen should return FILE without as:\n{}",
        result.stderr
    );
}

#[test]
fn fopen_to_object_without_as_fails() {
    let source = r#"
use fopen in libc of c
fn main() => int:
    let p: object = fopen("a", "r")
    return 0
"#;
    let result = compile_coffee(source, &["--emit-llvm", "-o", "/dev/null"]).unwrap();
    assert_ne!(
        result.exit_code, 0,
        "FILE must not implicitly coerce to object:\n{}",
        result.stderr
    );
}

fn write_cht_cfc(lib: &str, body: &str) -> String {
    let path = format!("lib{lib}.cfc");
    std::fs::write(&path, body).expect("write cfc");
    path
}

#[test]
fn c_class_point_copy_compiles_without_rm() {
    let id = unique_temp_id();
    let lib = format!("chtp{id}");
    let cfc = write_cht_cfc(
        &lib,
        r#"
c class Point sizeof=8 align=4:
    x: int(4)+
    y: int(4)+
c fn origin() => Point:
"#,
    );
    let source = format!(
        r#"
use origin in {lib} of c
fn main() => int:
    let a: Point = origin()
    let b: Point = a
    return b.x
"#
    );
    let ll = format!("cht_point_{id}.ll");
    let result = compile_coffee(&source, &["--emit-llvm", "-o", &ll]).unwrap();
    let ir = std::fs::read_to_string(&ll).unwrap_or_default();
    let _ = std::fs::remove_file(&ll);
    let _ = std::fs::remove_file(&cfc);
    assert_eq!(
        result.exit_code, 0,
        "c class copy should compile:\n{}",
        result.stderr
    );
    assert!(
        !ir.contains("call void @free") || !ir.contains("Point"),
        "c class copy must not free like a Coffee class:\n{ir}"
    );
}

#[test]
fn c_union_overlapping_borrows_error() {
    let id = unique_temp_id();
    let lib = format!("chtu{id}");
    let cfc = write_cht_cfc(
        &lib,
        r#"
c union Num sizeof=8 align=8:
    i: int
    f: float
c fn make_num() => Num:
"#,
    );
    let source = format!(
        r#"
use make_num in {lib} of c
fn main() => int:
    let u: Num = make_num()
    let a: &int = &u.i
    let b: &float = &u.f
    let keep: &int = a
    return 0
"#
    );
    let result = compile_coffee(&source, &["--emit-llvm", "-o", "/dev/null"]).unwrap();
    let _ = std::fs::remove_file(&cfc);
    assert_ne!(
        result.exit_code, 0,
        "overlapping union field borrows must fail:\n{}",
        result.stderr
    );
}

#[test]
fn disagreeing_c_class_defs_across_cfc_is_error() {
    let id = unique_temp_id();
    let lib_a = format!("cfla{id}");
    let lib_b = format!("cflb{id}");
    let path_a = write_cht_cfc(
        &lib_a,
        r#"
c class Point sizeof=8 align=4:
    x: int(4)+
    y: int(4)+
c fn origin_a() => Point:
"#,
    );
    let path_b = write_cht_cfc(
        &lib_b,
        r#"
c class Point sizeof=16 align=8:
    x: int(8)+
    y: int(8)+
c fn origin_b() => Point:
"#,
    );
    let source = format!(
        r#"
use origin_a in {lib_a} of c
use origin_b in {lib_b} of c
fn main() => int:
    return 0
"#
    );
    let result = compile_coffee(&source, &["--emit-llvm", "-o", "/dev/null"]).unwrap();
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
    assert_ne!(
        result.exit_code, 0,
        "disagreeing c class Point must error:\n{}",
        result.stderr
    );
    let hay = format!("{}{}", result.stdout, result.stderr);
    assert!(
        hay.contains("Point")
            && (hay.contains("mismatch")
                || hay.contains("disagree")
                || hay.contains("already")
                || hay.contains("different")
                || hay.contains("conflict")),
        "stderr should name Point and the conflict:\n{hay}"
    );
}
