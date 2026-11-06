# compat

Integration tests of the Rust compiler (`tests/*_tests.rs`), ported to run against the
self-hosted compiler with `tea test`. A case is a program and one expectation:

- `compiles(name, src)`: the program compiles to an object (`assert_compiles`);
- `rejected(name, src, code)`: the front end reports an error, with that number when `code` > 0
  (`assert_compile_error`; messages are not compared, only `E100`-style numbers);
- `exits(name, src, n)`: the program runs and exits with `n` (`assert_exit_code`).

`tests/` holds all 627 cases; each file is named after the Rust file it came from. Every one passes.

Some ports differ from the Rust test on purpose. Four `imports` cases only check that the `use`
syntax parses (`parses`): the Rust helper does not resolve modules, the self-hosted driver does. Two
Rust tests assert `is_err()` under a name that says so; they are `rejected` here.

## Not ported

About 440 of the 1063 Rust tests are not here. They test the Rust compiler itself and have no
counterpart in the self-hosted one: the command line (`--emit-llvm`, `--show-memory`, `-c`,
static linking, `std install`, `fetch`), project mode (`coffee.toml`), the Rust MIR, LLVM output,
and tests that build their source with `format!`. They stay in `tests/` and run with `cargo test`.

The cases were extracted once by a script from the raw-string programs of the Rust tests; the
script is not kept.
