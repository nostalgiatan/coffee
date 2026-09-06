# One C Symbol Table Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans. Do **not** git commit. Do **not** bump version.

**Goal:** Bundled `libc.cfc`/`libm.cfc` as the only C signature table; session and analyzer share one Arc; codegen declares LLVM types only from `CSymbol`.

**Architecture:** Parse bundled `.cfc` at `Session::with_config` into a shared `Arc`. Delete `c_builtins.rs` and the LLVM builtin signature match. `declare_external_function` already maps `CSymbol` → LLVM.

**Tech Stack:** Existing `crate::c::parse_cfc_content` (make `pub` if needed), `include_str!` for the two files.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-06-one-c-symbol-table.md`
- Do **not** git commit. Do **not** bump version.
- Dual `foo.rs` + `foo/` forbidden.
- `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock cargo test --offline --test <name> -- --test-threads=1`
- `--test-mode` on the binary.
- Table keys **`libc`** and **`libm`**, not `c`.

## File map

| Area | Files |
| --- | --- |
| CFC | Create `library/cfc/libc.cfc`, `library/cfc/libm.cfc` |
| Load | `src/c/parser.rs` (`pub parse_cfc_content`), `src/compiler/session.rs`, `src/compiler/pkg/` or `src/c/bundled.rs` |
| Share Arc | `src/semantic/analyzer/mod.rs`, `src/compiler/session.rs`, pipeline `set_c_imports` |
| Delete | `src/semantic/c_builtins.rs`, `src/semantic/mod.rs` mod line |
| Codegen | `src/backend/functions/declare.rs`, `mod.rs`, `expr/literal.rs`, `expr/binary.rs` |
| Tests | `tests/cfc_table_tests.rs` (or extend imports tests) |
| Docs | CLAUDE.md one line: libc/libm signatures are bundled `.cfc` |

---

### Task 1: Bundled `.cfc` + shared Arc + delete Rust tables + codegen from `CSymbol`

**Files:** as in the file map (this task owns all of them except the audit-only cleanup in Task 2).

- [ ] **Step 1:** Convert `builtin_cfc_tables()` into two `.cfc` files. Each entry:

```coffee
c fn write(fd: int, buf: object, count: int) => int(4)+:
```

Variadic: match how `parse_cfc_content` already parses `printf` (see `src/c/parser.rs` tests). Preserve every insert in `c_builtins.rs` (libc + libm).

- [ ] **Step 2:** `pub fn parse_cfc_content`. Add `load_bundled_c_tables() -> HashMap<String, CSymbolTable>` using `include_str!("../../library/cfc/libc.cfc")` (path relative to the Rust file you choose) **and** parse with library name `"libc"` / `"libm"`.

- [ ] **Step 3:** Failing test `tests/cfc_table_tests.rs`:
  - bundled parse: `tables["libc"].get("write")` and `printf`; `tables["libm"].get("sin")`.
  - compile `--emit-llvm --test-mode`:

```coffee
use write in libc of c
fn main() => int:
    write(1, "x", 1)
    return 0
```

  IR must contain `write` as a declare/call (not error `unknown C function`).

- [ ] **Step 4:** `Session::with_config`: create `cfc_symbols` Arc from `load_bundled_c_tables()`, pass **clone of Arc** into `SemanticAnalyzer` (new constructor `with_cfc_symbols`). Do **not** `init_builtin_c_symbols`. Same Arc for `session.cfc_symbols`. Share `c_imports` Arc the same way; stop `set_c_imports` copy if both point at one vec.

- [ ] **Step 5:** Delete `c_builtins.rs`. Delete `is_builtin_c_function`, `declare_builtin_c_function`, `get_builtin_c_function_signature`. `declare_external_function` only: existing `cfc_symbols` lookup + `declare_external_function_from_symbol`. `is_c_library_function`: `c_imports` or name in any loaded table. `declare_runtime_functions`: look up puts/exit/malloc/free in `cfc_symbols["libc"]` and declare from those symbols (thread `cfc_symbols` into that fn).

- [ ] **Step 6:** `literal.rs` / `binary.rs` / `declare_used_c_functions`: `declare_external_function` only.

- [ ] **Step 7:** `load_cfc_file` for module `libc` still overrides key `libc`. If filename would extract to `"c"`, still insert as the `use` module name (`libc`).

- [ ] **Step 8:** `flock … cargo test --offline --test cfc_table_tests --test imports_tests -- --test-threads=1` and a broader `cargo test --offline -- --test-threads=1` if time (must not skip if C tests fail).

- [ ] **Step 9:** CLAUDE.md: libc/libm from `library/cfc/*.cfc`, one session table.

---

### Task 2: Audit other frontend/codegen splits

**Do not** rewrite Task 1’s table. **Do** grep and fix leftover duplicates.

- [ ] Search `src/` for: `is_builtin_c_function`, `c_builtins`, `LIBC_FUNCTIONS`, `create_std_module`, `std_exports`, hardcoded C name arrays, `declare_runtime` duplicate types after Task 1.
- [ ] Remove or wire **dead** `create_std_module` / fake `print`/`memcpy` Coffee std exports in `pipeline/imports.rs` if nothing calls them for real compiles.
- [ ] If `is_c_library_function` still uses a name list, fold into Task 1’s rule (coordinate: only edit if Task 1 left a list).
- [ ] Add `tests/cfc_table_tests.rs` (or a unit test) that `include_str` the repo and asserts `src/` has **zero** matches for `is_builtin_c_function` and `builtin_cfc_tables` (string scan of those identifiers in `.rs` files excluding testdata).
- [ ] Return a short list of remaining dual sources you did **not** change (e.g. `buf` vs `object` ABI).

Verify: `flock … cargo test --offline --test cfc_table_tests -- --test-threads=1`
