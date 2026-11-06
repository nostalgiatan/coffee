# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Coffee is a statically-typed language with Python-like indentation syntax. The compiler is written in Coffee (`coffee/`, self-hosted), emits x86-64 machine code and an ELF object itself, and links with its own linker (`cc` only for programs that call C functions the built-in C library lacks). The language uses `.cf` source files and `coffee.toml` for project config. Its headline feature is **bidirectional C interop** via `.cfc` (C function declaration) files. The first-generation Rust/LLVM compiler is archived in the git tag `rust-stage0-final`; do not look for it in the tree.

For syntax see `SYNTAX.md` (written for the first generation; the self-hosted compiler takes a subset plus some additions, noted in it). For the compiler's design see `docs/` (`docs/reference/pipeline.md`, `docs/explanation/`). Old Rust-era architecture prose is in `docs/legacy-rust/`. This file captures what is not obvious from those.

## Build & test

```bash
coffee/driver/tools/build-from-seed.sh /tmp/w          # needs no C compiler; leaves /tmp/w/coffee2, /tmp/w/tea and /tmp/w/cfjit
coffee/driver/tools/build-from-seed.sh /tmp/w --refresh # also rewrite the committed seed (do this when a compiler change should be in the seed)
cd coffee/compat && PATH=/tmp/w:$PATH tea test --compiler /tmp/w/coffee2 -j 16   # 1023 ported cases
```

Each stage directory (`lexer parser sema types own ir rt ld driver tea compat`) has its own `tests/`, run with `tea test` from that directory. `tea` output truncates failure text; to see a failing case, compile that test file by hand with `coffee2 file.cf -I <dirs from coffee.toml>` and run it.

The seed must always be able to build the sources (it is the only bootstrap): after an incompatible change to the compiler's own sources or object format, `build-from-seed.sh` runs an extra round automatically; commit a refreshed seed with the change. Never write compiler sources the previous seed cannot compile in the same commit that needs a new feature.

## Architecture (coffee/)

`lexer → parser (Ast node table) → sema (scopes) → types (Typer) → own (ownership/borrow events) → ir/lower (SSA) → modopt → x86 asm/gen → ELF writer`; `ld/` is the linker (`cfld`, with section gc), `rt/` the runtime pieces linked on demand (`fmt.cf` number formatting), `driver/` the command line, `tea/` the build and test tool, `compat/` the ported Rust test cases.

- The driver **flattens** modules into one text (`driver/src/flat.cf`, plus `alias.cf` for `use f as g`/`use m as a`, `nested.cf` for same-named nested classes) before the front end; diagnostics refer to flattened line numbers (`--emit-flat`).
- Incremental builds (`--incremental dir`, `driver/src/incr.cf`): one `.o` per source file, keys by the symbols a file mentions and the interfaces they name; only stale files' bodies are checked; small functions' bodies count as interface so other files can inline them; one `.text.<fn>` section per function so `cfld`/`cc --gc-sections` drop unreached ones.
- Arithmetic never traps (wraps); an out-of-range index does trap.
- `compat/README.md` lists where behaviour differs from the first generation and what was not ported.

## Things that will surprise you

- **Memory contract.** Value types (`int`/`float`/`bool`, tuples/arrays of those) end at scope without `rm`. Resources (`class`, `str`, slice, `buf`, `object`) cannot be copied with `=` / `let b = a` unless **last-use implicit move**; else `mv` or `clone`. `rm` is early drop only. `buf` is `{ptr, len}` owning a malloc block; `buf<Txt>` of classes is allowed and drops its elements (a new buffer's elements are zero; assignment checks for null before releasing the old one). `object` is a C handle: never freed. Slices are `{ptr, len}` views; `b[i..j]` makes one (not storable in a `let`). `clone` of a class copies its `buf` fields of plain elements into blocks of their own (a bare `buf` cannot be cloned; a `buf` of resource elements inside a cloned class is still shared).
- **Borrows.** `&` / `&mut`; the compiler's own sources mutate through `&T`, so assignment to a field through `&T` is not an error (a store `*r = v` through `&T` is).
- **Integer notation.** `int(N)+` / `int(N)-` mean N-*byte* signed/unsigned (`int(4)+` = C `int`). `float(N)` is N bytes. `int` and `float` default to 8.
- **Quirks.** No ternary. `copy` is a keyword. A `str` is a pointer to NUL-terminated text with its length in the 8 bytes before it (`len`/index are O(1); a string may hold NUL bytes). Every string the compiler makes (literals, `+`, f-strings, `clone`) has that word; a C string becomes one only through `x as str` (which copies it). Tuple indexing is not supported. A resource assignment moves (E302 when it would copy).
- **C interop.** libc/libm signatures are bundled (`library/cfc/libc.cfc`, mirrored in `coffee/sema/src/c_names.cf` by hand); types a `.cfc` declares (`type FILE: object`) are visible to programs. Other C libraries need `<lib>.cfc` next to the program, in `lib/`, or in an `-I` directory. A program function named like one the generated code calls in libc (`malloc`, `free`, `write`, ...) gets a `__cf` suffix in its symbol.
- **`--static`.** Links with `ld/` (in-process) and appends `rt/src/min.cf` (generated into `ir/src/minrt.cf` by `rt/gen.sh`; rerun it after editing min.cf). That runtime cannot allocate, so it uses the intrinsics `peek8/peek64/poke8/poke64(addr, ...)` and `syscall(n, a..f)` and `c fn` (exported under its exact name; other functions named like libc runtime ones get `__cf`). A text with no `main` defines its module-level `let`s zeroed in its own object.
- **Static compiler.** `coffee2 driver/src/main.cf <-I dirs> --static -o coffee2s` builds a compiler with no shared libraries, which reproduces itself byte for byte. The compiler's own sources must therefore stay off `printf`/`fopen`/`putchar` (variadic or stdio): print with `say(f"...")` (ir/src/txt.cf), read and write files with `open`/`read`/`write` (driver/src/files.cf). A new libc function the sources call needs a definition in `rt/src/min.cf` (then `sh rt/gen.sh`) and its name in `keep_off_runtime` (ir/src/lower.cf).
- **`main`.** `fn main() => int` returns the exit status; a `void` main exits 0.
- **Versions.** The first generation's `0.3.9` is retired; the self-hosted compiler is not versioned yet.
