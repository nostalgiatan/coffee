# coffee/ir: the intermediate representation and the optimizer

Design, written before any code. It replaces the Rust compiler's `src/hir` for the self-hosted
compiler. The Rust MIR is a reference for what has to be handled, not a model.

## Goals

1. **Fewer stages, fewer paths.** One IR, one way to build it, one way to print, check and
   emit it. Every extra pass or representation is a place for two stages to disagree.
2. **Fast to compile.** Every stage linear in the size of the function (or close to it); no
   rebuilding of the IR between passes; unchanged functions are not compiled again.
3. **Better code than a generic pipeline gives.** Pass to LLVM the facts only Coffee knows:
   who owns what, what cannot alias, what a function can touch.
4. **Cheap to extend.** Adding an operation or a type touches one table, not six files.

## What the Rust compiler does that we will not

| there | here |
|---|---|
| AST -> MIR -> SSA (unused) -> LLVM via inkwell | typed AST -> IR -> LLVM text. SSA is built directly |
| a separate CFG builder for `match` and for `for` | lowered in place to the same blocks and `switch` |
| variables are named; a stack slot for each, LLVM's mem2reg fixes it | only variables that must live in memory get a slot; the rest are SSA from the start |
| drop flags in memory | a drop flag is a one-byte block parameter, and usually folds away |
| types as strings, re-parsed by the backend | type ids of the checker's `Tys`; representation from the storage table |
| optimization = LLVM's pipeline | a short pipeline of our own (ownership, effects, inlining) and then LLVM's |

## Pipeline

    parse -> sema -> types -> own -> lower -> [effects, inline, own-opt, cleanup] -> emit -> clang
                                       \_ folds and numbers values as it builds _/

Five of our stages after `own`: **lower** (builds SSA, folds and shares as it goes), **summarize**
(per-function effects), **optimize** (a fixed list of linear passes), **emit** (LLVM text with
attributes, glue for drop and clone), and the cache in front of the last two. There is no MIR,
no separate SSA construction, no legalization pass.

## The IR

A function is a handful of `buf<int>` columns, as the AST and the type table are:

    instruction i:  op[i]  rep[i]  a[i] b[i] c[i]  aux[i]  next[i] prev[i]  uses[i]  fwd[i]
    block b:        first[b] last[b]  nparams[b] params[b]  (preds, succs from terminators)

- Instructions are numbered once and never renumbered. Removing one unlinks it from its block
  (`next`/`prev`), inserting links it in: O(1), no shifting.
- The result of instruction `i` is the value `i`. Operands are instruction ids. Constants are
  instructions too, shared (below), so an operand is always a value.
- `fwd[i]` is a forwarding pointer: replacing all uses of `v` by `w` is `fwd[v] = w`, one
  store. Passes read operands through `fwd` (path-compressed); there is no walk over users.
  `uses[i]` is a count kept by the builder and by the few passes that delete.
- **Block parameters instead of phi**: a block has typed parameters, a jump passes arguments.
  A drop flag, a loop variable and a joined value are all the same thing.

### Types

The IR types are the *representations* of the storage table, a closed set: `i8 i16 i32 i64`,
`f32 f64`, `ptr`, and aggregates `{..}` for tuples and the fat `{ptr, i64}` of `buf<T>` and
slices. Aggregates are SSA values (`extract`, `insert`). Each value also keeps its Coffee type
id (`ty[i]`): `drop` and `clone` are defined by it. `bool` is `i8`, `void` has no value,
a handle is a `ptr`.

### The operation table is the definition

There is one table, `OpInfo`, with one row per operation: its name, operand count, how the
result type follows from the operands, and flags: `pure`, `commutative`, `reads`, `writes`,
`terminator`, `consumes` (takes ownership of an operand), `may-trap`, and the LLVM mnemonic.
The **printer, verifier, folder, value numbering, dead-code removal, effect analysis and the
emitter are all driven by it.** A new operation is one row and one case in the folder.

Operations:

- constants: `const`, `str.lit` (static, never freed), `undef`
- arithmetic and logic, `cmp.<pred>` (result `i8`), conversions, `extract`, `insert`
- memory: `slot`, `load`, `store`, `field`, `index`
- calls: `call` (Coffee), `ccall` (C, ABI facts from the `.cfc` table), `calli`
- ownership: `alloc`, `free`, `drop`, `clone`, `own.move`
- terminators: `br`, `condbr`, `switch`, `ret`, `unreachable`

