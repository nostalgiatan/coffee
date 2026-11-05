# coffee/ir

The self-hosted intermediate representation: typed SSA with block parameters (no phi), stored
in columns. See `DESIGN.md` for the design and the order of work.

- `src/op.cf` — opcodes, predicates, the `OpTab` that drives printer, verifier and folder
- `src/rep.cf` — representations (scalars and interned aggregates)
- `src/fun.cf` — `Fun`: the columns and the builder; smart constructors fold, simplify and share
- `src/print.cf` — text form (what tests compare against)
- `src/verify.cf` — well-formedness: terminators, edge arity/types, op types, dominance
- `src/ssa.cf` — block parameters added after the edges exist; removing the trivial ones
- `src/lower.cf` — typed tree to IR, SSA built directly (Braun et al.): scalars, locals, `if`, `while`,
  `for` over a range, `break`/`continue`/`return`, calls of functions and C functions; locals
  whose address is taken live in a slot; classes (literal, fields, methods, parents), `buf<T>`,
  arrays and `len`, with bounds checks
- `src/drive.cf` — test support: source text through the front end, IR and clang, then run
- `src/emit.cf` — LLVM text: phis for block parameters, slots in the entry block, `Syms` for callee names

Steps 1 to 4 are done (builder, printer, verifier; emitter; lowering of scalars, control flow and
calls; slots, classes, `buf<T>`, arrays). Lowering reads the typed tree of `coffee/types`, so the
tests need its paths:

    coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src

Field offsets and sizes come from the storage table of `coffee/types`; `test_layout.cf` pins them.
Memory is not freed yet (step 5), so programs leak until ownership is lowered.

Not lowered yet (a function that uses one is reported as `bad`, not emitted): strings beyond
literals and `len`, enums, `match`, `raise`, generics, `for` over collections, ownership.
