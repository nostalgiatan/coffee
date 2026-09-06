// Classify + `.cfc` metadata helpers (no clang).

use std::path::Path;

use coffee::c::dep_graph::{
    derive_linker_candidates, format_cfc_meta, is_satellite_name, parse_cfc_meta_comments,
    posix_or_compiler_cut, CfcFileMeta,
};

#[test]
fn posix_cut_unistd_basename() {
    assert!(posix_or_compiler_cut(Path::new("unistd.h")));
    assert!(posix_or_compiler_cut(Path::new("/usr/include/stdio.h")));
    assert!(posix_or_compiler_cut(Path::new("math.h")));
}

#[test]
fn posix_cut_sys_and_bits_prefixes() {
    assert!(posix_or_compiler_cut(Path::new("sys/foo.h")));
    assert!(posix_or_compiler_cut(Path::new("/usr/include/sys/types.h")));
    assert!(posix_or_compiler_cut(Path::new("/usr/include/bits/wordsize.h")));
    assert!(posix_or_compiler_cut(Path::new("linux/limits.h")));
    assert!(posix_or_compiler_cut(Path::new("asm/unistd.h")));
    assert!(posix_or_compiler_cut(Path::new("asm-generic/int-ll64.h")));
    assert!(posix_or_compiler_cut(Path::new("android/log.h")));
}

#[test]
fn posix_cut_clang_resource_dir() {
    assert!(posix_or_compiler_cut(Path::new(
        "/data/data/com.termux/files/usr/lib/clang/21/include/stddef.h"
    )));
}

#[test]
fn posix_cut_false_for_zlib_and_png() {
    assert!(!posix_or_compiler_cut(Path::new("zlib.h")));
    assert!(!posix_or_compiler_cut(Path::new("/usr/include/zlib.h")));
    assert!(!posix_or_compiler_cut(Path::new("/usr/include/png.h")));
    assert!(!posix_or_compiler_cut(Path::new("/usr/include/pngconf.h")));
}

#[test]
fn derive_linker_zlib_includes_z() {
    let names = derive_linker_candidates("zlib.h");
    assert!(names.contains(&"zlib".to_string()), "{names:?}");
    assert!(names.contains(&"z".to_string()), "{names:?}");
}

#[test]
fn derive_linker_png_and_libpng() {
    assert_eq!(derive_linker_candidates("png.h"), vec!["png".to_string()]);
    let libpng = derive_linker_candidates("libpng.h");
    assert!(libpng.contains(&"png".to_string()), "{libpng:?}");
}

#[test]
fn satellite_pngconf_of_png() {
    assert!(is_satellite_name("pngconf.h", "png"));
    assert!(is_satellite_name("pnglibconf.h", "png"));
    assert!(!is_satellite_name("unistd.h", "zlib"));
    assert!(!is_satellite_name("png.h", "z"));
}

#[test]
fn parse_missing_block_is_default() {
    let meta = parse_cfc_meta_comments("c fn foo() => int\n");
    assert_eq!(meta, CfcFileMeta::default());
    assert!(meta.headers.is_empty());
    assert!(meta.needs.is_empty());
}

#[test]
fn parse_known_block_and_ignore_unknown_keys() {
    let src = r#"
// coffee-cfc 1
// module: z
// linker: z
// headers: zlib.h zconf.h
// extra: ignored
// needs: libc
c fn zlibVersion() => str
"#;
    let meta = parse_cfc_meta_comments(src);
    assert_eq!(meta.version, 1);
    assert_eq!(meta.module.as_deref(), Some("z"));
    assert_eq!(meta.linker.as_deref(), Some("z"));
    assert_eq!(
        meta.headers,
        vec!["zlib.h".to_string(), "zconf.h".to_string()]
    );
    assert_eq!(meta.needs, vec!["libc".to_string()]);
}

#[test]
fn format_roundtrip() {
    let meta = CfcFileMeta {
        version: 1,
        module: Some("z".into()),
        linker: Some("z".into()),
        headers: vec!["zlib.h".into(), "zconf.h".into()],
        needs: vec!["libc".into()],
    };
    let text = format_cfc_meta(&meta);
    assert!(text.starts_with("// coffee-cfc 1\n"), "{text}");
    assert!(text.contains("// module: z\n"), "{text}");
    assert!(text.contains("// linker: z\n"), "{text}");
    assert!(text.contains("// headers: zlib.h zconf.h\n"), "{text}");
    assert!(text.contains("// needs: libc\n"), "{text}");
    assert_eq!(parse_cfc_meta_comments(&text), meta);
}