`raise` is not an operation: lowering makes it a call of the listener and a return, or a call
of the abort routine and `unreachable`.

## Lowering builds good IR the first time

The builder has *smart constructors*. Every `add`, `cmp`, `field`, ... goes through one entry:

1. **fold** if the operands are constants (`1 + 2` never exists as an instruction),
2. **simplify** by identities (`x + 0`, `x * 1`, `x & x`, comparison of a value with itself),
3. **share**: a pure operation with the same operands in a dominating position is the same
   value (a hash table keyed by `op, a, b, c, aux`, scoped to the dominator tree as the
   lowering walks it).

So local constant folding, algebraic simplification and common-subexpression elimination are
not passes; the IR is born reduced. The later passes see far fewer instructions.

### SSA without a conversion pass

Variables are SSA from the start (Braun et al.: each block keeps the current definition of
each variable; a block with several predecessors gets a parameter on demand; a sealed block
whose parameter turned out trivial is folded away). This is linear and needs no dominator tree
or dominance frontier.

A variable gets a `slot` only if something takes its address: the ownership checker already
knows exactly which variables are borrowed by pointer (`Borrow` events on value types). Handle
types (`str`, classes, `object`, raw `buf`) are their own pointer, so `&x` of them needs no slot.

### Ownership in the IR

- an implicit move (`moved_at`) is `own.move`; the variable is dead afterwards
- scope end and `rm`: `exit_of` says owned on all paths (a plain `drop`), none (nothing), or
  some (the drop is guarded by the flag: an `i8` block parameter, 1 while owned, cleared at the
  move). Jump threading and constant folding remove most flags, because most "some" cases come
  from `if` branches that can be duplicated or sunk
- **values of `move` types are linear**: consumed once on every path (moved into a call, store,
  aggregate or `ret`, or dropped), never after. This is the ownership result carried inside
  the IR, and the verifier checks it, so a lowering bug is a verifier error, not a double free

## What ownership gives LLVM

This is where a language-aware IR beats a generic one. The emitter derives these from facts
the front end already proved and states them in the LLVM text:

- `noalias` on a parameter that is an owned handle or a `&mut` (the checker allows one `&mut`
  and no other access). LLVM then reorders and keeps values in registers across calls
- `readonly` / `nocapture` on `&T` parameters the function only reads and never stores
- `nonnull` and `dereferenceable(N)` on handles and references, `N` from the storage table;
  `align` from the layout; `inbounds` on every `field`/`index` address
- `nounwind` everywhere (Coffee has no unwinding), `noreturn` on the abort path
- `!range` on `bool` (0..2) and on enum tags (0..variants)
- function attributes from the effect summary below: `readnone`, `readonly`,
  `argmemonly`, `willreturn` where proven

Whether integer arithmetic carries `nsw`/`nuw` depends on the language's overflow rule; the
default is wrapping with no flags, and `--enable-safety` would select checked operations.

## Effects: one summary per function

A bottom-up pass over the call graph computes for each function: does it read memory, write
memory, only through its arguments, allocate, call C, or never return. It is a few bits, and
it drives:

- removal of a call with no used result when it is pure
- value sharing of pure calls
- the attributes above
- the safety rule of drop sinking (never past a call that may observe the order)
- **alias facts from ownership**: two owned roots never alias; a reference aliases its loan's
  root and nothing else. An owned object that was not passed to a call cannot be changed by
  that call, so loads of its fields survive the call. No generic alias analysis is needed.

## Optimizer: a fixed list, each pass linear, run once

    summarize  ->  inline  ->  own-opt  ->  cleanup  ->  (emit)

1. **summarize**: effects, above. Also the call graph and use counts.
2. **inline**: calls of small functions (by instruction count) and of functions called once,
   not recursive, in bottom-up order, so the callee is already optimized. The ownership of
   arguments is transparent in the linear form.
