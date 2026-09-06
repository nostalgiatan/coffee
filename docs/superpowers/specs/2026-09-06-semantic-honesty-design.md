# Semantic honesty — 2026-09-06

## f-string

Replace `{name}` with printf spec by placeholder type: `int`/`int(N)` → `%lld` (or width-correct), `float` → `%g`, `bool` → `%d`, `str` → `%s`. Then `snprintf`. Placeholders are identifiers only (already parsed). Empty placeholders: string constant.

## `clone` (stmt + unary `clone x`)

Resources (`class` heap ptr, `str`): new allocation + memcpy of object bytes (`str`: `strlen+1`). Value types: load/store. Not recursive through nested pointers. Unary `clone` uses the same path as `clone a b` into a temp/result. `copy` stays a type error.

## C signatures

Keep libc/libm **builtin** tables and `.cfc`. Remove name-heuristic fallback in `declare.rs` (math/stdio/`_void`/guess). Unknown C symbol without `.cfc` / builtin: hard error.

## `__drop`

Already: nested class fields reverse-order `__drop`; pointer `rm` calls `__drop` then `free`. Do not re-litigate here.

## Dead AST

`compile_match` / `compile_for` / fake `Vec` already removed.
