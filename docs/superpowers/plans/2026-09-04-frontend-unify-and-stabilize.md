# Unify Frontend and Stabilize Compiler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make parse → semantic → type-check a single live pipeline used by both single-file and project mode, then fix known compiler bugs and hygiene issues — without changing Coffee syntax or the memory model.

**Architecture:** Live compilation already goes through `CompilerFrontend` (`src/compiler.rs`) for both `coffee file.cf` (`src/main.rs`) and `coffee.toml` projects (`src/compiler/builder.rs`). `src/compiler/pipeline.rs` is a second, unused copy of that flow (not even `mod pipeline` in `compiler.rs`) and does not type-check against current `CompilationResult`. Fold the live `compile()` body into `CompilationPipeline`, make `CompilerFrontend` a thin owner of `Session` + pipeline, delete the dead duplicate, then fix match LLVM terminators, debug noise, and the unused `--enable-safety` flag.

**Tech Stack:** Rust 2024, existing integration tests under `tests/` (spawn `target/debug/coffee --test-mode`), LLVM 21 via inkwell, clang linker.

## Global Constraints

- Do **not** change Coffee syntax (`mv`/`clone`/`copy`/`rm`/`clean`, `let`, assignment) in this plan.
- Do **not** remove or retype `malloc`/`free`, change `.cfc` rules, or implement the new memory contract (that is Plan B, after this lands).
- Keep the `CompilationResult` handoff: `program`, `c_imports`, `cfc_symbols` must still reach `backend::codegen::CodeGenerator::compile_program`.
- Tests must pass `--test-mode` (already in `tests/common/mod.rs`).
- Integer `int(N)+`/`int(N)-` remains N **bytes**, not bits.
- Do not commit unless the user explicitly asks during execution (plan still lists commit steps for when they do).
- Do not split `semantic/analyzer.rs` (~5k lines) in this plan.
- Termux/Android: `clean_target_triple` behavior stays.

## Out of scope (Plan B — do not implement here)

Locked language decisions for the **next** plan, recorded so they are not lost:

- Resource assignment `b = a` is a **compile error**; must `mv` or `clone` (choice A).
- Values (`int`/`float`/`bool`) auto-end at scope; no mandatory `rm`.
- User-facing `malloc`/`free` removed; compiler allocates; C pointers are not `int`.
- Delete `copy` and `clean out`; keep `mv`, `clone` (prefer expression `let b = clone a`), `rm` only for early drop.

## File map

| File | Role after this plan |
|------|----------------------|
| `src/compiler/pipeline.rs` | **The** staged compile: parse, import, declare, analyze, type-check, `CompilationResult`. |
| `src/compiler.rs` | `CompilationResult`, `CompilerConfig`, `CompilerFrontend` as wrapper; `pub mod pipeline`. |
| `src/compiler/session.rs` | Shared analyzer / type checker / emitter / C tables (already exists). |
| `src/compiler/builder.rs` | Keep creating `CompilerFrontend` per module (thread isolation); no second compile algorithm. |
| `src/compiler/statistics.rs` | Delete if still unused after unify (duplicate of `CompilationStatistics` in `compiler.rs`). |
| `src/main.rs` | Still calls `CompilerFrontend::compile`; optionally pass `enable_safety` into codegen (Task 5). |
| `src/backend/codegen.rs` | `compile_match`: every block gets a terminator. |
| `src/debug_log.rs` | New: `coffee_debug!` gated on `COFFEE_DEBUG`. |
| `src/parser/expr.rs.backup` | Delete. |
| `tests/frontend_parity_tests.rs` | New: single-file vs project mode both compile the same program. |
| `tests/pattern_matching_tests.rs` | Existing failing match cases become the match-terminator regression suite. |
| `CLAUDE.md` | Correct the “two live frontends” description. |

---

### Task 1: Characterization tests (parity + current match failures)

**Files:**
- Create: `tests/frontend_parity_tests.rs`
- Modify: `tests/common/mod.rs` (add project-mode helper)
- Test: `tests/frontend_parity_tests.rs`, `tests/pattern_matching_tests.rs`

