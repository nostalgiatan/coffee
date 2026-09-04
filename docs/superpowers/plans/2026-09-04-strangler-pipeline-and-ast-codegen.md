# Strangler: Single Pipeline + AST Codegen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One compilation pipeline for single-file and project mode, then compile from an expression-complete AST so production codegen never re-parses source strings.

**Architecture:** Keep the existing parser and inkwell backend. First unify drivers (`fn main` vs `Statement::Main`, `--emit-llvm` in project mode, live `CompilationPipeline`). Then replace remaining `String` expression holes with `parser::expr::Expression`. Then add `compile_expr(&Expression)` and delete `compile_expression_str` / `compile_body_line` from the production path. Do not change memory keywords or C `malloc` in this plan.

**Tech Stack:** Rust 2024, nom parser, inkwell/LLVM 21, integration tests spawning `target/debug/coffee --test-mode`.

**Spec:** `docs/superpowers/specs/2026-09-04-strangler-architecture.md`

## Global Constraints

- Do **not** change Coffee syntax (`mv`/`clone`/`copy`/`rm`/`clean`, `let`, assignment meaning).
- Do **not** remove or retype `malloc`/`free`, or implement the new memory contract.
- `CompilationResult` still hands `program`, `c_imports`, `cfc_symbols` to `CodeGenerator::compile_program`.
- Tests use `--test-mode`. Integer `int(N)+` is N **bytes**.
- Do not split `semantic/analyzer.rs` in this plan.
- `clean_target_triple` behavior stays.
- Do not invent a third SSA MIR; typed HIR is optional and last.
- E700 ignored enum-match tests in `tests/pattern_matching_tests.rs` stay ignored unless a task explicitly un-ignores them.
- Wrap `cargo` with `flock .superpowers/sdd/cargo.lock` if other compiles may run.
- Commit with `git -c user.name='Coffee Language Contributors' -c user.email='coffee@local'` if the user has asked to commit; otherwise leave commits for the user.

## File map

| File | Responsibility |
|------|----------------|
| `src/compiler/pipeline.rs` | Live staged compile (parse → import → declare → analyze → typecheck → `CompilationResult`). |
| `src/compiler.rs` | `CompilationResult`, `CompilerConfig`; `CompilerFrontend` wraps `Session` + pipeline; `count_entry_points` (not only `Statement::Main`). |
| `src/compiler/session.rs` | Shared analyzer / checker / emitter / C tables. |
| `src/compiler/builder.rs` | Project scan/link; same frontend; honor emit flags from driver. |
| `src/main.rs` | Pass emit/opt/safety into project compile, same as single-file. |
| `src/parser/expr.rs` | `Expression` tree (already exists). |
| `src/parser/if.rs`, `while.rs`, `for.rs`, `match.rs` | Conditions / match value+arms become `Expression` (match result may be `Vec<Statement>` or `Expression`). |
| `src/parser/mod.rs` | `Assignment(String, Expression)` or `(Expression, Expression)`. |
| `src/parser/function.rs` | `FunctionBody::Expression(Expression)`. |
| `src/backend/expressions.rs` | New `compile_expr(&Expression)`; string parser dies. |
| `src/backend/statements.rs` | No `to_string()` then compile; no `compile_body_line` on the main path. |
| `src/backend/codegen.rs` | if/while/for/match/function call `compile_expr`. |
| `src/debug_log.rs` | `coffee_debug!` after string path is gone (optional; do not treat gating DEBUG as the IR fix). |
| `tests/frontend_parity_tests.rs` | Parity + emit-llvm both modes. |

**Supersedes leftover tasks** from `docs/superpowers/plans/2026-09-04-frontend-unify-and-stabilize.md` Tasks 2, 3, 8 (pipeline, DEBUG gate, full regression). Tasks 1, 4, 5, 6, 7 of that plan are already done.

---

### Task 1: Entry points — `fn main` counts in project mode

