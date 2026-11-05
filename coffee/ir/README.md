# coffee/ir

The self-hosted intermediate representation: typed SSA with block parameters (no phi), stored
in columns. See `DESIGN.md` for the design and the order of work.

- `src/op.cf` — opcodes, predicates, the `OpTab` that drives printer, verifier and folder
- `src/rep.cf` — representations (scalars and interned aggregates)
- `src/fun.cf` — `Fun`: the columns and the builder; smart constructors fold, simplify and share
- `src/print.cf` — text form (what tests compare against)
- `src/verify.cf` — well-formedness: terminators, edge arity/types, op types, dominance
- `src/ssa.cf` — block parameters added after the edges exist; removing the trivial ones
- `src/opt.cf` — the optimizer: constant branches become jumps, unreachable blocks go, a block
  with one predecessor joins it, unused instructions and block parameters (also those only
  passed back to themselves) go, an `alloc` that is only freed goes, `clone x` whose original is
  dropped at once is `x`; every pass keeps the use counts right, and the verifier runs after
- `src/lower.cf` — typed tree to IR, SSA built directly (Braun et al.): scalars, locals, `if`, `while`,
  `for` over a range, `break`/`continue`/`return`, calls of functions and C functions; locals
  whose address is taken live in a slot; classes (literal, fields, methods, parents), `buf<T>`,
  arrays and `len`, with bounds checks; ownership: a flag variable per owning variable, moves
  clear it, scope ends / `return` / `break` / `rm` and reassignment drop through it, temporaries
  that were only borrowed are dropped at the end of the statement; `drop.N` / `clone.N` glue
  functions are generated per class type; enums (a bare tag, or a heap block with the tag and the
  payload of the live variant), `match` on enums and integers (arms tried in order, guards,
  payload names bound to copies), `for` over arrays, slices and `buf` of plain elements; `raise`
  (with a listener: an `Error` is built, the listener called and its result returned after the
  scope is released; without: message to stderr, then exit 1); generics by instantiation: a call
  of a generic function or a method of a generic class asks for `name__<args>`, lowered once per
  set of type arguments
- `src/drive.cf` — test support: source text through the front end, IR and clang, then run
- `src/emit.cf` — LLVM text: phis for block parameters, slots in the entry block, `Syms` for callee names

Steps 1 to 7 are done, the optimizer without effect summaries, inlining and attributes (builder, printer, verifier; emitter; scalars,
control flow and calls; slots, classes, `buf<T>`, arrays; ownership; enums, `match`, `for`, `raise`, generics). Lowering reads the typed tree of `coffee/types`, so
the tests need its paths:

    coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src

The lowering tests compile with AddressSanitizer: a leak or a double free fails them (exit 99).
Field offsets and sizes come from the storage table of `coffee/types`; `test_layout.cf` pins them.

Not lowered yet (a function that uses one is reported as `bad`, not emitted): nested patterns, generic
methods of their own and static functions of generic classes, `for` over owned elements, arrays of resources, tuples that
own something. `match` is a chain of tests, not a `switch`; LLVM turns it into one.
Temporaries inside `&&`/`||` that were only borrowed are not released.
