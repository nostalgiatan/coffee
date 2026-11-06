# Coffee

Statically typed language with Python-like indentation, compiled to x86-64 machine code by a compiler written in Coffee. Source files are `.cf`; projects use `coffee.toml`. Headline feature: **bidirectional C interop** through `.cfc` files (no original C headers required). Long-term goal: the smallest possible footprint, down to an embedded JIT.

- **Compiler:** self-hosted, in [coffee/](coffee/): lexer, parser, semantic analysis, types, ownership, SSA IR with an optimizer, x86-64 code generator, ELF object writer, linker (`cfld`). It links with its own linker; `cc` is used only for programs that call C functions the built-in C library lacks (`printf`, `fopen`, ...). No LLVM, no Rust.
- **Docs:** [docs/](docs/README.md) (中文; tutorial, how-to, reference, explanation). Syntax: [SYNTAX.md](SYNTAX.md). Contributor map: [CLAUDE.md](CLAUDE.md).
- **First generation:** the Rust/LLVM compiler is archived. Its last version is the git tag `rust-stage0-final`; its docs are in [docs/legacy-rust](docs/legacy-rust/README.md). What it had and the self-hosted compiler does not: [docs/explanation/stage0.md](docs/explanation/stage0.md).

## Build

x86-64 Linux is all that is needed: no C compiler either. A committed seed (a static executable of the compiler) builds it:

```bash
coffee/driver/tools/build-from-seed.sh /tmp/coffee-seed     # → coffee2 and tea in /tmp/coffee-seed
```

```bash
coffee2 input.cf -o prog                  # compile and link
coffee2 input.cf --emit-obj               # object file only
coffee2 prog.cf --incremental target/obj -j 8   # one .o per source file, unchanged ones reused
tea test                                  # tests/ of the current package, in parallel
tea build / tea run                       # coffee.toml [build] main
```

Run the whole suite with `coffee/compat` (1023 cases ported from the Rust tests) and each stage's own `tests/` through `tea test --compiler /tmp/coffee-seed/coffee2`. After a compiler change, `build-from-seed.sh <dir> --refresh` updates the seed.

`int(N)+` / `float(N)`: **N is bytes** (`int(4)+` ≈ C `int`). Memory: values copy, resources (`class`, `str`, `buf`, slices) move or `clone`; `rm` drops early; `&` / `&mut` borrow. See [docs/explanation/ownership-and-drop.md](docs/explanation/ownership-and-drop.md).

## Not done

- x86-64 Linux only (aarch64 backend, other platforms are future work).
- No package manager, no `.h` → `.cfc` generator, no static linking flags (see the stage0 note above).
- No vtable / dynamic dispatch; anonymous `fn` does not capture; no `try`/`catch`.