**Interfaces:**
- Consumes: `compile_coffee` in `tests/common/mod.rs`; compiler binary `target/debug/coffee`
- Produces: `compile_project_fixture(source: &str, extra_args: &[&str]) -> Result<TestResult, String>` that writes a temp `coffee.toml` + `src/main.cf` and runs the compiler in that directory

- [ ] **Step 1: Write the failing/characterization project helper and parity test**

Add to `tests/common/mod.rs`:

```rust
/// Compile `source` as a one-file Coffee *project* (`coffee.toml` + `src/main.cf`).
/// Runs the compiler with cwd = the temp project dir so project mode is selected.
pub fn compile_project_fixture(source: &str, extra_args: &[&str]) -> Result<TestResult, String> {
    use std::process::Command;

    let test_id = format!(
        "{:?}_{:?}",
        std::thread::current().id(),
        std::time::SystemTime::now()
    );
    let dir = std::env::temp_dir().join(format!("coffee_proj_{}", test_id.replace(['(', ')', ':', ' '], "_")));
    fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;

    let toml = r#"[package]
name = "parity_fixture"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#;
    fs::write(dir.join("coffee.toml"), toml).map_err(|e| e.to_string())?;
    fs::write(dir.join("src/main.cf"), source).map_err(|e| e.to_string())?;

    let compiler = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join("target/debug/coffee");

    let mut cmd = Command::new(&compiler);
    cmd.current_dir(&dir);
    cmd.arg("--test-mode");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.arg(dir.join("src/main.cf"));

    let output = cmd.output().map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&dir);

    Ok(TestResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}
```

Create `tests/frontend_parity_tests.rs`:

```rust
include!("common/mod.rs");

fn tiny_program() -> &'static str {
    r#"
fn main() => int:
    let x: int = 1
    rm x
    return 0
"#
}

#[test]
fn test_single_file_compiles_tiny_program() {
    assert_compiles(tiny_program()).unwrap();
}

#[test]
fn test_project_mode_compiles_tiny_program() {
    let r = compile_project_fixture(tiny_program(), &["--emit-llvm"]).unwrap();
    assert_eq!(r.exit_code, 0, "project compile failed: {}", r.stderr);
}

#[test]
fn test_single_and_project_emit_llvm_both_succeed() {
    let single = compile_coffee(tiny_program(), &["--emit-llvm"]).unwrap();
    let project = compile_project_fixture(tiny_program(), &["--emit-llvm"]).unwrap();
    assert_eq!(single.exit_code, 0, "single: {}", single.stderr);
    assert_eq!(project.exit_code, 0, "project: {}", project.stderr);
}
```

If `assert_compiles` is not public from the include, use the same helpers already used in other test files (`assert_compiles` lives in `tests/common/mod.rs` — confirm it is in scope after `include!`).

- [ ] **Step 2: Record current match failures (do not fix yet)**

Run:

```bash
cd /data/data/com.termux/files/home/coffee
cargo test --test pattern_matching_tests -- --nocapture
```

Expected: several tests fail with LLVM `[E800] Basic Block ... does not have terminator` (historically: `test_match_with_nested_expressions`, `test_match_with_guard`, `test_match_with_nested_class`, `test_match_with_class_field_patterns`, `test_match_complex_nested`, `test_match_enum_with_multiple_fields`, `test_match_enum_with_named_fields`, `test_match_enum_with_value`, `test_match_nested_enum`). Write the exact failing names into a comment at the top of `tests/pattern_matching_tests.rs` as `// REGRESSION baseline Task 1: ...` so Task 6 knows the list.

- [ ] **Step 3: Run parity tests**

```bash
cargo test --test frontend_parity_tests
```

Expected: PASS if project mode already works for this fixture. If project mode fails, keep the test failing — Task 2 must not change semantics except to share one pipeline; fix project discovery only if the helper’s `coffee.toml` is wrong (`ProjectConfig` fields: see `src/compiler/project.rs`).

- [ ] **Step 4: Commit (only if user asked)**

