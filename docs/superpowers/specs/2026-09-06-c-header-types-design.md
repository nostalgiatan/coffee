# C headers → Coffee types (and `type` newtypes)

**Status:** landed as `0.3.8`. C++ still out. Persistence is still one `.cfc` from `coffee -c`.

**C++ is out.** Mangling, overloads, templates, methods, vtables wait until C interop is done.

## Goal

`coffee -c header.h` still writes **one** `.cfc`. That file is the only persistent C ABI. Completing a C header means Coffee can **name** the types (not collapse every pointer to untyped `object`), while Coffee source still looks like Coffee and C headers still look like C.

Two languages, one conversion:

- **C** lives in `.h` / clang.
- **Coffee** lives in `.cf` and in `.cfc` (Coffee spellings that *correspond* to C).
- The generator is the C → Coffee conversion. `as` is the Coffee-side wrap/unwrap when representations match.

## `type Name: T` (language extension)

This is the handle story. It is a **nominal newtype**, not an alias and not inheritance (`of` stays for classes).

```coffee
type FILE: object
type HWND: object
```

Rules:

1. After the declaration, `FILE` is a distinct type. `FILE` ≠ `object` ≠ `HWND`.
2. The declaration records a **source type** `T`. This round `T` is only `object` (generated `.cfc` and user `.cf`). Other sources (`type X: int`) are out. LLVM, drop, `rm`, pointer-sized ops, and anything you do **after** `as` unwrap — **same as today for `object`**. No second execution model.
3. Further checking does not treat `FILE` as a subtype of `object`. Ordinary assignment and arguments do not mix them.
4. **`as` is allowed** between a newtype and its source, because the representation is the source: `f as object`, `p as FILE`. Sibling newtypes with the same source are **not** directly convertible: `f as HWND` is an error; `f as object as HWND` is the explicit lie.
5. `as` does not change drop of the *name* you hold. A `FILE` still does not `free` (same as `object`). `object` → `buf` stays the existing opt-in `free`.
6. No methods on `type` (it is not `class`). C operations stay `c fn` (`fclose(f)` with `f: FILE`).
7. `buf` stays a builtin; do **not** rewrite it as `type buf: object` (`free` is not `object` behavior).
8. Checker delta vs today: names introduced by `type` must **not** use the current “any `NamedType` is a C handle” implicit coerce to `object`. User `class` keeps existing coerce/resource rules.

Parser: `type` is a new top-level keyword (`is_top_level_keyword`, `SYNTAX.md` keyword list). Form is one line: `type Name: Type`. No body, no generics this round.

## `.cfc` contents

Still one artifact. Parser grows beyond `c fn` only.

| C (clang) | In `.cfc` (Coffee) |
|---|---|
| incomplete struct/union (`FILE`) | `type FILE: object` |
| `FILE*` in a signature | `FILE` (the handle newtype). No Coffee `*FILE`. `FILE**` is `object`. |
| complete `struct Point` by value | `c class Point:` fields in clang order; size/align from clang |
| `Point*` in a signature | `object` (complete-type pointers stay untyped handles). Call sites may pass `&Point` / `&mut Point` via **existing** `&` → `object` coerce |
| `void*` | `object` |
| `char*` / `const char*` | `str` / `string` on **`c fn` params and results only** (unchanged). The same type as a **record field** is `object` (bitwise copy of a C struct must not take Coffee `str` drop) |
| `union` complete | `c union U:` all members, one storage, clang size/align |
| `enum` | `c enum E:` integer constants (Coffee `int` width from clang) |
| `typedef struct Foo Foo` | one Coffee name `Foo` |
| function pointer | Coffee `fn(...) => R` on params/fields/`c fn` results |
| `const` | dropped at generate time; never becomes Coffee `&` |
| array parameter | decays to pointer, then the pointer rule above; `c fn` still forbids Coffee `[T]` |
| bitfield in a record | that record is **opaque**: `type Name: object`, no field access this round |
| macros, `#define` constants, globals | out of this round |

`c class` / `c union` / `c enum` appear **only in `.cfc`**, not as a new user-facing `of c` syntax in `.cf`. Loading a library table registers those types with the functions.

`c class` copy is **C memcpy of the record** (including pointer fields as bits). Scope end does not `rm` / `free` pointees. That is not a Coffee resource `class`. User `class` in `.cf` stays a resource. Do not write `of` between `c class` and user `class`.

