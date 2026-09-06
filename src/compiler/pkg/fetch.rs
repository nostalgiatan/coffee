//! Download, extract `.tar.gz`, verify tree hash, cache under `$COFFEE_CACHE`.

use super::hash::{canonical_tree_hash, hash_hex};
use crate::compiler::project::PackageDep;
use flate2::read::GzDecoder;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `$COFFEE_CACHE`, else `$XDG_CACHE_HOME/coffee`, else `$HOME/.cache/coffee`.
pub fn cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("COFFEE_CACHE") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(p) = std::env::var("XDG_CACHE_HOME") {
        if !p.is_empty() {
            return PathBuf::from(p).join("coffee");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cache").join("coffee")
}

pub fn package_cache_dir(hash_hex_str: &str) -> PathBuf {
    cache_dir().join("p").join(hash_hex_str.to_ascii_lowercase())
}

pub fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("failed to create '{}': {e}", dest.display()))?;
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(dest).map_err(|e| {
        format!("unknown archive (expected gzip tarball): {e}")
    })?;
    Ok(())
}

/// If `dest` contains a single top-level directory, move its contents up.
pub fn unwrap_single_root(dest: &Path) -> Result<(), String> {
    let mut children: Vec<fs::DirEntry> = fs::read_dir(dest)
        .map_err(|e| format!("failed to read '{}': {e}", dest.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to read '{}': {e}", dest.display()))?;

    children.retain(|e| {
        let n = e.file_name();
        n != "." && n != ".."
    });

    if children.len() != 1 {
        return Ok(());
    }
    let inner = children.into_iter().next().unwrap().path();
    if !inner.is_dir() {
        return Ok(());
    }
    if dest.join("coffee.toml").is_file() {
        return Ok(());
    }

    let staging = dest.join(".coffee-unwrap-tmp");
    if staging.exists() {
        fs::remove_dir_all(&staging).ok();
    }
    fs::rename(&inner, &staging).map_err(|e| {
        format!("failed to unwrap '{}': {e}", inner.display())
    })?;

    let staged_children: Vec<fs::DirEntry> = fs::read_dir(&staging)
        .map_err(|e| format!("failed to read unwrap dir: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to read unwrap dir: {e}"))?;

    for child in staged_children {
        let name = child.file_name();
        let to = dest.join(&name);
        fs::rename(child.path(), &to).map_err(|e| {
            format!("failed to move '{}' up: {e}", name.to_string_lossy())
        })?;
    }
    fs::remove_dir_all(&staging).ok();
    Ok(())
}

fn file_url_path(url: &str) -> Result<PathBuf, String> {
    let rest = url
        .strip_prefix("file://")
        .ok_or_else(|| format!("not a file URL: {url}"))?;
    let path = if rest.starts_with('/') {
        PathBuf::from(rest)
    } else if let Some(stripped) = rest.strip_prefix("localhost/") {
        PathBuf::from(format!("/{stripped}"))
    } else {
        PathBuf::from(rest)
    };
    if !path.exists() {
        return Err(format!("file URL not found: {url}"));
    }
    Ok(path)
}

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    if url.starts_with("file://") {
        let path = file_url_path(url)?;
        return fs::read(&path).map_err(|e| format!("failed to read '{}': {e}", path.display()));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("unsupported URL scheme: {url}"));
    }

    let resp = ureq::get(url).call().map_err(|e| format!("HTTP request failed: {e}"))?;
    let status = resp.status();
    if status != 200 {
        return Err(format!("HTTP {status} fetching {url}"));
    }
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("failed to read HTTP body: {e}"))?;
    Ok(bytes)
}

fn tmp_extract_dir() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    cache_dir().join("tmp").join(format!("{}-{n}", std::process::id()))
}

