# Composite Resource Drop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: executing-plans. Do not commit. Do not bump version.

**Goal:** Drop `[T; N]` / tuple resource fields and locals; leave `object` and slice-fields alone.

**Architecture:** `Type::from_str` drives drop instead of string prefixes. One helper for class `__drop` and `compile_remove`.

**Tech Stack:** existing LLVM inkwell backend.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-06-composite-drop-design.md`
- Tests: `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock cargo test --offline --test memory_tests -- --test-threads=1`
- `--test-mode` in harness.
- Dual `foo.rs` + `foo/` forbidden.
- Do not free `object` pointees. Do not implement generics/macros.

## File map

| Area | Files |
| --- | --- |
| Drop helper | `src/backend/classes.rs` and/or `src/backend/memory_ops/compile.rs` (prefer a function in `memory_ops` that both call) |
| Locals | `src/backend/memory_ops/compile.rs` `compile_remove` |
| Tests | `tests/memory_tests.rs` |
| Docs | `SYNTAX.md` memory section; short note in `docs/en/runtime.md` / zh if needed |

---

### Task 1: IR tests first

Add tests (same style as `test_nested_drop` / Holder str):

- Class with `[str; 2]` field: `Holder__drop` contains two `free` of loaded field GEPs (or a loop with two frees). Must not `free(self)`.
- Class with tuple `(Point, Point)` or two named classes: both `__drop` calls.
- Class with `p: object` (malloc stored in field): `__drop` must **not** call `free` on that field (may still exist empty drop). Use `--emit-llvm` and inspect.
- Local `let a: [str; 2] = ...` leaving scope: IR in `main` frees both (if current language can even construct that; if typecheck forbids, skip and test only class field).
- Existing nested Inner/Outer, Pair reverse order, Holder str: still pass.

### Task 2: Helper + class `__drop`

Replace the `ft.starts_with("int(")` / `is_str` / `Nested__drop` skip chain with walking `Type`.

For `Type::Array { elem, size }`: LLVM GEP index `i in 0..size`, drop elem.
For `Type::Tuple`: GEP each index.
For `Type::Slice(_)` as **field**: no-op (spec).
For `Type::Variadic`: no-op.
For `Type::Ref`: no-op.
For `Type::String`: existing free.
For `Type::NamedType`: existing nested `__drop`.

Bitfields: keep skip.

### Task 3: `compile_remove` composites

If local is tuple struct, drop resource fields. If name is in array_allocas with known N and resource elems, loop. Need CodeGenerator to pass array_allocas **or** store size/type on MemoryContext — prefer extending `MemoryContext` lifetime info with optional `array_len` + `elem_type` when binding arrays, so `compile_remove` does not need the whole CodeGenerator.

If wiring array_allocas through `compile_remove` is a signature mess, only implement class-field arrays in Task 2 and add a CodeGenerator method that loops array_allocas before `compile_remove` for those names. Spec requires locals too if feasible.

### Task 4: Docs + flock tests

SYNTAX: 定长资源数组随容器 drop；class 的 `[T]` 字段本轮不 drop 元素；`object` 仍不释放载荷。

`flock … cargo test --offline --test memory_tests -- --test-threads=1`

Return files changed and any skipped local-array case.
