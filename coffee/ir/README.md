# coffee/ir

The self-hosted intermediate representation: typed SSA with block parameters (no phi), stored
in columns. See `DESIGN.md` for the design and the order of work.

- `src/op.cf` — opcodes, predicates, the `OpTab` that drives printer, verifier and folder
- `src/rep.cf` — representations (scalars and interned aggregates)
- `src/fun.cf` — `Fun`: the columns and the builder; smart constructors fold, simplify and share
- `src/print.cf` — text form (what tests compare against)
- `src/verify.cf` — well-formedness: terminators, edge arity/types, op types, dominance
- `src/emit.cf` — LLVM text: phis for block parameters, slots in the entry block, `Syms` for callee names

Steps 1 (builder, printer, verifier) and 2 (emitter, run through clang) are done. Test with
`coffee test --test-mode -I src` in this directory.