/// Fetch `url` (`.tar.gz`), extract, hash the tree. If `expected_hash` is set, it must match.
/// Returns the cache directory that contains `coffee.toml` and the lowercase hash.
pub fn fetch_url(url: &str, expected_hash: Option<&str>) -> Result<(PathBuf, String), String> {
    let expected = expected_hash.map(|h| h.to_ascii_lowercase());
    if let Some(h) = &expected {
        let cached = package_cache_dir(h);
        if cached.join("coffee.toml").is_file() {
            return Ok((cached, h.clone()));
        }
        if cached.exists() {
            fs::remove_dir_all(&cached).ok();
        }
    }

    let bytes = download_bytes(url)?;
    let tmp = tmp_extract_dir();
    if tmp.exists() {
        fs::remove_dir_all(&tmp).ok();
    }
    if let Err(e) = extract_tar_gz(&bytes, &tmp) {
        fs::remove_dir_all(&tmp).ok();
        return Err(e);
    }
    if let Err(e) = unwrap_single_root(&tmp) {
        fs::remove_dir_all(&tmp).ok();
        return Err(e);
    }

    let digest = match canonical_tree_hash(&tmp) {
        Ok(d) => d,
        Err(e) => {
            fs::remove_dir_all(&tmp).ok();
            return Err(e);
        }
    };
    let actual = hash_hex(&digest);

    if let Some(exp) = &expected {
        if actual != *exp {
            fs::remove_dir_all(&tmp).ok();
            let bad = package_cache_dir(exp);
            if bad.exists() {
                fs::remove_dir_all(&bad).ok();
            }
            return Err(format!(
                "package hash mismatch: expected {exp}, got {actual}"
            ));
        }
    }

    if !tmp.join("coffee.toml").is_file() {
        fs::remove_dir_all(&tmp).ok();
        return Err("extracted archive has no coffee.toml at package root".to_string());
    }

    let dest = package_cache_dir(&actual);
    if dest.join("coffee.toml").is_file() {
        fs::remove_dir_all(&tmp).ok();
        return Ok((dest, actual));
    }
    if dest.exists() {
        fs::remove_dir_all(&dest).ok();
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            fs::remove_dir_all(&tmp).ok();
            format!("failed to create cache dir: {e}")
        })?;
    }
    fs::rename(&tmp, &dest).map_err(|e| {
        fs::remove_dir_all(&tmp).ok();
        format!("failed to move cache dir: {e}")
    })?;
    Ok((dest, actual))
}

pub fn fetch_url_to_cache(url: &str, expected_hash: &str) -> Result<PathBuf, String> {
    fetch_url(url, Some(expected_hash)).map(|(p, _)| p)
}

