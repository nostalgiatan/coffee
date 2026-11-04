// 循环里的算术不能让栈随迭代增长。
// `create_entry_block_alloca` 在入口块已有终结指令时曾退回到"在当前块建 alloca"，
// 于是循环体内每次 `+` 都多占一块栈，-O0 下约十万次迭代后段错误。

include!("common/mod.rs");

const LOOP_ARITH: &str = r#"
fn main() => int:
    let i: int = 0
    let acc: int = 0
    while i < 1000:
        acc = acc + i * 2 - 1
        i = i + 1
    if acc > 0:
        return 0
    else:
        return 1
"#;

/// 返回 IR 中不在 `entry:` 块里的 `alloca` 行。
fn allocas_outside_entry(ir: &str) -> Vec<String> {
    let mut label = String::new();
    let mut bad = Vec::new();
    for line in ir.lines() {
        if line.starts_with("define ") {
            label = "entry".to_string();
        } else if !line.starts_with(' ') && line.contains(':') && !line.starts_with(';') {
            label = line.split(':').next().unwrap_or("").trim().to_string();
        } else if line.contains("= alloca") && label != "entry" {
            bad.push(format!("{label}: {}", line.trim()));
        }
    }
    bad
}

#[test]
fn test_loop_arithmetic_allocas_stay_in_entry_block() {
    let name = format!("stack_growth_{}.ll", unique_temp_id());
    let r = compile_coffee(LOOP_ARITH, &["-O0", "--emit-llvm", "-o", &name]).unwrap();
    let ir = fs::read_to_string(&name);
    let _ = fs::remove_file(&name);
    assert_eq!(r.exit_code, 0, "compile failed: {}", r.stderr);
    let bad = allocas_outside_entry(&ir.expect("IR file"));
    assert!(bad.is_empty(), "alloca outside the entry block grows the stack per loop iteration:\n{}", bad.join("\n"));
}

#[test]
fn test_long_loop_does_not_overflow_the_stack() {
    let source = LOOP_ARITH.replace("1000", "5000000");
    assert_exit_code(&source, 0).unwrap();
}
