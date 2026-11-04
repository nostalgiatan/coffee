// 聚合值（class 实例、带 payload 的枚举）的存储测试：运行程序并比较退出码。
// 设计见 docs/superpowers/specs/2026-11-04-aggregate-storage.md。
// 返回的聚合值必须在调用者里仍然有效；之前是指向已返回栈帧的 `alloca`，
// 同一程序连续运行会得到不同的退出码。

include!("common/mod.rs");

/// 连续运行同一可执行文件，确认结果稳定（悬空栈指针会随运行变化）。
fn assert_stable_exit_code(source: &str, expected: i32) {
    for _ in 0..4 {
        assert_exit_code(source, expected).unwrap();
    }
}

#[test]
fn test_return_class_literal_without_constructor() {
    let source = r#"
class P:
    x: int
    y: int

fn mk() => P:
    return P { x: 1, y: 2 }

fn sum(p: P) => int:
    return p.x + p.y

fn main() => int:
    let a: P = mk()
    return sum(a)
"#;
    assert_stable_exit_code(source, 3);
}

#[test]
fn test_return_class_through_two_calls() {
    let source = r#"
class P:
    x: int
    y: int

fn inner() => P:
    return P { x: 10, y: 20 }

fn outer() => P:
    return inner()

fn main() => int:
    let a: P = outer()
    return a.x + a.y
"#;
    assert_stable_exit_code(source, 30);
}

#[test]
fn test_return_class_with_class_field() {
    let source = r#"
class In:
    v: int

class Out:
    i: In
    n: int

fn mk() => Out:
    let i: In = In { v: 7 }
    return Out { i: i, n: 5 }

fn main() => int:
    let o: Out = mk()
    return o.i.v + o.n
"#;
    assert_stable_exit_code(source, 12);
}

#[test]
fn test_return_class_with_constructor_still_works() {
    let source = r#"
class P:
    x: int
    y: int

    fn new(x: int, y: int) => P:
        return P { x: x, y: y }

fn mk() => P:
    return P::new(1, 2)

fn main() => int:
    let a: P = mk()
    return a.x + a.y
"#;
    assert_stable_exit_code(source, 3);
}

const SHAPE: &str = r#"
enum Shape:
    Circle(r: int)
    Rect(w: int, h: int)
    Dot

fn area(s: Shape) => int:
    match s:
        Shape.Circle(r) => r * 3
        Shape.Rect(w, h) => w * h
        Shape.Dot => 0
"#;

#[test]
fn test_return_payload_enum_from_function() {
    let source = format!(
        r#"{SHAPE}
fn make_rect() => Shape:
    return Shape.Rect(3, 5)

fn make_dot() => Shape:
    return Shape.Dot

fn main() => int:
    return area(make_rect()) + area(make_dot())
"#
    );
    assert_stable_exit_code(&source, 15);
}

// ---- 配平：构造次数等于释放次数 ----
//
// 用 LD_PRELOAD 的小垫片统计 malloc/free。双重释放会被 glibc 直接中止，
// 所以这些测试同时覆盖"泄漏"和"重复释放"。

const COUNT_SHIM: &str = r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
static long m, f;
static void *(*real_malloc)(size_t);
static void (*real_free)(void *);
static void report(void) { fprintf(stderr, "COUNT malloc=%ld free=%ld\n", m, f); }
__attribute__((constructor)) static void init(void) {
    real_malloc = dlsym(RTLD_NEXT, "malloc");
    real_free = dlsym(RTLD_NEXT, "free");
    atexit(report);
}
void *malloc(size_t n) { if (!real_malloc) real_malloc = dlsym(RTLD_NEXT, "malloc"); m++; return real_malloc(n); }
void free(void *p) { if (!real_free) real_free = dlsym(RTLD_NEXT, "free"); if (p) f++; real_free(p); }
"#;

/// 编译并带计数垫片运行，返回 (malloc, free)。垫片或编译器不可用时返回 None。
fn count_allocations(source: &str) -> Option<(i64, i64)> {
    use std::process::Command;
    let id = unique_temp_id();
    let dir = std::env::temp_dir().join(format!("coffee_count_{id}"));
    fs::create_dir_all(&dir).ok()?;
    let shim_c = dir.join("count.c");
    let shim_so = dir.join("count.so");
    fs::write(&shim_c, COUNT_SHIM).ok()?;
    let built = Command::new("clang")
        .args(["-shared", "-fPIC", "-O1", "-o"])
        .arg(&shim_so)
        .arg(&shim_c)
        .arg("-ldl")
        .output()
        .ok()?;
    if !built.status.success() {
        eprintln!("skipping allocation count: cannot build the LD_PRELOAD shim");
        let _ = fs::remove_dir_all(&dir);
        return None;
    }
    // The compiler only accepts paths inside the working directory.
    let src = format!("test_count_{id}.cf");
    let exe = format!("test_count_{id}");
    fs::write(&src, source).ok()?;
    let current_dir = std::env::current_dir().ok()?;
    let compiled = Command::new(coffee_bin(&current_dir))
        .args(["--test-mode", "-b", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .ok()?;
    let _ = fs::remove_file(&src);
    if !compiled.status.success() {
        remove_exe_and_object(&exe);
        let _ = fs::remove_dir_all(&dir);
        panic!("compile failed:\n{}", String::from_utf8_lossy(&compiled.stderr));
    }
    let run = Command::new(format!("./{exe}")).env("LD_PRELOAD", &shim_so).output();
    remove_exe_and_object(&exe);
    let _ = fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&run.ok()?.stderr).to_string();
    let line = stderr.lines().find(|l| l.starts_with("COUNT "));
    let Some(line) = line else {
        panic!("program did not report counts (crashed or double free?):\n{stderr}");
    };
    let num = |key: &str| -> i64 {
        line.split(key).nth(1).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).unwrap_or(-1)
    };
    Some((num("malloc="), num("free=")))
}

