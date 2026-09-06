# Last-use implicit move

Reduce `mv` ceremony. **Not** implicit `clone`. `=` between two live resources stays wrong unless the source is dead after this statement.

## Rule

For a **simple variable** resource `a` (not `p.x`, not `a[i]`):

If this statement is the **last use** of `a` in the rest of the function (including no use on other CFG paths that still execute after), then these are moves (same as `mv a b`):

- `let b: T = a`
- `b = a`
- argument `foo(a)` when the parameter is that resource type (by value)
- `return a`

If `a` is used later on any path, keep today’s error (let/`=` copy) or require `mv`/`clone`. Borrowed `a`: still cannot move.

Loops: a use in a loop body that can iterate again is **not** last-use (conservative: any name used in a loop that also appears later in the same loop is live). `break` before later use: still conservative “used in loop” ⇒ not last-use unless the only uses are after a point that cannot iterate (v1: **if the variable is referenced anywhere in a loop that contains this statement, do not implicit-move** except `return` from inside the loop).

Explicit `mv` / `clone` unchanged.

## Checker

New `src/types/last_use.rs`: given function body stmts, compute a set of (stmt identity or span, name) that are last uses. Conservative CFG: walk linearly; for `if`, a name is last in then only if not used in else **and** not used after the if; etc. Prefer conservative false negatives (still ask for `mv`) over false positives (use-after-move).

Wire `check_variable_decl` / `check_assignment` to allow Variable RHS when last-use. Call args: if last-use, mark value Dropped/moved after the call. Return: same.

Do **not** edit `src/parser/**`. Do **not** edit `Type::App` / `from_str` / `reject_fake_generics`. Touch `stmt.rs` / `call.rs` only at the resource-copy branches. Prefer not rewriting `check_class`.

## Codegen

Resource `let`/`assign` from a Variable that typecheck accepted as move must `compile_move` (invalidate source), not a copy load/store. Call: after passing, mark source moved / skip drop of callee-owned... callee receives ownership; caller must not drop. Match existing `mv` IR.

Avoid `memory_ops/compile.rs` clone path (other agent). Use existing `compile_move`.

## Tests

Create `tests/last_use_move_tests.rs` (do not pile onto `memory_tests.rs`):

- `let b: C = a` then not using `a` compiles; using `a` after still errors.
- `foo(a)` last use compiles.
- `return a` last use compiles.
- still used after `=` errors.
- borrowed then let-copy still errors.
- explicit `mv` still works.

`flock … cargo test --offline --test last_use_move_tests -- --test-threads=1`

No commit, no version bump.
