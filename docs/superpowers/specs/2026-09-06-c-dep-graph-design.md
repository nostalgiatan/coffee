# C library dependency graph (headers, `.cfc`, link)

**Status:** landed as `0.3.9`. Builds on `2026-09-06-c-header-types-design.md` (`0.3.8`). C++ still out. Do not git commit unless the user asks.

**C++ is out.** Same as the header-types spec.

## Goal

Compute a **complete, owned** graph of C headers and libraries so `#include` chains do not drop this library’s split headers, do not copy another library’s functions into this `.cfc`, and do not fail when a dependency is **installed** at standard search paths.

Installed means: clang can open the header (standard include and `-I`), and `find_library_file` (or an explicit `static_lib_path`) can open the link file. If it is installed, Coffee can **typecheck, generate `.cfc`, link, and run**. If it is not installed, that is a hard error.

## Why not `isFromMainFile` alone

`clang_Location_isFromMainFile` stops `unistd` leaking into `libz.cfc`, but it also drops `pngconf.h` in the same library and cannot tell `png.h` from `zlib.h` when both live in `$PREFIX/include`. Clang’s `isInSystemHeader` is **not** “third-party vs OS”. Third-party headers in `/usr/include` or `$PREFIX/include` are installed and must be found.

## Two entry points

### `coffee -c header.h`

The user asked to generate. Walk the include DAG from that file. Auto-generate `.cfc` for every **installed library node** on that walk (cycle detect). Does not require `coffee.toml`.

### Project compile (`coffee.toml` present)

Auto-generate and refresh `.cfc` only for:

1. Libraries listed in `[dependencies.c_libraries]`, and
2. **Installed library nodes** reached from those roots’ include graphs (transitive, including type-only uses).

`use foo in bar of c` with **no** matching `c_libraries` entry and **no** `libbar.cfc` on disk does **not** invent a header name from `bar` (cannot map `z` → `zlib.h`). That still needs a declaration (`headers = [...]`) or a manual `coffee -c`. Bundled `libc` / `libm` stay loaded without declaration.

Single-file mode (no `coffee.toml`): same as today for `use`, plus whatever `.cfc` files are already on the search path. No compile-time auto graph.

## Declaration shape (existing table, now honored)

```toml
[dependencies.c_libraries.z]
name = "z"
headers = ["zlib.h"]
```

- Table **key** is the Coffee module name (`use … in z of c`, file `libz.cfc`).
- `name` is the linker short name (`-lz`), same as today. If omitted, default to the key.
- `headers` are the **root** headers for this node (basename or path). Resolved with `include_paths` then standard include dirs (compiler `-E` paths, `$PREFIX/include`, `/usr/include`, `/usr/local/include`).
- `include_paths` are extra `-I` for non-standard prefixes only.
- `link_flags`, `static_link`, `static_lib_path` stay as today.
- `headers` empty on a declared lib: no auto-generate; load an existing `.cfc` or error if `use` needs one.

Do not add a second C-dep table.

## Graph

### File DAG

Nodes: header files in the clang TU. Edges: `#include` (`CXCursor_InclusionDirective`).

### Library nodes

A **library node** has: Coffee module key, linker `name`, root headers, owned files, `.cfc` path, link file.

Classify file `F` while walking from a root, **in this order**. Do not use `clang_Location_isInSystemHeader` as the library boundary (`png.h` and `zlib.h` are often in the same include dir). Same-directory alone is also unsound there (`unistd.h` sits next to `zlib.h` on Termux).

1. An existing `.cfc` on the search path lists `F` in its owned-headers metadata → that module.
2. `find_library_file` succeeds for a linker name derived from `F` **and that name is not the current node’s `name` and not `c`/`m`** → **another installed library**. Derivation: basename without `.h`, strip a leading `lib` (`libpng.h` → `png`). Extra alias: `zlib.h` → `z` (soname `libz`). If several names match, prefer a `c_libraries` key or an on-disk `lib<name>.cfc`.
3. **libc / POSIX / compiler cut** — never emit these functions into another `.cfc`: path under `bits/`, `linux/`, `asm/`, `sys/`, `asm-generic/`, or basename in the fixed set `stdio.h`, `stdlib.h`, `stddef.h`, `stdarg.h`, `stdint.h`, `stdbool.h`, `stdatomic.h`, `string.h`, `strings.h`, `memory.h`, `unistd.h`, `signal.h`, `errno.h`, `limits.h`, `float.h`, `time.h`, `fcntl.h`, `pthread.h`, `wchar.h`, `wctype.h`, `locale.h`, `math.h`, `setjmp.h`, `ctype.h`, `assert.h`, `features.h`, `endian.h`, `alloca.h`, `dlfcn.h`, `dirent.h`, `poll.h`, `sched.h`, `ucontext.h`. Also: clang resource-dir headers. `math.h` is `libm` when the current node is not already `m`.
4. **Satellite of the current node** if step 2 did not fire: included from a file already owned by this node, and either the basename starts with the root stem (`png` → `pngconf.h`, `pnglibconf.h`) or there is no distinct `.so` (covers `zconf.h` while walking `zlib.h`; covers extra headers on a project `-I`).
5. Otherwise libc cut if `F` is in a compiler/system include search path; else satellite of current (leftover project header).

