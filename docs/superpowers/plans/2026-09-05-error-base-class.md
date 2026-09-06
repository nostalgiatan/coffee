# Error Base Class Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `raise` is legal only for builtin `Error` and classes that inherit it; exception classes keep extra fields and methods.

**Architecture:** Registry stores `Error` as `TypeDef::Class`. Typecheck uses `is_subtype_of` instead of `ends_with("Error")`. Language tests add `of Error`. Docs drop the suffix rule.

**Tech Stack:** Coffee compiler (Rust), existing `class C of P` parser, LLVM class layout (ancestors first).

## Global Constraints

- Do not git commit.
- Dual `foo.rs` + `foo/` modules forbidden.
- Tests: `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock cargo test --offline … -- --test-threads=1`
- Binary needs `--test-mode` (harness already does).
- Do not rewrite listener abort-vs-call IR except as needed for types.
- Spec: `docs/superpowers/specs/2026-09-05-error-base-class.md`

## File map

| Area | Files |
| --- | --- |
| Types | `src/types/registry.rs`, `src/types/checker/stmt.rs`, `src/types/checker/tests.rs`, `tests/type_check_tests.rs` |
| Language tests | `tests/error_handling_tests.rs`, `tests/functions_tests.rs` (only if it defines error classes), `tests/mir_cfg_tests.rs`, `tests/memory_tests.rs` |
| Docs | `SYNTAX.md`, `docs/en/parser.md`, `docs/zh/parser.md`, `docs/superpowers/specs/2026-09-05-error-listener.md`, `CLAUDE.md` if it mentions suffix |

---

### Task 1: Typecheck + registry

- [ ] `builtin_error_def` → `TypeDef::Class` with `parent: None` and the same three fields.
- [ ] `bind_class`: `Err` if `class.name == "Error"`.
- [ ] Public `TypeRegistry::is_exception_type(name) -> bool` via existing `is_subtype_of` (`Error` or descendant). Make `is_subtype_of` available to that helper.
- [ ] Replace `is_error_type_name` / `ends_with("Error")` in `check_raise` / `check_raise_expression`.
- [ ] `check_class`: error if name is `Error`.
- [ ] TDD: failing tests first for (1) `raise` of `class Point` fails (2) `class E of Error` + `raise` ok (3) extra field + method on subclass typechecks (4) user `class Error` errors (5) `class FooError` without `of Error` cannot raise.
- [ ] Run `cargo test --offline --lib types::` and `--test type_check_tests` under flock.

### Task 2: Integration tests

- [ ] Every class that is **raised** must be `class Name of Error:`.
- [ ] Do not redeclare `code`/`note`/`e`; keep extra fields (`message`, `path`, …).
- [ ] `test_error_recovery` does not raise — leave as a normal class or `of Error` without raise; either is fine.
- [ ] Nested `inner: InnerError`: only types that appear in `raise` need `of Error`. If `InnerError` is only a field, it may stay a normal class (rename not required).
- [ ] Call-form `raise Name(...)` can stay.
- [ ] Add one test: subclass with extra field + method, construct, read extra field (compile or jit).
- [ ] flock the listed test files.

### Task 3: Docs

- [ ] SYNTAX raise / Error: inherit `of Error`, extra fields allowed, no suffix rule. Example `class DivisionByZero of Error`.
- [ ] Parser docs en/zh if they mention `*Error` suffix or `#error_handler` dummy.
- [ ] Append a line to error-listener spec pointing at the base-class spec.
- [ ] CLAUDE.md borrow/memory bullets: raiseable = subtype of `Error`, not name suffix.