3. **own-opt**, each a local rewrite over the linear form:
   - drop of a type whose drop kind is `nothing`, or of a value whose flag is constant 0: gone
   - `clone x` where `x` has no later use: `own.move x`. (A deep copy of a string whose
     original dies at once is never made.)
   - `alloc` whose value never escapes (loaded, stored through, dropped in the same function):
     its fields become SSA values, no heap and no `free`
   - `clone` of a `str.lit` that is only read: the literal itself
   - `drop` moved to just after the last use of the value, when no intervening call can
     observe the order
4. **cleanup**: unreachable blocks, block merging, jump threading (which folds drop flags),
   dead code (pure and unused, by the `OpInfo` flags). Because the builder folded and shared,
   there is no big instcombine.

A pass that changes nothing costs one scan. There is no fixed-point pipeline: the order above is
chosen so one run reaches the result (inlining first exposes everything the rest needs).
`-O0` runs only what the builder did and cleanup; `-O1` and up add the rest; clang's own
`-O` is a separate, optional stage after emit.

## Emit

Prints LLVM text from the columns into one byte buffer, in block order, driven by the `OpInfo`
table. `drop`/`clone`/`alloc`/`free` expand here, not in a separate pass: the emitter keeps
one glue function per type (name from the storage table, generated on first use, from the
type's drop and clone kind) and prints a call, or the few instructions inline for a `free`
or a field-by-field drop of a small class. `own.move` prints nothing.

## Incremental builds

Lowering, optimizing and emitting a function depend only on its typed body and the signatures
and effect summaries of what it calls. The key is a hash of those plus the compiler hash
(`COFFEE_COMPILER_HASH`). The emitted LLVM text of each function is cached under that key. An
edit changes one function's key and those of its callers only if the callee's summary
changed. This is finer than the current per-file object cache and costs one hash per function.

## Generics, nested functions, C

- Generics are monomorphized by a worklist with a memo table keyed by (function, type
  arguments); an instance is lowered once and named from the storage table. The checker already
  substitutes.
- Nested `fn` are lifted to top-level functions with a unique name; closures cannot capture, so
  there is no environment.
- C functions carry their `.cfc` signature; `ccall` keeps variadic and integer-width facts and
  the emitter declares exactly the ones used.

## Checking it

- A printer of the IR text, with golden tests for each lowering shape and each pass
  (the program before, the IR, the IR after).
- The **verifier**: one terminator per block; argument counts and types; dominance; operand
  types by `OpInfo`; **linearity**. It runs in tests after every pass and in debug builds.
- **Run, don't read.** The 896 programs extracted from the Rust test suite and
  `tests/coffee_programs` are compiled by both compilers; exit codes and output must match.
  The Rust compiler is the oracle until ours compiles itself.
- Ownership at run time: a tracking allocator requires no leak and no double free.
- Compile-time budget: a benchmark of lowering plus optimizing plus emitting the whole
  `coffee/` tree, tracked per commit, with the instruction count after each pass.

## Order of work

1. `OpInfo`, the columns, builder with smart constructors, printer, verifier. Hand-built IR in
   tests.
2. Emitter for scalars and control flow; compile hand-built IR with clang and run it.
3. Lowering of scalars, control flow, calls, with direct SSA construction: first programs run.
4. Slots for borrowed values; classes, `str`, `buf`, arrays, tuples (layout from the storage
   table).
5. Ownership: `own.move`, drop flags as parameters, `drop`/`clone`/`alloc`, glue in the emitter.
6. Enums and `match` (a decision tree to `switch`), `for`, `raise` and listeners, generics.
7. Passes in the order above, each with before/after tests; attributes from ownership.
8. Per-function cache; differential run over the corpus; benchmark.

## Decisions to confirm

- LLVM **text** through `clang`; no LLVM binding (self-hosting forces it).
- Block parameters instead of phi; SSA built directly, no mem2reg.
- Fold, simplify and share while building, instead of separate passes.
- Drop flags as block parameters, not memory.
- Ownership facts exported to LLVM as attributes, and `noalias` from `&mut` and owners.
- One `OpInfo` table drives printer, verifier, folder, DCE, effects and emitter.
- Per-function incremental cache.
- Integer overflow: wrapping with no `nsw`/`nuw`, unless the language says otherwise.
- The Rust compiler stays as test oracle until the Coffee one can compile itself.
