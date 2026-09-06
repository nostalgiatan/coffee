//! Compiler identity for `--version` and incremental `.o` cache.
//!
//! Cargo patch stays in `Cargo.toml`. Rebuilding `coffee` (same `0.3.9`)
//! changes `COFFEE_COMPILER_HASH` from `build.rs`, so projects rebuild.

/// Hex digits of the compiler source hash shown after `+` in `--version`.
pub const COMPILER_HASH_SUFFIX_LEN: usize = 12;

/// SHA-256 (64 hex) of the compiler tree hashed at `coffee` build time.
pub fn compiler_fingerprint() -> &'static str {
    option_env!("COFFEE_COMPILER_HASH").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// `0.3.9+` plus the first 12 hex digits of [`compiler_fingerprint`].
pub fn compiler_display_version() -> String {
    let fp = compiler_fingerprint();
    if fp.len() >= COMPILER_HASH_SUFFIX_LEN
        && fp.chars().all(|c| c.is_ascii_hexdigit())
        && fp != env!("CARGO_PKG_VERSION")
    {
        format!("{}+{}", env!("CARGO_PKG_VERSION"), &fp[..COMPILER_HASH_SUFFIX_LEN])
    } else {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_version_is_crate_version_plus_source_hash_prefix() {
        let fp = compiler_fingerprint();
        assert_eq!(fp.len(), 64, "fp={fp}");
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        let v = compiler_display_version();
        assert_eq!(
            v,
            format!("{}+{}", env!("CARGO_PKG_VERSION"), &fp[..COMPILER_HASH_SUFFIX_LEN])
        );
    }
}
