# Slice fat pointer (items 7 and 9)

**Checked:** `[T]` is a resource, but LLVM maps `Slice` to a **bare pointer** (same as `[T; N]` decay). Locals keep length in `array_sizes` / `array_lengths` **beside** the value. Class fields and function parameters have **no** length, so `__drop` skips `Type::Slice`, and `resolve_param_type` rejects `[T]`. That rejection is ABI, not grammar.

## Rule

Coffee `[T]` is an **owned fat pointer**: LLVM `{ ptr, i64 len }` (ptr to first element, element count).

- **Parameters / returns** (Coffee `fn`, not `c fn`): allowed. Pass/return the struct. On entry, fill `array_allocas` + `array_sizes` from the struct fields so `xs[i]` and `for x in xs` work.
- **`c fn`:** still reject `[T]` (C has no this ABI). Keep the existing error text for C, or “slices cannot be C parameters”.
- **Class fields:** store the struct. `__drop` / `drop_coffee_place`: loop `0..len` and drop each `T` (same as `[T; N]`). **Do not `free` the buffer pointer** unless `is_heap_allocated` is already set for that local (stack `[1,2,3]` must not `free` the alloca). Field slices: drop elements only, **no** `free(ptr)` (unknown allocator). Same as not freeing `object`.
- **Copy:** still a resource (`mv` / last-use / `clone`). `clone` of a slice: v1 memcpy the fat struct only (shallow elements) **or** element-wise clone if deep-clone helper already exists for `T`; do not invent a new buffer unless `T` is `str` and you can malloc per element.
- Indexing: GEP field 0; length is field 1.

## Do not

- Change `[T; N]` (still ptr + compile-time N).
- Change `object`.
- Parser grammar (already parses `[T]`).

## Tests

New `tests/slice_abi_tests.rs`:

- `fn sum(xs: [int]) => int` + `for`/`xs[i]` compiles (`--emit-llvm` ok).
- `c fn` with `[int]` still errors.
- Class field `xs: [str]` `__drop` contains `free` per element (two strings) like `[str; 2]` test; must not `free(self)`.
- Existing `resolve_param_type_rejects_slice` **deleted or inverted** for Coffee params; add `resolve_param_type_accepts_slice`.

`flock … cargo test --offline --test slice_abi_tests --lib types::checker -- --test-threads=1`

No commit, no version bump.
