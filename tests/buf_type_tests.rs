// Builtin `buf` vs unchanged `object` (owned malloc pointer vs C handle).

include!("common/mod.rs");

fn emit_llvm_ir(source: &str, ll_name: &str) -> String {
    let result = compile_coffee(source, &["--emit-llvm", "-o", ll_name]).unwrap();
    let ir = std::fs::read_to_string(ll_name).unwrap_or_default();
    let _ = std::fs::remove_file(ll_name);
    assert_eq!(result.exit_code, 0, "emit-llvm failed:\n{}", result.stderr);
    ir
}

fn llvm_fn_body<'a>(ir: &'a str, name: &str) -> &'a str {
    let marker = format!("@{}", name);
    let start = ir
        .find(&format!("define void {}", marker))
        .or_else(|| ir.find(&format!("define i64 {}", marker)))
        .unwrap_or_else(|| panic!("missing define for {} in IR:\n{}", name, ir));
    let after = &ir[start..];
    after.split("\ndefine ").next().unwrap_or(after)
}

#[test]
fn test_buf_field_drop_frees_pointer() {
    let source = r#"
class Holder:
    p: buf
    n: int

    fn new(p: buf) => Holder:
        Holder { p: p, n: 0 }

fn main() => int:
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_buf_field_drop_{}.ll", pid);
    let ir = emit_llvm_ir(source, &ll);
    let holder_drop = llvm_fn_body(&ir, "Holder__drop");
    assert!(
        holder_drop.contains("getelementptr"),
        "Holder__drop should GEP the buf field:\n{}",
        holder_drop
    );
    assert!(
        holder_drop.contains("load "),
        "Holder__drop should load the buf pointer:\n{}",
        holder_drop
    );
    assert!(
        holder_drop.contains("@free"),
        "Holder__drop should free the loaded buf pointer:\n{}",
        holder_drop
    );
    assert!(
        !holder_drop.contains("call void @free(ptr %0)")
            && !holder_drop.contains("call void @free(ptr noundef %0)"),
        "Holder__drop must not free(self):\n{}",
        holder_drop
    );
}

#[test]
fn test_object_field_drop_does_not_free_pointee() {
    let source = r#"
class Box:
    p: object
    n: int

    fn new(p: object) => Box:
        Box { p: p, n: 0 }

fn main() => int:
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_buf_object_drop_{}.ll", pid);
    let ir = emit_llvm_ir(source, &ll);
    let box_drop = llvm_fn_body(&ir, "Box__drop");
    assert!(
        !box_drop.contains("@free"),
        "Box__drop must not free(object) pointee:\n{}",
        box_drop
    );
}

#[test]
fn test_malloc_into_buf_rm_frees() {
    let source = r#"
use malloc in libc of c

fn main() => int:
    let p: buf = malloc(8)
    rm p
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_buf_malloc_rm_{}.ll", pid);
    let ir = emit_llvm_ir(source, &ll);
    let main_body = llvm_fn_body(&ir, "main");
    assert!(
        main_body.contains("@free"),
        "rm of buf from malloc should call free:\n{}",
        main_body
    );
}

#[test]
fn test_malloc_into_buf_scope_end_frees() {
    let source = r#"
use malloc in libc of c

fn main() => int:
    let p: buf = malloc(8)
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_buf_malloc_scope_{}.ll", pid);
    let ir = emit_llvm_ir(source, &ll);
    let main_body = llvm_fn_body(&ir, "main");
    assert!(
        main_body.contains("@free"),
        "scope-end drop of buf from malloc should call free:\n{}",
        main_body
    );
}

#[test]
fn test_clone_buf_is_type_error() {
    let source = r#"
use malloc in libc of c

fn main() => int:
    let p: buf = malloc(8)
    clone p q
    return 0
"#;
    assert_compile_error(source, "clone buf needs a size").unwrap();
}

#[test]
fn test_unary_clone_buf_is_type_error() {
    let source = r#"
use malloc in libc of c

fn main() => int:
    let p: buf = malloc(8)
    let q: buf = clone p
    return 0
"#;
    assert_compile_error(source, "clone buf needs a size").unwrap();
}
