// Type checker coverage: nested lets, unary ops, method lookup.

include!("common/mod.rs");

#[test]
fn test_let_int_from_bool_is_type_error() {
    let source = r#"
fn main() => int:
    let x: int = true
    rm x
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
    assert_compile_error(source, "E100").unwrap();
}

#[test]
fn test_nested_let_in_if_is_type_error() {
    let source = r#"
fn main() => int:
    if true:
        let x: int = true
        rm x
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_unary_not_on_int_is_error() {
    let source = r#"
fn main() => int:
    if !42:
        return 1
    return 0

"#;
    assert_compile_error(source, "invalid operation").unwrap();
}

#[test]
fn test_unary_neg_on_bool_is_error() {
    let source = r#"
fn main() => int:
    let x: int = -true
    rm x
    return 0

"#;
    assert_compile_error(source, "invalid operation").unwrap();
}

#[test]
fn test_undefined_in_if_body_is_error() {
    let source = r#"
fn main() => int:
    if true:
        let x: int = missing
        rm x
    return 0

"#;
    assert_compile_error(source, "undefined").unwrap();
}

#[test]
fn test_undefined_in_while_condition_is_error() {
    let source = r#"
fn main() => int:
    while missing:
        break
    return 0

"#;
    assert_compile_error(source, "undefined").unwrap();
}

#[test]
fn test_missing_method_is_error() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

fn main() => int:
    let b: Box = Box::new(1)
    b.nope(1)
    rm b
    return 0

"#;
    assert_compile_error(source, "method").unwrap();
}

#[test]
fn test_assign_bool_to_int_is_type_error() {
    let source = r#"
fn main() => int:
    let x: int = 0
    x = true
    rm x
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_match_bool_payload_used_as_int_is_type_error() {
    let source = r#"
enum Flag:
    Some(bool)
    None

fn as_int(n: int) => int:
    return n

fn main() => int:
    let opt: Flag = Flag.Some(true)
    match opt:
        Flag.Some(x) => as_int(x)
        Flag.None => 0
    rm opt
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_match_int_payload_ok() {
    let source = r#"
enum Option:
    Some(int)
    None

fn as_int(n: int) => int:
    return n

fn main() => int:
    let opt: Option = Option.Some(42)
    match opt:
        Option.Some(x) => as_int(x)
        Option.None => 0
    rm opt
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_method_body_type_error() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

    fn bad(self) => int:
        let x: int = true
        rm x
        return 0

fn main() => int:
    let b: Box = Box::new(1)
    rm b
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_call_bool_arg_to_int_param_is_type_error() {
    let source = r#"
fn takes_int(x: int) => int:
    return x

fn main() => int:
    let n: int = takes_int(true)
    rm n
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_main_entry_bool_arg_to_int_param_is_type_error() {
    let source = r#"
fn run(n: int) => int:
    return n

main(run(true))
"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_if_int_condition_is_error() {
    let source = r#"
fn main() => int:
    if 1:
        return 1
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_unknown_class_field_is_error() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

fn main() => int:
    let b: Box = Box::new(1)
    let x: int = b.missing
    rm x
    rm b
    return 0

"#;
    assert_compile_error(source, "field").unwrap();
}

#[test]
fn test_struct_literal_unknown_field_is_error() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 1, y: 2, z: 3 }
    rm p
    return 0

"#;
    assert_compile_error(source, "field").unwrap();
}

#[test]
fn test_struct_literal_missing_field_is_error() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 1 }
    rm p
    return 0

"#;
    assert_compile_error(source, "field").unwrap();
}

#[test]
fn test_constructor_bool_arg_to_int_param_is_error() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