```bash
git add tests/common/mod.rs tests/frontend_parity_tests.rs tests/pattern_matching_tests.rs
git commit -m "$(cat <<'EOF'
test: add single-file vs project compile parity fixture

EOF
)"
```

---

### Task 2: One live compilation pipeline

**Files:**
- Modify: `src/compiler.rs` (add `pub mod pipeline`; shrink `CompilerFrontend::compile` to delegate)
- Modify: `src/compiler/pipeline.rs` (replace unused API with the **moved** live `compile` from `CompilerFrontend`)
- Modify: `src/compiler/session.rs` (Frontend should construct `Session` and pass it in)
- Modify: `src/compiler/builder.rs` (keep `CompilerFrontend::new()` per thread; comments only if needed)
- Delete usage of duplicate `CompilationStatistics` in `src/compiler/statistics.rs` — **delete the file** and do not `mod` it
- Modify: `CLAUDE.md` architecture paragraph (two frontends → one pipeline, two *callers*)

**Interfaces:**
- Consumes: existing `CompilerFrontend` fields (`analyzer`, `type_checker`, `emitter`, `config`, import cache, `c_imports`, `cfc_symbols`)
- Produces:

```rust
impl CompilationPipeline {
    pub fn new(session: Arc<Session>) -> Self { /* ... */ }
    pub fn compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult { /* live stages */ }
}

impl CompilerFrontend {
    pub fn compile(&mut self, source: &str, file_name: Option<&str>) -> CompilationResult {
        self.pipeline.compile(source, file_name)
    }
}
```

`CompilationResult` stays the struct in `src/compiler.rs` **without** the extra fields currently imagined in dead `pipeline.rs` (`imported_modules_data`, `imported_symbols`). Do not add those fields unless a caller needs them.

- [ ] **Step 1: Make `pipeline.rs` compile as a module with a stub**

In `src/compiler.rs` after the other `pub mod` lines:

```rust
pub mod pipeline;
```

Replace `src/compiler/pipeline.rs` contents with a module that:
1. `use super::{CompilationResult, CompilerConfig, CompilationStatistics, ...}` as needed
2. Does **not** reference fields that do not exist on `CompilationResult`
3. Initially **copy** `CompilerFrontend::compile` and its private helpers (`parse_source`, `analyze_statement`, `type_check_statement`, import processing, class/enum registration, function declare pass) into the pipeline, taking `Session` **or** the same `Arc<RwLock<_>>` fields via `Session`

`Session` already has `analyzer`, `type_checker`, `emitter`, `config`, `c_imports`, `cfc_symbols`. Extend `Session` if the live frontend has extra state the pipeline needs:

```rust
// src/compiler/session.rs — add only if compile() needs them
pub module_cache: Arc<RwLock<HashMap<String, ParsedModule>>>,
pub import_stack: Arc<RwLock<Vec<String>>>,
```

Move `ParsedModule` to `session.rs` or `pipeline.rs` so both can use it. `CompilerFrontend` should hold `session: Arc<Session>` and `pipeline: CompilationPipeline` instead of duplicating Arcs.

- [ ] **Step 2: Point `CompilerFrontend::new` at Session + Pipeline**

```rust
impl CompilerFrontend {
    pub fn new() -> Self {
        let session = Arc::new(Session::new());
        let pipeline = CompilationPipeline::new(session.clone());
        CompilerFrontend { session, pipeline }
    }

    pub fn compile(&mut self, source: &str, file_name: Option<&str>) -> CompilationResult {
        self.pipeline.compile(source, file_name)
    }
}
```

Re-export helpers that `builder.rs` still calls on `CompilerFrontend` (`count_main_functions`, `compile_module_to_object`) as inherent methods that use `self.pipeline` / `self.session`. Do **not** leave a second copy of the declare-functions / type-check loop in `compiler.rs`.

- [ ] **Step 3: Delete the old `compile` body from `compiler.rs` once tests pass**

Search `src/compiler.rs` for `eprintln!("DEBUG: compile_source` — those loops must exist in exactly one file (`pipeline.rs`).

- [ ] **Step 4: Delete unused `src/compiler/statistics.rs`**

