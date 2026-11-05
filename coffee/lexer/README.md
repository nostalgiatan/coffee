# coffee/lexer

The lexer of the second-generation (self-hosted) Coffee compiler, written in Coffee.
It is the first stage handed over from the Rust compiler: source text in, a token table out.

## API

```
use * in lexer

let src = "fn add(a: int) => int:\n    return a\n"
let t = lex(&src)            /#/ Tokens
t.count                      /#/ number of tokens, last one is always Eof
t.kind[i]                    /#/ int(Tk.X); compare with int(Tk.Ident) etc.
t.start[i], t.end[i]         /#/ byte range [start, end) of the lexeme in `src`
t.line[i], t.col[i]          /#/ 1-based position
```

A token is five `buf<int>` columns, so the table needs no per-token allocation and the
lexemes are never copied. Kinds live in `src/token.cf` (`enum Tk`); the keyword block
(`KwFn..KwVoid`) and the binary-operator block (`Plus..Ge`) are contiguous.

## What it does

- Python-style layout: `Newline`, `Indent`, `Dedent` tokens; blank and comment-only lines are
  ignored; a newline inside `()[]{}` or after a trailing binary operator / `=` continues the
  statement, except after the `int(N)+` / `int(N)-` size suffix.
- Comments: `/#/ line` and `/#* block *#/`.
- Literals: decimal, `0x`/`0b`/`0o`, `_` separators, floats with exponent, strings with
  `\n \t \r \\ \" \' \0 \xHH`, `f"..."`, char literals, `#name` error listeners.
- Malformed input never aborts: it becomes an `Error` token (unterminated string/comment, bad
  escape, bad number or char, stray byte, unbalanced bracket, tab indentation, inconsistent dedent).

Known limit: an f-string is scanned as a plain string, so a quote inside `{...}` ends it.

## Layout

| file | role |
| --- | --- |
| `src/token.cf` | `enum Tk` |
| `src/lexer.cf` | `Tokens`, `lex` |
| `src/check.cf` | assertion helpers for tests (`miss_shape`, `miss_text`, ...) |
| `src/dump.cf` | token printers for debugging |
| `src/main.cf` | demo entry point |
| `tests/test_*.cf` | one program per area; each exits with its number of failed checks |

## Running

```bash
cd coffee/lexer
coffee test -I src      # all tests
coffee --bin            # build ./target/debug/lexer
```

## Verification

- `tests/test_*.cf` pin the behaviour per area, including error recovery and the `;`, UTF-8 and
  `/#/`-inside-a-string cases found while verifying.
- `check.cf::invariants` is asserted on a realistic program: ranges ordered and inside the source,
  positions monotonic, Indent/Dedent balanced, and the text between tokens only blanks or comments.
- `tools/fuzz.py` is a differential fuzzer against `tools/ref_lexer.py`, an independent
  reimplementation of the same spec: tens of thousands of random token-soup sources agree.
  It is not part of `cargo test` (needs Python): `COFFEE=… python3 tools/fuzz.py <seed> <cases>`.
- All of `library/std`, `tests/coffee_programs`, this project's own sources and the code blocks
  of `SYNTAX.md` lex with zero `Error` tokens.
- A non-ASCII character outside a string or comment is one `Error` token, not one per byte.
