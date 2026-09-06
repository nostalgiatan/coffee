# Deep clone (nested resources)

`clone` of a class is malloc+memcpy of the object **bytes**. Nested `str` / class fields alias. After `__drop` frees nested `str`, clone pairs double-free or share buffers.

## Rule

`clone` of `NamedType` (class): after memcpy (or instead of memcpy for pointer fields), for each field in drop order reversed (construct order):

- `str`/`string`: clone bytes (`strlen+1` malloc+memcpy) into the new object’s field (same as clone of str locals).
- nested class: call that class’s clone path / `Class__clone` if you emit one, or recursively apply this rule on the field pointer.
- `[T; N]` resource elems: clone each.
- tuple resource elems: clone each.
- `object`, refs, slices: **memcpy only** (same as today; do not invent slice length).

Value-type fields: memcpy is enough.

Unary `clone a` and `clone a b` share this path.

Reuse `drop.rs` field walking / `Type::from_str`. Do not `free` object pointees. Do not edit `src/parser` or `src/types/last_use.rs`.

## Tests

In `tests/memory_tests.rs` (clone section only): class with `s: str` field; clone; IR has extra malloc/memcpy for the str field, not only the object. Nested Inner class field: clone calls inner clone or second malloc.

`flock … cargo test --offline --test memory_tests -- --test-threads=1`

No commit, no version bump.
