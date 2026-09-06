# Stability leftovers + versioning (2026-09-06)

Honesty queue is closed. This file is the **stability** slice (not SSA).

## Versioning

Current: `0.3.9` (C library dep graph: owned headers, declared `c_libraries` auto-`.cfc`, installed transitives). Previous: `0.3.8` C header types. Each landed feature → **patch +1**. After **10** patches on a minor, bump minor (`0.3.10` is `0.4.0`, not `0.3.10`). Parent tags `Cargo.toml`; feature agents should not invent a version.

`--version` is `0.3.9+` plus 12 hex digits of the compiler source tree (`COFFEE_COMPILER_HASH` from `build.rs`). Incremental unit hashes include that fingerprint so installing a new `coffee` with the **same** Cargo patch still invalidates `.o` cache. Do not bump the patch only to force rebuilds.

## This batch (parallel, disjoint files)

1. Class LLVM layout: `try_map_type.ok()?` → `Err`/`None` with context, not silent drop.
2. `invalid_operation` dummy `Type::unit()` RHS → `NamedType { name: "_" }` (or omit unused right via existing API only if it already allows it).
3. `HirStmt::Nested` holds `NestedDecl`, not `Statement`.
4. Same-module call-site borrow: if callee **return type is `Ref`**, argument `&`/`&mut` loans do **not** end at the statement; they last until the result binding’s last use (or function end if discarded). No `'a` syntax. No SSA.

## Next small versions (not this dispatch)

- Expression `{ span, kind }` (parser-wide).
- Unify leftover AST `compile_expr` (not SSA). **Done:** trap + MIR-only values.
