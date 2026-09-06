# Error listener (`#name`) — 2026-09-05

## Locked

- `raise` without a listener: print to stderr and `exit(1)` (unchanged).
- `fn f(...) #on_err => R`: only `raise` **written in `f`'s body** calls `on_err`; **no** call-stack penetration.
- `on_err` return type is `R`; that value is `f`'s return.
- `raise` inside `on_err` (or any listener): always terminate, even if the listener has `#…`.
- `#on_err` names an existing function; it is not a keyword. `f` and `on_err` must be different names.
- Listener signature: `fn on_err(err: Error) => R` with builtin `Error { code: int, note: str, e: object }`.
- Exception types are builtin `Error` and subclasses (`of Error` / descendants); extra fields and methods are allowed. The listener still sees only the `Error` prefix. See [2026-09-05-error-base-class.md](2026-09-05-error-base-class.md).
- IR: unmarked functions keep abort `raise`. Marked functions: `raise E` → build `Error` → `call on_err` → `ret`. No `invoke`/`setjmp` on ordinary calls.

## Non-goals

- Unwind through callees or C frames
- `try` / `catch`
- Resume after `raise`
