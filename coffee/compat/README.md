# compat

Integration tests of the Rust compiler (`tests/*_tests.rs`), ported to run against the
self-hosted compiler with `tea test`. A case is a program and one expectation:

- `compiles(name, src)`: the program compiles to an object (`assert_compiles`);
- `rejected(name, src, code)`: the front end reports an error, with that number when `code` > 0
  (`assert_compile_error`; messages are not compared, only `E100`-style numbers);
- `exits(name, src, n)`: the program runs and exits with `n` (`assert_exit_code`).

`tests/` holds 1023 cases; each file is named after the Rust file it came from, and `*_rest.cf` files hold
the cases a first pass left out. Every one passes. A fourth kind, `balanced(name, src)`, runs the program
and fails on a memory error or a leak (the Rust `assert_balanced`).

Some ports differ from the Rust test on purpose. Four `imports` cases only check that the `use`
syntax parses (`parses`): the Rust helper does not resolve modules, the self-hosted driver does. Two
Rust tests assert `is_err()` under a name that says so; they are `rejected` here. Tests that only
tried to compile a program and accepted either outcome were dropped.

## Where the two compilers differ

Found while porting; the cases for these are left out or written to the self-hosted behaviour.

- Arithmetic never traps: overflow wraps and division by zero is not checked, so the Rust tests of
  those traps are not here. An out-of-range index still traps.
- A function that gets a value through `&T` may assign its fields (`c.x = 9`); the compiler's own
  sources rely on it. The Rust compiler rejected that. A store through `*r = v` with `r: &T` is rejected.
- `buf<T>` may hold `str` and classes (the compiler's own `buf<Txt>` does); the Rust compiler allowed only values.
- A class may not have a field and a method with the same name (the Rust compiler read the field).

## Not ported

About 120 of the 1063 Rust tests are not here. They test the Rust compiler itself and have no
counterpart in the self-hosted one:

- the command line: `--emit-ast`, `--jit`, `-c` (header to `.cfc`), `--have-c` (header from `c fn`
  exports), `--static`, `--show-memory`, version and help text;
- project mode: `coffee.toml` `[dependencies]`, packages, `fetch`, `std install`, `init`, the process lock;
- the C dependency graph (`c_dep_graph`, `c_dep_graph_gen`, `c_diag`): the generator that maps headers to libraries;
- crash behaviour of the running program (runtime traps).

The cases were extracted by a script from the raw-string programs of the Rust tests; the script is not kept.
The Rust sources are in the git tag `rust-stage0-final`.
