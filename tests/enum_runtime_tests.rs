// 枚举的运行时行为测试（运行程序并比较退出码，不只是检查能否编译）。

include!("common/mod.rs");

const SHAPE: &str = r#"
enum Shape:
    Circle(r: int)
    Rect(w: int, h: int)
    Dot

fn area(s: Shape) => int:
    match s:
        Shape.Circle(r) => r * 3
        Shape.Rect(w, h) => w * h
        Shape.Dot => 0
"#;

fn with_main(main_body: &str) -> String {
    format!("{SHAPE}\nfn main() => int:\n{main_body}\n")
}

#[test]
fn test_enum_multi_field_variant_payload() {
    assert_exit_code(&with_main("    return area(Shape.Rect(3, 5))"), 15).unwrap();
}

#[test]
fn test_enum_single_field_variant_payload() {
    assert_exit_code(&with_main("    return area(Shape.Circle(4))"), 12).unwrap();
}

#[test]
fn test_enum_unit_variant_in_payload_enum_passed_to_fn() {
    assert_exit_code(&with_main("    return area(Shape.Dot) + 7"), 7).unwrap();
}

#[test]
fn test_enum_unit_variant_in_local() {
    let body = "    let s: Shape = Shape.Dot\n    return area(s) + 9";
    assert_exit_code(&with_main(body), 9).unwrap();
}

#[test]
fn test_enum_mixed_variants_in_one_function() {
    let body = "    let a: int = area(Shape.Rect(3, 4))\n    let b: int = area(Shape.Dot)\n    return a + b";
    assert_exit_code(&with_main(body), 12).unwrap();
}

#[test]
fn test_enum_returned_from_function() {
    let source = format!(
        r#"{SHAPE}
fn make_rect() => Shape:
    return Shape.Rect(3, 5)

fn make_dot() => Shape:
    return Shape.Dot

fn main() => int:
    return area(make_rect()) + area(make_dot())
"#
    );
    assert_exit_code(&source, 15).unwrap();
}

#[test]
fn test_unit_only_enum_still_works() {
    let source = r#"
enum Color:
    Red
    Green
    Blue

fn name(c: Color) => int:
    match c:
        Color.Red => 1
        Color.Green => 2
        Color.Blue => 3

fn main() => int:
    return name(Color.Blue)
"#;
    assert_exit_code(source, 3).unwrap();
}
