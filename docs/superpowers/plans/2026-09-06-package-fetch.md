# Package Fetch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (inline) or implement task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Zig-style content-addressed deps in `coffee.toml` (`path` XOR `url`+`hash`), `coffee fetch`, cache, and project-mode import roots — no registry, no std package.

**Architecture:** New `src/compiler/pkg/` does hash + fetch + resolve. `ProjectConfig` deserializes `PackageDep`. `ProjectBuilder` prepends resolved import roots before compile. CLI `fetch` is a subcommand like `init`.

**Tech Stack:** Existing `sha2`/`hex`/`toml`/`serde`; add `ureq`, `tar`, `flate2`.

## Global Constraints

- Do **not** git commit. Do **not** bump `Cargo.toml` `[package].version`.
- Dual `foo.rs` + `foo/` modules forbidden.
- Tests: `flock /data/data/com.termux/files/home/coffee/.superpowers/sdd/cargo.lock cargo test --offline --test pkg_fetch_tests -- --test-threads=1` (HTTP tests use `file://` only so `--offline` cargo is fine; no crates.io during test run if lockfile updated first).
- Binary invocations need `--test-mode`.
- Spec: `docs/superpowers/specs/2026-09-06-package-fetch-design.md`
- Leave `[dependencies.std]` unread. Do not invent `build.cf`.
- `use` still splices modules via `pipeline/imports.rs`; only **search roots** change.

## File map

| Area | Files |
| --- | --- |
| Types | `src/compiler/project.rs` (`PackageDep`) |
| Pkg | `src/compiler/pkg/{mod,hash,fetch,resolve}.rs` |
| Wire | `src/compiler.rs` (`pub mod pkg`), `src/compiler/builder.rs`, `src/main.rs` |
| Crates | `Cargo.toml` / `Cargo.lock`: `ureq`, `tar`, `flate2` |
| Tests | `tests/pkg_fetch_tests.rs`, `tests/common/mod.rs` if helpers help |
| Docs | `docs/en/README.md`, `docs/zh/README.md`, `docs/en/compiler.md`, `docs/zh/compiler.md`, `coffee init` template, `CLAUDE.md` one bullet, `print_usage` |

---

### Task 1: `PackageDep` serde + XOR validation

**Files:**
- Modify: `src/compiler/project.rs`
- Modify: `src/compiler/builder.rs` unit test that round-trips toml if it mentions `packages`

**Produces:**

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PackageDep {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
}

// Dependencies.packages: HashMap<String, PackageDep>
```

- [ ] **Step 1: Failing unit tests** in `project.rs` `#[cfg(test)]`:
  - `path` only parses.
  - `url`+`hash` parses; hash normalized to lowercase in `validate`.
  - both path and url → `validate` Err.
  - url without hash → Err.
  - old `foo = "1.0.0"` → `from_str` Err (not a table).

- [ ] **Step 2: Implement** `PackageDep`, change `packages` map type, XOR + 64 hex hash checks in `validate()`. Hash charset `[0-9a-fA-F]{64}`.

- [ ] **Step 3:** `flock … cargo test --offline --lib compiler::project -- --test-threads=1`

---

### Task 2: Canonical tree hash

**Files:** Create `src/compiler/pkg/mod.rs`, `src/compiler/pkg/hash.rs`. Modify `src/compiler.rs`: `pub mod pkg;`

**Produces:**

```rust
pub fn canonical_tree_hash(root: &Path) -> Result<[u8; 32], String>;
pub fn hash_hex(digest: &[u8; 32]) -> String; // lowercase hex
```

Skip `target/` only if it is a **direct child of root**. Skip any directory named `.git` anywhere. Regular files only.

- [ ] Write unit tests with temp dirs (two identical trees → same hash; extra file → different).
- [ ] Implement walker + `Sha256`.
- [ ] `flock … cargo test --offline --lib compiler::pkg -- --test-threads=1`

---

### Task 3: Cache, extract `.tar.gz`, `file://` fetch

**Files:** `src/compiler/pkg/fetch.rs`, `resolve.rs`

**Produces:**

```rust
pub fn cache_dir() -> PathBuf; // COFFEE_CACHE, else XDG_CACHE_HOME/coffee, else HOME/.cache/coffee
pub fn package_cache_dir(hash_hex: &str) -> PathBuf; // cache_dir()/p/<lowercase hash>/
pub fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String>;
/// Unwrap single top-level dir so dest contains coffee.toml
pub fn unwrap_single_root(dest: &Path) -> Result<(), String>;
pub fn fetch_url_to_cache(url: &str, expected_hash: &str) -> Result<PathBuf, String>;
```

HTTP via `ureq` `get(url).call()`. `file://` read path. Extract to `cache/tmp/<uuid>/`, unwrap, hash, if mismatch `remove_dir_all` tmp **and** do not create `p/<hash>`. On match, rename tmp → `p/<hash>`. If `p/<hash>/coffee.toml` already exists, skip download.

Add to `Cargo.toml`:

```toml
ureq = "2.12"
tar = "0.4"
flate2 = "1"
```

Run `cargo fetch` / update lockfile once (network allowed for that). Tests stay `--offline`.

