# Ownership rules

What `coffee/own` checks, written before the code. It reads the typed tree of `coffee/types`
and the storage table (`types/STORAGE.md`): a variable is a *resource* when its type is
`move`, a *value* when it is `copy`, a *reference* when it is `borrow`.

## Method: a graph and three analyses

Each function body is lowered to a control-flow graph of small events (define, use, consume,
borrow, drop). `if`, `while`, `for`, `match`, `break`, `continue` and `return` become edges, so
no rule has to special-case them. Three dataflow analyses run on it:

- **Liveness** (backwards): is a variable used again on some path from here?
- **Move state** (forwards): may a variable be moved or dropped here?
- **Loans** (forwards, with liveness): which borrows may a reference hold, and are they used?

This replaces the Rust compiler's syntactic last-use heuristic. Loops, `break` and `return`
need no special cases, and the answer is exact for the graph, not conservative about it.

## Moves

A *consuming* position takes a value: the initializer of `let`, the right side of `=`, a
by-value argument, a `return` value, and `mv a b`. Struct-literal fields, tuple and array
elements, `for` iterables and `match` scrutinees only look at their variable: the Rust compiler
accepts `rm` of both a part and its whole, so this checker does too. Borrows bind to a
variable only when written directly in its initializer (`let r = &x`), not through a call.

1. Consuming a resource variable `a` **moves** it when `a` is not used again on any path from
   there (it is dead afterwards). That is the implicit move; the source is gone.
2. If `a` may be used again, it is an error (E302): write `clone a` or `mv a b`.
3. Using a variable that may have been moved is E300; one that may have been dropped by `rm`
   is E301. "May" means on some path: a move in one branch of an `if` poisons the join.
4. `let a = ...` or `a = ...` makes the variable live again, whatever it was.
5. A resource cannot be moved out of a place: `p.name`, `a[i]` and `*r` in a consuming position
   are E302. `clone p.name` and `&p.name` are fine; so is any copy-type field.
6. `rm a` drops early; `a` must be live and not borrowed, and is dropped afterwards.
7. `clone` of a `buf` is an error (storage says `never`); `copy` and `clean` are errors.

At the end of a variable's scope the result records whether it is still owned on every path
(drop it), on none (nothing to do) or on some (it needs a drop flag).

## Borrows

A place is a variable, a field path `p.x.y`, or an index `a[i]`. Two places overlap when one is
a prefix of the other; different fields do not overlap; an index overlaps any other index.

1. `&place` and `&mut place` start a loan. A loan lives while the reference that holds it is
   used again; a loan that is not stored anywhere ends with its statement. A reference
   assigned from a call returning `&T` holds the loans of that call's arguments.
2. Shared or exclusive: any number of `&` loans, or one `&mut`, never both.
3. While a place is borrowed it cannot be moved, dropped or assigned (E303). While it is
   mutably borrowed it cannot be read either (E303). Borrowing again against the rules is E401.
4. A function cannot return a reference to a local or to a parameter's own storage that is
   not a reference it was given (E400); returning a reference taken from a `&` parameter is fine.
5. A borrow of a variable that may be moved or dropped is a use of it (E300 / E301).

## Deliberately not here

Lifetimes across functions (no `'a`), aliasing through fields of references, and the
generated drop code itself. Closures cannot capture (sema reports it), so they are not
analysed for captured loans.
