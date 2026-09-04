# 拆分 codegen / expressions（Task 7 完成后立刻执行）

> **For agentic workers:** 当前 Task 7–9 子代理结束后 **立即** 派本计划，不要先做 E700 / 内存 / 类型检查。拆完文件才允许三人并行修那些。

**Goal:** Move live methods out of `codegen.rs` and `expressions.rs` so later agents have disjoint write leases.

**Architecture:** Mechanical move of existing `impl CodeGenerator` methods; no language or IR behavior change. `cargo test --test control_flow_tests --test expressions_tests --test pattern_matching_tests --test functions_tests` must stay equivalent.

## Global Constraints

- Do not change Coffee syntax or memory keywords.
- Do not implement E700 enum construction, malloc removal, or type-checker expansion in this plan.
- Do not merge stringly `compile_expression_str` back in if Task 7 deleted it; call `compile_expr` from moved methods.
- One writer per lease (see `docs/superpowers/specs/2026-09-04-module-ownership.md`).
- File identity: `git -c user.name='Coffee Language Contributors' -c user.email='coffee@local'`; never `git config`.
- `flock .superpowers/sdd/cargo.lock cargo …`

## New priority (after Task 7–9 lands)

0. **This split** (was P3; now first)
1. Project `.o` uses `CompilationPipeline` (driver lease)
2. E700 enum construction (codegen-expr lease, after split)
3. Type checker covers more than `let`/`rm` (types lease)
4. Match `Pattern` enum (parser + codegen-match, after `match_gen.rs` exists)
5. Memory / C handles (c-builtins + types; separate spec)
6. DEBUG already gated in Task 8 — skip if done
7. Full `cargo test` warnings / docs

## Parallel wave (three agents, disjoint files)

### Task S1: Extract control flow from `codegen.rs`

**Lease:** codegen-coord + codegen-cf + codegen-match  
**Files:** `src/backend/codegen.rs`, `src/backend/control_flow.rs`, create `src/backend/match_gen.rs`, `src/backend/mod.rs`

Move as inherent methods on `CodeGenerator` (same as today) into:

- `control_flow.rs`: `compile_if`, `compile_while`, `compile_for` (keep `LoopContext` / `value_to_bool`)
- `match_gen.rs`: `compile_match` and match-only helpers (`branch_to_if_unterminated` if only used by match)
- Leave in `codegen.rs`: struct fields, `compile_program*`, `compile_function`, `compile_main_entry`, tiny helpers used by everyone

Register `pub mod match_gen` in `backend/mod.rs`. Do not edit `expressions.rs`.

**Test:** `cargo test --test control_flow_tests --test pattern_matching_tests`

### Task S2: Split `expressions.rs` into `src/backend/expr/`

**Lease:** codegen-expr only  
**Files:** `src/backend/expressions.rs` → replace with thin `compile_expr` match that calls submodules; create `src/backend/expr/mod.rs` and `literal.rs` / `binary.rs` / `call.rs` / `member.rs` / `index.rs` as needed. Update `backend/mod.rs` (`pub mod expr` or keep `pub mod expressions` as facade).

Do not edit `codegen.rs`. If `compile_expression_str` still exists, delete it here only if Task 7 left leftovers in this file.

**Test:** `cargo test --test expressions_tests --test functions_tests --test class_tests`

### Task S3: Project object compile through pipeline

**Lease:** driver  
**Files:** `src/compiler/pipeline.rs`, `src/compiler/builder.rs`, `src/compiler.rs` (wrapper only)

`compile_module_to_object` must run `CompilationPipeline::compile` (semantic + types) before codegen, not parse-only. Do not edit `src/backend/**`.

**Test:** `cargo test --test frontend_parity_tests --test imports_tests`

---

Do **not** start S1/S2 until `compile_expr` exists and Task 7’s uncommitted `expressions.rs` / `codegen.rs` is committed or reverted to a consistent tree.
