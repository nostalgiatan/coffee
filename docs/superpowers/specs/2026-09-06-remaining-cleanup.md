# Remaining cleanup — 2026-09-06

Independent of in-flight MIR-required / listener agents.

1. **For-body types:** `lower_for` must not set `syntactic_types = true`. Bind the loop variable in `extra_types` (`int` for range, element type for collection) for the body. Nested exprs use the infer callback.

2. **Struct literals + inheritance:** `check_struct_literal` required names = own fields **plus ancestor fields** (walk `Class.parent`). `class Child of Base` / `of Error` literals must include parent fields. Update tests that omit `code`/`note`/`e` on exception subclasses.

3. **Docs:** ~~`docs/superpowers/specs/2026-09-05-compiler-hardening-design.md` still says codegen is parser AST — correct to statement MIR.~~ **Done:** omitted `hir_fns` is a compile error, not AST fallback.

Non-goals: `ClassName__drop` no-op (heap `free` is in `memory_ops` for pointer locals); `hir_expr_to_ast` test helper.
