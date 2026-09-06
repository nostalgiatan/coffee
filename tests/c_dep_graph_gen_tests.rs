// Wave-2 `coffee -c` / generate_cfc header ownership (satellites vs other libs).

include!("common/mod.rs");

use coffee::c::dep_graph::parse_cfc_meta_comments;
use coffee::c::generator::generate_cfc;
use std::path::Path;

fn cleanup(paths: &[&str]) {
    for p in paths {
        let _ = fs::remove_file(p);
    }
}

#[test]
fn generate_cfc_keeps_satellite_header_and_splits_other_lib() {
    let id = unique_temp_id();
    let a = format!("ow_{id}_a.h");
    let extra = format!("ow_{id}_a_extra.h");
    let b = format!("ow_{id}_b.h");
    let lib_a = format!("libow_{id}_a.cfc");
    let lib_b = format!("libow_{id}_b.cfc");
    let stub_so = format!("libow_{id}_b.so");

    fs::write(&extra, "int keep_extra(int x);\n").expect("write extra");
    fs::write(&b, "int b_only(int y);\n").expect("write b.h");
    fs::write(
        &a,
        format!("#include \"{extra}\"\n#include \"{b}\"\nint keep_a(int z);\n"),
    )
    .expect("write a.h");
    fs::write(&stub_so, []).expect("stub libb.so for find_library_file");

    let result = generate_cfc(&a, Some(&lib_a), &[".".to_string()]);
    let a_text = fs::read_to_string(&lib_a).ok();
    let b_text = fs::read_to_string(&lib_b).ok();
    cleanup(&[&a, &extra, &b, &lib_a, &lib_b, &stub_so]);
    let _ = fs::remove_file(format!("cfc/libow_{id}_b.cfc"));

    let out = result.expect("generate_cfc a.h");
    assert!(out.contains("ow_"), "output path: {out}");
    let content = a_text.expect("liba.cfc written");
    assert!(
        content.contains("keep_a"),
        "main-header fn:\n{content}"
    );
    assert!(
        content.contains("keep_extra"),
        "satellite extra.h fn must be kept:\n{content}"
    );
    assert!(
        !content.contains("b_only"),
        "other library fn must not be copied:\n{content}"
    );
    let meta = parse_cfc_meta_comments(&content);
    assert_eq!(meta.module.as_deref(), Some(a.trim_end_matches(".h")));
    assert!(
        meta.needs.iter().any(|n| n == b.trim_end_matches(".h")),
        "needs should include b: {:?}",
        meta.needs
    );
    assert!(
        meta.headers.iter().any(|h| h == extra.split('/').next_back().unwrap() || h == extra.as_str()),
        "owned headers should include extra: {:?}",
        meta.headers
    );
    let b_cfc = b_text.expect("recursive libb.cfc should be auto-created");
    assert!(
        b_cfc.contains("b_only"),
        "recursive libb.cfc should own b_only:\n{b_cfc}"
    );
    assert!(
        !b_cfc.contains("keep_a"),
        "libb.cfc must not copy a:\n{b_cfc}"
    );
}

#[test]
fn generate_cfc_zlib_h_module_is_z() {
    let id = unique_temp_id();
    let dir = std::env::temp_dir().join(format!("coffee_zlib_cfc_{id}"));
    fs::create_dir_all(&dir).expect("mkdir");
    let header = dir.join("zlib.h");
    let out = dir.join("libz.cfc");
    fs::write(&header, "int zlibVersion(void);\n").expect("write zlib.h");

    let result = generate_cfc(&header, Some(&out), &[]);
    let text = fs::read_to_string(&out).ok();
    let _ = fs::remove_dir_all(&dir);

    result.expect("generate_cfc zlib.h");
    let content = text.expect("libz.cfc");
    let meta = parse_cfc_meta_comments(&content);
    assert_eq!(meta.module.as_deref(), Some("z"), "{content}");
    assert_eq!(meta.linker.as_deref(), Some("z"), "{content}");
    assert!(content.contains("zlibVersion"), "{content}");
}

#[test]
fn generate_cfc_missing_header_uses_diag() {
    let id = unique_temp_id();
    let header = format!("missing_ow_{id}.h");
    assert!(!Path::new(&header).exists());
    let err = generate_cfc(header.as_str(), None, &[]).expect_err("must fail");
    let lower = err.to_lowercase();
    assert!(
        lower.contains("not found") || lower.contains("cannot find"),
        "{err}"
    );
}

#[test]
fn generate_cfc_missing_include_is_hard_error() {
    let id = unique_temp_id();
    let header = format!("ow_missinc_{id}.h");
    let missing = format!("ow_does_not_exist_{id}.h");
    let cfc = format!("libow_missinc_{id}.cfc");
    fs::write(&header, format!("#include \"{missing}\"\nint keep(int x);\n")).expect("write");
    let err = generate_cfc(&header, Some(&cfc), &[".".to_string()]).expect_err("include must fail");
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&cfc);
    let lower = err.to_lowercase();
    assert!(
        lower.contains("ow_does_not_exist") || lower.contains("not found") || lower.contains("cannot find"),
        "{err}"
    );
}

#[test]
fn generate_cfc_existing_cfc_headers_claim_file() {
    let id = unique_temp_id();
    let a = format!("ow_claim_{id}_a.h");
    let claimed = format!("ow_claim_{id}_b.h");
    let lib_a = format!("libow_claim_{id}_a.cfc");
    let lib_b = format!("libow_claim_{id}_b.cfc");
    let module_b = format!("ow_claim_{id}_b");
    fs::write(
        &lib_b,
        format!(
            "// coffee-cfc 1\n// module: {module_b}\n// linker: {module_b}\n// headers: {claimed}\n// needs:\nc fn already_b() => void:\n"
        ),
    )
    .expect("write existing cfc");
    fs::write(&claimed, "int from_claimed(int y);\n").expect("write claimed header");
    fs::write(
        &a,
        format!("#include \"{claimed}\"\nint from_a(int z);\n"),
    )
    .expect("write a.h");

    let result = generate_cfc(&a, Some(&lib_a), &[".".to_string()]);
    let a_text = fs::read_to_string(&lib_a).ok();
    let _ = fs::remove_file(&a);
    let _ = fs::remove_file(&claimed);
    let _ = fs::remove_file(&lib_a);
    let _ = fs::remove_file(&lib_b);

    result.expect("generate_cfc");
    let content = a_text.expect("liba");
    assert!(content.contains("from_a"), "{content}");
    assert!(
        !content.contains("from_claimed"),
        "claimed header belongs to existing .cfc:\n{content}"
    );
    let meta = parse_cfc_meta_comments(&content);
    assert!(
        meta.needs.iter().any(|n| n == &module_b),
        "needs should include claimed module: {:?}",
        meta.needs
    );
}

#[test]
fn generate_cfc_skips_static_functions() {
    let id = unique_temp_id();
    let header = format!("ow_static_{id}.h");
    let cfc = format!("libow_static_{id}.cfc");
    fs::write(
        &header,
        "static int hidden_static(int x);\nint visible_export(int y);\n",
    )
    .expect("write header");
    let result = generate_cfc(&header, Some(&cfc), &[]);
    let text = fs::read_to_string(&cfc).ok();
    let _ = fs::remove_file(&header);
    let _ = fs::remove_file(&cfc);
    result.expect("generate_cfc");
    let content = text.expect(".cfc");
    assert!(
        content.contains("visible_export"),
        "exported fn:\n{content}"
    );
    assert!(
        !content.contains("hidden_static"),
        "static fn must not be in .cfc:\n{content}"
    );
}
