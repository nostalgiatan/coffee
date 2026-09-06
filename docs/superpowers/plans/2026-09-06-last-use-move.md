# Last-use move plan

> executing-plans. Forbidden: `src/parser/**`, `src/backend/memory_ops/compile.rs` clone, `Type::from_str`. Spec: `docs/superpowers/specs/2026-09-06-last-use-move-design.md`

Create `src/types/last_use.rs`. Wire copy-error branches. Codegen via existing `compile_move`. Tests: `tests/last_use_move_tests.rs`. SYNTAX one paragraph under 内存管理.
