# coffee/types

The type checker of Coffee, written in Coffee. It reads the parse tree of `coffee/parser`
and the names resolved by `coffee/sema`, gives every expression a type and reports what does
not fit. The rules are in [RULES.md](RULES.md); they are this project's own, with the Rust
compiler's `src/types/` as a reference for what has to be handled, not as the specification.

    let g = new_globals()
    let t = new_tys()
    let a = parse(src)
    let m = declare_module(&g, &a, "main")
    declare_types(&t, &g, &a, m)          // signatures: functions, methods, fields, variants
    let r = check_types(&t, &g, &a, m)    // bodies
    r.diags.n                              // findings; r.node[n] is the type of node n

Modules a module imports from are declared (both `declare_module` and `declare_types`) first.

## Layout

- `src/ty.cf` — the type table: interned types, equal types have equal ids.
- `src/type_ref.cf` — printing types (`show`) and the byte-text `Out` used by tests.
- `src/reader.cf` — `Reader`: spelled types (`&mut [u8]`, `(int, str)`, `Box<int>`) to ids,
  and the signature of a function in a module's scope.
- `src/declare.cf` — the declaration pass; types go in `ty` of the symbol rows.
- `src/typer.cf` — `Typer`: expressions, statements, calls, generics.
- `src/types_diag.cf` — findings (the Rust compiler's E100…E112 numbers).
- `src/types_probe.cf` — helpers for tests.

## Testing

    coffee test --test-mode -I ../sema/src -I ../parser/src -I ../lexer/src -I src

`tools/corpus.py` type-checks every `.cf` file of the repository (std, lexer, parser, sema,
types and their tests, the example programs) as one program: it must report nothing, and it
prints what share of the expression nodes got a type. `tools/differential.py` compares the
verdicts with the Rust compiler on the programs in the Rust test suite: no program it accepts
may get a finding here (`MISSED=1` lists the ones it rejects and this does not).

## Not yet

C functions from `.cfc` files are untyped (a call to one is `Error`, silently); ownership and
borrowing are the next stage. Aliases are transparent where Rust makes them newtypes.

## Where it stands

Corpus: 52 files, no findings, 99.6% of the expression nodes typed (the rest are calls into
libc, which has no signatures yet). Differential against the Rust compiler on 896 programs:
673 both accept, 56 both reject, 0 false positives. The 74 programs it rejects and this
does not are mostly ownership and borrow errors (the next stage), undefined names and
declarations (`coffee/sema` reports those), and C functions called with the wrong types.
