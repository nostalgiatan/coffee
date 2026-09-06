# Generics v1 (monomorphize in the typechecker)

**Depends on:** unified type parser producing full `List<List<int>>` strings and `ClassDef.type_params` / `Function.type_params`. If those fields are missing, wait or only implement `Type::from_str` `App` until they exist — **do not edit `src/parser/`** (other agent owns it). **Do not edit `src/backend/`** (composite-drop agent). Codegen must see **already-monomorphized ordinary classes**.

## Surface (v1)

- `class List<T>:` / `fn id<T>(x: T) => T:` (parser).
- Annotations: `List<int>`, nested `List<List<int>>`.
- No bounds, no `impl`, no `fn` type-param on methods only, no value-dependent types.

## `Type`

Replace `reject_fake_generics` with:

```text
Type::App { name: String, args: Vec<Type> }
```

`from_str`: `Name<...>` with nested `<>` (existing `parse_type_list` depth). `Display` prints `Name<...>` again. `NamedType` stays for `Point`.

`is_resource`: `App` is a resource (class-like).

## Instantiation (checker / pipeline, not LLVM)

When typechecking a `App { name: List, args: [int] }`:

1. Find `class List` with `type_params == ["T"]` (arity must match).
2. Clone the class AST. Substitute type-param **tokens** in `field_type`, method param/return strings, and recursively in nested type strings (`T` → `int`, not as a substring of `Other`).
3. Concrete name: `List__int` (args printed with `__` separators, sanitize `(` `)` `+` `-` `[` `;` `]` `,` to `_`). Nested: `List__List__int`.
4. Register/bind that class as a normal `NamedType` if not already.
5. Rewrite the use-site type to `NamedType { name: "List__int" }` (or keep `App` internally but codegen only sees the concrete class in `program.statements`).

Inject instantiated `Statement::Class` into the program **before** MIR/codegen (pipeline after parse/semantic, during/after typecheck). Existing `List_push` style methods become `List__int_push` via existing `to_standalone_function(class_name)`.

Functions: `id<T>` + call `id<int>(1)` or infer from argument — **v1 call syntax** `id(1)` with `T` inferred from param types; if inference fails, error “cannot infer T”. Explicit `id<int>(1)` only if parser already allows type args on calls — **if not parsed, inference-only**.

## Errors

- `List` used without args but class has params.
- `List<int, int>` arity.
- `List<int>` where `List` is not generic.
- Unbound `T` in a non-generic class.

## Tests

`tests/type_check_tests.rs` (and lib `types::`):

- `class Box<T>: v: T` + `let b: Box<int> = Box { v: 1 }` typechecks (struct lit).
- Nested `Box<Box<int>>`.
- Wrong arity errors.
- `fn id<T>(x: T) => T` + `let y: int = id(1)` if inference wired; else skip call until parser has call type-args.

Compile-to-LLVM **one** integration test only if you can do it **without** editing `backend/` (pipeline injects `Box__int`). Prefer `--emit-llvm` in `tests/` that only needs checker+existing class codegen.

## Out of scope

Parser files, `classes.rs` drop, macros, version, commit.
