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

- **Single-file mode** (`coffee input.cf`, no `coffee.toml`): `CompilationPipeline::compile` (`src/compiler/pipeline/`), then `main.rs` drives `backend::codegen::CodeGenerator` and writes `.o` / `.ll` / etc.
- **Project mode** (`coffee.toml` present, or `coffee init`): `ProjectBuilder` (`src/compiler/builder.rs`) scans `src/`, validates entry points (`fn main` or `main(...)`), schedules units, and either links objects or (with `--emit-llvm` / `--emit-bc` / `--emit-asm`) writes the **entry module** artifact without clang. Per-file frontend work is the same `CompilationPipeline` (`CompilerFrontend` is a type alias).

Do not grow a second copy of the declare-functions / type-check loop outside `src/compiler/pipeline/`.

If you change parsing/semantic/type-checking behavior, change it in `src/compiler/pipeline/` only (then both modes pick it up).

### Compilation pipeline (stages → files)

1. **Parse** — `src/parser/` (`mod.rs::parse_program` is the entry; `ty.rs` unified type strings; `expr/`, `program.rs`, `stmt.rs`, `function.rs`, `class/`, `if.rs`, `for.rs`, `while.rs`, `match.rs`, `memory.rs`, `import.rs`). nom-based, indentation-aware (`indent.rs`, `tracker.rs`). Nested `List<List<int>>` is one type. `class List<T>:` / `fn id<T>(x: T) => T` parse `type_params`. Produces `parser::Program` with `statements` and `stmt_spans` (byte ranges in that source). Import expansion copies **module** spans onto prepended stmts; `Program::new` uses dummy `(0,0)` spans.
2. **Semantic analysis** — `src/semantic/analyzer/` (`mod.rs` plus `decls`, `expr`, `const_eval`, `report`, `memory`): scopes (`scope.rs`), symbols (`symbols.rs`), lifetimes (`lifetime.rs`).
3. **Type checking** — `src/types/`: `checker/` (`expr/` for expression checking), `borrow.rs` (intra-procedural `&`/`&mut` loans), `definition/` (`Type::App`), `mono.rs` (instantiate `List<int>` → `List__int`; methods `List__int_push`), `registry.rs`, `errors.rs`. Two-pass: first declare all functions (mutual recursion), then check bodies. Unresolvable C/`.cfc` types and unknown literals are errors, not `()` / `int`. No `T: Trait` / trait bounds. There is no std `List`.
4. **Lower to MIR** — after typecheck, `pipeline/` binds each function’s locals then lowers to `hir::MirFn` (statement-list CFG, **not** SSA). Complete MIR includes `if`/`while`, range and collection `for` (as `ForRange`), `match` (as `If`), `raise`, and `mv`/`rm`/`clone`. Nested `class`/`fn` are `MirStmt::Nested(NestedDecl)` (source name + `hir_fns`/LLVM key), not a cloned `Statement`. Complete MIR **never** contains leftover `Match`/`ForIn`: if `match_is_simple` is false or a collection `for` is not array/slice/tuple/literal, `lower_function` returns `Err` (Error diagnostic; omit that fn from `hir_fns` — missing MIR is a hard codegen error, not Warning + AST). End-of-body `Dropped` after bind must not block `p.x` that appears before `rm p`. `hir_expr_to_ast` is `#[cfg(test)]` only.
5. **Code generation** — `src/backend/`: `codegen/` (`CodeGenerator` coordinator) plus the modular split (`control_flow/`, `memory_ops/`, `memory/`, `functions/`, `expressions`, `expr/`, `variables`, `statements`, `classes`, `types`, `type_inference`, `error`, `mir_gen`, `mir_expr`, `mir_raise`, `mir_nested`, `class_layout`, `opt_passes`). Drivers call `compile_program_with_hir`. `compile_program` is a **dead trap**: it errors and tells you to use `compile_program_with_hir` (it must not compile bodies from empty MIR). Complete MIR compiles in `mir_gen.rs`; MIR expressions use `compile_hir_expr_typed`. Omitted `hir_fns` is `missing MIR for function`. `compile_statement` Match/For/Raise already error if hit. Nested decls compile via `nested_asts` keyed by `NestedDecl.hir_key`. AST `compile_expr` is a dead trap. String compilers were removed. LLVM I/O and JIT live in `backend/mod.rs`.
6. **C interop** — `src/c/`: `parser.rs` (parses `.cfc`), `signature.rs` (`CSymbol`/`CSymbolTable`), `generator.rs` (`.h`/`.c` → `.cfc` via `clang-sys`), `header_gen.rs` (Coffee functions → `.h`).

