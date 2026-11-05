# Storage of types

Every type has exactly one storage description: how a value of it is represented, how big
it is, who owns it, and what dropping and cloning it mean. It is computed in one place
(`src/storage.cf`) from the type alone; no stage guesses it again. When a bug asks "what is
a `str` here?", the answer is this table.

## Principles

1. **One representation per type.** A value in a register, in a variable, in a field, as an
   argument and as a result has the same LLVM type. There are no transient forms: a
   comparison produces a `bool`, which is an `i8` holding 0 or 1, never an `i1`.
2. **`void` has no storage.** It has no value. A call that returns `void` produces nothing;
   no stage makes up a placeholder.
3. **A handle is a single pointer.** `str`, a class, an enum with payloads, `object` and the
   raw `buf` are each one `ptr`. What it points at is its *block*.
4. **Ownership follows the type.** Every type is `copy` (a bit copy is a copy), `move` (it
   has an owner and is dropped) or `borrow` (a reference).
5. **Generic code has no storage.** `T` is replaced before storage is asked; the table
   substitutes the type arguments of a `Name<A>` when it looks inside it.

## The table

| type | value | size/align | block | own | drop | clone |
|---|---|---|---|---|---|---|
| `void` | none | 0/1 | | copy | nothing | bit |
| `bool` | `i8` | 1/1 | | copy | nothing | bit |
| `int(N)±` | `iN*8` | N/N | | copy | nothing | bit |
| `float(N)` | `f32`/`f64` | N/N | | copy | nothing | bit |
| `str` | `ptr` | 8/8 | NUL-terminated bytes | move | free | deep |
| `object` | `ptr` | 8/8 | C's | move | nothing | shallow |
| `buf` (raw) | `ptr` | 8/8 | malloc'd | move | free | never |
| `buf<T>` | `{ptr, i64}` | 16/8 | `len` elements | move | free | never |
| `[T]` slice | `{ptr, i64}` | 16/8 | borrowed elements | move | its elements | shallow |
| `[T; N]` | `ptr` | 8/8 | `N` elements | copy if `T` is, else move | its elements | deep |
| `(A, B)` | `{A, B}` | fields, natural alignment | | copy if all are, else move | its fields | deep |
| `&T`, `&mut T` | `ptr` | 8/8 | see below | borrow | nothing | shallow |
| `fn(..) => R` | `ptr` | 8/8 | code | copy | nothing | bit |
| class | `ptr` | 8/8 | parent fields, then own, natural alignment | move | its fields, then the block | deep |
| enum, no payloads | `i64` | 8/8 | | copy | nothing | bit |
| enum with payloads | `ptr` | 8/8 | `{i64 tag, payload}`, as big as the biggest variant | move | the live variant's fields, then the block | deep |

`own` decides what `let b = a` means: copy duplicates, move transfers (the source is gone),
borrow copies the pointer and owns nothing. The memory stage reads it from here.

## References

`&T` is a `ptr`. For a *handle* type (`str`, class, enum with payloads, `object`, raw `buf`)
the reference **is** the handle: a borrowed copy of the same pointer, so `&str` can go to C
as a `const char *` and `&Class` needs no second indirection. For every other type it points
at the place holding the value. `*r` reverses this. This is the one place where the rule
depends on the form of the target, and it depends on nothing else.

## String literals

A string literal is static text: its type is `&str` and it is never freed. Where a `str` is
wanted it must be copied (`clone`), exactly as any other borrowed `str` would be.

## Layout

Fields are laid out in declaration order, a parent's before the child's, each at its own
alignment, the whole padded to the largest alignment. `packed` classes with bit fields are a
separate layout and not described here. The size of a handle's block is `-1` when it is not
known statically (`str`, `buf`).
