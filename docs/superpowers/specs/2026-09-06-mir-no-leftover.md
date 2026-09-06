# MIR without leftover Match/ForIn — 2026-09-06

Architecture docs (`CLAUDE.md`, `IFLOW.md`, `docs/{en,zh}/compiler.md`) state this contract. Enum variants `MirStmt::Match` / `ForIn` may still exist in Rust until nothing constructs them (other agents).

## Locked

- `MirFn` that reaches codegen is always `complete: true` and **never** contains `MirStmt::Match` or `MirStmt::ForIn`.
- Codegen must not report `internal leftover: MirStmt::Match` / `ForIn`.
- Legal, typechecked `match` always lowers to `If` (existing `match_cfg`). If `match_is_simple` is false, `lower_function` returns `Err` (Error diagnostic; omit that fn from `hir_fns` → codegen `missing MIR for function`) — do **not** copy a leftover match into MIR.
- Legal, typechecked collection `for` always lowers to `ForRange`. If the collection type is not array/slice/tuple/literal, `lower_collection_for` / `lower_function` returns `Err` the same way — do **not** emit `HirStmt::ForIn` into MIR.
- Delete `MirStmt::Match` and `MirStmt::ForIn` once nothing constructs them.
- `bind_function_locals` + HIR infer: do not treat end-of-body `Dropped` as a reason to fail `p.x` that appears before `rm p`. After bind, revive locals to `Alive` (keep types) for lowering, **or** equivalent. Valid `let n = p.x` / `rm p` / `return n` must produce complete MIR for `main` (update `test_member_then_rm_*` accordingly: `hir_fns` contains `main`, no MIR-lower warning).
## Also in this batch

- Function bodies always compile from MIR. `compile_function` must not fall back to AST `compile_match` / `compile_for` / `compile_raise`. Missing `hir_fns` is a hard codegen error (not a warning + AST body). `compile_raise` may remain as a thin wrapper to `compile_mir_raise` only if still referenced; Match/For AST compilers must not run for Coffee functions that typechecked.
- `Backend::get_ir` must be used (e.g. `write_ir` / tests) or deleted with docs updated — no unused warning.
- Listen path: `raise` of an exception subclass must pass a pointer to the **subclass instance** (parent fields first, extra fields present), not a freshly packed 3-field `Error`. Call-form sugar may still fill inherited `code`/`note`/`e`. Listener param stays `err: Error` (prefix). Abort path may still stringify.


## Non-goals

- Unwind / try-catch
- New match pattern forms
