# Slice fat pointer plan

> executing-plans. Spec: `docs/superpowers/specs/2026-09-06-slice-fat-pointer-design.md`

Touch: `src/backend/types.rs` Slice → `{ptr,i64}`, `src/types/checker/stmt.rs` `resolve_param_type` (allow Slice unless `c fn`), `src/backend/functions/compile.rs` + declare LLVM types, `src/backend/memory_ops/drop.rs` Slice arm, index/for-in length from fat, SYNTAX.md. Tests: `tests/slice_abi_tests.rs`.

Forbidden: `object`/`buf` special cases, `src/parser/**`, last_use.rs.
