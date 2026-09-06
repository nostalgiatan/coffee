# Coffee compiler hardening (2026-09-05)

## Goal

Split oversized compiler files without changing language meaning, then add a real HIR + statement MIR, finer intra-procedural places, and remove string-based expression codegen from the test/production dual path. Fix class LLVM layout so GEP indices match ABI.

## Non-goals

- Lifetime parameters `'a`, NLL, SSA register-allocation MIR
- `try` / `catch` exception IR
- Full nom rewrite of `parser/expr.rs`
- Changing `mv` / `clone` / `rm` / `int(N)+` byte meaning
- Second type-check loop outside `CompilationPipeline`
- Git commits (controller/user only)

## Layout contract (binding)

LLVM struct field **order is declaration order** (parent fields first, then the class’s own fields). Do **not** permute LLVM `set_body` to pack padding. `StructLayout::calculate` may still run for `--show-memory` reports. `get_field_index` / GEP must use the same order as the LLVM struct. `get_or_create_struct_type_from_class` sets the body once; `compile_class` must not overwrite with a reordered body.

## HIR / MIR (binding)

- After typecheck, the pipeline binds locals and lowers each function to `hir::MirFn` (statement-list CFG, **not** SSA). Drivers pass `hir_fns` into `compile_program_with_hir`; complete MIR compiles in `mir_gen.rs`. Expression values still go through `CodeGenerator::compile_expr`.
- **HIR:** typed copy of expressions (`HirExpr { ty, kind }`) lowered from parser `Expression` after a successful type check of that expr. Not a third string IR.
- **MIR:** per-function statement list + scope begin/end; not SSA. Borrow checker may later walk MIR.
- Typechecked function bodies compile from MIR only. Omitted `hir_fns` (`lower_function` Err) is a compile error (`missing MIR for function`), not an AST body. `compile_statement` Match/For/Raise already error if hit.

## Borrow (binding)

`Place` is `Var(String)` or `Field { base: Box<Place>, field: String }`. `&p.x` / `&mut p.x` are legal. Loans on a parent place conflict with loans on nested fields (conservative). Keep `acquire(&str)` as a wrapper for `Place::Var`.

## String codegen (binding)

`compile_expression_str*` must not live in `expressions.rs` next to `compile_expr`. Move to a `#[cfg(test)]` module. Prefer AST tests. Do not reintroduce production string compile.

## Modularization (binding)

Convert `foo.rs` → `foo/mod.rs` + focused submodules. Public types stay at the old path (`types::checker::TypeChecker`, etc.). No behavior change except layout + borrow + HIR additions above.

## Cargo

Use `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock` around `cargo check` / `cargo test`. Tests: `--offline -- --test-threads=1`. Add `--test-mode` only when invoking the `coffee` binary.
