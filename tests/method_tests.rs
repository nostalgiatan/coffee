// 方法调用测试
// 
// 测试类方法调用（self参数）的功能

include!("common/mod.rs");

//=============================================================================
// 基础方法调用测试
//=============================================================================

#[test]
fn test_simple_method_call() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }
    
    fn move(self, dx: int, dy: int) => Point:
        self.x = self.x + dx
        self.y = self.y + dy
        return self

fn main() => int:
    let p: Point = Point::new(10, 20)
    p.move(5, 5)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_no_args() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self

fn main() => int:
    let c: Counter = Counter::new()
    c.increment()
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_single_arg() {
    let source = r#"
class Accumulator:
    total: int
    
    fn new() => Accumulator:
        Accumulator { total: 0 }
    
    fn add(self, value: int) => Accumulator:
        self.total = self.total + value
        return self

fn main() => int:
    let acc: Accumulator = Accumulator::new()
    acc.add(10)
    rm acc
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_multiple_args() {
    let source = r#"
class Rectangle:
    x: int
    y: int
    width: int
    height: int
    
    fn new(x: int, y: int, w: int, h: int) => Rectangle:
        Rectangle { x: x, y: y, width: w, height: h }
    
    fn resize(self, w: int, h: int) => Rectangle:
        self.width = w
        self.height = h
        return self

fn main() => int:
    let r: Rectangle = Rectangle::new(0, 0, 100, 50)
    r.resize(200, 100)
    rm r
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 方法链式调用测试
//=============================================================================

#[test]
fn test_chained_method_calls() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self
    
    fn add(self, n: int) => Counter:
        self.value = self.value + n
        return self

fn main() => int:
    let c: Counter = Counter::new()
    c.increment()
    c.increment()
    c.add(10)
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_multiple_methods_on_same_object() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn move_x(self, dx: int) => Point:
        self.x = self.x + dx
        return self
    
    fn move_y(self, dy: int) => Point:
        self.y = self.y + dy
        return self

fn main() => int:
    let p: Point = Point { x: 0, y: 0 }
    p.move_x(3)
    p.move_y(7)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 方法逻辑测试
//=============================================================================

#[test]
fn test_method_with_computation() {
    let source = r#"
class Calculator:
    result: int
    
    fn new() => Calculator:
        Calculator { result: 0 }
    
    fn multiply(self, a: int, b: int) => Calculator:
        self.result = a * b
        return self

fn main() => int:
    let calc: Calculator = Calculator::new()
    calc.multiply(5, 6)
    rm calc
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_with_condition() {
    let source = r#"
class State:
    active: int
    
    fn new() => State:
        State { active: 0 }
    
    fn set_active(self, flag: int) => State:
        if flag > 0:
            self.active = 1
        else:
            self.active = 0
        return self

fn main() => int:
    let s: State = State::new()
    s.set_active(1)
    rm s
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_with_loop() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn add_multiple(self, n: int, times: int) => Counter:
        let i: int = 0
        while i < times:
            self.value = self.value + n
            i = i + 1
        rm i
        return self

fn main() => int:
    let c: Counter = Counter::new()
    c.add_multiple(5, 3)
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_with_local_vars() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }
    
    fn translate(self, dx: int, dy: int) => Point:
        let new_x: int = self.x + dx
        let new_y: int = self.y + dy
        self.x = new_x
        self.y = new_y
        rm new_y
        rm new_x
        return self

fn main() => int:
    let p: Point = Point::new(10, 20)
    p.translate(5, 10)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 多个对象的方法调用测试
//=============================================================================

#[test]
fn test_multiple_objects_methods() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self

fn main() => int:
    let c1: Counter = Counter::new()
    let c2: Counter = Counter::new()
    let c3: Counter = Counter::new()
    c1.increment()
    c2.increment()
    c2.increment()
    c3.increment()
    c3.increment()
    c3.increment()
    rm c3
    rm c2
    rm c1
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_in_loop() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self

fn main() => int:
    let c: Counter = Counter::new()
    let i: int = 0
    while i < 5:
        c.increment()
        i = i + 1
    rm i
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_in_if_statement() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self

fn main() => int:
    let c: Counter = Counter::new()
    let flag: int = 1
    if flag > 0:
        c.increment()
        c.increment()
    rm flag
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 方法访问字段测试
//=============================================================================

#[test]
fn test_method_read_field() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }
    
    fn get_x(self) => int:
        let result: int = self.x
        return result

fn main() => int:
    let p: Point = Point::new(42, 100)
    let x_val: int = p.get_x()
    rm x_val
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_write_field() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }
    
    fn set_x(self, value: int) => Point:
        self.x = value
        return self

fn main() => int:
    let p: Point = Point::new(10, 20)
    p.set_x(100)
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_modify_field() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn double(self) => Counter:
        self.value = self.value * 2
        return self

fn main() => int:
    let c: Counter = Counter::new()
    c.double()
    c.double()
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 方法返回值测试
//=============================================================================

#[test]
fn test_method_return_int() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn get_value(self) => int:
        let result: int = self.value
        return result

fn main() => int:
    let c: Counter = Counter::new()
    let val: int = c.get_value()
    rm val
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_return_bool() {
    let source = r#"
class State:
    active: int
    
    fn new() => State:
        State { active: 0 }
    
    fn is_active(self) => int:
        if self.active > 0:
            return 1
        else:
            return 0

fn main() => int:
    let s: State = State::new()
    let active: int = s.is_active()
    rm active
    rm s
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 多个方法测试
//=============================================================================

#[test]
fn test_multiple_methods() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }
    
    fn move_x(self, dx: int) => Point:
        self.x = self.x + dx
        return self
    
    fn move_y(self, dy: int) => Point:
        self.y = self.y + dy
        return self
    
    fn reset(self) => Point:
        self.x = 0
        self.y = 0
        return self

fn main() => int:
    let p: Point = Point::new(10, 20)
    p.move_x(5)
    p.move_y(10)
    p.reset()
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_interactions() {
    let source = r#"
class Counter:
    value: int
    
    fn new() => Counter:
        Counter { value: 0 }
    
    fn increment(self) => Counter:
        self.value = self.value + 1
        return self
    
    fn get_value(self) => int:
        let result: int = self.value
        return result
    
    fn reset(self) => Counter:
        self.value = 0
        return self

fn main() => int:
    let c: Counter = Counter::new()
    c.increment()
    c.increment()
    let val: int = c.get_value()
    c.reset()
    rm val
    rm c
    return 0

"#;
    assert_compiles(source).unwrap();
}

//=============================================================================
// 边界情况测试
//=============================================================================

#[test]
fn test_method_with_zero_args() {
    let source = r#"
class Flag:
    value: int
    
    fn new() => Flag:
        Flag { value: 0 }
    
    fn toggle(self) => Flag:
        if self.value == 0:
            self.value = 1
        else:
            self.value = 0
        return self

fn main() => int:
    let f: Flag = Flag::new()
    f.toggle()
    rm f
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_with_many_args() {
    let source = r#"
class Config:
    a: int
    b: int
    c: int
    d: int
    e: int
    
    fn new() => Config:
        Config { a: 0, b: 0, c: 0, d: 0, e: 0 }
    
    fn set_all(self, a: int, b: int, c: int, d: int, e: int) => Config:
        self.a = a
        self.b = b
        self.c = c
        self.d = d
        self.e = e
        return self

fn main() => int:
    let cfg: Config = Config::new()
    cfg.set_all(1, 2, 3, 4, 5)
    rm cfg
    return 0

"#;
    assert_compiles(source).unwrap();
}

#[test]
fn test_method_call_after_field_access() {
    let source = r#"
class Point:
    x: int
    y: int
    
    fn new(x: int, y: int) => Point:
        Point { x: x, y: y }
    
    fn move(self, dx: int, dy: int) => Point:
        self.x = self.x + dx
        self.y = self.y + dy
        return self

fn main() => int:
    let p: Point = Point::new(10, 20)
    let x_val: int = p.x
    let y_val: int = p.y
    p.move(5, 5)
    rm y_val
    rm x_val
    rm p
    return 0

"#;
    assert_compiles(source).unwrap();
}