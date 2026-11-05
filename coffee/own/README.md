# coffee/own

The ownership and borrow checker of Coffee, written in Coffee. It reads the typed tree of
`coffee/types` (the types of declarations and the storage table decide what is a resource) and
reports moves of things that are still needed, uses after a move or `rm`, borrows that clash,
and references to locals that escape. The rules are in [RULES.md](RULES.md).

    let r = check_types(&t, &g, &a, m)
    let f = check_own(&t, &g, &a, &r)
    f.diags.n          // findings; oe_number(f.diags.code[i]) is the Rust compiler's number
    f.moved_at[n]      // 1 where name node n is an implicit move (the last use)
    f.exit_of[d]       // for declaration node d: 1 owned at scope end, 2 gone, 3 on some paths

## How it works

Each function becomes a graph of events (`Flow`). Liveness runs backwards, move state and
loans run forwards to a fixed point, and one last pass reports. Because "is it used again?" is
answered on the graph, `return`, `break`, `continue` and loops need no special cases, and
`s` is moved exactly when no later path uses it.

## Layout

- `src/own_diag.cf` — findings (E300 E301 E302 E303 E400 E401).
- `src/own.cf` — `Flow`: the builder, liveness, move state, loans, the driver `check_own`.
- `src/own_probe.cf` — helpers for tests.
- `tests/` — Coffee tests; `tools/corpus.py` runs the checker on every `.cf` of the repository
  (must give no findings), `tools/differential.py` compares it with the Rust compiler on the
  programs of the Rust test suite (no false positives allowed), `tools/probe.py` prints
  findings for chosen programs.

Run the tests: `coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src`.
