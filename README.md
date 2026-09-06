# Coffee

Statically typed language with Python-like indentation, compiled with Rust to LLVM 21.1. Source files are `.cf`; projects use `coffee.toml`. Headline feature: **bidirectional C interop** through `.cfc` files (no original C headers required).

- **Version:** **0.3.9** — **not 1.0**. Experimental compiler you can already write programs with. `coffee --version` prints `0.3.9+` plus 12 hex digits of the compiler source hash (same Cargo patch still rebuilds user `.o` files after `cargo install`).
- **Versioning:** one landed feature → one patch. Ten patches on a minor → next minor (`0.3.10` becomes `0.4.0`). `1.0.0` is a product decision, not a rename. Do **not** bump the patch only to bust incremental cache — that is the hash suffix.
- **Catch-up from 0.2.2:** many landings were never tagged. Counted as ten features → `0.3.0`, then six more → `0.3.6`, then AST `compile_expr` trap → `0.3.7`, C header types → `0.3.8`, C dep graph → `0.3.9` (memory contract, borrow, `Error` base, listeners, MIR-only bodies, last-use move, composite drop, deep clone, `buf` vs `object`, slice fat pointer, package fetch, generics v1, official std, unified type parser + one C symbol table, multi/`*` import, `IntBuf`).
- **Docs:** [English](docs/en/README.md) · [中文](docs/zh/README.md)
- **Syntax:** [SYNTAX.md](SYNTAX.md)
- **Architecture notes:** [IFLOW.md](IFLOW.md) · contributor map [CLAUDE.md](CLAUDE.md)

## Build

LLVM 21.1 (`llvm-config` on `PATH`) and `clang` (linker / `--bin`) are required.

```bash
cargo build --release          # → target/release/coffee
cargo test --offline -- --test-threads=1
```

On Termux/Android, tests should pass `--test-mode` (the suite already does). Dev lock: `.coffee_compiler.lock`.

```bash
coffee input.cf                # object file
coffee --bin input.cf          # executable via clang
coffee run                     # project: --bin then exec (or `coffee run file.cf`)
coffee test                    # project: tests/*.cf (or test_*.cf); or `coffee test file.cf`
coffee --emit-llvm input.cf
coffee --jit input.cf
coffee -c header.h             # .h → .cfc
coffee fetch                   # url+hash deps → cache
coffee fetch <url> --save      # download .tar.gz, write packages.<name>
coffee std install             # extract bundled std
```

User programs typically `use print in std` (or `use * in std`). `print` writes to fd 1 via libc `write`; `fn main` returning 0 then uses **libc** `exit` (Coffee `fn exit` is LLVM `_cf_exit` so it does not steal the C runtime symbol). `use printf in libc of c` remains the C FFI; official std keeps `of c` inside `library/std/src/sys.cf`.

Declared `[dependencies.c_libraries]` with `headers` auto-generates `.cfc` into `target/cfc/` for installed libraries and their `needs` transitives. `coffee -c header.h` walks the include graph.

## Pipeline (current)

`parse_program` → semantic → typecheck + intra-procedural borrow → statement MIR (`hir::MirFn`, **not SSA**) → LLVM.

- Function bodies compile from MIR only (`compile_program_with_hir`). Missing MIR is an error; there is no AST fallback for `if`/`while`/`for`/`match`/`raise`.
- `Program.stmt_spans` are byte ranges in the parsed file; imported statements keep the **module** file’s spans.
- Nested `fn`/`class` in MIR are `NestedDecl` (name + key), not a cloned `Statement`.
- Expression values in function bodies go through `compile_hir_expr_typed`. Parser `compile_expr` is a dead trap (same idea as `compile_program`). SSA is tests-only (not LLVM). Per-expression source spans exist as `Expression::Spanned`.

`int(N)+` / `float(N)`: **N is bytes** (`int(4)+` ≈ C `int`). `raise` uses `fprintf`/`exit` unless the function has `#listener`.

## Still not 1.0

- Host tools required: LLVM 21.1, `clang`, libclang (not a self-hosted toolchain). **Out of scope for catch-up work.**
- No package registry / semver crates. Zig-style deps: `path` **XOR** `url` + SHA-256 tree `hash`. Cache under `COFFEE_CACHE` / `~/.cache/coffee`.
- No vtable / dynamic dispatch; anonymous `fn` does not capture; no `try`/`catch`. Coffee `[T]` fat pointer on Coffee `fn`; `c fn` still rejects slices.