Unresolvable `#include` → hard error (not installed, or missing `-I`).

A library node whose link file cannot be found (and is not `c`/`m`) → hard error. Installed ⇒ compilable and linkable.

### Declaration DAG (use-closure)

For each library node `L`:

- **Export functions:** non-`static` `FunctionDecl` whose defining file is owned by `L`.
- **Type closure:** from those functions’ signatures, walk fields, typedefs, function-pointer params. Every type is owned by some node (or is a builtin Coffee/`c fn` spelling).
- **`needs`:** other library nodes that own a type or a called function in that closure. Type-only needs **do not** require a `c_libraries` entry if the other node is installed (header + `.so` or bundled). Generate or load that node’s `.cfc` and record `needs`.
- Cycles in `needs` → hard error listing keys.

Emit into `libL.cfc` only `L`’s types and functions. Refer to other nodes by name; do not duplicate `c class z_stream_s` into `libpng.cfc` when `z` owns it.

## `.cfc` metadata

Still one file per library. At the top, after the existing comment banner:

```
// coffee-cfc 1
// module: z
// linker: z
// headers: zlib.h zconf.h
// needs: libc
```

Parser ignores unknown `// coffee-cfc` keys. Older `.cfc` files without this block: owned headers unknown; they still provide functions/types; they cannot claim files for step 2 until regenerated.

## Where auto `.cfc` is written

- Auto (compile or recursive `-c`): `<project target_dir>/cfc/lib<module>.cfc` (single-file `-c` without a project: `$TMPDIR` or `-o` as today for the **root**; recursive deps next to that `-o` directory, or `./cfc/` if `-o` is omitted).
- Search order for load: `.`, `lib/`, `import_paths`, then `target_dir/cfc`. User files in `.` / `lib/` **replace** auto files for that module key.
- Cache: regenerate a node when any owned header’s mtime/size changes, or when clang mapping would change (include path set). Identical output may be left in place.

## Compile and link

Project compile, before typecheck of `.cf`:

1. Resolve `[dependencies.c_libraries]` roots (headers + `name`).
2. Build the graph; auto-generate missing/stale `.cfc` for every installed node in the walk.
3. Load those tables into `Session.cfc_symbols` (same one table as today). Recursively load `needs` (cycle check). Identical type defs merge; mismatch → error.
4. `use` symbols still decide what the Coffee file may call. Loading a table does not import every C function into scope.
5. Link: `-l` for every graph node that is not `c`/`m`, plus existing `c_imports` from `use … of c`. Installed `.so` must be found. `collect_link_options` must not treat `include_paths` as linker `-L` (today it does; fix that: include dirs are `-I` for generate only; lib dirs stay `find_library_file` / `link_flags`).

## Errors (user-facing)

- Header not found in `-I` + standard include → not installed / add `-I` or `include_paths`.
- Linker name with no `.so`/`.a` at standard lib paths and no `static_lib_path` → not installed.
- `use … in bar of c` with no declaration, no `.cfc` → same class of error as today, plus hint to add `[dependencies.c_libraries.bar]` with `headers` or run `coffee -c`.
- Type/function owned by another node but that node is not installed → hard error naming the header and expected `lib<name>`.
- Duplicate type name, two defs disagree → error both modules.
- Include / `needs` cycle → error.

Never silently map a missing C type to `object` to “complete” the graph.

## Tests

- Fixture headers: `a.h` includes `a_extra.h` (no `.so`) and `b.h`; `b.h` is a second lib with a stub `.so` or a recorded `find_library_file` search path. `coffee -c a.h` emits `a` functions from `a_extra.h`, does not emit `b`’s functions, `needs` includes `b`, types from `b` are not duplicated.
- System zlib (if present): declared `z` + `headers = ["zlib.h"]` project `--bin` runs `zlibVersion` without a hand-written `.cfc`.
- Undeclared `use zlibVersion in z of c` without `libz.cfc` still fails.
- Missing header / missing `libz.so` (search path empty) → hard error, no fake `.cfc`.
- `target/cfc` is used; a user `libz.cfc` in `.` wins.

## Out of scope

C++, pkg-config files, distro package install, fetching C libraries from the network, changing Coffee `use` syntax, regenerating bundled `libc.cfc` from the platform `stdio.h` in this change.
