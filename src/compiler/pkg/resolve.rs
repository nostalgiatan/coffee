//! Resolve package graphs to import roots.

use super::fetch::{fetch_url_to_cache, path_dep_root};
use crate::compiler::project::{PackageDep, ProjectConfig};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ResolvedPkg {
    pub key: String,
    pub root: PathBuf,
    pub import_root: PathBuf,
}

pub fn import_root_for(pkg_root: &Path) -> PathBuf {
    let src = pkg_root.join("src");
    if src.is_dir() {
        src
    } else {
        pkg_root.to_path_buf()
    }
}

fn canonical_id(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn dep_id(root: &Path, dep: &PackageDep) -> String {
    if let Some(h) = dep.hash.as_ref().filter(|s| !s.is_empty()) {
        format!("hash:{}", h.to_ascii_lowercase())
    } else if let Some(p) = dep.path.as_ref() {
        let joined = if Path::new(p).is_absolute() {
            PathBuf::from(p)
        } else {
            root.join(p)
        };
        format!("path:{}", canonical_id(&joined))
    } else {
        format!("unknown:{}", root.display())
    }
}

fn resolve_one(
    project: &ProjectConfig,
    out: &mut Vec<ResolvedPkg>,
    done: &mut HashSet<String>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    let mut entries: Vec<(String, PackageDep)> = project
        .dependencies
        .packages
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, dep) in entries {
        let id = dep_id(&project.root, &dep);
        if stack.iter().any(|s| s == &id) {
            let mut cycle = stack.clone();
            cycle.push(id);
            return Err(format!("package dependency cycle: {}", cycle.join(" -> ")));
        }
        if done.contains(&id) {
            continue;
        }

        let pkg_root = if dep.is_path() {
            path_dep_root(&project.root, &dep)?
        } else {
            let url = dep.url.as_deref().ok_or_else(|| {
                format!("package '{key}': missing url")
            })?;
            let hash = dep.hash.as_deref().ok_or_else(|| {
                format!("package '{key}': missing hash")
            })?;
            fetch_url_to_cache(url, hash)?
        };

        let toml_path = pkg_root.join("coffee.toml");
        let pkg_cfg = ProjectConfig::from_package_file(&toml_path)?;
        let visit_id = if dep.hash.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
            id.clone()
        } else {
            format!("{}@{}", pkg_cfg.package.name, canonical_id(&pkg_root))
        };

        if stack.iter().any(|s| s == &visit_id || s == &id) {
            return Err(format!(
                "package dependency cycle involving '{key}' ({visit_id})"
            ));
        }

        stack.push(id.clone());
        out.push(ResolvedPkg {
            key: key.clone(),
            root: pkg_root.clone(),
            import_root: import_root_for(&pkg_root),
        });
        resolve_one(&pkg_cfg, out, done, stack)?;
        stack.pop();
        done.insert(id);
        done.insert(visit_id);
    }
    Ok(())
}

pub fn resolve_packages(project: &ProjectConfig) -> Result<Vec<ResolvedPkg>, String> {
    let mut out = Vec::new();
    let mut done = HashSet::new();
    let mut stack = Vec::new();
    let root_id = format!(
        "{}@{}",
        project.package.name,
        canonical_id(&project.root)
    );
    stack.push(root_id);
    resolve_one(project, &mut out, &mut done, &mut stack)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn uniq(tag: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "coffee_resolve_{}_{}_{n}",
            std::process::id(),
            tag
        ))
    }

    fn write_lib(dir: &Path, name: &str, extra_toml: &str) {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("coffee.toml"),
            format!("[package]\nname = \"{name}\"\n{extra_toml}"),
        )
        .unwrap();
        fs::write(
            dir.join("src").join(format!("{name}.cf")),
            format!("fn {name}_f() => int:\n    return 1\n"),
        )
        .unwrap();
    }

    #[test]
    fn transitive_path_deps_three_roots() {
        let base = uniq("trans");
        let app = base.join("app");
        let a = base.join("a");
        let b = base.join("b");
        let c = base.join("c");
        write_lib(&c, "c", "");
        write_lib(
            &b,
            "b",
            "[dependencies.packages.c]\npath = \"../c\"\n",
        );
        write_lib(
            &a,
            "a",
            "[dependencies.packages.b]\npath = \"../b\"\n",
        );
        write_lib(
            &app,
            "app",
            "[dependencies.packages.a]\npath = \"../a\"\n",
        );

        let cfg = ProjectConfig::from_package_file(&app.join("coffee.toml")).unwrap();
        let resolved = resolve_packages(&cfg).unwrap();
        assert_eq!(
            resolved.len(),
            3,
            "{:?}",
            resolved.iter().map(|r| &r.key).collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn cycle_a_b_a_errors() {
        let base = uniq("cycle");
        let a = base.join("a");
        let b = base.join("b");
        write_lib(
            &a,
            "a",
            "[dependencies.packages.b]\npath = \"../b\"\n",
        );
        write_lib(
            &b,
            "b",
            "[dependencies.packages.a]\npath = \"../a\"\n",
        );
        let cfg = ProjectConfig::from_package_file(&a.join("coffee.toml")).unwrap();
        let err = resolve_packages(&cfg).unwrap_err();
        assert!(err.contains("cycle"), "{err}");
        let _ = fs::remove_dir_all(&base);
    }
}
