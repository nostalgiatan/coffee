//! Coffee package identity, fetch, cache, and import-root resolution.

pub mod hash;
pub mod fetch;
pub mod resolve;
pub mod std_fs;

pub use fetch::{
    cache_dir, extract_tar_gz, fetch_project_deps, fetch_url, fetch_url_to_cache,
    package_cache_dir, save_dep_to_toml, unwrap_single_root,
};
pub use hash::{canonical_tree_hash, hash_hex};
pub use resolve::{import_root_for, resolve_packages, ResolvedPkg};
pub use std_fs::{
    default_std_install_dir, extract_embedded_std, std_import_root, std_package_root,
};