**Files:**
- Modify: `src/compiler.rs` (`count_main_functions` ~1564)
- Modify: `src/compiler/builder.rs` (any `count_main_functions` / `no main() statement` checks)
- Modify: `src/compiler/entry_point.rs` if it duplicates the count
- Test: `tests/frontend_parity_tests.rs`

**Interfaces:**
- Consumes: `parser::Statement::Function`, `parser::Statement::Main`
- Produces:

```rust
pub fn count_entry_points(program: &parser::Program) -> usize {
    program.statements.iter().filter(|stmt| match stmt {
        parser::Statement::Main(_) => true,
        parser::Statement::Function(f) => f.name == "main",
        _ => false,
    }).count()
}
```

If both `Statement::Main` and `fn main` exist in one file, that count is ≥ 2 and must remain an error (same diagnostic family as multiple mains).

- [ ] **Step 1: Confirm the parity tests still fail for project mode**

Run:

```bash
cd /data/data/com.termux/files/home/coffee
flock .superpowers/sdd/cargo.lock cargo test --test frontend_parity_tests
```

Expected: `test_single_file_compiles_tiny_program` PASS; project tests FAIL with `no main() statement found` (characterization).

- [ ] **Step 2: Replace `count_main_functions` with `count_entry_points` as above**

Keep a deprecated wrapper `count_main_functions` that calls `count_entry_points` only if many call sites; prefer updating all call sites in the same task.

Read `builder.rs` / `compile_module_to_object` — project validation must use the new count on the **parsed AST**, not a second string scan.

- [ ] **Step 3: Re-run parity tests**

Expected: all three tests in `frontend_parity_tests.rs` reach compiler success **or** fail only on `--emit-llvm` no-op (Task 2). If `tiny_program` still fails after entry count, stop and report: do not paper over with a `main(main())` rewrite of tests.

- [ ] **Step 4: `cargo test --test basic_types_tests --test functions_tests`**

Expected: PASS (they use `fn main`).

- [ ] **Step 5: Commit (if user asked)**

```bash
git add src/compiler.rs src/compiler/builder.rs src/compiler/entry_point.rs
git commit -m "$(cat <<'EOF'
fix: treat fn main as a project entry point

EOF
)"
```

---

### Task 2: Project mode honors `--emit-llvm` (and siblings)

**Files:**
- Modify: `src/main.rs` (`compile_project` ~259 currently ignores emit flags)
- Modify: `src/compiler/builder.rs` / `linker.rs` as needed to stop before link when emitting IR
- Test: `tests/frontend_parity_tests.rs`

**Interfaces:**
- Consumes: CLI flags already parsed in `main` (`emit_llvm`, `emit_bc`, `emit_asm`, `-o`)
- Produces: project compile writes a `.ll` (or `-o` path) like single-file `llvm_backend.write_ir`

Today single-file: default `-o` is `{stem}.ll` when `--emit-llvm` (`src/main.rs` ~698–760). Project path: `builder.compile()` → object/exe only.

- [ ] **Step 1: Extend `compile_project` signature (or a options struct) with emit mode**

```rust
enum EmitKind {
    Object,
    LlvmIr,
    Bitcode,
    Assembly,
    Binary,
}

fn compile_project(..., emit: EmitKind, output_file: Option<&str>) -> Result<(), String>
```

Do **not** link with clang when `EmitKind::LlvmIr`. After codegen+verify, call `write_ir` on the entry module (if project codegen is per-module, emit the **entry** unit’s IR or a documented concatenated policy — pick **entry module `.ll` only** for v1, and assert that in the test).

- [ ] **Step 2: Change parity test `test_single_and_project_emit_llvm_both_succeed` to check a `.ll` file exists and is non-empty**

Mirror `tests/memory_tests.rs` (`-o file.ll` then read file) if that pattern already works.

- [ ] **Step 3: Run**

```bash
flock .superpowers/sdd/cargo.lock cargo test --test frontend_parity_tests
```

