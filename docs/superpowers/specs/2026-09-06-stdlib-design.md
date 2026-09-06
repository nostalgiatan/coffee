# Coffee standard library (layered, language-first)

**Status:** implement now.  
**Not in this spec:** `Vec<T>` as a polished product, `io.Writer`, capturing closures, vtable, `try`/`catch`, `#listener` as a library error channel, `build.cf`, a registry.

## Goal

Ship a **thin official std** written in Coffee. Only the OS/heap edge uses C. The compiler CLI **embeds** that tree and can **install** it. Project compile finds it like any other package import root (route 3), without a network fetch.

## Hard rules (lock)

1. **Language wins.** Do not reimplement `mv` / `clone` / `rm` / `Error` / `raise` / `buf` / `object` / `[T]` / `[T; N]` / f-string / `str` `+` / `int()` casts / `for` / `match`.
2. **C only at the edge.** `of c` is allowed **only** in `sys.cf`. No `printf`. No wrapping `clone`.
3. **Everything else is Coffee.** `print`, growable buffer logic, capacity policy: `.cf`.
4. **One official package**, internal files as layers (`sys` → `mem`/`io` → facade `std`). Not three Rust crates named core/alloc/std.
5. **Allocation:** the language already allocates for `str` / f-string / class-with-`new`. Std does **not** add an `alloc` keyword. Growable buffers call **one** heap pair in `sys` (`malloc`/`free`), plus `memcpy` to move bytes into `buf` (Coffee cannot index `buf`).
6. **`strlen` exception:** `str` has no `.len`. `sys` may import `strlen` **only** to pass a length to `write`. Do not use it as a user-facing API.
7. **Library errors return values** (`int` / `bool`). Do not `raise` / `#listener` for normal std failures.
8. **Do not `free` `object`.** Heap ownership is `buf` or language `str`/class drop.
9. **`c fn` still rejects `[T]`.** Std Coffee `fn` may take `[T]`; do not put slices on C wrappers.
10. Selective `use foo in bar` imports **functions only**. Classes need a **full** `use mem` (or put helpers as functions in `std.cf`).

## Layout (repo + installed tree)

One package directory (embedded and installed identically):

```
library/std/coffee.toml     # [package] name = "std"
library/std/src/sys.cf      # only file with `of c`
library/std/src/mem.cf      # IntBuf (Coffee)
library/std/src/io.cf       # print / eprint (Coffee); uses sys
library/std/src/std.cf      # facade functions: print, eprint, exit
```

`coffee.toml` of std has **no** `[dependencies.packages]` (no cycle). Import root = `src/` (existing pkg rule).

### `sys.cf` (C edge)

Import from libc, then wrap so other modules never write `of c`:

- `malloc` / `free`
- `memcpy`
- `write` (POSIX)
- `exit`
- `strlen` (only for `write` length; not exported as a std convenience name)

Coffee wrappers (names used by io/mem):

```coffee
fn sys_malloc(n: int) => buf:
fn sys_free(p: buf) => void:
fn sys_memcpy(dst: buf, src: object, n: int) => object:
fn sys_write(fd: int, p: object, n: int) => int:
fn sys_exit(code: int) => void:
fn sys_strlen(s: str) => int:
```

`sys_malloc`: `let p: buf = malloc(n)` then return `p` (last-use move). Null: return still, callers treat as failure via `n <= 0` or a later check; do not `raise`.

### `io.cf` / `std.cf`

```coffee
fn print(s: str) => void:     /#/ write(1, s, strlen(s)); ignore write errors for v1
fn eprint(s: str) => void:    /#/ fd 2
fn exit(code: int) => void:   /#/ sys_exit
```

Implement once. Put the bodies in `io.cf`. `std.cf` defines **the same function names** that call `io` (`use print_io in io` is wrong if names collide). **Preferred:** implement `print`/`eprint`/`exit` in `std.cf` with `use sys_write, sys_strlen, sys_exit in sys`. Keep `io.cf` as a thin re-layer: either empty of public names or `fn write_fd(fd: int, s: str)` used by `std.cf`.

User code:

```coffee
use print in std
use eprint in std
use exit in std
```

### `mem.cf` — fill the growable-heap hole

