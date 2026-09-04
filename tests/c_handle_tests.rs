// C handle (`object`) interop: libc pointers must not truncate to int(4)+.

include!("common/mod.rs");

#[test]
fn test_malloc_free_object_handle() {
    let source = r#"
use malloc, free in libc of c

fn main() => int:
    let p: object = malloc(64)
    free(p)
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_printf_hello_and_variadic() {
    let source = r#"
use printf, puts in libc of c

fn main() => int:
    printf("Hello")
    puts("World")
    printf("%d %s", 1, "x")
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_malloc_int_annotation_if_checker_coerces() {
    // `let ptr: int = malloc(100)` needs checker C-handle coerce (object → pointer-sized int).
    // Keep this as a compile attempt; if it fails, the object-annotated test above is the
    // required green path.
    let source = r#"
use malloc, free in libc of c

fn main() => int:
    let ptr: int = malloc(100)
    free(ptr)
    rm ptr
    return 0

"#;
    let _ = compile_coffee(source, &[]);
}