- [ ] Tests: build a tiny dir with `coffee.toml` + `src/x.cf`, tar.gz it, `file://` fetch with correct hash; wrong hash errors and `p/<wrong>` absent.
- [ ] Implement.
- [ ] flock lib tests for `compiler::pkg`.

---

### Task 4: Resolve graph + import roots

**Files:** `src/compiler/pkg/resolve.rs`

**Produces:**

```rust
pub struct ResolvedPkg {
    pub key: String,
    pub root: PathBuf,       // directory with coffee.toml
    pub import_root: PathBuf, // src/ if present else root
}

pub fn resolve_packages(project: &ProjectConfig) -> Result<Vec<ResolvedPkg>, String>;
```

- Path: `project.root.join(path)`; must contain `coffee.toml`.
- Url: `fetch_url_to_cache` then that dir.
- Transitive: load each dep’s `ProjectConfig::from_file` but **do not** require `build.main` for libraries — if `validate()` requires main, split `validate_package` vs `validate_root_project` so library tomls can omit main or keep default `src/main` unused.
- Cycle: track visiting keys as `(package.name, hash or canonical path)`.
- Dedup by hash (path deps: hash the tree or use canonicalized path).

If `validate()` forcing non-empty main blocks library packages, change validation: `main` required only when compiling a binary project, not when loading a dependency toml. Default main can stay; empty name still invalid.

- [ ] Unit test: A path-dep B path-dep C; `resolve_packages` returns three import roots.
- [ ] Cycle A→B→A errors.
- [ ] Implement.
- [ ] flock lib tests.

---

### Task 5: Wire `ProjectBuilder` import paths

**Files:** `src/compiler/builder.rs`

In `codegen_frontend` **and** the `par_iter` `CompilerConfig` setup:

1. `let mut config = CompilerConfig::default();`
2. Call `pkg::resolve_packages(&self.config)?` — `codegen_frontend` currently does not return `Result`. Change it to prepend paths on `ProjectBuilder::compile` / `build` entry **once**, store `Vec<PathBuf>` on the builder (`pkg_import_roots`), clone into each `CompilerConfig`.

**Order:** `pkg_import_roots` first, then existing default `import_paths` from `CompilerConfig::default()`.

Call resolve at the start of the public build method (whatever `compile_project` uses — `build` / `compile`). Surface errors as `String`.

- [ ] Integration: see Task 7.

---

### Task 6: `coffee fetch` CLI

**Files:** `src/main.rs`

Treat `fetch` like `init` (early return):

- `coffee fetch [--test-mode] [--save [name]] [url]`
- No args: resolve/fetch all url+hash deps of nearest `coffee.toml`.
- URL: fetch, print `hash = "..."` .
- `--save`: require project toml; key = `name` or fetched `[package].name`; serialize that table into toml.

Toml rewrite: parse with `toml::from_str::<toml::Value>`, set `dependencies.packages.<key>`, write with `toml::to_string_pretty` **or** append a well-formed table if pretty-print would scramble comments — prefer `toml_edit` only if pretty destroys comments badly. If adding `toml_edit` is heavy, overwrite packages section via `toml_edit::DocumentMut` (add `toml_edit` crate). **Allowed.** If lockfile fight, append commented block as last resort only if tests still parse.

`print_usage`: document `fetch`.

`init` template: comment path and url+hash examples; remove the lie that `std = "0.1.0"` is wired.

- [ ] CLI tests in `tests/pkg_fetch_tests.rs`.

---

### Task 7: Integration tests

**Files:** Create `tests/pkg_fetch_tests.rs`

Use `tests/common/mod.rs` (`compile_coffee` / unique temp / `--test-mode`). Set `COFFEE_CACHE` per test.

Cases from spec:

1. Path dep compile: root `src/main.cf` `use helper` / `use printf in libc of c`; helper package `src/helper.cf` with a public fn; `[dependencies.packages.helper] path = "../helper_pkg"` (layout as you need so `use helper` finds `helper.cf` under import_root).
2. Without packages entry, same `use helper` fails.
3. `file://` tarball + hash compile.
4. Wrong hash fails.
5. Transitive path deps.
6. `coffee fetch --save` writes url+hash.

Helper sources must typecheck (libc printf ok). Prefer `--emit-llvm` or object compile without needing a full link if simpler; if `use` expansion needs a fn, export a real function.

- [ ] `flock … cargo test --offline --test pkg_fetch_tests -- --test-threads=1`

---

### Task 8: Docs

Update package sections in `docs/en/README.md`, `docs/zh/README.md`, `docs/en/compiler.md`, `docs/zh/compiler.md` (and `IFLOW.md` if it shows `packages = "1.0"`). One CLAUDE.md bullet: packages are path XOR url+hash; `coffee fetch`; cache `COFFEE_CACHE`.

Do not claim 1.0 or that std is a package.

---

### Task 9: Full offline test subset

- [ ] `flock … cargo test --offline --test pkg_fetch_tests --lib compiler::pkg --lib compiler::project -- --test-threads=1`
- [ ] If builder tests parse dummy toml, fix `packages` type.
- Return a summary: files changed, crates added, test names, remaining gaps.
