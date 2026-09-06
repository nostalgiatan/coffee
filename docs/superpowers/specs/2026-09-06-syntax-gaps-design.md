# Syntax gaps — 2026-09-06

Locked (brainstorming). No vtable. No closure capture. Slice params still unsupported.

## `~x`

Integer-only bitwise not. Parse `Unary { op: "~" }`. LLVM `not` / xor all-ones. Float, bool, pointer: type error.

## Inherited methods (static walk)

`lookup_method` / codegen `{Class}_{method}`: if missing, walk `Class.parent`. Same-name on child wins. Pass child `self*` to parent method (parent-first layout). `Animal` typed values call `Animal_*` even if the object is a subclass.

## `for x in <expr>`

Lower collection once. If not a simple variable, `Let` a temp then existing `for_in_cfg` (array/slice/tuple/literal). Unsupported collection: `lower_function` Err.

Also: `for` body `let` must enter `extra_types` so `p.x` after `let p` inside the loop lowers (today infer snapshot misses loop-body locals).

## Anonymous `fn` (no capture)

`let callback: fn(int) => int = fn(x: int) => int:` + indented body.

- New `Expression` / `HirExprKind` holding params, return type, `FunctionBody`.
- Unique name (`__anon_N` / `unique_function_key`); pipeline `lower_function_mir` like nested `fn`.
- Type matches `fn(...) => R`.
- Outer **locals and enclosing params** used in the body: type error. Globals / `use` C / top-level `fn` OK.
- Codegen: function pointer to that LLVM function (existing `Type::Function` ptr).
