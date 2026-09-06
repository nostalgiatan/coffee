# C header types + `type` newtypes Implementation Plan

> **For agentic workers:** Parallel file-owned workstreams (not sequential SDD on one tree). REQUIRED: disjoint files listed per task. Do **not** git commit. Do **not** bump `Cargo.toml` version. Follow TDD. User rule: no commits unless the human asks.

**Goal:** Land spec `docs/superpowers/specs/2026-09-06-c-header-types-design.md`: Coffee `type Name: object` newtypes with `as` to/from source; `.cfc` can declare those types (and later `c class` / `c union`); clang generator emits them.

**Architecture:** Newtype is a **brand** in `TypeRegistry`, LLVM still `object`. `.cfc` parser stores extra decls on `CSymbolTable`. Generator writes Coffee spellings. Pipeline (Task 4, after parallel tasks) registers `.cfc`/`type` decls into the registry.

**Tech Stack:** Rust, nom parser, `TypeChecker`, clang-sys generator, `flock …/.cargo-test.lock cargo test --offline`.

## Global Constraints

- Spec path: `docs/superpowers/specs/2026-09-06-c-header-types-design.md` — follow it over this plan if they conflict on behavior.
- `type Name: T` is nominal, not an alias (`TypeRegistry.aliases` must **not** be used). Source this round is only `object`.
- No `class Name of object`. No Coffee `*T` syntax.
- Do not git commit. Do not bump version.
- Tests: `flock /data/data/com.termux/files/home/coffee/.cargo-test.lock cargo test --offline --test <name> -- --test-threads=1` or `cargo test --offline --lib <filter> -- --test-threads=1`. Binary tests need `--test-mode` (harness already does).
- Termux: workspace `/data/data/com.termux/files/home/coffee`.
- `parse_with_clang` stays `pub(crate)`.
- Existing `object`/`buf` drop rules unchanged.
- **File ownership is exclusive.** If you need a file not in your list, stop with BLOCKED.

---

### Task 1: Parse `type Name: object` in `.cf`

**Files (exclusive):**
- Create: `src/parser/type_decl.rs`
- Modify: `src/parser/mod.rs`, `src/parser/stmt.rs`, `src/parser/line.rs` (`is_top_level_keyword` add `"type"`; dispatch `type ` in `parse_single_line_statement_at`)
- Modify: `SYNTAX.md` keyword list — add `type` next to `class`
- Test: `src/parser/type_decl.rs` `#[cfg(test)]` and/or `tests/type_decl_parse_tests.rs`

**Do not touch:** `src/types/**`, `src/c/**`, `src/compiler/**`, `library/**`

**Interfaces:**
- Produces: `pub struct TypeDecl { pub name: String, pub source: String }`
- Produces: `Statement::TypeDecl(TypeDecl)`
- Parse line `type FILE: object` → name `FILE`, source `object`. Reject empty name, missing `:`, leftover junk.
- `type` is a keyword (cannot be an identifier). Keep existing parse of `as` expressions unchanged.

- [ ] **Step 1: Write failing parse tests** for `type FILE: object`, reject `type FILE object`, reject `type FILE:`.

- [ ] **Step 2: Run tests — expect fail** (no `TypeDecl`).

- [ ] **Step 3: Implement parser** using nom like `src/parser/var.rs` (`tag("type ")`, ident, `:`, type string via `crate::parser::ty::parse_type` or rest trim).

- [ ] **Step 4: Run parse tests — expect pass.**

- [ ] **Step 5: Do not commit.** Write report to `.superpowers/sdd/task-1-report.md`.

---

### Task 2: Registry newtypes + `as` (no implicit coerce)

**Files (exclusive):**
- Create: `src/types/newtype.rs` (helpers + unit tests OK)
- Modify: `src/types/mod.rs` (mod newtype), `src/types/registry.rs` (new field + `share`/clone Arcs), `src/types/checker/expr/ops.rs` (`cast_allowed`), `src/types/checker/compat.rs` (assignment/arg coerce), `src/semantic/analyzer/expr.rs` if it uses `can_coerce_from` for args
- Test: `src/types/newtype.rs` tests and extend `src/types/checker/tests.rs` **only by appending tests** (do not rewrite existing tests)

**Do not touch:** `src/parser/**`, `src/c/**`, `library/**`

**Interfaces:**
- Produces: `TypeRegistry::register_newtype(name: String, source: Type) -> Result<(), TypeSystemError>`
  - Error if `source` is not `Type::Variadic` (`object`) this round
  - Error if name already taken as alias/class/newtype
  - Stores `name → source`; `resolve_type("FILE")` returns `Type::NamedType { name: "FILE" }` **not** `object`
- Produces: `TypeRegistry::newtype_source(name: &str) -> Option<Type>`
- Produces: `TypeRegistry::is_newtype(name: &str) -> bool`
- `Type::can_coerce_from`: keep current behavior for non-newtypes (including `NamedType` class → `object`). **TypeChecker** must not use raw `can_coerce_from` when either side is a registered newtype.
- `cast_allowed(from, to)`:
  - equal types → true
  - newtype `FILE` ↔ source `object` → true (one hop)
  - `FILE` ↔ `HWND` (both newtypes) → false
  - newtype ↔ anything else → false (no numeric, no `buf` unless it goes object→buf via two-step in the expression `as object` then annotate)
