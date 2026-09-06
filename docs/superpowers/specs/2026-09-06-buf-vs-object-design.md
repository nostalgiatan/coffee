# `buf` vs `object` (item 8)

**Checked:** `object` is `Type::Variadic`, C handle. Plan B: **never Coffee-drop the pointee**. `__drop` already no-ops `Variadic`. `rm` of a **local** only `free`s if `is_heap_allocated` (malloc path). A class field typed `object` is indistinguishable from `FILE*` — auto-`free` would be wrong.

## Rule

- **`object` unchanged.** No field auto-free. Tests that object fields do not `free` the pointee must stay.
- New builtin **resource type `buf`**: opaque pointer (LLVM ptr, like `str`), **drop = `free` the pointer**, no `strlen`. Meaning: Coffee-owned `malloc` buffer, not a C handle.
- `from_str("buf")` → dedicated handling. Prefer `Type::NamedType { name: "buf" }` **without** requiring a user `class buf`, and special-case drop/clone like `str` (clone = malloc+memcpy **not** available without a length: **clone of `buf` is a type error** “clone buf needs a size; use a slice”).
- Coerce: `object` → `buf` when the user **annotated** `let p: buf = malloc(...)` (existing C result already assigns to `object` / pointer-sized int). Do **not** coerce `buf` → `bool`. `fopen` stored as `object` stays unfreed; writing `let f: buf = fopen(...)` is allowed and **will free** (user opted in).
- `is_resource()` true for `buf`.

## Tests

`tests/memory_tests.rs` or `tests/buf_type_tests.rs`:

- class `Holder { p: buf }` `__drop` calls `free` on the field (like str Holder).
- class `Holder { p: object }` still does **not** free (existing test).
- `let p: buf = malloc(8)` + scope end / `rm p` IR has `free`.
- `clone p` with `p: buf` typechecks as error.

Do not edit slice mapping in `backend/types.rs` (other agent). Special-case only the name `buf` in drop/checker.

No commit, no version bump.