Expected: all PASS.

- [ ] **Step 4: Commit (if user asked)**

```bash
git commit -m "$(cat <<'EOF'
feat: honor --emit-llvm in coffee.toml project builds

EOF
)"
```

---

### Task 3: Live `CompilationPipeline` (no second algorithm)

**Files:**
- Modify: `src/compiler.rs` (`pub mod pipeline`; `CompilerFrontend::compile` delegates)
- Modify: `src/compiler/pipeline.rs` (replace dead extra `CompilationResult` fields with a **move** of the live `CompilerFrontend::compile` body)
- Modify: `src/compiler/session.rs` if frontend state must live on `Session`
- Delete: `src/compiler/statistics.rs` if still unused (duplicate of `CompilationStatistics` in `compiler.rs`)
- Modify: `CLAUDE.md` — one pipeline, two drivers

**Interfaces:**
- Produces:

```rust
impl CompilationPipeline {
    pub fn new(session: Arc<Session>) -> Self { ... }
    pub fn compile(&self, source: &str, file_name: Option<&str>) -> CompilationResult { ... }
}

impl CompilerFrontend {
    pub fn compile(&mut self, source: &str, file_name: Option<&str>) -> CompilationResult {
        self.pipeline.compile(source, file_name)
    }
}
```

`CompilationResult` stays as in `compiler.rs` (no `imported_modules_data` unless a caller needs it — none do today).

- [ ] **Step 1: `pub mod pipeline` and make `pipeline.rs` compile**

Copy the **live** `compile` from `CompilerFrontend` (search `DEBUG: compile_source: First pass`). Do not resurrect the unused pipeline’s extra struct fields.

- [ ] **Step 2: Frontend holds `Arc<Session>` + `CompilationPipeline`; delete duplicated loops from `compiler.rs`**

`compile_module_to_object` and `count_entry_points` stay callable from `builder.rs`.

- [ ] **Step 3: Tests**

```bash
flock .superpowers/sdd/cargo.lock cargo test --test frontend_parity_tests --test imports_tests --test functions_tests
```

Expected: PASS. Behavior must match pre-move (including DEBUG noise until a later optional gate).

- [ ] **Step 4: Commit (if user asked)**

```bash
git commit -m "$(cat <<'EOF'
refactor: run all compiles through CompilationPipeline

EOF
)"
```

---

### Task 4: Parse `if` / `while` conditions as `Expression`

**Files:**
- Modify: `src/parser/if.rs` (`IfExpr.condition`, `ElifBranch.condition`: `String` → `Expression`)
- Modify: `src/parser/while.rs` (`WhileLoop.condition` → `Expression`)
- Modify: `src/parser/for.rs` if range bounds are strings
- Modify: every match on `.condition` in semantic, types, backend
- Test: `tests/control_flow_tests.rs` (existing)

**Interfaces:**
- Consumes: `parser::expr::parse_expression`
- Produces: typed fields `condition: Expression`

After colon-split, parse the condition slice with `parse_expression`, not `to_string()` keep.

- [ ] **Step 1: Add a parser unit test in `src/parser/if.rs` `#[cfg(test)]`**

```rust
#[test]
fn if_condition_is_binary_expr() {
    let src = "if x > 0:\n    return 1\n";
    let (_, ife) = parse_if(src).expect("parse");
    match ife.condition {
        crate::parser::expr::Expression::Binary { op, .. } => assert_eq!(op, ">"),
        other => panic!("{:?}", other),
    }
}
```

Adjust source to whatever `parse_if` actually accepts (indent). Run:

```bash
flock .superpowers/sdd/cargo.lock cargo test --lib
```

If the crate is binary-only, put the test in `tests/control_flow_tests.rs` or `cargo test --bin coffee` — `src/main.rs` has `mod parser`, so `cargo test` runs `parser` module tests. Prefer `#[cfg(test)]` under `if.rs`.

Expected: FAIL to compile until the field type changes, then FAIL/PASS as you update parse.

