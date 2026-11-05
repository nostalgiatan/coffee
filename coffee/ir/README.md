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
  `for` over a range, `break`/`continue`/`return`, calls of functions and C functions
- `src/emit.cf` — LLVM text: phis for block parameters, slots in the entry block, `Syms` for callee names

Steps 1 (builder, printer, verifier), 2 (emitter) and 3 (lowering of scalars, control flow and
calls) are done. Lowering reads the typed tree of `coffee/types`, so the tests need its paths:

    coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src

Not lowered yet (a function that uses one is reported as `bad`, not emitted): classes, fields,
indexing, tuples, `buf`, strings beyond literals, enums, `match`, `raise`, ownership.