- Implicit assignment/args: `FILE` is not `object`. `object` is not `FILE`.
- LLVM: do not change backend this task. `NamedType` already maps like other named types/handles.

Test sketch (TypeChecker, no parser):

```rust
let mut reg = TypeRegistry::root();
reg.register_newtype("FILE".into(), Type::Variadic).unwrap();
reg.register_newtype("HWND".into(), Type::Variadic).unwrap();
let registry = Arc::new(RwLock::new(reg));
let mut checker = TypeChecker::simple(registry);
let file_ty = Type::NamedType { name: "FILE".into() };
let hwnd_ty = Type::NamedType { name: "HWND".into() };
assert!(checker.cast_allowed(&file_ty, &Type::Variadic));
assert!(checker.cast_allowed(&Type::Variadic, &file_ty));
assert!(!checker.cast_allowed(&file_ty, &hwnd_ty));
// check_variable_decl let x: object = <FILE expr> must err
```

`cast_allowed` is `pub(in super::super)` — tests in `checker/tests.rs` can call it. If you need it from `newtype.rs` tests, add `pub(crate) fn cast_allowed_for_test`.

Also: `check_expression` on `Expression::TypeCast { target_type: "object", value: Variable FILE }` after binding.

- [ ] **Step 1: Failing tests**
- [ ] **Step 2: Run — RED**
- [ ] **Step 3: Implement registry + checker**
- [ ] **Step 4: GREEN** `cargo test --offline --lib newtype -- --test-threads=1` and checker tests you added
- [ ] **Step 5: No commit.** Report `.superpowers/sdd/task-2-report.md`

---

### Task 3: `.cfc` `type` lines + table + libc `FILE`

**Files (exclusive):**
- Modify: `src/c/parser.rs`, `src/c/signature.rs`, `src/c/mod.rs` (re-export if needed)
- Modify: `library/cfc/libc.cfc` — add `type FILE: object` at top; change `fopen` to `=> FILE` and `fclose(stream: FILE)`; **leave** `fprintf(stream: object, …)` as `object` this slice (callers can `as object` later)
- Test: append in `src/c/parser.rs` `#[cfg(test)]` and `tests/cfc_table_tests.rs` **only append** if that file exists

**Do not touch:** `src/parser/**`, `src/types/**`, `src/c/generator.rs`

**Interfaces:**
- Add to `CSymbolTable`:
  ```rust
  pub type_defs: HashMap<String, CTypeDef>,
  ```
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub enum CTypeDef {
      Newtype { name: String, source: String }, // source == "object"
      // stubs for later slices; you MAY add these variants but parser for c class can wait:
      // Class { name: String, fields: Vec<(String, String)> },
      // Union { name: String, fields: Vec<(String, String)> },
      // Enum { name: String, variants: Vec<(String, i64)> },
  }
  ```
- `CSymbolTable::new` initializes `type_defs` empty. Update **every** struct literal of `CSymbolTable` in this file.
- `parse_cfc_content`: skip `//` and `/#/` comments; if line starts with `type `, parse `type NAME: object` into `type_defs`. Other `type ` sources → `ParseError`.
- Empty check: empty iff **no** function symbols **and** no `type_defs`.
- Existing `c fn` lines unchanged.
- Multiline `c class` **out of this task** (Task 4/5). If you see `c class`, return a clear ParseError for now OR skip implementing class (prefer error so generator tests fail closed until Task 5).

- [ ] **Step 1: Failing test** `parse_cfc_content("type FILE: object\nc fn fopen(filename: string, mode: string) => FILE:\n", "libc")` has type_defs FILE and symbol fopen return FILE
- [ ] **Step 2: RED**
- [ ] **Step 3: Implement**
- [ ] **Step 4: GREEN** lib tests + `tests/cfc_table_tests.rs` if you add cases
- [ ] **Step 5: No commit.** Report `.superpowers/sdd/task-3-report.md`

**Note:** Changing `fopen` return may fail **other** integration tests until Task 4 registers `FILE`. That is expected; mention which tests you saw fail; do not rewrite unrelated tests to use `object` again.

---

### Task 4 (controller, after 1–3): Pipeline glue + `fopen` compile test

Not dispatched in parallel. Registers `Statement::TypeDecl` and `CSymbolTable.type_defs` into `TypeRegistry::register_newtype`. Compile test:

```coffee
type FILE: object
fn main() => int:
    let p: object = 0
    let f: FILE = p as FILE
    let q: object = f as object
    return 0
```

And `use fopen in libc of c` + `let f: FILE = fopen("a", "r")` without `as`.

Union/`c class`/clang visitor: follow-up after glue (spec slices 2–3 remainder).

---

## Spec coverage

| Spec item | Task |
|---|---|
| `type Name: object` parse | 1 |
| Nominal + `as` one hop, no sibling `as`, no implicit | 2 |
| Execute as `object` (LLVM) | existing NamedType/object; glue 4 |
| `.cfc` type + FILE | 3 |
| `c class` / union / clang | not in parallel wave |
| `char*` field vs param | generator later |
