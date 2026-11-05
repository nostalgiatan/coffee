# coffee/driver: the self-hosted compiler as a program

`src/main.cf` is `coffee2`: the lexer, parser, sema, types, ownership check and IR of this directory
tree behind a command line.

    coffee2 input.cf [-o output] [-I dir] [-O0|-O1|-O2|-O3] [--emit-llvm] [--emit-obj] [--mc] [--emit-flat]

- Imports of Coffee modules are resolved like the Rust compiler does: the module's text goes in front
  of the importing file, once, and its `use` line is dropped (`src/flat.cf`). Modules are looked for
  next to the importing file, then in each `-I` directory. `use ... of c` lines stay.
- The result is lowered to LLVM text (`-O0` leaves the IR optimizer off) and `clang` links it.
  `--mc` generates machine code (x86-64) instead of LLVM text and links the `.o`, `--emit-obj` stops at that `.o`; `--emit-llvm` stops at the `.ll`, `--emit-flat` at the flattened source (handy for line numbers in
  diagnostics, which refer to the flattened text).
- Arguments are read from `/proc/self/cmdline` (Linux and Android): Coffee's `main` has none.
- Strings from bytes: there is no way to make a `str` from a buffer, so `src/cstr.cf` joins
  one-character literals pairwise.

C libraries other than libc and libm: `use f in mylib of c` needs `mylib.cfc` next to the program, in
its `lib/`, or in an `-I` directory (or its `lib/`), as with the Rust compiler. The signatures of all
such files are declared together; a name the bundled libc/libm tables have keeps its bundled signature.
The link line gets `-L` and an rpath for the directory it was found in, and `-lmylib`.

Project mode (`coffee.toml`), packages, `fetch` and the like belong to a toolchain program that calls
this compiler (as cargo calls rustc), not to the compiler. Not here yet: `--target`, diagnostics with
file names, `.o` as the default product.

## Bootstrap

    tools/bootstrap.sh <rust coffee binary> <work dir>

The Rust compiler builds `coffee2`; `coffee2` compiles the driver's own sources (about 12k lines) to
LLVM text, `clang` makes `gen3` of that, and `gen3` compiles the sources again: the two texts are
byte for byte the same, so the compiler reproduces itself. It takes several minutes, most of it the Rust compiler building `coffee2` (one measurement: about 4 minutes).

Known costs: `s[i]` on a string calls `strlen` (as in the Rust compiler), so scanning 12k lines is
quadratic: the front end needs about 10 s of the 12 s for the whole run.

Machine-code bootstrap: the compiler built with `--mc` compiles its own sources to a byte-identical `.o` and the same LLVM text as the LLVM-built one.
