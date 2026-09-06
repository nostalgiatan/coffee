# Composite resource drop (no generics)

**Status:** implement now. Not macros, not `List<T>`, not “free every `object`”.

## Goal

Memory contract already says resources are cleaned at scope end and class `__drop` walks nested class/`str` fields. That walk is **string-prefix** (`"str"` / `Point__drop`) and **skips** `[T; N]`, tuples, and anything `Type::from_str` would understand but the drop loop does not.

Fill that hole so a **monomorphic container class** (e.g. `class IntPair: a: Point, b: Point` or `items: [str; 4]`) actually drops contents. Keep **`object` payload unfreed** (C handles / `FILE*`).

## Rules (lock)

1. **`object` fields and `object` locals:** still no `free` of the pointee. Same as Plan B.
2. **`&T` / `&mut T` fields:** do not drop the referent.
3. **`str` / `string`:** `free` the pointer (existing).
4. **Named class field:** call `Class__drop` if it exists (existing). If the field type is a class name but `__drop` is missing, that is still a compile error on remove of a class *value*; for a field, skip only if it is not a known class (same as today for unknown names).
5. **`[T; N]`:** if `T` is a resource, drop each of the `N` elements (GEP + same rules as a field of type `T`). Value-type elements: no-op.
6. **Tuple `(T, U, …)`:** drop each resource element by struct GEP index.
7. **`[T]` slice fields:** **this slice does not** element-drop or `free` the buffer. LLVM maps slices to a bare pointer with no length in the class layout. Document in SYNTAX; do not guess a length. Locals that live in `array_allocas` with a known length from a literal may drop `N` elements on `compile_remove` (same as `[T; N]`).
8. No user `fn drop(self)`. No new keywords.

## Implementation

- Parse class `field.field_type` with `Type::from_str` (already used elsewhere). Drive `__drop` from `Type`, not `starts_with("int(")`.
- Shared helper used by `classes.rs` `__drop` **and** `memory_ops/compile.rs` `compile_remove` for array/tuple *locals* that are LLVM structs/arrays (not only `StructType` class instances).
- `compile_remove` today: class struct → `Class__drop`; heap ptr → drop+free. Extend: tuple struct → per-field; fixed array in `array_allocas` → loop elements. Do not treat `object` pointers as `should_free` unless already `is_heap_allocated`.
- Tests in `tests/memory_tests.rs` (IR contains nested calls / `N`× `free` for `[str; 2]`). Existing nested class / Holder str tests must stay green.
- SYNTAX.md + `docs/{en,zh}` memory: arrays-of-resources drop; slices-as-fields do not; `object` still not.

## Out of scope

- Generics, stamp macros, `fn drop`.
- Recursive `clone` through nested pointers (still memcpy of object bytes).
- Changing C ABI / `.cfc`.
- Bumping crate version or git commit.
