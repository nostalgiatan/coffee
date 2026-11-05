// The self-hosted semantic analyzer in `coffee/sema` is a Coffee project with its own tests;
// this runs them through `coffee test` so `cargo test` covers it too.

include!("common/mod.rs");

#[test]
fn test_coffee_sema_project_tests_pass() {
    let root = std::env::current_dir().expect("cwd");
    let project = root.join("coffee").join("sema");
    let out = Command::new(coffee_bin(&root))
        .current_dir(&project)
        .args(["test", "--test-mode", "-I", "../parser/src", "-I", "../lexer/src", "-I", "src"])
        .output()
        .expect("run coffee test");
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{}", text);
    assert!(text.contains("0 failed"), "{}", text);
}
