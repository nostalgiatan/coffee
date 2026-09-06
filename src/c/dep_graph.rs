//! C header / library ownership graph helpers (see spec 2026-09-06-c-dep-graph).
//! Classification and `.cfc` metadata; clang walking lives in `generator`.

use std::path::Path;

/// POSIX / compiler headers never owned by a third-party `.cfc`.
const POSIX_BASENAMES: &[&str] = &[
    "stdio.h",
    "stdlib.h",
    "stddef.h",
    "stdarg.h",
    "stdint.h",
    "stdbool.h",
    "stdatomic.h",
    "string.h",
    "strings.h",
    "memory.h",
    "unistd.h",
    "signal.h",
    "errno.h",
    "limits.h",
    "float.h",
    "time.h",
    "fcntl.h",
    "pthread.h",
    "wchar.h",
    "wctype.h",
    "locale.h",
    "math.h",
    "setjmp.h",
    "ctype.h",
    "assert.h",
    "features.h",
    "endian.h",
    "alloca.h",
    "dlfcn.h",
    "dirent.h",
    "poll.h",
    "sched.h",
    "ucontext.h",
];

const POSIX_DIR_COMPONENTS: &[&str] = &["bits", "linux", "asm", "sys", "asm-generic", "android"];

/// True when `path` is a libc/POSIX/compiler header that must not be emitted into another `.cfc`.
pub fn posix_or_compiler_cut(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    if path_str.contains("lib/clang") {
        return true;
    }

    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if POSIX_BASENAMES.contains(&name) {
            return true;
        }
    }

    for component in path.components() {
        if let std::path::Component::Normal(os) = component {
            if let Some(s) = os.to_str() {
                if POSIX_DIR_COMPONENTS.contains(&s) {
                    return true;
                }
            }
        }
    }

    false
}

/// Linker short-name candidates from a header basename (`zlib.h` → `zlib`, `z`).
pub fn derive_linker_candidates(header_basename: &str) -> Vec<String> {
    let stem = strip_h_suffix(header_basename);
    if stem.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    push_unique(&mut out, stem.to_string());

    if let Some(rest) = stem.strip_prefix("lib") {
        if !rest.is_empty() {
            push_unique(&mut out, rest.to_string());
        }
    }

    if stem == "zlib" || stem.strip_prefix("lib") == Some("zlib") {
        push_unique(&mut out, "z".to_string());
    }

    out
}

/// True when `header_basename` looks like a split header of `root_stem` (`png` → `pngconf.h`).
pub fn is_satellite_name(header_basename: &str, root_stem: &str) -> bool {
    if root_stem.is_empty() {
        return false;
    }
    let stem = strip_h_suffix(header_basename);
    stem != root_stem && stem.starts_with(root_stem)
}

/// Leading `// coffee-cfc` metadata on a `.cfc` file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CfcFileMeta {
    pub version: u32,
    pub module: Option<String>,
    pub linker: Option<String>,
    pub headers: Vec<String>,
    pub needs: Vec<String>,
}

/// Parse `// coffee-cfc` comment keys; unknown keys are ignored. No block → defaults.
pub fn parse_cfc_meta_comments(content: &str) -> CfcFileMeta {
    let mut meta = CfcFileMeta::default();
    for raw in content.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("//") else {
            continue;
        };
        let rest = rest.trim();

        if let Some(after) = rest.strip_prefix("coffee-cfc") {
            let after = after.trim();
            if after.is_empty() {
                continue;
            }
            // Version line: `coffee-cfc 1`. Unknown `coffee-cfc <key>:` forms are ignored.
            if after.contains(':') {
                continue;
            }
            if let Some(first) = after.split_whitespace().next() {
                if let Ok(v) = first.parse::<u32>() {
                    meta.version = v;
                }
            }
            continue;
        }

        let Some((key, value)) = rest.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "module" => meta.module = nonempty(value),
            "linker" => meta.linker = nonempty(value),
            "headers" => meta.headers = split_words(value),
            "needs" => meta.needs = split_words(value),
            _ => {}
        }
    }
    meta
}

/// Emit the metadata comment block (`// coffee-cfc 1` …).
pub fn format_cfc_meta(meta: &CfcFileMeta) -> String {
    let mut out = String::new();
    out.push_str(&format!("// coffee-cfc {}\n", meta.version));
    out.push_str(&format!(
        "// module: {}\n",
        meta.module.as_deref().unwrap_or("")
    ));
    out.push_str(&format!(
        "// linker: {}\n",
        meta.linker.as_deref().unwrap_or("")
    ));
    out.push_str(&format!("// headers: {}\n", meta.headers.join(" ")));
    out.push_str(&format!("// needs: {}\n", meta.needs.join(" ")));
    out
}

fn strip_h_suffix(basename: &str) -> &str {
    let name = basename.rsplit(['/', '\\']).next().unwrap_or(basename);
    if let Some(s) = name.strip_suffix(".h") {
        s
    } else if let Some(s) = name.strip_suffix(".H") {
        s
    } else {
        name
    }
}

fn push_unique(out: &mut Vec<String>, name: String) {
    if !out.iter().any(|e| e == &name) {
        out.push(name);
    }
}

fn nonempty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn split_words(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}
