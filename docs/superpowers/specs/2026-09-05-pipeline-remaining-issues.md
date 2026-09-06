# Compiler pipeline + layout audit (2026-09-05)

## Layout (landed)

- LLVM members = flattened ancestors then self, declaration order. Bitfield annotations do not add a synthetic storage-unit LLVM member.
- All class ASTs are registered, then compiled parent-before-child (`src/backend/class_layout.rs`).
- `--show-memory` field layouts and local stack budget use LLVM `TargetData` store size / ABI align from the module data layout.
- Tuple LLVM structs are interned in `TypeMapper`.
- Generated C headers flatten inherited fields the same way.

## Pipeline (landed)

- Field `&p.x` compiles to a field GEP (object codegen, not `--emit-ast` only).
- Frontend MIR is passed into `compile_program_with_hir`. Complete MIR (`if`/`while`, range and collection `for` as `ForRange`, `match`→`If`, `raise`, `mv`/`rm`/`clone`) compiles in `mir_gen.rs` and never contains leftover `Match`/`ForIn`. Codegen does not report `internal leftover`. Nested `class`/`fn` are `Nested` (MIR stays complete). Typechecked `p.x` then `rm` lowers to complete MIR. Omitted `hir_fns` is a compile error (`missing MIR for function`); `compile_statement` Match/For/Raise already error if hit. See `2026-09-06-mir-no-leftover.md`.
- Type Display uses byte widths; string expression compilers were removed.
