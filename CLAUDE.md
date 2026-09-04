# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Coffee is a compiler for a statically-typed language with Python-like indentation syntax, written in Rust and targeting LLVM. The language uses `.cf` source files and `coffee.toml` for project config. Its headline feature is **bidirectional C interop** via `.cfc` (C function declaration) files, allowing C libraries to be linked without the original headers.

For language syntax/type-system reference, see `SYNTAX.md`. For architecture prose, see `IFLOW.md` and `docs/{en,zh}/`. This file captures only what is not obvious from those.

## Build & prerequisites

```bash
cargo build              # debug build → target/debug/coffee
cargo build --release    # release build → target/release/coffee
cargo check              # type-check without codegen (fast iteration)
cargo test               # runs the integration test suite (builds the binary first)
cargo test --test control_flow_tests              # run one test file
cargo test control_flow_tests::test_name -- --nocapture   # run a single test
```

- **Hard dependency on LLVM 21.1** via `inkwell` (`features = ["llvm21-1"]` in `Cargo.toml`). `llvm-config` must be on `PATH` at build time.
- **`clang` must be installed** — the driver shells out to `clang` as the linker in `--bin` mode (`src/main.rs`).
- Dev environment is **Termux on Android** (`aarch64-linux-android`). `backend/mod.rs::clean_target_triple` strips the trailing Android API level (e.g. `aarch64-linux-android24`) from target triples; keep this in mind when touching cross-compilation.

## Architecture: one pipeline, two drivers

`main.rs` selects a **driver** by looking for `coffee.toml`. Both drivers run the same frontend:

- **Single-file mode** (`coffee input.cf`, no `coffee.toml`): `CompilerFrontend::compile` forwards to `CompilationPipeline` (`src/compiler/pipeline.rs`), then `main.rs` drives `backend::codegen::CodeGenerator` and writes `.o` / `.ll` / etc.
- **Project mode** (`coffee.toml` present, or `coffee init`): `ProjectBuilder` (`src/compiler/builder.rs`) scans `src/`, validates entry points (`fn main` or `main(...)`), schedules units, and either links objects or (with `--emit-llvm` / `--emit-bc` / `--emit-asm`) writes the **entry module** artifact without clang. Per-file frontend work is the same `CompilationPipeline` via `CompilerFrontend`.

`CompilerFrontend` holds `Arc<Session>` plus `CompilationPipeline` and must not grow a second copy of the declare-functions / type-check loop.

If you change parsing/semantic/type-checking behavior, change it in `src/compiler/pipeline.rs` only (then both modes pick it up).

### Compilation pipeline (stages → files)

1. **Parse** — `src/parser/` (`mod.rs::parse_program` is the entry; `expr.rs`, `function.rs`, `class.rs`, `if.rs`, `for.rs`, `while.rs`, `match.rs`, `memory.rs`, `import.rs`). nom-based, indentation-aware (`indent.rs`, `tracker.rs`). Produces `parser::Program`.
2. **Semantic analysis** — `src/semantic/analyzer.rs` (large, ~5k lines): scopes (`scope.rs`), symbols (`symbols.rs`), lifetimes (`lifetime.rs`).
3. **Type checking** — `src/types/`: `checker.rs`, `definition.rs`, `registry.rs`, `inference.rs`, `errors.rs`. Two-pass: first declare all functions (mutual recursion), then check bodies.
4. **Code generation** — `src/backend/`: `codegen.rs` (`CodeGenerator` is the coordinator) plus the modular split (`arithmetic`, `control_flow`, `memory_ops`, `memory/`, `functions`, `expressions`, `variables`, `statements`, `classes`, `types`, `type_inference`, `error`). LLVM I/O and JIT live in `backend/mod.rs` (`Backend`, `compile_and_run`).
5. **C interop** — `src/c/`: `parser.rs` (parses `.cfc`), `signature.rs` (`CSymbol`/`CSymbolTable`), `generator.rs` (`.h`/`.c` → `.cfc` via `clang-sys`), `header_gen.rs` (Coffee functions → `.h`).

### The `CompilationResult` contract

`compiler::CompilationResult` is the handoff from frontend to backend. Its key cross-stage fields are `program` (AST, possibly expanded with imported-module statements prefixed `module.symbol`), `c_imports` (strings shaped `"library:symbol"`), and `cfc_symbols` (per-library `CSymbolTable`). The backend's `compile_program(&program, &c_imports, cfc_symbols)` consumes all three. When adding a stage that produces info the backend needs, thread it through here rather than globals.

## Things that will surprise you

- **`eprintln!("DEBUG: ...")` is everywhere** in `src/compiler/pipeline.rs`. Compilation is very noisy on stderr by design (current state). Do not assume stderr is clean; tests check exit codes and specific stderr substrings, not silence.
- **Process mutex lock.** On startup `main.rs` acquires an exclusive `fs2` file lock at `.coffee_compiler.lock` in the project dir (found by walking up for `coffee.toml`, else cwd). Concurrent `coffee` invocations on the same project will fail. **Tests must pass `--test-mode`** (sets `COFFEE_TEST_MODE=1`) to skip the lock — the test harness in `tests/common/mod.rs` already does this; if you invoke the binary yourself, add `--test-mode`.
- **Explicit memory management is mandatory in Coffee.** The language has no implicit drop: variables must be cleaned up with `rm` / `clean out` (see `SYNTAX.md` §内存管理). Codegen and the type checker treat `mv`/`clone`/`copy`/`rm`/`clean` as first-class statements (`parser::Statement::MemoryOp`, checked in `types::checker::check_memory_op`). Test fixtures must `rm` what they `let`.
- **Integer type notation.** `int(N)+` / `int(N)-` mean N-*byte* signed/unsigned (e.g. `int(4)+` = C `int`), not bits. `float(N)` is N-byte float. `int` and `float` default to 8 bytes. The type mapper in `src/backend/types.rs` and `src/types/definition.rs::type_from_str` must agree on this.
- **Linker is `clang`.** `--bin` mode assembles a `clang` command with `-L.`, `-Wl,-rpath,.`/`$ORIGIN`, and `-l<lib>` flags derived from `c_imports`. Custom libraries (not libc/libm) are searched in `.`, `./lib`, `$PATH`, `/usr/lib`, `/usr/local/lib`, `~/.local/lib` (`find_library_file` in `main.rs`); missing ones produce a link error.
- **`libc`/`libm` have built-in default signatures** — `use printf in libc of c` works with no `.cfc` file. All other C libraries require a `.cfc` file in `.` or `lib/` (search order in `load_cfc_file`). Generate one with `coffee -c header.h [-I path]`.
- **Path security.** `validate_file_path` (`main.rs`) rejects null bytes, `..`, symlinks, paths escaping cwd, and a blocklist of system dirs (`/etc/`, `/sys/`, `/proc/`, `/dev/`, `/usr/bin/`, ...). Keep this in mind when wiring up file inputs.

## CLI reference (driver flags)

`--emit-{ast,llvm,bc,asm}` · `--bin` (link to executable, default output is `.o`) · `--jit` (run in-process via LLVM JIT, resolves C symbols with `dlsym(RTLD_DEFAULT)`) · `--link` (link `.o` files) · `-c`/`--gen-cfc` (`.h`/`.c` → `.cfc`) · `--have-c` (emit a `.h` for `c fn` exports) · `--target <triple>` (cross-compile) · `--static` / `--static-lib <name>` · `-O0..-O3` · `-o <file>` · `-I <path>` · `--show-memory` · `--enable-bitfields` · `--enable-safety` · `--test-mode` (skip process lock). See `print_usage()` in `src/main.rs`.
