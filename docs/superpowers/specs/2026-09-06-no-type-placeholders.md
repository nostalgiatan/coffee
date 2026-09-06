# No placeholder types / no AST if-while — 2026-09-06

Production must not invent `int`/`void` when types fail.

## HirLower (`src/hir/lower/expr.rs`)

- `recover_nonvar_type`: always `Err(err)`. Never `Expression::infer_type()`.
- `syntactic_types`: unused (`= true` nowhere). Delete the flag and the `infer_type` branch.
- Variable: `extra_types` else `infer` Ok else **registry** type/enum/fn name else **Err**. Do not `NamedType { name }` just to keep going (except when registry confirms that name is a type/enum).
- Failed `Call`: if callee is `Variable` and `infer(callee)` is `Function`, use return type. Do **not** `Type::void()` because `extra_types` is non-empty.
- Tests in `src/hir/mod.rs` that used `infer_type` as `stub_infer` may keep that **in tests only**.

## Lets (`stmts.rs`)

`type_from_str(var_type)` failure is `Err`, not `Type::int()`.

## Match enum payloads (`match_cfg.rs`)

`resolve_type` failure must not become `int`. Skip that bind or `Err` out of lowering.

## AST if/while

Function bodies are MIR. `compile_statement` for `If`/`While` must **Err** (“must come from MIR”) like Match/For/Raise. Then `compile_if`/`compile_while` AST loops are dead: delete them or leave unreachable. `ast_fallback_expr_type` / `compile_lowered_expr` must not use `infer_type`; if nothing calls them after the delete, remove them.

Top-level program `compile_statement` still handles `Function`/`Class`/`Import`/`global let`.