- [ ] **Step 2: Change structs and parse to `Expression`; update compiler/backend to `compile_expr` OR temporary `compile_expression_str(&format!("{}", condition))` ONLY if `compile_expr` does not exist yet**

If `compile_expr` is Task 6, use a **local helper** in this task:

```rust
fn expr_to_legacy_str(e: &Expression) -> String { e.to_string() }
```

and keep calling `compile_expression_str` once. That is allowed **only** as a bridge, listed in a `// BRIDGE:` comment. Task 6 must delete the bridge.

- [ ] **Step 3:** `cargo test --test control_flow_tests`

Expected: PASS.

- [ ] **Step 4: Commit (if user asked)**

```bash
git commit -m "$(cat <<'EOF'
refactor: parse if/while conditions as Expression trees

EOF
)"
```

---

### Task 5: Parse assignment, function-body expr, constructor/member args as `Expression`

**Files:**
- Modify: `src/parser/mod.rs` (`Statement::Assignment`)
- Modify: `src/parser/var.rs` if assignments parsed there
- Modify: `src/parser/function.rs` (`FunctionBody::Expression(Expression)`)
- Modify: `src/parser/expr.rs` (`ConstructorCall.args`, `Member.args`: `Vec<String>` → `Vec<Expression>`)
- Modify: type checker / semantic / backend call sites
- Test: `tests/expressions_tests.rs`, `tests/functions_tests.rs`

**Interfaces:**
- Produces: no `Vec<String>` for call arguments on those nodes

- [ ] **Step 1: Change types; fix compile errors**

Do not silently `join(",")` args back into one string except a `// BRIDGE:` helper.

- [ ] **Step 2: Tests**

```bash
flock .superpowers/sdd/cargo.lock cargo test --test expressions_tests --test functions_tests --test class_tests
```

Expected: PASS or only pre-existing ignores.

- [ ] **Step 3: Commit (if user asked)**

```bash
git commit -m "$(cat <<'EOF'
refactor: store assignments and call args as Expression

EOF
)"
```

---

### Task 6: Parse `match` value, patterns, and arm bodies as trees

**Files:**
- Modify: `src/parser/match.rs`
- Modify: `src/backend/codegen.rs` `compile_match`
- Test: `tests/pattern_matching_tests.rs`

**Interfaces:**
- Produces:

```rust
pub struct MatchExpr {
    pub value: Expression,
    pub arms: Vec<MatchArm>,
}
pub struct MatchArm {
    pub pattern: Pattern, // new enum; start with: Wildcard, Binding(String), Literal(Expression), Struct { .. } as needed
    pub guard: Option<Expression>,
    pub body: Vec<Statement>, // not a newline-joined String
}
```

YAGNI: if a full `Pattern` enum is too large for one task, use `pattern: Expression` plus `body: Vec<Statement>` first; parse `n if n > 5` as binding+guard in the match parser. Do **not** keep `result: String`.

- [ ] **Step 1: Parser tests for one literal match and one guarded match**

Put `#[cfg(test)]` in `match.rs` with sources copied from `test_simple_match` / `test_match_with_guard`.

- [ ] **Step 2: Stop `arm.result.split('\n')` + `compile_body_line` in `compile_match`**

Compile `arm.body` with `compile_statement` in a loop. Keep `branch_to_if_unterminated` / current-insert-block terminator logic from commit `e0f1160`.

- [ ] **Step 3:**

```bash
flock .superpowers/sdd/cargo.lock cargo test --test pattern_matching_tests
```

Expected: same 35 pass / 5 ignored (E700). No new E800.

- [ ] **Step 4: Commit (if user asked)**

```bash
git commit -m "$(cat <<'EOF'
refactor: match arms are statement lists, not raw strings

EOF
)"
```

---

### Task 7: `compile_expr(&Expression)` and delete string codegen from production

