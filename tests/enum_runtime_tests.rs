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

// match 绑定出来的 payload 要带着真实类型：之前语义分析把所有绑定都声明为 `int`，
// 绑定的 class / 枚举值一传给函数就报 `expected 'Tree', found 'int'`。

const TREE: &str = r#"
enum Tree:
    Leaf
    Node(l: Tree, r: Tree)

fn build(d: int) => Tree:
    if d == 0:
        return Tree.Leaf
    else:
        return Tree.Node(build(d - 1), build(d - 1))

fn count(t: Tree) => int:
    match t:
        Tree.Leaf => 1
        Tree.Node(l, r) => count(l) + count(r)
"#;

#[test]
fn test_recursive_enum_binding_passed_to_recursive_call() {
    let source = format!("{TREE}\nfn main() => int:\n    let t: Tree = build(3)\n    return count(t)\n");
    assert_exit_code(&source, 8).unwrap();
}

#[test]
fn test_recursive_enum_tree_is_balanced() {
    let source = format!("{TREE}\nfn main() => int:\n    let t: Tree = build(4)\n    return count(t)\n");
    assert_balanced(&source);
}

#[test]
fn test_class_payload_binding_passed_to_function() {
    let source = r#"
class K:
    v: int

enum W:
    Empty
    Has(k: K)

fn get_v(k: K) => int:
    return k.v

fn f(w: W) => int:
    match w:
        W.Empty => 0
        W.Has(k) => get_v(k)

fn main() => int:
    let k: K = K { v: 7 }
    let w: W = W.Has(k)
    return f(w)
"#;
    assert_exit_code(source, 7).unwrap();
}

#[test]
fn test_enum_payload_binding_of_enum_type_in_second_position() {
    let source = r#"
enum Chain:
    End
    Link(n: int, rest: Chain)

fn len(c: Chain) => int:
    match c:
        Chain.End => 0
        Chain.Link(n, rest) => n + len(rest)

fn main() => int:
    let c: Chain = Chain.Link(1, Chain.Link(2, Chain.Link(3, Chain.End)))
    return len(c)
"#;
    assert_exit_code(source, 6).unwrap();
}
