# tea

The tool around the compiler. `tea` picks a command; each command is a module of its own under
`src/`, so a new one is a new file and one line in `src/main.cf`.

    tea test [-I dir]... [--compiler path] [-j N] [file.cf]...

`tea test` compiles every file of `tests/` with `coffee2`, runs it and prints one line per test.
Each test gets `target/test/<name>/` with its program (`bin`), `compile.log`, `run.log` and
`status`, and runs inside that directory. Tests run `-j` at a time (default 4) through `xargs -P`.
Include directories come from `-I` and from `coffee.toml` (`[build] include`, then `[test] include`):

    [build]
    include = ["../ir/src", "src"]

`tea` and `coffee2` come out of `driver/tools/build-from-seed.sh <dir>`; no Rust is involved.
The exit code is 0 when every test passes, 1 when one fails, 2 for a bad command line.

    tea build [-O0..-O3] [-I dir]... [--compiler path]
    tea run   [same options] [-- program arguments...]

`tea build` compiles `[build] main` (default `src/main.cf`) into `target/build/<package name>`; `tea run`
starts it. The old `tea.toml` is still read, with a notice, when `coffee.toml` has no include list.
The `tea` tests need `tea` and `coffee2` on `PATH`.

Not done: caching of unchanged tests, packages.