**Files:**
- Modify: `src/backend/expressions.rs` (add `compile_expr`; stop using `compile_expression_str` from `compile_statement` / `compile_if` / `compile_while` / `compile_function`)
- Modify: `src/backend/statements.rs` (Expr arm: `compile_expr(expr)`, not `to_string()`)
- Modify: `src/backend/variables.rs`, `codegen.rs`, `functions.rs` remaining call sites
- Delete or `#[cfg(test)]`: `compile_expression_str`, `compile_body_line`, `try_compile_binary_op` string scan

**Interfaces:**
- Produces:

```rust
impl CodeGenerator<'_, '_> {
    pub fn compile_expr(&mut self, expr: &Expression) -> Result<BasicValueEnum<'ctx>, String> { ... }
}
```

Match `Expression::{Literal, Variable, Binary, Unary, Call, ...}` and reuse existing LLVM helpers (`arithmetic`, `compile_function_call` after args are values).

- [ ] **Step 1: Implement `compile_expr` for variants that already have tests (literals, vars, binary, call, index, struct literal, fstring)**

Leave a `todo!` only for unused variants — forbidden. Every enum variant must compile or return a clear `Err`.

- [ ] **Step 2: Grep production path**

```bash
rg -n "compile_expression_str|compile_body_line" src/backend src/compiler src/main.rs
```

Expected: no matches outside `#[cfg(test)]` or deleted files.

- [ ] **Step 3:**

```bash
flock .superpowers/sdd/cargo.lock cargo test --test basic_types_tests --test expressions_tests --test control_flow_tests --test functions_tests --test memory_tests --test frontend_parity_tests
```

Expected: PASS.

- [ ] **Step 4: Commit (if user asked)**

```bash
git commit -m "$(cat <<'EOF'
refactor: generate LLVM from Expression AST only

EOF
)"
```

---

### Task 8: Gate leftover DEBUG; do not treat it as the IR fix

**Files:**
- Create: `src/debug_log.rs` (`coffee_debug!` when `COFFEE_DEBUG` is set)
- Modify: remaining `eprintln!("DEBUG:` under `src/`
- Modify: `src/main.rs` (`mod debug_log`)
- Test: `tests/frontend_parity_tests.rs` — stderr lines do not start with `DEBUG:`

**Interfaces:** same as the old unify plan Task 3.

- [ ] **Step 1: Macro + replace remaining DEBUG eprintlns**

- [ ] **Step 2:** `cargo test --test frontend_parity_tests`

Expected: PASS including no `DEBUG:` on stderr.

- [ ] **Step 3: Commit (if user asked)**

```bash
git commit -m "$(cat <<'EOF'
chore: print DEBUG traces only when COFFEE_DEBUG is set

EOF
)"
```

---

### Task 9: Full regression + CLAUDE.md

**Files:**
- Modify: `CLAUDE.md` (one pipeline; codegen from AST; string path gone)
- Test: entire `cargo test`

- [ ] **Step 1:**

```bash
flock .superpowers/sdd/cargo.lock cargo test
```

Expected: all non-ignored tests PASS. Record any new ignore with a one-line reason in the test file.

- [ ] **Step 2: Update `CLAUDE.md` architecture bullets** so they match the tree.

- [ ] **Step 3: Stop.** Do not start memory-keyword work. Next spec: memory contract (assignment error, no user malloc, auto-rm values).

---

## Out of scope (next plan)

- Resource `b = a` is a compile error; `mv` / `clone` required.
- Values auto-drop; `rm` early-only; delete `copy` / `clean out`.
- No user `malloc`/`free`; C pointers are not `int`.
- Un-ignore E700 enum constructors (`Option.Some`).
- SSA MIR / borrow checker.

## Self-review

1. Spec coverage: entry + emit + pipeline + Expression holes + compile_expr + DEBUG + regression. Memory excluded.
2. Bridge comments in Tasks 4–5 are explicit and must die in Task 7.
3. `count_entry_points` vs old `count_main_functions` naming is consistent across tasks.
