# Runtime

There is **no** Coffee runtime crate (`src/runtime/` was never wired and is gone). Abort/`raise` is codegen + libc, not a `coffee_panic` library.

What actually runs:

- **`raise`:** LLVM calls libc `fprintf` / `exit`, or the function’s `#listener`. See `src/backend/mir_raise.rs`.
- **`declare_runtime_functions`:** LLVM declarations for C symbols the backend may call (`puts`, `exit`, `malloc`, `free`, …). Direct libc names in *user* source still need `use … in libc of c`; typical programs use **std** instead (`use print in std`). The C boundary lives in `library/std/src/sys.cf`.
- **Official std:** Coffee sources under `library/std`, embedded in the CLI (`coffee std install`). Not a Rust module. `IntBuf` is in `mem`. Compile prepends std unless `[dependencies.packages.std]` is set.

**`buf`:** owned malloc pointer; drop/`rm` `free`s it. `object` pointees are not `free`d.

**Last-use move:** `src/types/last_use.rs` (checker), then existing `mv` codegen.

**Composite drop:** `[T; N]` resource arrays and tuple resource elements are dropped with the container. Class `[T]` slice fields drop each element using the fat-pointer length and do not `free` the buffer.

**Deep `clone`:** nested `str` / class / resource array / tuple fields are cloned; `object` / refs / slice fields stay shallow. `clone buf` is a type error.
