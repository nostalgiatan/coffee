//! Canonical SHA-256 of an unpacked Coffee package tree.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Lowercase hex encoding of a 32-byte digest.
pub fn hash_hex(digest: &[u8; 32]) -> String {
    hex::encode(digest)
}

/// SHA-256 of the canonical dump of `root` (directory that contains `coffee.toml`).
///
/// Regular files only. Skip `target/` if it is a direct child of `root`.
/// Skip any directory named `.git`. Paths use `/`, sorted as UTF-8 bytes.
/// Each file is `path`, NUL, 8-byte little-endian length, then raw bytes.
pub fn canonical_tree_hash(root: &Path) -> Result<[u8; 32], String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut hasher = Sha256::new();
    for (path, bytes) in &files {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    Ok(arr)
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| {
        format!("failed to read '{}': {e}", dir.display())
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to read dir entry: {e}"))?;
        let path = entry.path();
        let name = entry.file_name();

        let meta = fs::symlink_metadata(&path).map_err(|e| {
            format!("failed to stat '{}': {e}", path.display())
        })?;

        if meta.is_dir() {
            if name == ".git" {
                continue;
            }
            if dir == root && name == "target" {
                continue;
            }
            collect_files(root, &path, out)?;
            continue;
        }

        if !meta.is_file() {
            continue;
        }

        let rel = path.strip_prefix(root).map_err(|_| {
            format!("path '{}' is not under '{}'", path.display(), root.display())
        })?;
        let rel_str = rel_to_slash(rel);
        let bytes = fs::read(&path).map_err(|e| {
            format!("failed to read '{}': {e}", path.display())
        })?;
        out.push((rel_str, bytes));
    }
    Ok(())
}

fn rel_to_slash(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_tree(tag: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "coffee_hash_{}_{}_{}",
            std::process::id(),
            tag,
            n
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_pkg(root: &Path) {
        fs::write(
            root.join("coffee.toml"),
            "[package]\nname = \"ex\"\nversion = \"0.0.1\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/x.cf"), "fn x() => int:\n    return 1\n").unwrap();
    }

    #[test]
    fn identical_trees_same_hash() {
        let a = temp_tree("a");
        let b = temp_tree("b");
        write_pkg(&a);
        write_pkg(&b);
        let ha = canonical_tree_hash(&a).unwrap();
        let hb = canonical_tree_hash(&b).unwrap();
        assert_eq!(hash_hex(&ha), hash_hex(&hb));
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn extra_file_changes_hash() {
        let a = temp_tree("a2");
        let b = temp_tree("b2");
        write_pkg(&a);
        write_pkg(&b);
        fs::write(b.join("extra.txt"), "nope").unwrap();
        let ha = canonical_tree_hash(&a).unwrap();
        let hb = canonical_tree_hash(&b).unwrap();
        assert_ne!(hash_hex(&ha), hash_hex(&hb));
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn skips_root_target_and_git_dirs() {
        let a = temp_tree("skip");
        write_pkg(&a);
        fs::create_dir_all(a.join("target")).unwrap();
        fs::write(a.join("target/junk"), "x").unwrap();
        fs::create_dir_all(a.join(".git")).unwrap();
        fs::write(a.join(".git/config"), "x").unwrap();
        let with_skip = canonical_tree_hash(&a).unwrap();
        let b = temp_tree("skip2");
        write_pkg(&b);
        let without = canonical_tree_hash(&b).unwrap();
        assert_eq!(hash_hex(&with_skip), hash_hex(&without));
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }
}
