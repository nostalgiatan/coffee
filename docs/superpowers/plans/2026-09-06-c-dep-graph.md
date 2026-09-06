# C dependency graph Implementation Plan

> **For agentic workers:** Implement the spec `docs/superpowers/specs/2026-09-06-c-dep-graph-design.md`. **Do not git commit.** Do not bump version. C++ is out.

**Goal:** Owned C header/library graph: satellite headers stay in-library; other installed libs become `needs`; project compile auto-generates `.cfc` only from declared `c_libraries` plus installed transitives.

**Architecture:** Pure classify/meta in `src/c/dep_graph.rs`. Clang generate in `generator.rs` uses classify instead of `isFromMainFile` alone. Project builder auto-gens into `target/cfc/`.

## Global Constraints

- Do not git commit unless the user asks.
- Do not bump `Cargo.toml` version in these tasks.
- C++ out. Persistence remains one `.cfc` per library.
- One `CSymbolTable` map; no second C-dep table.
- Tests: `flock /data/data/com.termux/files/home/coffee/.cargo-test.lock cargo test --offline --test <file> -- --test-threads=1`
- Binary invocations need `--test-mode`.
- Never silently map missing C types to `object` to finish the graph.

## File ownership (parallel wave 1 — do not edit another agent's files)

- **Agent A:** `src/c/dep_graph.rs`, `tests/c_dep_graph_tests.rs` only. `src/c/mod.rs` already has `pub mod dep_graph`.
- **Agent B:** `src/c/signature.rs`, `src/c/parser.rs`, `tests/cfc_header_cli_tests.rs` (metadata parse tests only).
- **Agent C:** `src/compiler/project.rs`, `src/compiler/builder.rs`, `src/compiler/pipeline/imports.rs`, plus a small project-mode test file `tests/c_dep_project_tests.rs` (may skip generate-on-compile until wave 2; must test name default + include_paths not used as `-L`).

Wave 2 (after A+B): `src/c/generator.rs` ownership walk + recursive `-c`; builder hook to call generate.

---

### Task A: classify + meta helpers

**Produces:**

```rust
pub fn posix_or_compiler_cut(path: &std::path::Path) -> bool;
pub fn derive_linker_candidates(header_basename: &str) -> Vec<String>; // zlib.h → ["zlib","z"] after strip lib; png.h → ["png"]
pub fn is_satellite_name(header_basename: &str, root_stem: &str) -> bool; // pngconf, png.h stem png
pub struct CfcFileMeta {
    pub version: u32,
    pub module: Option<String>,
    pub linker: Option<String>,
    pub headers: Vec<String>,
    pub needs: Vec<String>,
}
pub fn parse_cfc_meta_comments(content: &str) -> CfcFileMeta;
pub fn format_cfc_meta(meta: &CfcFileMeta) -> String; // // coffee-cfc 1\n// module: ...
```

Unknown `// coffee-cfc` keys ignored. Missing block → empty headers/needs.

Unit tests in `tests/c_dep_graph_tests.rs` without clang: posix `unistd.h` / `sys/foo.h` / `android/` optional if you add android/ prefix as posix-adjacent under `android/`; `derive_linker_candidates("zlib.h")` contains `z`; `is_satellite_name("pngconf.h","png")`; parse/format roundtrip.

---

### Task B: table fields + parse metadata

**Produces:** `CSymbolTable` gains `pub linker: String` (default same as `library`), `pub owned_headers: Vec<String>`, `pub needs: Vec<String>`. `CSymbolTable::new` sets them empty / linker=library.

`parse_cfc_content` reads `// coffee-cfc` / `// module:` / `// linker:` / `// headers:` / `// needs:` even though they are comments (do not `continue` before checking these prefixes).

Test: parse a fixture string with the metadata block; assert `owned_headers` and `needs`. Existing parse tests still pass.

---

### Task C: declare + load path + link fix

- `CLibrary.name`: `#[serde(default)]` empty means default to toml key in `ProjectConfig::validate`.
- `collect_link_options`: stop pushing `get_c_include_paths()` into `lib_paths`. Include dirs are not `-L`.
- `load_cfc_file`: also search `target_dir/cfc` (from session/project config if available; if pipeline only has import_paths, add `target/cfc` relative to cwd and config root).
- Test: toml without `name` uses key; builder/link options don't contain include path as lib path. Undeclared `use` still requires `.cfc` (existing behavior).

---

### Task D (wave 2, generator): not in wave 1
