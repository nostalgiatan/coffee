# coffee/driver: the self-hosted compiler as a program

`src/main.cf` is `coffee2`: the lexer, parser, sema, types, ownership check and IR of this directory
tree behind a command line.

    coffee2 input.cf [-o output] [-I dir] [-O0|-O1|-O2|-O3] [--emit-obj] [--emit-flat]

- Imports of Coffee modules are resolved this way: the module's text goes in front
  of the importing file, once, and its `use` line is dropped (`src/flat.cf`). Modules are looked for
  next to the importing file, then in each `-I` directory. `use ... of c` lines stay.
- The result is lowered to x86-64 machine code (`-O0` leaves the IR optimizer off) and the system `cc` links the `.o`.
  `--emit-obj` stops at that `.o`, `--emit-flat` at the flattened source (handy for line numbers in
  diagnostics, which refer to the flattened text).
- Arguments are read from `/proc/self/cmdline` (Linux and Android): Coffee's `main` has none.
- Strings from bytes: there is no way to make a `str` from a buffer, so `src/cstr.cf` joins
  one-character literals pairwise.

C libraries other than libc and libm: `use f in mylib of c` needs `mylib.cfc` next to the program, in
its `lib/`, or in an `-I` directory (or its `lib/`). The signatures of all
such files are declared together; a name the bundled libc/libm tables have keeps its bundled signature.
The link line gets `-L` and an rpath for the directory it was found in, and `-lmylib`.

Project mode (`coffee.toml`), packages, `fetch` and the like belong to a toolchain program that calls
this compiler (as cargo calls rustc), not to the compiler. Not here yet: `--target`, diagnostics with
file names, `.o` as the default product.

## Bootstrap

    tools/build-from-seed.sh <dir>            (add --refresh to update the seed)

The committed seed object (`seed/`) is linked with `cc` into a first `coffee2`; it compiles the sources
to `gen1.o`, the result compiles them again to `gen2.o`, and the two are byte for byte the same, so the
compiler reproduces itself (about 35 s). When the object format has changed since the seed was made, one
more round is run. The seed came from the Rust compiler (git tag `rust-stage0-final`), which is no
longer in the tree.