fn main() => int:
    let b: Box = Box::new(true)
    rm b
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_enum_variant_bool_payload_for_int_is_error() {
    let source = r#"
enum Flag:
    Some(int)
    None

fn main() => int:
    let opt: Flag = Flag.Some(true)
    rm opt
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_self_field_assign_type_mismatch() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

    fn bad(self) => int:
        self.n = true
        return 0

fn main() => int:
    let b: Box = Box::new(1)
    rm b
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_for_in_bool_array_binds_as_bool() {
    let source = r#"
fn main() => int:
    let xs: [bool; 2] = [true, false]
    for x in xs:
        if x:
            return 1
    rm xs
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_match_enum_incomplete_is_error() {
    let source = r#"
enum Color:
    Red
    Green
    Blue

fn main() => int:
    let c: Color = Color.Red
    match c:
        Color.Red => 1
        Color.Green => 2
    rm c
    return 0

"#;
    assert_compile_error(source, "missing variants").unwrap();
}

#[test]
fn test_duplicate_enum_variant_is_error() {
    let source = r#"
enum Color:
    Red
    Red

fn main() => int:
    return 0

"#;
    assert_compile_error(source, "duplicate").unwrap();
}

#[test]
fn test_break_outside_loop_is_error() {
    let source = r#"
fn main() => int:
    break
    return 0

"#;
    assert_compile_error(source, "loop").unwrap();
}

#[test]
fn test_undefined_param_type_is_error() {
    let source = r#"
fn takes_bad(x: NoSuchType) => int:
    return 0

fn main() => int:
    return 0

"#;
    assert_compile_error(source, "undefined type").unwrap();
}

#[test]
fn test_raise_int_is_error() {
    let source = r#"
fn main() => int:
    raise 1

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_nested_field_assign_type_error() {
    let source = r#"
class Inner:
    n: int

class Outer:
    inner: Inner

fn main() => int:
    let i: Inner = Inner { n: 0 }
    let o: Outer = Outer { inner: i }
    o.inner.n = true
    rm o
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_raise_undefined_name_is_error() {
    let source = r#"
fn main() => int:
    raise nope

"#;
    assert_compile_error(source, "undefined").unwrap();
}

#[test]
fn test_for_in_non_array_is_error() {
    let source = r#"
fn main() => int:
    for x in 1:
        return 0
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_malloc_pointer_as_int_compiles() {
    let source = r#"
use malloc in libc of c

fn main() => int:
    let ptr: int = malloc(8)
    rm ptr
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_printf_variadic_extra_arg_compiles() {
    let source = r#"
use printf in libc of c

fn main() => int:
    printf("%d", 1)
    printf("%d %s", 1, "x")
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_printf_bool_variadic_extra_is_error() {
    let source = r#"
use printf in libc of c

fn main() => int:
    printf("%d", true)
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_int_cast_from_bool_is_error() {
    let source = r#"
fn main() => int:
    let x: int = int(true)
    rm x
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_unknown_field_expr_stmt_is_error() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

fn main() => int:
    let b: Box = Box::new(1)
    b.missing
    rm b
    return 0

"#;
    assert_compile_error(source, "field").unwrap();
}

#[test]
fn test_zero_arg_method_call_compiles() {
    let source = r#"
class Box:
    n: int

    fn new(n: int) => Box:
        Box { n: n }

    fn ping(self) => int:
        return self.n

fn main() => int:
    let b: Box = Box::new(1)
    b.ping()
    rm b
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_tuple_pattern_on_int_is_error() {
    let source = r#"
fn main() => int:
    let n: int = 1
    match n:
        (x, y) => x
        _ => 0
    rm n
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_range_bounds_must_be_int() {
    let source = r#"
fn main() => int:
    for i in true..3:
        return 0
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_match_guard_must_be_bool() {
    let source = r#"
fn main() => int:
    let x: int = 1
    match x:
        n if 1 => 1
        _ => 0
    rm x
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_int_match_requires_catch_all() {
    let source = r#"
fn main() => int:
    let x: int = 1
    match x:
        0 => 0
        1 => 1
    rm x
    return 0

"#;
    assert_compile_error(source, "exhaustive").unwrap();
}

#[test]
fn test_raise_bool_is_error() {
    let source = r#"
fn main() => int:
    raise true

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_bool_not_assignable_to_object() {
    let source = r#"
use free in libc of c

fn main() => int:
    free(true)
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_object_not_assignable_to_int4() {
    let source = r#"
use malloc, free in libc of c

fn main() => int:
    let p: int(4)+ = malloc(8)
    free(p)
    return 0

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_empty_slice_literal_uses_annotation() {
    let source = r#"
fn main() => int:
    let xs: [int] = []
    rm xs
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_guarded_ident_is_not_exhaustive() {
    let source = r#"
fn main() => int:
    let x: int = 1
    match x:
        n if n > 0 => 1
    rm x
    return 0

"#;
    assert_compile_error(source, "exhaustive").unwrap();
}

#[test]
fn test_type_name_is_not_a_value() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let x: Box = Box
    rm x
    return 0

"#;
    assert_compile_error(source, "undefined").unwrap();
}

#[test]
fn test_raise_non_error_class_is_error() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    raise Point { x: 1, y: 2 }

"#;
    assert_compile_error(source, "type mismatch").unwrap();
}

#[test]
fn test_use_after_move_is_error() {
    let source = r#"
fn main() => int:
    let a: int = 1
    mv a b
    let c: int = a
    rm c
    rm b
    return 0

"#;
    assert_compile_error(source, "moved").unwrap();
}

#[test]
fn test_int_without_rm_compiles() {
    let source = r#"
fn main() => int:
    let x: int = 1
    return x

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_resource_assign_copy_is_error() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    let b: Box = Box { n: 0 }
    b = a
    rm a
    rm b
    return 0

"#;
    assert_compile_error(source, "mv or clone").unwrap();
}

#[test]
fn test_resource_let_copy_is_error() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    let b: Box = a
    rm a
    rm b
    return 0

"#;
    assert_compile_error(source, "mv or clone").unwrap();
}

#[test]
fn test_copy_keyword_is_error() {
    let source = r#"
fn main() => int:
    let a: int = 1
    copy a b
    rm a
    return 0

"#;
    assert_compile_error(source, "copy").unwrap();
}

#[test]
fn test_clean_out_keyword_is_error() {
    let source = r#"
fn main() => int:
    let a: int = 1
    clean out
    return 0

"#;
    assert_compile_error(source, "clean out").unwrap();
}

#[test]
fn test_resource_let_clone_expr() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    let b: Box = clone a
    rm a
    rm b
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_statement_clone_binds_target() {
    let source = r#"
class Box:
    n: int

fn main() => int:
    let a: Box = Box { n: 1 }
    clone a b
    rm a
    rm b
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_if_block_int_without_rm() {
    let source = r#"
fn main() => int:
    if true:
        let x: int = 1
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_if_block_class_without_rm() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    if true:
        let p: Point = Point { x: 1, y: 2 }
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_malloc_without_import_is_error() {
    let source = r#"
fn main() => int:
    let p: int = malloc(8)
    return 0

"#;
    assert_compile_error(source, "not a Coffee memory primitive").unwrap();
}
