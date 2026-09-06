# Unified type parser (foundation for generics)

**Status:** implement now, `src/parser/` only. Checker may still reject `List<int>` until the generics track lands.

## Problem

Three scanners produce a **type substring**:

- `function.rs::parse_type`: `take_till('>')` — nested `List<List<int>>` and `Map<str, List<int>>` **truncate at the first `>`**.
- `var.rs::parse_type_identifier`: tracks `()` `[]`, not `<>`; allows `&` / `fn(...)`.
- `class/mod.rs::parse_type_identifier`: tracks `<>` `()` `[]`, **does not allow a leading `&`**.

`ClassDef` comments already show `class Name<T>` but `parse_class` only reads `parse_identifier`.

## Goal

One balanced type scanner. Same substring in params, returns, `let`, fields, methods. Nested `<>` `()` `[]`. Then parse `class List<T, U>` / `fn id<T>` into AST lists (empty = none).

Still store types as **strings** on AST (do not migrate `Parameter.param_type` to a tree this slice — too many backend call sites). Strings must be **complete** so `Type::from_str` can grow `App` later.

## Scanner (`src/parser/ty.rs`)

`pub fn parse_type(input: &str) -> IResult<&str, &str>`:

1. Optional `&mut ` or `&` (not `&&`) then recurse; consumed span is the whole `&mut T`.
2. `fn` + `parse_fn_type_annotation` (reuse `var.rs` logic; move it here).
3. Otherwise walk with `angle`, `paren`, `bracket` depths (ignore those chars inside `"…"`).
4. Stop when all depths are 0 and the next char would start a **type terminator**: `,` `:` newline, `=>` (peek two chars), or `=` that is not inside the type. Do **not** stop on `>` while `angle > 0`. Include trailing `+`/`-` of `int(N)+`.
5. Reject unmatched closer (nom error).

Replace all three old functions with `crate::parser::ty::parse_type` (re-export as `parse_type_identifier` if needed).

## Class / function type parameters

After the name, optional `<T, U>`: identifiers separated by commas, balanced scan (no nested `<>` in param names).

```coffee
class List<T>:
    head: T

class Child<T> of Parent:
    x: T

fn id<T>(x: T) => T:
    return x
```

- `ClassDef.type_params: Vec<String>` (default empty). Update every `ClassDef {` in the repo the parser crate tests use.
- `Function.type_params: Vec<String>` similarly.
- Methods: no extra syntax this slice (`fn map<U>` later).
- `packed class List<T>:` same as `class`.
- Duplicate param names → parse or later type error; **parse error** if empty `<>` or non-identifier.

Do **not** substitute `T` in the parser. Do **not** edit `src/types/` `reject_fake_generics`.

## Tests (parser)

- Nested `List<List<int>>` as a parameter type: full string, rest starts at `,` or `)`.
- `Map<str, List<int>>` not truncated.
- `[int; 2]`, `(int, str)`, `int(4)+`, `&mut int`, `fn(int) => int`.
- Field type `&int` on a class (currently class scanner may fail).
- `class List<T>:` → `type_params == ["T"]`.
- `fn id<T>(x: T) => T` → function `type_params == ["T"]`.
- `take_till` regression: old parser would leave `>` leftover; remaining input after param type must not start with `>`.

`flock … cargo test --offline --lib parser:: -- --test-threads=1` plus any `--test parser_error_tests` you touch.

## Out of scope

`src/types`, `src/backend`, codegen, version bump, git commit.
