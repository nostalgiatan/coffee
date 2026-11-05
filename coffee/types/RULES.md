# Type rules

The rules of `coffee/types`, written down before the code. A rule is a decision: where it
differs from the Rust compiler in `src/types/` it says so, and each has a test.

## Types

`int(N)+` / `int(N)-` (N bytes, signed/unsigned), `float(N)`, `bool`, `str`, `void`, `object`
(a raw C pointer), `buf<T>` / `buf` (owned buffer / raw owned pointer), `[T]` slice, `[T; N]`
array, `(A, B)` tuple, `&T` / `&mut T`, `fn(A) => R`, a class or enum, `Name<A>`.
Types are interned: equal types have equal ids.

## Error type

An expression that is already reported wrong has the type `Error`, which fits everywhere and
reports nothing, so one mistake gives one message.

## Implicit conversions (value `from` used where `to` is wanted)

1. Equal types.
2. Integers widen when no value is lost: same signedness and not narrower, or unsigned into a
   strictly wider signed. (Rust: same signedness only, so `u8` does not fit `int` there.)
3. `float(4)` widens to `float(8)`. An integer never becomes a float, nor a float an integer.
4. An integer *literal* (also `-lit`) fits any integer type that holds its value; a float
   literal fits `float(4)` when finite and in range. A variable is never narrowed.
5. `&mut T` is accepted as `&T`. A string literal is accepted as `&str`.
6. `&buf<T>` and `&mut buf<T>` are accepted as `[T]`, `&[T; N]` too.
7. The literal `0` is a null `object`. An `object` is accepted as `buf`, `buf<T>`, `str`.
8. A class is accepted where its ancestor class is wanted by reference or value.
9. Nothing else: not bool/int, not int/object, not class/class.

## Operators

- Arithmetic `+ - * %` needs two numbers of one kind after rule 4; the result is the wider.
  `int` and `float` do not mix. `+` also joins two `str`.
- `/` is for floats, `//` for integers; `%` for integers and floats.
- Bit operators and shifts need integers. `&& || !` need `bool`.
- Comparisons `< <= > >=` need two numbers or two `str`; `== !=` need equal types, or an
  `object` against `0`. The result is `bool`.
- `-x` needs a number, `~x` an integer, `*r` a reference, `&x` / `&mut x` give a reference.

## Calls and members

- A call needs a function type; the argument count and each argument must fit the parameter.
- `x.name` is a field of the class or an ancestor, or a method. A method call passes `x` as
  the first parameter (`self`) and checks the rest.
- A function returning `void` has no value: using it as an operand, an initializer or an
  argument is an error.
- Struct literals give each field of the class and its ancestors exactly once.

## Statements

- `let x = e` takes the type of `e` (never `void`); `let x: T = e` needs `e` to fit `T`.
- Assignment needs the value to fit the target's type. `return e` fits the declared return type,
  a bare `return` needs `void`. Conditions of `if`/`while` are `bool`.
- `for i in a..b` has integer bounds. `for x in c` takes the element type of an array, slice,
  tuple of one type or `buf`.
- `match` patterns have the scrutinee's type; a variant pattern binds its payload types.

## Deliberately not here

Ownership and borrowing (moves, loans), integer overflow, bounds. They use these types but
are the next stage. C functions from `.cfc` are not typed yet: a call to one is `Error`.
