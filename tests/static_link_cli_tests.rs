include!("common/mod.rs");

fn tiny_program() -> &'static str {
    r#"
fn main() => int:
    return 0
"#
}

fn coffee_cmd() -> Command {
    let cwd = std::env::current_dir().expect("cwd");
    let mut cmd = Command::new(coffee_bin(&cwd));
    cmd.arg("--test-mode");
    cmd
}

/// `--static-lib` with no following token is ignored (parse loop only
/// consumes a name when `i < args.len()`). Trailing after a `.cf` source
/// must not steal the input file or fail the compile.
#[test]
fn test_static_lib_without_name_is_ignored() {
    let id = unique_temp_id();
    let src = format!("static_lib_no_name_{}.cf", id);
    fs::write(&src, tiny_program()).expect("write source");
    let out_ll = format!("static_lib_no_name_{}.ll", id);

    let output = coffee_cmd()
        .arg("--emit-llvm")
        .arg("-o")
        .arg(&out_ll)
        .arg(&src)
        .arg("--static-lib")
        .output()
        .expect("run coffee");

    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&out_ll);

    let r = test_result_from_output(&output);
    assert_eq!(
        r.exit_code, 0,
        "trailing --static-lib should be ignored; stderr={}",
        r.stderr
    );
}

/// Bare `--static-lib` (no source, no name) still fails for missing input,
/// same as any other incomplete argv — not a special static-lib error.
#[test]
fn test_static_lib_alone_exits_nonzero() {
    let output = coffee_cmd()
        .arg("--static-lib")
        .output()
        .expect("run coffee");
    let r = test_result_from_output(&output);
    assert_ne!(r.exit_code, 0, "expected failure with no input file");
}

#[test]
fn test_bin_static_does_not_require_android_fully_static_exe() {
    let r = compile_coffee(tiny_program(), &["--bin", "--static"]).unwrap();
    if r.exit_code == 0 {
        return;
    }
    if cfg!(target_os = "android") {
        return;
    }
    let hay = format!("{}\n{}", r.stderr, r.stdout).to_lowercase();
    assert!(
        hay.contains("static") || hay.contains("link"),
        "non-android --bin --static failure should mention static/link: {}",
        hay
    );
}

/// `--static-lib` followed by a dummy name is still parsed (not treated as
/// a source path). `--emit-llvm` does not link, so a missing lib is fine.
#[test]
fn test_static_lib_dummy_name_still_parses() {
    let id = unique_temp_id();
    let src = format!("static_lib_dummy_{}.cf", id);
    fs::write(&src, tiny_program()).expect("write source");
    let out_ll = format!("static_lib_dummy_{}.ll", id);

    let output = coffee_cmd()
        .arg("--emit-llvm")
        .arg("-o")
        .arg(&out_ll)
        .arg("--static-lib")
        .arg("not_a_real_static_lib")
        .arg(&src)
        .output()
        .expect("run coffee");

    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&out_ll);

    let r = test_result_from_output(&output);
    assert_eq!(
        r.exit_code, 0,
        "--static-lib dummyname should parse; stderr={}",
        r.stderr
    );
}

/// `--bin --static` on Android is allowed to fail when clang cannot fully
/// statically link libc (`-lc`). Success is also acceptable.
#[test]
fn test_static_on_android_may_fail_on_lc() {
    let r = compile_coffee(tiny_program(), &["--bin", "--static"]).unwrap();
    if r.exit_code == 0 {
        return;
    }
    let hay = format!("{}\n{}", r.stderr, r.stdout).to_lowercase();
    if cfg!(target_os = "android") {
        assert!(
            hay.contains("-lc")
                || hay.contains("libc")
                || hay.contains("static")
                || hay.contains("link"),
            "android --bin --static failure may mention -lc/static/link: {}",
            hay
        );
        return;
    }
    assert!(
        hay.contains("static") || hay.contains("link"),
        "non-android --bin --static failure should mention static/link: {}",
        hay
    );
}

#[test]
fn test_link_missing_object_file_errors_with_path() {
    let missing = format!("missing_link_{}.o", unique_temp_id());
    let output = coffee_cmd()
        .arg("--link")
        .arg(&missing)
        .output()
        .expect("run coffee --link");
    let r = test_result_from_output(&output);
    assert_ne!(r.exit_code, 0);
    let hay = format!("{}\n{}", r.stderr, r.stdout);
    assert!(
        hay.contains(&missing),
        "--link missing .o should include the path:\n{hay}"
    );
    assert!(
        hay.contains(".o") && (hay.contains("hint") || hay.contains("compile")),
        "--link missing .o should hint how to produce an object:\n{hay}"
    );
}

/// A present but invalid `.o` must still print clang's stderr (not a silent exit 1).
#[test]
fn test_link_invalid_object_prints_clang_stderr() {
    let obj = format!("bad_link_{}.o", unique_temp_id());
    fs::write(&obj, b"not a real object file").expect("write dummy .o");
    let output = coffee_cmd()
        .arg("--link")
        .arg(&obj)
        .output()
        .expect("run coffee --link");
    let _ = fs::remove_file(&obj);
    let r = test_result_from_output(&output);
    assert_ne!(r.exit_code, 0);
    let hay = format!("{}\n{}", r.stderr, r.stdout);
    assert!(
        hay.contains(&obj) || hay.to_lowercase().contains("link") || hay.to_lowercase().contains("error"),
        "--link clang failure should print linker output:\n{hay}"
    );
    assert!(
        !hay.trim().is_empty(),
        "--link clang failure must not be a silent exit 1"
    );
}

fn elf_e_type(path: &str) -> u16 {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    assert!(
        bytes.len() >= 18 && bytes.starts_with(b"\x7fELF"),
        "{path} is not ELF ({} bytes)",
        bytes.len()
    );
    u16::from_le_bytes([bytes[16], bytes[17]])
}

/// `--bin -o foo.bin` must write the linked executable at `foo.bin`, not a
/// relocatable `.o` there while linking to `foo` (`with_extension("")`).
#[test]
fn test_bin_dash_o_writes_executable_at_that_path() {
    let id = unique_temp_id();
    let src = format!("bin_dash_o_{id}.cf");
    let exe = format!("bin_dash_o_{id}.bin");
    fs::write(&src, tiny_program()).expect("write source");

    let output = coffee_cmd()
        .arg("--bin")
        .arg("-o")
        .arg(&exe)
        .arg(&src)
        .output()
        .expect("run coffee --bin -o");

    let r = test_result_from_output(&output);
    let hay = format!("{}\n{}", r.stdout, r.stderr);
    let exe_exists = std::path::Path::new(&exe).exists();
    let e_type = if exe_exists { Some(elf_e_type(&exe)) } else { None };
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(format!("{exe}.o"));
    let stem_exe = exe.trim_end_matches(".bin");
    let _ = fs::remove_file(stem_exe);

    assert_eq!(r.exit_code, 0, "--bin -o should succeed:\n{hay}");
    assert!(exe_exists, "--bin -o {exe} must create that path:\n{hay}");
    let e_type = e_type.expect("elf type");
    assert!(
        e_type == 2 || e_type == 3,
        "--bin -o must be ET_EXEC/ET_DYN at {exe}, got e_type={e_type} (ET_REL=1):\n{hay}"
    );
    assert!(
        hay.contains(&exe),
        "success message should name the -o path:\n{hay}"
    );
}
