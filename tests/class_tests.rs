// Tests for class definitions and usage

include!("common/mod.rs");

#[test]
fn test_simple_class_declaration() {
    let source = r#"
class Point:
    x: int
    y: int

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_empty_class() {
    let source = r#"
class Empty:

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_with_single_field() {
    let source = r#"
class Counter:
    count: int

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_with_multiple_fields() {
    let source = r#"
class Person:
    name: string
    age: int
    height: float

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_with_different_type_fields() {
    let source = r#"
class Mixed:
    a: int
    b: float
    c: string
    d: bool

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_struct_literal_basic() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_struct_literal_with_expressions() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let a: int = 5
    let b: int = 10
    let p: Point = Point { x: a + 3, y: b * 2 }
    printf("Point: %d, %d\n", p.x, p.y)
    rm p, a, b
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_struct_literal_nested() {
    let source = r#"
class Point:
    x: int
    y: int

class Rectangle:
    top_left: Point
    bottom_right: Point

fn main() => int:
    let r: Rectangle = Rectangle {
        top_left: Point { x: 0, y: 10 },
        bottom_right: Point { x: 20, y: 0 }
    }
    rm r
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_field_access() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let x_val: int = p.x
    let y_val: int = p.y
    printf("Point: %d, %d\n", x_val, y_val)
    rm p, x_val, y_val
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_field_access() {
    let source = r#"
class Point:
    x: int
    y: int

class Rectangle:
    top_left: Point
    bottom_right: Point

fn main() => int:
    let r: Rectangle = Rectangle {
        top_left: Point { x: 0, y: 10 },
        bottom_right: Point { x: 20, y: 0 }
    }
    let x: int = r.top_left.x
    let y: int = r.bottom_right.y
    printf("Rectangle: %d, %d\n", x, y)
    rm r, x, y
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_field_assign() {
    let source = r#"
class Point:
    x: int
    y: int

class Wrapper:
    p: Point

fn main() => int:
    let w: Wrapper = Wrapper { p: Point { x: 1, y: 2 } }
    w.p.x = 42
    printf("%d\n", w.p.x)
    rm w
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_as_function_parameter() {
    let source = r#"
class Point:
    x: int
    y: int

fn print_point(p: Point) => ():
    printf("Point: %d, %d\n", p.x, p.y)

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    print_point(p)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_as_return_value() {
    let source = r#"
class Point:
    x: int
    y: int

fn make_point(x: int, y: int) => Point:
    return Point { x: x, y: y }

fn main() => int:
    let p: Point = make_point(10, 20)
    printf("Point: %d, %d\n", p.x, p.y)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_reuse() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 10, y: 20 }
    let p2: Point = Point { x: 30, y: 40 }
    printf("Points: %d,%d and %d,%d\n", p1.x, p1.y, p2.x, p2.y)
    rm p1, p2
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_if_statement() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    if p.x > 5:
        printf("x is greater than 5\n")
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_loop() {
    let source = r#"
class Counter:
    count: int

fn main() => int:
    let c: Counter = Counter { count: 0 }
    let i: int = 0
    while i < 5:
        printf("Count: %d\n", c.count)
        i = i + 1
    printf("Final count: %d\n", c.count)
    rm c, i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_expression() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 10, y: 20 }
    let p2: Point = Point { x: 5, y: 5 }
    let sum: int = p1.x + p2.x
    printf("Sum: %d\n", sum)
    rm p1, p2, sum
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_classes() {
    let source = r#"
class Point:
    x: int
    y: int

class Rectangle:
    top_left: Point
    bottom_right: Point

class Circle:
    center: Point
    radius: int

fn main() => int:
    let p: Point = Point { x: 10, y: 20 }
    let r: Rectangle = Rectangle {
        top_left: Point { x: 0, y: 10 },
        bottom_right: Point { x: 20, y: 0 }
    }
    let c: Circle = Circle {
        center: Point { x: 5, y: 5 },
        radius: 10
    }
    printf("Point: %d,%d\n", p.x, p.y)
    printf("Circle radius: %d\n", c.radius)
    rm p, r, c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_nested_class_definition() {
    let source = r#"
class Outer:
    value: int

class Inner:
    data: int

fn main() => int:
    let o: Outer = Outer { value: 42 }
    let i: Inner = Inner { data: 123 }
    printf("Outer: %d, Inner: %d\n", o.value, i.data)
    rm o, i
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_defined_inside_function_compiles() {
    let source = r#"
fn main() => int:
    class Point:
        x: int
        y: int
    let p: Point = Point { x: 1, y: 2 }
    printf("Point: %d,%d\n", p.x, p.y)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_large_class() {
    let source = r#"
class Large:
    f1: int
    f2: int
    f3: int
    f4: int
    f5: int
    f6: int
    f7: int
    f8: int
    f9: int
    f10: int

fn main() => int:
    let l: Large = Large {
        f1: 1, f2: 2, f3: 3, f4: 4, f5: 5,
        f6: 6, f7: 7, f8: 8, f9: 9, f10: 10
    }
    rm l
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_with_nested_class() {
    let source = r#"
class Point:
    x: int
    y: int

class Rectangle:
    top_left: Point
    bottom_right: Point

fn main() => int:
    let p1: Point = Point { x: 0, y: 10 }
    let p2: Point = Point { x: 10, y: 0 }
    let r: Rectangle = Rectangle {
        top_left: p1,
        bottom_right: p2
    }
    rm p1, p2, r
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_with_array_field() {
    let source = r#"
class Matrix:
    data: [int; 9]
    rows: int
    cols: int

fn main() => int:
    let m: Matrix = Matrix {
        data: [1, 2, 3, 4, 5, 6, 7, 8, 9],
        rows: 3,
        cols: 3
    }
    rm m
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_with_tuple_field() {
    let source = r#"
class Pair:
    values: (int, int)
    sum: int

fn main() => int:
    let p: Pair = Pair {
        values: (10, 20),
        sum: 30
    }
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_with_mixed_types() {
    let source = r#"
class Mixed:
    id: int
    name: str
    active: bool
    score: float
    tags: [int; 3]

fn main() => int:
    let m: Mixed = Mixed {
        id: 1,
        name: "test",
        active: true,
        score: 95.5,
        tags: [1, 2, 3]
    }
    rm m
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_assignment() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 10, y: 20 }
    let p2: Point = clone p1
    p2.x = 30
    rm p1, p2
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_field_access() {
    let source = r#"
class Person:
    name: str
    age: int

fn main() => int:
    let p: Person = Person {
        name: "Alice",
        age: 30
    }
    let name: str = p.name
    let age: int = p.age
    rm p, name, age
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_class_in_array() {
    let source = r#"
class Point:
    x: int
    y: int

fn main() => int:
    let p1: Point = Point { x: 1, y: 2 }
    let p2: Point = Point { x: 3, y: 4 }
    let p3: Point = Point { x: 5, y: 6 }
    let points: [Point; 3] = [p1, p2, p3]
    rm p1, p2, p3, points
    return 0

"#;
    assert_compiles(source).unwrap();
}

/// Fields that packing would reorder (i8, i64, i8). GEP must still use
/// declaration order so `value` is not read from a bool slot.
#[test]
fn test_field_access_keeps_declaration_order() {
    let source = r#"
class MixedPad:
    flag_a: bool
    value: int
    flag_b: bool

fn main() => int:
    let m: MixedPad = MixedPad { flag_a: true, value: 42, flag_b: false }
    let v: int = m.value
    rm m
    return v

"#;
    let result = compile_coffee(source, &["--jit"]).expect("compile+jit");
    assert_eq!(
        result.exit_code, 42,
        "declaration-order GEP should read value=42; stdout={} stderr={}",
        result.stdout, result.stderr
    );
}

#[test]
fn test_inherited_fields_are_indexed_after_parent() {
    let source = r#"
class Base:
    x: int

class Child of Base:
    y: int

fn main() => int:
    let c: Child = Child { x: 7, y: 9 }
    let a: int = c.x
    let b: int = c.y
    rm c
    return a * 10 + b

"#;
    let result = compile_coffee(source, &["--jit"]).expect("compile+jit");
    assert_eq!(
        result.exit_code, 79,
        "parent field x then child field y; stdout={} stderr={}",
        result.stdout, result.stderr
    );
}

fn packed_bitfield_source() -> &'static str {
    r#"
class PackBits:
    lo: int:3
    hi: int:5

    fn poke(self) => int:
        self.lo = 1
        self.hi = 2
        let a: int = self.lo
        let b: int = self.hi
        return a + b * 10

fn main() => int:
    return 0
"#
}

/// Default ABI: one LLVM member per declared field, even with bit-width annotations.
#[test]
fn test_bitfields_flag_off_keeps_one_llvm_member_per_field() {
    let pid = std::process::id();
    let ll = format!("test_bitfields_off_{}.ll", pid);
    let result = compile_coffee(packed_bitfield_source(), &["--emit-llvm", "-o", &ll]).unwrap();
    let ir = std::fs::read_to_string(&ll).unwrap_or_default();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(result.exit_code, 0, "emit-llvm failed:\n{}", result.stderr);
    assert!(
        ir.contains("%PackBits = type { i64, i64 }") || ir.contains("%PackBits = type { i64, i64,") ,
        "flag off should keep two i64 members; ir=\n{}",
        ir
    );
}

/// `--enable-bitfields`: consecutive bitfields share a storage-unit integer; load/store compile.
#[test]
fn test_enable_bitfields_packs_consecutive_fields_and_compiles_access() {
    let pid = std::process::id();
    let ll = format!("test_bitfields_on_{}.ll", pid);
    let result = compile_coffee(
        packed_bitfield_source(),
        &["--enable-bitfields", "--emit-llvm", "-o", &ll],
    )
    .unwrap();
    let ir = std::fs::read_to_string(&ll).unwrap_or_default();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(
        result.exit_code, 0,
        "enable-bitfields compile failed:\n{}",
        result.stderr
    );
    assert!(
        ir.contains("%PackBits = type { i8 }"),
        "consecutive 3+5 bit fields should pack into one i8 storage unit; ir=\n{}",
        ir
    );
    assert!(
        ir.contains("lshr") || ir.contains("ashr") || ir.contains("shl"),
        "bitfield load/store should use shift+mask; ir=\n{}",
        ir
    );
}

/// `packed class` with `:N` fields packs storage even without `--enable-bitfields`.
#[test]
fn test_packed_class_bitfields_pack_without_enable_flag() {
    let source = r#"
packed class PackedBits:
    lo: int:3
    hi: int:5

    fn poke(self) => int:
        self.lo = 1
        self.hi = 2
        let a: int = self.lo
        let b: int = self.hi
        return a + b * 10

fn main() => int:
    return 0
"#;
    let pid = std::process::id();
    let ll = format!("test_packed_bits_no_flag_{}.ll", pid);
    let result = compile_coffee(source, &["--emit-llvm", "-o", &ll]).unwrap();
    let ir = std::fs::read_to_string(&ll).unwrap_or_default();
    let _ = std::fs::remove_file(&ll);
    assert_eq!(result.exit_code, 0, "emit-llvm failed:\n{}", result.stderr);
    let packed_i8 = ir.contains("%PackedBits = type { i8 }")
        || ir.contains("%PackedBits = type <{ i8 }>");
    assert!(
        packed_i8,
        "packed class 3+5 bit fields should pack into one i8 without --enable-bitfields; ir=\n{}",
        ir
    );
}