Confirm nothing `mod statistics`. Then `rm src/compiler/statistics.rs`.

- [ ] **Step 5: Update `CLAUDE.md`**

Replace the “two parallel frontend implementations” paragraph with: single-file and project both call `CompilerFrontend`, which delegates to `CompilationPipeline`; project mode additionally scans `src/`, graphs deps, and rayon-compiles modules.

- [ ] **Step 6: Run tests**

```bash
cargo test --test frontend_parity_tests
cargo test --test basic_types_tests
cargo test --test imports_tests
```

Expected: same pass/fail as before this task except compile errors you introduced. `cargo check` must succeed.

- [ ] **Step 7: Commit (only if user asked)**

```bash
git add src/compiler.rs src/compiler/pipeline.rs src/compiler/session.rs src/compiler/builder.rs CLAUDE.md
git add -u src/compiler/statistics.rs
git commit -m "$(cat <<'EOF'
refactor: run single-file and project compiles through one pipeline

EOF
)"
```

---

### Task 3: Gate DEBUG stderr

**Files:**
- Create: `src/debug_log.rs`
- Modify: `src/main.rs` (`mod debug_log;`)
- Modify: every `eprintln!("DEBUG:` site under `src/` (parser, compiler, backend, semantic)

**Interfaces:**
- Produces:

```rust
// src/debug_log.rs
#[macro_export]
macro_rules! coffee_debug {
    ($($arg:tt)*) => {{
        if std::env::var_os("COFFEE_DEBUG").is_some() {
            eprintln!($($arg)*);
        }
    }};
}
```

- [ ] **Step 1: Add the module and one replacement, prove silence**

Add `mod debug_log;` to `src/main.rs`. Replace a single known-noisy site (e.g. `pipeline.rs` declare-functions `eprintln!`) with `coffee_debug!("DEBUG: compile_source: First pass: declaring all functions");`.

Write in `tests/frontend_parity_tests.rs`:

```rust
#[test]
fn test_default_compile_stderr_has_no_debug_prefix() {
    let r = compile_coffee(tiny_program(), &["--emit-llvm"]).unwrap();
    assert_eq!(r.exit_code, 0, "{}", r.stderr);
    assert!(
        !r.stderr.lines().any(|l| l.starts_with("DEBUG:")),
        "stderr still has DEBUG lines:\n{}",
        r.stderr
    );
}
```

- [ ] **Step 2: Run test — expect FAIL until all DEBUG lines are gated**

```bash
cargo test --test frontend_parity_tests::test_default_compile_stderr_has_no_debug_prefix -- --nocapture
```

Expected: FAIL listing leftover `DEBUG:` lines.

- [ ] **Step 3: Replace all `eprintln!("DEBUG:` in `src/` with `coffee_debug!(`**

Do not gate non-DEBUG `eprintln!` (real diagnostics). Leave `src/parser/expr.rs.backup` for Task 4 delete.

- [ ] **Step 4: Re-run the stderr test**

Expected: PASS. Optional: `COFFEE_DEBUG=1 cargo test --test frontend_parity_tests::test_default_compile_stderr_has_no_debug_prefix` may fail the assertion — do **not** set that env in the test.

- [ ] **Step 5: Commit (only if user asked)**

```bash
git commit -m "$(cat <<'EOF'
chore: print compiler DEBUG traces only when COFFEE_DEBUG is set

EOF
)"
```

---

### Task 4: Remove junk files

**Files:**
- Delete: `src/parser/expr.rs.backup`

- [ ] **Step 1: Confirm it is not referenced**

```bash
rg -n "expr.rs.backup" /data/data/com.termux/files/home/coffee
```

Expected: no references (or only this plan).

- [ ] **Step 2: Delete the file**

```bash
rm /data/data/com.termux/files/home/coffee/src/parser/expr.rs.backup
```

- [ ] **Step 3: `cargo check`**

Expected: success.

- [ ] **Step 4: Commit (only if user asked)**

```bash
git add -u src/parser/expr.rs.backup
git commit -m "$(cat <<'EOF'
chore: remove leftover parser backup file

EOF
)"
```

