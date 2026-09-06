# Builtin `Error` as exception base class — 2026-09-05

## Locked

- `Error` is a **builtin class** (not a user-definable name): `{ code: int, note: str, e: object }`.
- Source may not declare `class Error` (any fields). Binding/checking must fail with a clear type error; the builtin stays.
- `raise T` is allowed iff `T` is `Error` or a class whose inheritance chain reaches `Error` (`class C of Error`, or `class D of C` where `C` is already an exception class). **Not** iff the name ends with `Error`.
- Exception classes keep normal class capabilities: extra fields, methods, `self`, nested use as values, `of` further subclasses. Do not redeclare `code` / `note` / `e` on the subclass (use the inherited fields). Extra fields must use other names.
- Construction: same as other classes (`C { ... }`, methods). `raise C(...)` call-form remains raise sugar (args still pack into abort text / listener `Error` prefix); it does not replace class construction.
- Listener parameter stays `err: Error`. Subclass layout is parent fields first; the listener may only rely on the `Error` prefix. Extra fields are for Coffee code that has the subclass type.
- `class FooError:` without `of Error` is a normal class and **cannot** be `raise`d.

## Non-goals

- Java-style checked exceptions
- Changing `#on_err` stack rules
- Requiring every `*Error`-named class to inherit `Error`