`src/compiler.rs` is the frontend module (`CompilationResult`, `CompilerFrontend`). Submodules live in `src/compiler/` (`pipeline/{mod,parse,imports,collect}.rs`, `builder.rs`, `session.rs`, `pkg/` fetch/cache/resolve, …). There is no `pipeline.rs` / `analyzer.rs` / `types/checker.rs` file.

### The `CompilationResult` contract

`compiler::CompilationResult` is the handoff from frontend to backend. Key cross-stage fields: `program` (AST plus `stmt_spans`; imported stmts keep **module** file byte ranges), `c_imports` (`"library:symbol"`), `cfc_symbols` (per-library `CSymbolTable`), and `hir_fns` (`Vec<MirFn>`). Drivers pass all four into `compile_program_with_hir`. When adding a stage that produces info the backend needs, thread it through here rather than globals.

## Things that will surprise you

- **`coffee_debug!` traces** (`src/debug_log.rs`) print only when `COFFEE_DEBUG` is set. Default compile stderr has no `DEBUG:` prefix (`tests/frontend_parity_tests.rs::test_default_compile_stderr_has_no_debug_prefix`).
- **Process mutex lock.** On startup `main.rs` acquires an exclusive `fs2` file lock at `.coffee_compiler.lock` in the project dir (found by walking up for `coffee.toml`, else cwd). Concurrent `coffee` invocations on the same project will fail. **Tests must pass `--test-mode`** (sets `COFFEE_TEST_MODE=1`) to skip the lock — the test harness in `tests/common/mod.rs` already does this; if you invoke the binary yourself, add `--test-mode`.
- **Memory contract (Plan B).** Value types (`int`/`float`/`bool` and tuples/arrays of those) end at scope without `rm`. Resources (`class`, `str`, slice, `buf`, `object`) cannot be copied with `=` / `let b = a` unless **last-use implicit move** (`src/types/last_use.rs`: simple variable, not `p.x`/`a[i]`, conservative in loops/`if`). Else `mv` or `clone`. `copy` and `clean out` are type errors. `rm` is early drop only. **`buf`:** owned malloc pointer; drop `free`s it; `clone buf` is a type error; `object` fields do **not** free the pointee. **Slices:** LLVM `{ptr, i64 len}` on Coffee `fn`; class fields drop elements, not the buffer. **Deep `clone`:** nested `str` / class / resource array / tuple (`src/backend/memory_ops/clone.rs`); `object` / refs / slice fat-struct stay shallow. Bare `malloc`/`free` need `use … in libc of c`.
- **Borrow checker.** Intra-procedural loans on variable, field (`p.x`), and index (`a[i]`) places (`src/types/borrow.rs`): many `&` or one `&mut`, never both; no `mv`/`rm`/assign while borrowed; no return of `&local`. Same-module: if a callee’s return type is `&T`, argument `&`/`&mut` loans may outlive the call statement (in-flight). No `'a` syntax. See `docs/superpowers/specs/2026-09-05-borrow-checker.md`.
- **Versions.** `Cargo.toml` is `0.3.9`. Each landed feature is a **patch** bump; every **10** patches on a minor becomes the next minor (`0.3.10` → `0.4.0`). `coffee --version` adds `+` plus 12 hex digits of the compiler-tree hash (`build.rs` → `COFFEE_COMPILER_HASH`); incremental `.o` cache includes that hash so a same-patch `cargo install` still rebuilds. See `docs/superpowers/specs/2026-09-06-stability-and-versioning.md`.
- **Raiseable types.** `raise` is legal only for builtin `Error` and classes that inherit it (`of Error` and descendants), not because the class name ends with `Error`. Extra fields/methods on those subclasses are allowed; `#name` listeners still take `err: Error`. See `docs/superpowers/specs/2026-09-05-error-base-class.md`.
- **Integer type notation.** `int(N)+` / `int(N)-` mean N-*byte* signed/unsigned (e.g. `int(4)+` = C `int`), not bits. Unannotated numeric literals may fit a narrower annotation if the value is in range; narrowing from a wider *variable* is still an error. `float(N)` is N-byte float. `int` and `float` default to 8 bytes.
- **Linker is `clang`.** `--bin` mode assembles a `clang` command with `-L.`, `-Wl,-rpath,.`/`$ORIGIN`, and `-l<lib>` flags derived from `c_imports`. Custom libraries (not libc/libm) are searched in `.`, `./lib`, `$PATH`, `/usr/lib`, `/usr/local/lib`, `~/.local/lib` (`find_library_file` in `main.rs`); missing ones produce a link error.
- **Packages:** `PackageDep` (`src/compiler/project.rs`): `[dependencies.packages.<key>]` is `path` XOR (`url` + SHA-256 of the **unpacked** tree `hash`). Not semver `foo = "1.0"`. `coffee fetch` / `coffee fetch <url> --save [name]` (`src/compiler/pkg/`). Cache `COFFEE_CACHE`, else `$XDG_CACHE_HOME/coffee`, else `~/.cache/coffee`. Path deps are local directories; url deps are `.tar.gz`. Project compile resolves and prepends import roots. Official **std** is embedded (`build.rs` → `pkg::std_fs`); `coffee std install` extracts it. Compile prepends that import root unless the project already has package key `std`. No registry.
- **`libc`/`libm` signatures** come from bundled `library/cfc/libc.cfc` and `library/cfc/libm.cfc`, loaded once into `Session.cfc_symbols` and shared with `SemanticAnalyzer` (same `Arc`). `use printf in libc of c` needs no user `.cfc`. All other C libraries require a `.cfc` file in `.` or `lib/` (search order in `load_cfc_file`). Generate one with `coffee -c header.h [-I path]`. User `libc.cfc`/`libm.cfc` still override the bundled table under keys **`libc`** / **`libm`** (not `extract_library_name` → `"c"`).
- **Path security.** `validate_file_path` (`main.rs`) rejects null bytes, `..`, symlinks, paths escaping cwd, and a blocklist of system dirs (`/etc/`, `/sys/`, `/proc/`, `/dev/`, `/usr/bin/`, ...). Keep this in mind when wiring up file inputs.

## CLI reference (driver flags)

`coffee fetch` / `coffee fetch <url> [--save [name]] [--force]` · `coffee std install [dir]` · `coffee run` · `coffee test` · `--emit-{ast,llvm,bc,asm}` · `--bin` (link to executable, default output is `.o`) · `--jit` (run in-process via LLVM JIT, resolves C symbols with `dlsym(RTLD_DEFAULT)`) · `--link` (link `.o` files) · `-c`/`--gen-cfc` (`.h`/`.c` → `.cfc`) · `--have-c` (emit a `.h` for `c fn` exports) · `--target <triple>` (cross-compile) · `--static` / `--static-lib <name>` · `-O0..-O3` · `-o <file>` · `-I <path>` · `--show-memory` · `--enable-bitfields` · `--enable-safety` · `--test-mode` (skip process lock). See `print_usage()` in `src/main.rs`.