---

### Task 5: Stop advertising a no-op `--enable-safety` (minimal wire)

**Files:**
- Modify: `src/backend/codegen.rs` (`enable_safety` is stored but never read)
- Modify: `src/backend/variables.rs` (`compile_array_index`)
- Modify: `src/backend/memory/safety.rs` (remove `#![allow(dead_code)]` once used)
- Modify: `src/main.rs` only if help text needs a one-line clarification
- Test: `tests/memory_tests.rs` or new cases in `tests/frontend_parity_tests.rs`

**Interfaces:**
- Consumes: `CodeGenerator.enable_safety: bool` (already set from CLI in `main.rs`)
- Produces: when `enable_safety` is true, `compile_array_index` calls `SafetyContext::check_bounds` before the GEP/load

- [ ] **Step 1: Write a test that OOB index with `--enable-safety` does not silently succeed**

Add a test that compiles a program indexing past a fixed array **without** the flag (current behavior: may compile) and **with** `--enable-safety`. Prefer a `--jit` or `--bin` run that should abort/nonzero if checks work.

If JIT/bin is too flaky on Termux, weaker test: `--emit-llvm` output with `--enable-safety` contains `bounds_panic` (the basic block name in `safety.rs`).

```rust
#[test]
fn test_enable_safety_emits_bounds_panic_block() {
    let source = r#"
fn main() => int:
    let arr: [int; 2] = [1, 2]
    let i: int = 1
    let x: int = arr[i]
    rm x
    rm i
    rm arr
    return 0
"#;
    let off = compile_coffee(source, &["--emit-llvm"]).unwrap();
    let on = compile_coffee(source, &["--enable-safety", "--emit-llvm"]).unwrap();
    assert_eq!(on.exit_code, 0, "{}", on.stderr);
    assert!(
        on.stdout.contains("bounds_panic") || on.stderr.contains("bounds_panic"),
        "expected bounds_panic in LLVM output:\nstdout={}\nstderr={}",
        on.stdout, on.stderr
    );
    let _ = off;
}
```

`--emit-llvm` may write a file instead of stdout — inspect `compile_coffee` / `main.rs` emit path. If IR goes to a `.ll` file next to the source, read that file in the test (extend `compile_coffee` to return the output path or search `*.ll` in cwd). Adjust the test to the **actual** emit behavior; do not invent a stdout format.

- [ ] **Step 2: Run test, expect FAIL**

Because `enable_safety` is never read.

- [ ] **Step 3: Wire the flag**

In `compile_array_index`, after index and length `IntValue`s exist:

```rust
if self.enable_safety {
    let safety = crate::backend::memory::safety::SafetyContext::new();
    safety.check_bounds(&self.backend.builder, index_int, length_int, "array index")?;
}
```

If `SafetyContext` needs a panic function, set it to an existing runtime `coffee_panic` declaration if the module already has one; otherwise `check_bounds` already emits `unreachable` in the panic block when `panic_func` is `None`. Keep that.

- [ ] **Step 4: Re-run the test**

Expected: PASS. Also `cargo test --test memory_tests` must not regress.

- [ ] **Step 5: Commit (only if user asked)**

```bash
git commit -m "$(cat <<'EOF'
feat: honor --enable-safety with array bounds checks in codegen

EOF
)"
```

---

### Task 6: Fix `compile_match` LLVM terminators

**Files:**
- Modify: `src/backend/codegen.rs` (`compile_match`, approx. line 1344)
- Test: `tests/pattern_matching_tests.rs` (the Task 1 baseline list)

**Interfaces:**
- Consumes: `MatchExpr` arms; builder insert point after `compile_expression_str` / nested control flow
- Produces: every basic block created for match (arm, next, merge, inner expression merges) has a terminator before `verify()`

- [ ] **Step 1: Add a helper on `CodeGenerator` (or next to `compile_match`)**

