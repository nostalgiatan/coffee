# Package fetch (Zig-style, no registry)

**Status:** implement now (user: finish package deps; std is a separate conversation).  
**Not in this spec:** standard library contents, `build.cf`, self-hosting, crates.io-style registry, git clone URLs.

## Goal

Coffee projects declare **content-addressed** dependencies in `coffee.toml`. Identity is a **SHA-256 of the unpacked file tree**, not a version string and not a URL. The `coffee` binary (Rust) fetches, verifies, caches, and puts package roots on the import path. No central index.

## Why this and not Cargo

Cargo needs a registry + semver. Zig’s `build.zig.zon` uses `url` + `hash` (path XOR url+hash). Coffee already has unused `[dependencies.packages]` as `name = "1.0.0"` strings; that shape is wrong for a no-registry world.

## Package identity

- **Hash:** SHA-256 hex of a canonical tree dump of the **extracted** package (not the download bytes).
- **Canonical dump:**
  - Root is the directory that contains `coffee.toml` (if the archive has a single top-level folder, unwrap it).
  - Walk regular files only; skip `target/` at the package root and any `.git/` directory.
  - Paths: relative to that root, `/` separators, sorted lexicographically as UTF-8 bytes.
  - For each file, append: `path`, NUL, 8-byte little-endian length, then raw bytes.
- Two archives that unpack to the same files have the same hash. Recompression does not change identity.
- **URL is a mirror.** Changing the URL with the same hash is allowed; the hash must still match after fetch.

## `coffee.toml` shape

Replace `packages: HashMap<String, String>` with `HashMap<String, PackageDep>`.

```toml
[dependencies.packages.foo]
url = "https://example.com/foo.tar.gz"
hash = "0123...64 hex chars..."

[dependencies.packages.local_bar]
path = "../bar"
```

**XOR:** each entry has either `path` or (`url` **and** `hash`). Error if both, or url without hash, or hash without url, or neither.

- **Key** (`foo`) is the import prefix used by this project (`use foo` / `use foo.mod`).
- **`path`:** directory containing that package’s `coffee.toml`, relative to **this** project root. Resolved at compile time; not copied into cache. No hash required (like Zig `.path`).
- **`url`:** `https://`, `http://`, or `file://` pointing at a `.tar.gz` (gzip tarball only for this slice).
- **`hash`:** 64 hex digits (SHA-256 of canonical tree). Case-insensitive on input; store lowercase.

Keep `[dependencies.c_libraries]` unchanged. Leave `[dependencies.std]` deserialized and **unread** (std is not a fetched package in this work).

`coffee init` template: comment examples for `path` and `url`+`hash`, not `std = "0.1.0"` as if it did something.

## Cache

- Default: `$COFFEE_CACHE` if set, else `$XDG_CACHE_HOME/coffee` if set, else `~/.cache/coffee` (Termux: `$HOME/.cache/coffee`).
- Extracted tree: `<cache>/p/<hash>/` where `<hash>` is lowercase hex. Presence of `<cache>/p/<hash>/coffee.toml` means the dep is ready.
- Downloads may land in `<cache>/tmp/` then rename after verify. Never leave a hash directory that failed verification.

Tests must set `COFFEE_CACHE` to a temp dir so they do not touch the user cache.

## Commands

### `coffee fetch`

- `coffee fetch` (project mode): for every `url`+`hash` dep in the nearest `coffee.toml`, ensure cache, fetch if missing, verify hash. Path deps are checked to exist; not copied.
- `coffee fetch <url> --save [name]`: download, compute hash, extract to cache, append/update `[dependencies.packages.<name>]` in the project `coffee.toml`. `name` defaults to `[package].name` inside the fetched tree. Fails if that key already exists with a different hash unless `--force`.
- `--test-mode` still skips the process lock (existing rule).

No network in unit tests: use `file://` tarballs and `path` deps.

### Compile (project mode)

Before scheduling units:

1. Load `coffee.toml`.
2. For each package dep: resolve to a **package root** (path as-is, or cache dir for url+hash).
3. If url+hash is missing from cache: **fetch it** (same as `coffee fetch` for that entry). Offline + missing cache → error naming the hash and `coffee fetch`.
4. Transitive: read each package’s `coffee.toml` `[dependencies.packages]` and fetch/resolve those too. Cycle = error listing hashes/keys.
5. Add each package’s **import root** to `CompilerConfig.import_paths` **before** generic `lib`/`std`/`.` so a dep named `foo` is found as `<pkg-root>/foo.cf` or `<pkg-root>/src/foo.cf` via existing dotted path rules.

**Import root:** if `<pkg>/src/` exists, use that; else use `<pkg>/`. Same as project `src_dir` default.

**Single-file mode** (`no coffee.toml`): no package fetch. Existing filesystem `use` only.

Do not scan fetched packages into the project’s own `src/**` compile graph. Only the importing module graph (`use`) pulls them in, as today.

## Layout of a Coffee package

A package is a directory with `coffee.toml` (`[package] name` required) and Coffee sources. It may have its own `[dependencies.packages]`. It does not need `fn main`; packages are libraries. `ProjectBuilder` still only requires `main` for the **root** project being built.

## Errors (user-facing)

- Path dep missing / no `coffee.toml`.
- url without hash / hash without url / both path and url.
- Hash mismatch after unpack: show expected vs actual; delete bad cache dir.
- Unknown archive (not gzip tar).
- HTTP non-200 / file URL not found.
- Transitive cycle.

## Implementation home

- New: `src/compiler/pkg/` (`dep.rs` types, `hash.rs` canonical hash, `fetch.rs` download+extract, `resolve.rs` cache + import roots).
- Wire: `ProjectConfig` serde, `ProjectBuilder` / `CompilerConfig` import paths, `main.rs` `fetch` subcommand.
- HTTP: `ureq` (blocking). Tarball: `tar` + `flate2`. Hash: existing `sha2` + `hex`.
- Do **not** bump `Cargo.toml` package version unless the parent session asks.
- Do **not** git commit unless asked.

## Tests

Integration tests under `tests/pkg_fetch_tests.rs` (and unit tests in `src/compiler/pkg/`):

1. Path dep: project A `use helper`; helper is a sibling dir with `coffee.toml`; compile succeeds; helper not on default import_paths without the toml entry.
2. XOR validation: packages as string `"1.0"` still parse-fail or migrate: **reject** old `name = "1.0.0"` string values with a clear error (breaking: unused today).
3. Canonical hash stable for two copies of the same tree.
4. `file://` tarball fetch into `COFFEE_CACHE`; hash match; compile `use`.
5. Wrong hash → error, no usable cache dir left.
6. Transitive path deps (A → B → C).
7. `coffee fetch --save` writes url+hash into toml (file URL).

## Out of scope

- Git URLs, zip, tar.xz.
- Registry, semver ranges, lockfile besides hashes in toml.
- `build.cf`.
- Shipping or fetching `std`.
- Compiling package tests / multiple bins.