Language has no `new [T; n]` that owns a malloc buffer. Std provides **`IntBuf`** (monomorphic `int`, not `List<T>` product):

```coffee
class IntBuf:
    p: buf
    len: int
    cap: int

    fn new() => IntBuf:
        /#/ sys_malloc a small cap (8 ints) or cap 0 with null-as-empty if malloc(0) is too sharp;
        /#/ prefer cap = 8, len = 0

    fn push(self, x: int) => void:
        /#/ if len == cap: grow (new cap*2 or 8), memcpy old bytes, sys_free old, store new p/cap
        /#/ memcpy x into slot len; len = len + 1

    fn get(self, i: int) => int:
        /#/ if i < 0 or i >= len: return 0 (no raise)

    fn length(self) => int:
        return self.len
```

`clone IntBuf` uses language deep clone (buf clone is a **type error**). **Do not** offer `clone` of IntBuf until buf+len clone exists; document that IntBuf is move-only (`mv` / last-use).

Users: `use mem` (full module) so the class is spliced. Not `use IntBuf in mem` (selective import is functions only).

**Not in v1:** generic `List<T>`, slice-owning constructors, `String` type (language `str` exists).

## Embed + install + compile

### Embed

Building `coffee` (the Rust CLI) **embeds** every regular file under `library/std/` (paths relative to that directory). `build.rs` walks the tree and generates `src/compiler/pkg/std_embed.rs` (or similar) with `include_bytes!` / `include_str!`. Do not hand-maintain the file list.

### `coffee std install`

- Extracts the embedded tree to an install directory.
- Default dir: `$COFFEE_STD` if set, else `$XDG_DATA_HOME/coffee/std` if `XDG_DATA_HOME` set, else `~/.local/share/coffee/std` (Termux: `$HOME/.local/share/coffee/std`).
- Writes files; creates parents. Idempotent overwrite of the std tree.
- Prints the absolute path of the installed package root (the directory that contains `coffee.toml`).
- `--test-mode` still skips the process lock.
- No network.

### Compile (project mode)

After resolving `[dependencies.packages]`, **always prepend** the official std import root (unless the project already has a package key `std` — then that path/url wins, no double prepend).

Std import root lookup order:

1. `$COFFEE_STD` if it contains `coffee.toml`
2. Installed default path if it contains `coffee.toml`
3. **Dev fallback:** `<coffee-repo>/library/std` only when compiling tests / when `COFFEE_STD` points there; for the binary running from a git checkout, also try `library/std` next to cwd **or** relative to the executable’s ancestors **only if** `coffee.toml` + `src/std.cf` exist (so `cargo test` works without install).
4. Else: error `standard library not installed; run coffee std install`.

Single-file mode (`no coffee.toml`): still prepend the same std import root so `use print in std` works for one-file programs.

### `coffee init`

- `src/main.cf` uses `use print in std` and `print("Hello, Coffee!\n")`.
- Do **not** write a machine-local `path = "/home/..."` std dep; the compiler injects std as above.
- Comment in toml: std is bundled; `coffee std install` if missing.

## Testing

- `tests/std_lib_tests.rs`: project (and one single-file) `use print in std`; capture `--bin` or `--jit` stdout. `use mem` + `IntBuf.new`/`push`/`get`/`length`. Set `COFFEE_STD` to repo `library/std`.
- `tests/std_install_tests.rs`: `coffee --test-mode std install` with `HOME`/`XDG_DATA_HOME`/`COFFEE_STD` temp dirs; installed tree has `src/std.cf`; compile against that dir without the repo path.
- `print_usage` lists `std install`.
- No `of c` outside `library/std/src/sys.cf` (grep in test or review).

## Docs

Short: `SYNTAX.md` (std section), `docs/en|zh/README.md`, `CLAUDE.md` one bullet, `print_usage`. Do not claim Rust `core`/`alloc` crates.

## Constraints

- Do not git commit. Do not bump `Cargo.toml` version.
- Dual `foo.rs` + `foo/` modules forbidden.
- Tests: `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock cargo test --offline -- --test-threads=1` for touched crates; prefer `--test std_lib_tests` / `--test std_install_tests`.
- Binary: `--test-mode`.
