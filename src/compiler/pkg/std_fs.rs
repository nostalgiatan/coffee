//! Embedded official `std` package: lookup, install extract, import root.

use super::resolve::import_root_for;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/std_embed.rs"));

const MISSING: &str = "standard library not installed; run coffee std install";

/// Import root (`src/` when present) of the official std package.
pub fn std_import_root() -> Result<PathBuf, String> {
    Ok(import_root_for(&std_package_root()?))
}

/// Package root (directory that contains `coffee.toml`).
///
/// Lookup order:
/// 1. `$COFFEE_STD` if it contains `coffee.toml`
/// 2. `$XDG_DATA_HOME/coffee/std`, else `$HOME/.local/share/coffee/std`
/// 3. `library/std` walking ancestors of cwd and the executable
/// 4. Err asking to run `coffee std install`
pub fn std_package_root() -> Result<PathBuf, String> {
    if let Ok(p) = env::var("COFFEE_STD") {
        let p = PathBuf::from(p);
        if p.join("coffee.toml").is_file() {
            return Ok(p);
        }
    }

    if let Some(p) = xdg_or_home_std_dir() {
        if p.join("coffee.toml").is_file() {
            return Ok(p);
        }
    }

    if let Some(p) = find_dev_library_std() {
        return Ok(p);
    }

    Err(MISSING.to_string())
}

/// Default extract destination for `coffee std install` (no `[dir]` argument).
pub fn default_std_install_dir() -> Result<PathBuf, String> {
    if let Ok(p) = env::var("COFFEE_STD") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    if let Some(p) = xdg_or_home_std_dir() {
        return Ok(p);
    }
    Err("cannot determine std install directory (set COFFEE_STD or HOME)".to_string())
}

fn xdg_or_home_std_dir() -> Option<PathBuf> {
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("coffee").join("std"));
        }
    }
    let home = env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".local/share/coffee/std"))
}

fn is_dev_std_tree(dir: &Path) -> bool {
    dir.join("coffee.toml").is_file() && dir.join("src").join("std.cf").is_file()
}

fn walk_ancestors_for_library_std(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let candidate = dir.join("library").join("std");
        if is_dev_std_tree(&candidate) {
            return Some(candidate);
        }
        cur = dir.parent();
    }
    None
}

fn find_dev_library_std() -> Option<PathBuf> {
    if let Ok(cwd) = env::current_dir() {
        if let Some(p) = walk_ancestors_for_library_std(&cwd) {
            return Some(p);
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(p) = walk_ancestors_for_library_std(parent) {
                return Some(p);
            }
        }
    }
    None
}

/// Write the embedded `library/std` tree under `dest` (package root).
pub fn extract_embedded_std(dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for (rel, bytes) in embedded_std_files() {
        let rel_path = Path::new(rel);
        if rel_path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(format!("embedded std path escapes dest: {rel}"));
        }
        let out = dest.join(rel_path);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}
