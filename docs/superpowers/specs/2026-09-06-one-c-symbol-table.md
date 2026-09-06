# One C symbol table (bundled `.cfc`)

**Status:** implement now. User: checker and codegen must not keep separate libc lists; ship `.cfc` and delete the Rust tables.

## Goal

Exactly **one** `HashMap<String, CSymbolTable>` for C signatures. Typecheck, MIR call declare, and LLVM `add_function` all read it. Source of truth is bundled `libc.cfc` / `libm.cfc`, not `c_builtins.rs` and not `get_builtin_c_function_signature`.

## Why it drifted

- Analyzer `cfc_symbols` loaded `builtin_cfc_tables()` (`write` included).
- `Session.cfc_symbols` started **empty**; `.cfc` loads went here; `CompilationResult` used the session map.
- Codegen also had `is_builtin_c_function` / LLVM signatures (no `write`).

Same names, three sources.

## Rules

1. **One Arc.** `Session.cfc_symbols` and `SemanticAnalyzer.cfc_symbols` are the **same** `Arc<RwLock<…>>`. `c_imports` likewise (stop copying at pipeline step).
2. **Bundled files:** `library/cfc/libc.cfc`, `library/cfc/libm.cfc`. Contents = every symbol currently in `c_builtins.rs` (same param/return Coffee types, including `write`). Key the tables as **`libc`** and **`libm`** (`use x in libc of c`). Do **not** use `extract_library_name` on `libc.cfc` (that yields `"c"`).
3. **Load at `Session::with_config`:** parse bundled files (disk next to crate, plus `include_str!` so `coffee` works without the repo tree). Insert under `libc` / `libm`.
4. **User `.cfc`:** existing `load_cfc_file` **replaces** that library key (override). Search paths: `.`, `lib/`, `import_paths`, and std package `lib/` if present.
5. **Delete:** `src/semantic/c_builtins.rs`, `is_builtin_c_function`, `declare_builtin_c_function`, `get_builtin_c_function_signature`. Call sites (`literal.rs`, `binary.rs`, `declare_used_c_functions`) go through `declare_external_function` / `from_symbol` using `cfc_symbols`.
6. **`declare_runtime_functions`:** declare `puts`/`exit`/`malloc`/`free` **from the libc table**, not hardcoded LLVM types. If a name is missing from the table, that is a bug in `libc.cfc`.
7. **`is_c_library_function`:** true iff the name is in `c_imports` **or** in `cfc_symbols`. No third name list.
8. **No silent libc without a table.** `use foo in libc of c` still works without a *user* file because bundled libc is always loaded. Missing *symbol* is an error (same as unknown C).
9. **Tests:** `write` appears as an LLVM declare when compiling `use write in libc of c` + a call (IR contains `declare` + `write`). Round-trip: parse bundled `.cfc` → table keys `libc`/`libm` → `printf`/`write`/`sin` present. After delete, `rg c_builtins` / `is_builtin_c_function` / `get_builtin_c_function_signature` find nothing under `src/`.
10. Do not git commit. Do not bump version.

## Out of scope

`buf`/`object` ABI for `memcpy` (separate type hole). Growing POSIX beyond current `c_builtins` set. Regenerating `.cfc` from system headers in this change.

## Audit (second agent)

Search `src/` for other **frontend vs codegen** duplicate facts (name lists, default types, fake std modules). Fix if it is the same class of bug (two lists that must match). Otherwise list in the PR-style summary; do not invent new std APIs.
