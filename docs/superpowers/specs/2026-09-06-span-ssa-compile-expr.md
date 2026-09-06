# Remaining IR / span / borrow (2026-09-06)

`HirStmt::Nested` is already `NestedDecl` (not `Statement`). Remaining AST grip: `CodeGenerator.nested_asts` still clones nested `fn`/`class` for codegen.

## Parallel tracks (disjoint files)

1. **Expression spans** — `Expression::Spanned { span, inner }` plus `Expression::kind()` / `span()`. `parse_expression` wraps nodes with byte range of the slice (caller passes base offset from `parse_program` when possible). Peel at checker/semantic/lower/`compile_expr` entry. Dummy `(0,0)` only for `Expression::literal` test helpers.

2. **Callee-body borrow** — `types/borrow.rs` + `checker/call.rs`. If the callee is a known same-module function whose return is `Ref`, alias the result to the argument that was actually returned (`return p` → loan of that arg). Do not invent `'a`.

3. **`compile_expr` off the function-body path** — `compile_statement` Assignment/Return/Expr/VariableDecl (inside a function) must error like `if` (MIR only). Keep `compile_expr` only for nested class/enum AST and tests. Add a source scan: `mir_gen.rs` / `compile_function` must not call `compile_expr`.

4. **SSA** — new `src/hir/ssa.rs`: convert `MirFn` to block-local SSA (`SsaFn`) with φ at joins for `if`. **Tests only this round**; do not switch LLVM `mir_gen` yet (that fights track 3).

5. **Nested codegen without Function AST** — `MirFn` carries param names/types + return type string; `compile_nested_function` looks up `hir_fns[key]` + LLVM function; stop requiring `nested_asts` for **Function**. Class/enum may still use `nested_asts`.