```rust
fn branch_to_if_unterminated(
    &self,
    dest: inkwell::basic_block::BasicBlock,
) -> Result<(), String> {
    let Some(block) = self.backend.builder.get_insert_block() else {
        return Ok(());
    };
    if block.get_terminator().is_none() {
        self.backend.builder
            .build_unconditional_branch(dest)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

- [ ] **Step 2: After compiling each arm’s body, before moving to `next_block`**

Call `self.branch_to_if_unterminated(merge_block)?`.

After setting up fall-through `next_block`, if that block is used as a continuation and may be empty at the end of the last arm, either: branch it to `merge_block`, or do not create a trailing empty `next_block` for the last arm (branch failed matches to `merge_block` / a dedicated `match_unmatched` that still terminates).

Empty `next_block` with no predecessors can still fail verify if it is appended and never terminated — **always** terminate unused next blocks (`unreachable` or `br merge`).

- [ ] **Step 3: Run the historical failing tests one by one**

```bash
cargo test --test pattern_matching_tests test_match_with_guard -- --nocapture
cargo test --test pattern_matching_tests test_match_with_nested_expressions -- --nocapture
cargo test --test pattern_matching_tests test_match_with_class_field_patterns -- --nocapture
cargo test --test pattern_matching_tests test_match_with_nested_class -- --nocapture
cargo test --test pattern_matching_tests test_match_complex_nested -- --nocapture
cargo test --test pattern_matching_tests test_match_enum_with_multiple_fields -- --nocapture
cargo test --test pattern_matching_tests test_match_enum_with_named_fields -- --nocapture
cargo test --test pattern_matching_tests test_match_enum_with_value -- --nocapture
cargo test --test pattern_matching_tests test_match_nested_enum -- --nocapture
```

Expected: no `[E800]` terminator errors. If a test still fails for **type/codegen incompleteness** (not terminator), leave a `#[ignore = "reason"]` with a one-line reason — do not expand enum match semantics in this plan unless the only bug was the terminator.

- [ ] **Step 4: Run full pattern matching file**

```bash
cargo test --test pattern_matching_tests
```

Expected: all non-ignored tests PASS.

- [ ] **Step 5: Commit (only if user asked)**

```bash
git commit -m "$(cat <<'EOF'
fix: terminate all LLVM basic blocks in match codegen

EOF
)"
```

---

### Task 7: Doc version and architecture truth

**Files:**
- Modify: `docs/zh/README.md`, `docs/en/README.md`, `IFLOW.md` (version `0.2.0` → `0.2.1` to match `Cargo.toml`)
- Modify: `CLAUDE.md` if Task 2 missed any dual-frontend sentence

- [ ] **Step 1: Replace stale version strings**

```bash
rg -n "0\\.2\\.0" /data/data/com.termux/files/home/coffee --glob '!target/**'
```

Update docs that claim the compiler version is 0.2.0.

- [ ] **Step 2: Commit (only if user asked)**

```bash
git commit -m "$(cat <<'EOF'
docs: align version and frontend architecture with the tree

EOF
)"
```

---

### Task 8: Full regression

**Files:** none new

- [ ] **Step 1: Build**

```bash
cargo build
```

Expected: success (LLVM 21.1 + clang already required).

- [ ] **Step 2: Test**

```bash
cargo test
```

Expected: all tests PASS except any `#[ignore]` from Task 6 with written reasons. No new failures in `basic_types_tests`, `control_flow_tests`, `functions_tests`, `imports_tests`, `memory_tests`, `class_tests`.

- [ ] **Step 3: Stop**

Do not start Plan B (syntax / memory keywords / malloc removal).

---

## Self-review

1. **Spec coverage:** Dual-frontend unify → Task 2. DEBUG noise → Task 3. backup file → Task 4. `--enable-safety` dead → Task 5. Match E800 → Task 6. Docs → Task 7. Characterization → Task 1. Memory/syntax explicitly out of scope.
2. **Placeholders:** Emit-LLVM test in Task 5 must follow real `main.rs` output (file vs stdout); implementer inspects `print_usage` / emit flags rather than inventing paths.
3. **Types:** `CompilationPipeline::compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult` is the single frontend entry; `CompilerFrontend::compile` delegates.
