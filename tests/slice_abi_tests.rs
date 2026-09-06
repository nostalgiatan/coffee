// Slice fat-pointer ABI: `{ ptr, i64 len }` for Coffee `[T]`.

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
        .or_else(|| ir.find(&format!("define %{}", marker)))
        .or_else(|| {
            ir.find(&format!("define {{ ptr, i64 }} {}", marker))
        })
        .unwrap_or_else(|| panic!("missing define for {} in IR:\n{}", name, ir));
    let after = &ir[start..];
    after.split("\ndefine ").next().unwrap_or(after)
}

fn count_free_calls(ir_fn: &str) -> usize {
    ir_fn.matches("call void @free").count()
}

#[test]
fn test_slice_param_sum_compiles() {
    let source = r#"
fn sum(xs: [int]) => int:
    for x in xs:
        let n: int = x
    let i: int = 0
    return xs[i]

fn main() => int:
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_slice_sum_{}.ll", pid);
    let ir = emit_llvm_ir(source, &ll);
    assert!(
        ir.contains("define i64 @sum") || ir.contains("@sum("),
        "expected sum in IR:\n{ir}"
    );
    let sum = llvm_fn_body(&ir, "sum");
    assert!(
        sum.contains("{ ptr, i64 }") || ir.contains("{ ptr, i64 }"),
        "slice param should be a fat pointer {{ ptr, i64 }}:\n{ir}"
    );
}

#[test]
fn test_c_fn_slice_param_errors() {
    let source = r#"
c fn bad(xs: [int]) => int:

fn main() => int:
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_c_fn_slice_{}.ll", pid);
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let _ = std::fs::remove_file(&ll);
    assert_ne!(result.exit_code, 0, "c fn slice param must fail");
    let msg = format!("{}{}", result.stderr, result.stdout).to_lowercase();
    assert!(
        msg.contains("slice"),
        "error should mention slices, got:\n{}",
        result.stderr
    );
}

#[test]
fn test_slice_str_field_drop_frees_elements_not_buffer() {
    let source = r#"
class Holder:
    xs: [str]

    fn new(xs: [str]) => Holder:
        Holder { xs: xs }

fn main() => int:
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_slice_field_drop_{}.ll", pid);
    let ir = emit_llvm_ir(source, &ll);
    let holder_drop = llvm_fn_body(&ir, "Holder__drop");
    assert!(
        count_free_calls(holder_drop) >= 1,
        "Holder__drop should free [str] elements:\n{}",
        holder_drop
    );
    assert!(
        !holder_drop.contains("call void @free(ptr %0)")
            && !holder_drop.contains("call void @free(ptr noundef %0)"),
        "Holder__drop must not free(self):\n{}",
        holder_drop
    );
}
