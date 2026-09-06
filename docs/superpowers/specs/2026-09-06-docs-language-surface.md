# Language-surface docs (what to write, what not to invent)

Update **user-facing** docs to match the tree. Do **not** describe in-flight work as shipping. Compiler version is **0.3.6** (experimental, **not 1.0**).

## Shipping (must document)

1. **Packages (Zig-style, no registry)**  
   `[dependencies.packages.foo]` is `path = "..."` **XOR** `url` + `hash` (SHA-256 of unpacked tree). `coffee fetch` / `coffee fetch <url> --save [name]`. Cache `COFFEE_CACHE` / `~/.cache/coffee`. Project compile resolves and prepends import roots. **Not** semver strings `foo = "1.0"`.

2. **Official std** (`library/std`, embedded in the CLI). `coffee std install`. Compile prepends the std import root unless `[dependencies.packages.std]` is set. User code: `use print in std` or `use * in std`. `of c` belongs in `library/std/src/sys.cf`, not in typical user files. Growable ints: `IntBuf` in `mem` (`new` / `push` / `get` / `length`; move-only `buf`; OOB `get` → 0).

3. **Star / multi import**  
   `use fn_a, fn_b in module` and `use * in module` (functions only; classes still need `use mem`). C: `use a, b in libc of c`.

4. **Last-use implicit move** (`src/types/last_use.rs`). On a simple variable that is last use on remaining paths: `let b = a`, `b = a`, `foo(a)`, `return a` move like `mv`. Not `p.x` / `a[i]`; conservative in loops / `if`. Else still `mv` or `clone`.

5. **Slice fat pointer on Coffee `fn`**  
   `[T]` is `{ptr, i64 len}` for Coffee params/returns/locals. Class `[T]` fields drop **elements** using that length and do **not** `free` the buffer. `c fn` still **rejects** slices.

6. **Builtin type `buf`**  
   Coffee-owned malloc pointer; drop/`rm` `free`s it; `clone buf` is a type error; `object` is an unfreed C handle. Distinct from libc parameter names.

7. **Composite drop**  
   `[T; N]` of resources and tuple resource fields drop with the container. `object` fields do **not** free the pointee.

8. **Deep `clone`**  
   Nested `str` / class / resource array / tuple fields are cloned (`src/backend/memory_ops/clone.rs`). `object` / refs / slice fields stay shallow.

9. **Type parser**  
   Nested `List<List<int>>` is one type string. `class List<T>:` / `fn id<T>(x: T) => T` parse `type_params`. Unified `src/parser/ty.rs`.

10. **Generics v1 (checker)**  
    `Type::App`, instantiate to `List__int` (token substitute). Methods become `List__int_push`. No `T: Trait`. Language does not ship a generic std `List` (it ships `IntBuf`).

11. **No `src/runtime`**. `raise` = libc / `#listener`.

12. **`coffee fetch`** and **`coffee std install`** in CLI usage. **No** `coffee run` / `coffee test` unless `main.rs` grows those commands.

## Do **not** document as done

- Vtable / dynamic dispatch.
- Closure capture on anonymous `fn`.
- `try` / `catch`.
- Self-hosted toolchain (still LLVM 21.1 + `clang` + libclang).
- Package registry / semver crates (`foo = "1.0"`).
- `c fn` slice parameters (still rejected).
- `coffee run` / `coffee test` (not in `main.rs`; use `--bin` / `--jit`).

## Files

Keep bilingual parity. Root [README.md](../../README.md): **0.3.6**, experimental, std + last-use + fat pointer + `buf` + packages; driver gaps for `run`/`test` only if still absent.

`SYNTAX.md`: packages, std (`use print in std`, `sys` `of c`, `IntBuf`), last-use, `buf`, Coffee `[T]` fat pointer vs `c fn`.

`CLAUDE.md`: fetch, std, PackageDep, `last_use.rs`, `ty.rs`, mono, composite drop, deep clone, `buf`. Version **0.3.6**.

`docs/en` + `docs/zh`: README, compiler (toml + std), types (`last_use.rs`, `buf`, slice ABI), parser, runtime (official std is Coffee, not a Rust crate), backend drop/clone.

No git commit. No version bump beyond aligning strings to **0.3.6**.
