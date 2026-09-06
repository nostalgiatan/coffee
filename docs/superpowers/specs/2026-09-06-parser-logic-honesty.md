# Parser / drop leftovers — 2026-09-06

Not the six in-flight tracks (`~`, clone, for-in, anon fn, f-string, C signatures).

## Operator precedence

`parse_expression` must match `SYNTAX.md` (low to high): `||` then `&&` then `|` `^` `&` then `==` `!=` then comparisons then `<<` `>>` then `+` `-` then `* / %`. Split on the **rightmost** operator of the **lowest** precedence still present at paren/bracket depth 0 (so `a || b && c` is `a || (b && c)`, `1+2*3` is `1+(2*3)`). Do not scan a fixed list for the first hit of `&&` before `||`.

## Short-circuit `&&` / `||`

Both sides must **not** be compiled before the branch. In `compile_expr` / `compile_hir_expr` for `Binary &&`/`||`: compile left; `value_to_bool`; branch; compile right only on the taken path; phi or select the bool. Side effects on the right must not run when short-circuiting (e.g. `false && foo()` does not call `foo`). Keep bitwise `&` `|` eager.

## `str` fields in `__drop`

`ClassName__drop` currently skips `str`/`string`. Call `free` on loaded string pointers when `free` is declared (same as heap `rm`). Do not `free(self)`.

## `packed class` bitfields

`packed class` with `:N` fields must pack storage **without** `--enable-bitfields`. Non-packed classes keep today’s flag-off ABI (one LLVM member per field) unless `--enable-bitfields`. Update `test_bitfields_flag_off_*` only if the class in that test is not `packed` (it is a normal `class PackBits` — leave flag-off two `i64`s). Add/adjust a test for `packed class` packing without the flag.

## Fake generics

`List<int>` / `Foo<T>` must be a **type error** (“no generics”), not a `NamedType` that later looks like a missing class. Parser may still tokenize; `Type::from_str` / checker rejects `<...>`.

## Docs

`docs/superpowers/specs/2026-09-05-borrow-checker.md` “non-goals: field/index” is stale — `borrow.rs` already has `Place::Field` / `Index`. Align the spec with the code.