fn assert_balanced(source: &str) {
    if let Some((m, f)) = count_allocations(source) {
        assert!(m > 0, "program allocated nothing; the test is not exercising the heap");
        assert_eq!(m, f, "malloc={m} free={f}: every allocation must be freed exactly once");
    }
}

#[test]
fn test_balanced_local_class() {
    assert_balanced("class P:\n    x: int\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    return 0\n");
}

#[test]
fn test_balanced_explicit_rm() {
    assert_balanced("class P:\n    x: int\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    rm a\n    return 0\n");
}

#[test]
fn test_balanced_class_with_constructor() {
    assert_balanced(
        "class P:\n    x: int\n\n    fn new(x: int) => P:\n        return P { x: x }\n\nfn main() => int:\n    let a: P = P::new(1)\n    return 0\n",
    );
}

#[test]
fn test_balanced_returned_class() {
    assert_balanced(
        "class P:\n    x: int\n\nfn mk() => P:\n    return P { x: 1 }\n\nfn main() => int:\n    let a: P = mk()\n    return a.x\n",
    );
}

#[test]
fn test_balanced_class_moved_into_field() {
    assert_balanced(
        "class In:\n    v: int\n\nclass Out:\n    i: In\n    n: int\n\nfn main() => int:\n    let i: In = In { v: 7 }\n    let o: Out = Out { i: i, n: 5 }\n    return 0\n",
    );
}

#[test]
fn test_balanced_clone() {
    assert_balanced(
        "class P:\n    x: int\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = clone a\n    return b.x\n",
    );
}

#[test]
fn test_balanced_class_built_in_a_loop() {
    assert_balanced(
        "class P:\n    x: int\n\nfn main() => int:\n    let i: int = 0\n    while i < 3:\n        let a: P = P { x: i }\n        i = i + 1\n    return 0\n",
    );
}

#[test]
fn test_balanced_payload_enum_locals() {
    assert_balanced(
        "enum S:\n    A(r: int)\n    B\n\nfn main() => int:\n    let s: S = S.A(1)\n    let t: S = S.B\n    return 0\n",
    );
}

#[test]
fn test_balanced_class_consumed_by_callee() {
    assert_balanced(
        "class P:\n    x: int\n\nfn eat(p: P) => int:\n    return 1\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    return eat(a)\n",
    );
}

#[test]
fn test_balanced_class_passed_through_callee() {
    assert_balanced(
        "class P:\n    x: int\n\nfn id(p: P) => P:\n    return p\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = id(a)\n    return b.x\n",
    );
}

#[test]
fn test_balanced_method_does_not_consume_receiver() {
    assert_balanced(
        "class P:\n    x: int\n\n    fn get(self) => int:\n        return self.x\n\nfn main() => int:\n    let a: P = P { x: 4 }\n    let n: int = a.get()\n    return n + a.get()\n",
    );
}

#[test]
fn test_balanced_method_consumes_own_class_parameter() {
    assert_balanced(
        "class P:\n    x: int\n\n    fn add(self, o: P) => int:\n        return self.x + o.x\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = P { x: 2 }\n    return a.add(b)\n",
    );
}

#[test]
fn test_balanced_two_class_parameters() {
    assert_balanced(
        "class P:\n    x: int\n\nfn two(a: P, b: P) => int:\n    return a.x + b.x\n\nfn main() => int:\n    let a: P = P { x: 1 }\n    let b: P = P { x: 2 }\n    return two(a, b)\n",
    );
}

#[test]
fn test_balanced_recursion_with_class_parameter() {
    assert_balanced(
        "class N:\n    v: int\n\nfn depth(n: N, k: int) => int:\n    if k == 0:\n        return n.v\n    let m: N = N { v: n.v + 1 }\n    return depth(m, k - 1)\n\nfn main() => int:\n    let s: N = N { v: 0 }\n    return depth(s, 5)\n",
    );
}

#[test]
fn test_balanced_payload_enum_parameter() {
    assert_balanced(
        "enum E:\n    A(r: int)\n    B\n\nfn val(e: E) => int:\n    match e:\n        E.A(r) => r\n        E.B => 0\n\nfn main() => int:\n    return val(E.A(5)) + val(E.B)\n",
    );
}
