# Type holes + C handles (2026-09-04)

Coffee ownership stays for **Coffee values**. C interop must not be narrower than C: `object` is an opaque/pointer-sized handle (`Type::Variadic`).

## C bridge
- `object` as C parameter: accept any Coffee value (int, string, named, ref, object).
- `object` as C result: `let p: object = malloc(...)` and `let p: int = malloc(...)` both OK (pointer-sized).
- `string` → `object` (C `char *`).
- Do **not** coerce `object` → `bool`.
- `rm`/`mv` on `object` must not fail type-check (C lifetime is `free`/`fclose`, not Coffee drop).
- libc `malloc`/`free`/`realloc` pointers: `object`, not `int(4)+` (that truncates on 64-bit).
- clang `.cfc` pointer types: map to `object`, not `int(4)+`.
- Variadic C calls: extra args unchecked (already true in analyzer).

## Checker holes (Coffee)
1. `check_raise`: propagate `Err`, do not `Ok(())`.
2. `check_expr_stmt`: do not ignore `FieldNotFound`. Distinguish zero-arg **method** (registry method + empty Member args) vs field.
3. Empty-args Member that is a method: type as function/method result only if it is a true call `()` — if parse uses empty args for both, treat registered method with empty args as a call with arity 0.
4. `for-in` non-array/slice: error, not silent `int`.
5. `TypeCast`: require source compatible with target or numeric/bool/string casts that `type_from_str` allows; not “any → resolved”.
6. `[]` empty array: keep `[int;0]` **or** error if you can do it without breaking tests — prefer keep if tests need it.
7. Pattern `unwrap_or(int)`: error when expected doesn’t match tuple/struct, don’t invent int.
8. Higher-order Call: if callee types as `Type::Function`, check args against that; else keep current error.
9. `types_compatible` for **Coffee** assignment: `expected.can_coerce_from(found)` only (not reverse), **except** C handle rules above.

Do not change `mv`/`rm` meaning for non-object Coffee values.