Integer/float mapping uses **clang size and signedness** (`clang_Type_getSizeOf` / type kind), not a hardcoded `long` → `int(8)+` table. `char*` detection stays special-case to `string`.

## Conversion (Coffee source)

Implicit at FFI / existing rules only:

- Matching integer/float/bool widths.
- `c class` by value ↔ C struct by value (same layout).
- `str` ↔ `char*` as today.
- `object` ↔ `void*` / complete-type pointers as today.
- `&T` / `&mut T` → `object` argument as today (address for the call; loan ends with the statement).
- `c fn` result already typed `FILE` → bind `let f: FILE = fopen(...)` with no `as`.

Explicit `as`:

- Newtype ↔ its source (`FILE` ↔ `object`).
- Existing `object` → `buf` (opt-in `free`).

Forbidden:

- C pointer result → Coffee `&T` (no lifetime).
- `FILE` ↔ `HWND` in one `as`.
- Value `Point` ↔ `object` (struct vs pointer).
- Coffee `[T]` on `c fn`.

## Unions and borrow

C overlay: all members, same storage, clang size.

Stricter than C, using the existing borrow checker (not a new `as` for members, not active-member tracking):

- Places `u.x` and `u.y` **overlap** for a `c union` (unlike ordinary class fields).
- At most one field loan at a time: `&u.x` live ⇒ no `&u.y`, no write of another member.
- Same overlap set for `mv` / `rm` / assign while borrowed.

LLVM: one alloca/storage of the union size; field access is GEP/bitcast to the member type. No tagged union.

## Generator and pipeline

- `parse_with_clang` visits **functions and records/enums/typedefs** used by those functions (plus types named in visited signatures). Emit type decls before `c fn` in the `.cfc`.
- Unsupported C type is still an **error**, not silent `object`, except the pointer cases listed above.
- `.cfc` parse loads types into `CSymbolTable` (extend the table; do not add a second map). Typecheck and LLVM `add_function` keep reading that one table.
- Drivers unchanged: `CompilationResult.cfc_symbols` is the handoff.
- Bundled `library/cfc/libc.cfc` gains `type FILE: object` (and any other incomplete types the regenerated libc signatures need). User override still replaces the library key.

## Errors (user-facing)

- Using `FILE` where `object` is required, or the reverse, without `as`.
- `as` between sibling newtypes.
- `as` between a value `c class` and `object`.
- Returning / binding a C pointer as `&T`.
- Field access on a `type` newtype or on a bitfield-opaque record.
- Two overlapping union field borrows.
- `coffee -c` on a type clang cannot size (incomplete used by value) → generate error, do not emit a fake `c class`.

## Tests

- Parse `type FILE: object` in `.cf`; `FILE` ≠ `object` on assignment; `as` both ways typechecks; `f as HWND` fails; `f as object as HWND` succeeds.
- LLVM for `FILE` locals/args matches `object` (opaque ptr); no `free` on drop; `let b: buf = f as object` then existing buf `free` if annotated that way.
- `c fn fopen(...) => FILE` from a fixture `.cfc`; call binds `FILE` without `as`.
- `coffee -c` on a small header: complete struct → `c class` with field order; `fopen`-style incomplete → `type FILE: object`; `char*` still `string`; function pointer param is `fn(...) => R` not `object`.
- Value `c class Point`: copy assignment legal; no `rm` required; IR is struct copy not a Coffee class heap object.
- `c union`: IR size from clang; borrow test two field `&` is an error.
- `c fn` still rejects `[int]`.
- Existing libc `printf` / `object` tests stay green. `parse_with_clang` remains `pub(crate)`.

## Out of scope

C++, macros, globals, `#define` constants, Coffee `*T` syntax, `class Name of object`, active-union-member tracking, bitfield field access, regenerating all of POSIX beyond what bundled libc already declares, Windows LLP64 as a second generator mode (clang sizes on the **host** used for `-c` are the truth for that `.cfc`).

## Implementation order (one spec, three landable slices)

1. `type Name: object` + `as` + checker (no implicit newtype ↔ `object`).
2. `.cfc` parse of `type` / `c class` / `c union` / `c enum`; table + typecheck using those names; libc `FILE`.
3. clang visitor emits those decls; value layout + union borrow; tests above.

Each slice is one patch when it lands.
