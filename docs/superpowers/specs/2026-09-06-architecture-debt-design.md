# Architecture debt (2026-09-06)

Type-placeholder honesty is a separate track. This spec only pays **structure** debt.

## Locked this round (independent files)

1. **`MirStmt::Nested` does not clone a full `Statement`.**  
   Store `NestedKind` + source name (and LLVM/`hir_fns` key when renamed). Codegen looks up the already-declared function/class/enum. Nested `fn` bodies already have MIR in `hir_fns`; do not compile nested bodies from a second AST copy.

2. **`hir_expr_to_ast` is test-only.**  
   Production already must not call it. Gate the helper with `#[cfg(test)]` (keep `hir/mod.rs` unit tests).

3. **Parser records statement byte ranges on `Program`.**  
   Do **not** wrap `Expression` as `{span, kind}` in this round (touches every match). Add `Program.stmt_spans: Vec<Span>` (same length as `statements`), filled from the source slice used by `parse_program` (byte offset of the statement’s first line through the last consumed line). Dummy `Span::new(0,0)` is only allowed when the statement was synthesized in tests.

## Explicit non-goals (later)

- Interprocedural / `'a` borrow (needs `types/checker/call.rs`; type-honesty agents own that file now).
- SSA MIR or deleting `compile_expr` (AST expressions still compile through `compile_expr`; MIR expressions through `compile_hir_expr_typed`).
- Changing `Type::from_str` unknown ident → `NamedType` (class names).