/// Insert or update `[dependencies.packages.<key>]` with url+hash, preserving comments.
pub fn save_dep_to_toml(
    toml_path: &Path,
    key: &str,
    url: &str,
    hash: &str,
    force: bool,
) -> Result<(), String> {
    let content = fs::read_to_string(toml_path).map_err(|e| {
        format!("failed to read '{}': {e}", toml_path.display())
    })?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("failed to parse '{}': {e}", toml_path.display()))?;

    let existing_hash = doc
        .get("dependencies")
        .and_then(|d| d.get("packages"))
        .and_then(|p| p.get(key))
        .and_then(|e| e.get("hash"))
        .and_then(|h| h.as_str())
        .map(|s| s.to_ascii_lowercase());

    if let Some(old) = existing_hash {
        if old != hash.to_ascii_lowercase() && !force {
            return Err(format!(
                "package '{key}' already exists with a different hash; pass --force to overwrite"
            ));
        }
    }

    if !doc.contains_key("dependencies") {
        doc["dependencies"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let deps = doc["dependencies"]
        .as_table_mut()
        .ok_or_else(|| "dependencies is not a table".to_string())?;
    if !deps.contains_key("packages") {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        deps["packages"] = toml_edit::Item::Table(t);
    }
    let packages = deps["packages"]
        .as_table_mut()
        .ok_or_else(|| "dependencies.packages is not a table".to_string())?;

    let mut entry = toml_edit::Table::new();
    entry["url"] = toml_edit::value(url);
    entry["hash"] = toml_edit::value(hash.to_ascii_lowercase());
    packages[key] = toml_edit::Item::Table(entry);

    let mut out = fs::File::create(toml_path).map_err(|e| {
        format!("failed to write '{}': {e}", toml_path.display())
    })?;
    write!(out, "{doc}").map_err(|e| format!("failed to write '{}': {e}", toml_path.display()))?;
    Ok(())
}

/// Fetch every url+hash dep of `config` (path deps only checked for existence).
pub fn fetch_project_deps(config: &crate::compiler::project::ProjectConfig) -> Result<(), String> {
    super::resolve::resolve_packages(config).map(|_| ())
}

pub fn path_dep_root(project_root: &Path, dep: &PackageDep) -> Result<PathBuf, String> {
    let rel = dep
        .path
        .as_ref()
        .ok_or_else(|| "not a path dependency".to_string())?;
    let root = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        project_root.join(rel)
    };
    if !root.is_dir() {
        return Err(format!("path dependency not found: {}", root.display()));
    }
    if !root.join("coffee.toml").is_file() {
        return Err(format!(
            "path dependency has no coffee.toml: {}",
            root.display()
        ));
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_cache() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("coffee_cache_{}_{n}", std::process::id()))
    }

    fn write_pkg_tree(root: &Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("coffee.toml"),
            "[package]\nname = \"xpkg\"\nversion = \"0.0.1\"\n",
        )
        .unwrap();
        fs::write(root.join("src/x.cf"), "fn x() => int:\n    return 1\n").unwrap();
    }

    fn make_tar_gz(src_dir: &Path, tar_path: &Path, wrap_name: &str) {
        if let Some(p) = tar_path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let f = fs::File::create(tar_path).unwrap();
        let enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        builder.append_dir_all(wrap_name, src_dir).unwrap();
        builder.finish().unwrap();
    }

    fn set_cache(cache: &Path) {
        unsafe {
            std::env::set_var("COFFEE_CACHE", cache);
        }
    }

    #[test]
    fn file_url_fetch_matches_hash() {
        let cache = unique_cache();
        set_cache(&cache);
        let src = unique_cache().join("srcpkg");
        write_pkg_tree(&src);
        let expected = hash_hex(&canonical_tree_hash(&src).unwrap());
        let tar = unique_cache().join("x.tar.gz");
        make_tar_gz(&src, &tar, "wrap");
        let url = format!("file://{}", tar.display());
        let dest = fetch_url_to_cache(&url, &expected).unwrap();
        assert!(dest.join("coffee.toml").is_file());
        assert_eq!(dest, package_cache_dir(&expected));
        let _ = fs::remove_dir_all(&cache);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(tar.parent().unwrap());
    }

    #[test]
    fn wrong_hash_leaves_no_cache_dir() {
        let cache = unique_cache();
        set_cache(&cache);
        let src = unique_cache().join("srcpkg2");
        write_pkg_tree(&src);
        let tar = unique_cache().join("y.tar.gz");
        make_tar_gz(&src, &tar, "wrap");
        let url = format!("file://{}", tar.display());
        let wrong = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let err = fetch_url_to_cache(&url, wrong).unwrap_err();
        assert!(err.contains("hash mismatch"), "{err}");
        assert!(!package_cache_dir(wrong).exists());
        let _ = fs::remove_dir_all(&cache);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_file(&tar);
    }

    #[test]
    fn save_dep_writes_url_and_hash() {
        let dir = unique_cache();
        fs::create_dir_all(&dir).unwrap();
        let toml_path = dir.join("coffee.toml");
        fs::write(
            &toml_path,
            "[package]\nname = \"app\"\n\n[build]\nmain = \"src/main\"\n",
        )
        .unwrap();
        let h = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        save_dep_to_toml(&toml_path, "xpkg", "file:///tmp/x.tar.gz", h, false).unwrap();
        let text = fs::read_to_string(&toml_path).unwrap();
        assert!(text.contains("xpkg"));
        assert!(text.contains(h));
        assert!(text.contains("file:///tmp/x.tar.gz"));
        let _ = fs::remove_dir_all(&dir);
    }
}
