# Type-system + leftover AST (2026-09-04)

HEAD at dispatch: `25e777f`. Type checker is the priority; remaining string AST is second.

## Type checker (`src/types/checker.rs`)

1. `Statement::Main` → existing `check_main_entry`.
2. `Assignment` LHS: same split as analyzer (`name.split_once('.')` → member, else variable); check RHS compatible with target.
3. `CleanOut` in `check_memory_op`: `clean out` / `except` / explicit list; mark dropped like `rm`. Do not invent a `"clean_out"` string arm that hits `Unknown memory operation`.
4. Match bindings: type from scrutinee + `Pattern::EnumVariant` field types in the registry, not default `int`.
5. `Statement::Class`: type-check method bodies (`self` + params). Enum: resolve variant field type strings. Pipeline already registers types first.
6. `Expression::Call`: `check_function_call` takes `&[Expression]`; stop `to_string()` / `parse_expression` on args. `FunctionCall.args` may become `Vec<Expression>` or the shim can die.

## Leftover AST (disjoint files)

- `expressions.rs` / `expr/call.rs`: production must not use `compile_source_as_expr`; string `compile_function_call` is `#[cfg(test)]` only.
- Move `compile_function` from `codegen.rs` into existing `functions.rs`.

Do not change `mv`/`clone`/`rm`/`copy` language meaning. No function-body parse parallelism.
