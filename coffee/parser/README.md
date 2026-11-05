# coffee/parser

The parser of the second-generation (self-hosted) Coffee compiler, written in Coffee.
It takes the token table of `coffee/lexer` and builds a syntax tree.

## API

```
use * in parser

let a = parse("fn add(a: int, b: int) => int:\n    return a + b\n")
a.root                       /#/ node id of the Program
a.kind[n]                    /#/ int(Nk.X)
a.child[n], a.next[n]        /#/ first child, next sibling (-1 = none)
a.pa[n] .. a.pd[n]           /#/ kind-specific payload: node ids, token ids or flags
a.tok[n], a.end[n]           /#/ token range [tok, end) the node covers
a.err_count                  /#/ number of errors; a.err_code[i], a.err_tok[i] describe them
```

The tree is a table of `buf<int>` columns, not a pointer graph, so building it allocates
only when a column grows. `src/ast.cf` lists the node kinds (`enum Nk`), what each payload
column means for each kind, and the error codes (`enum Ec`). Types are not parsed into
trees: a `TypeRef` node covers the tokens of the type.

## What it does

- Recursive descent with precedence climbing; the table in `bin_prec` matches `SYNTAX.md`.
- Never aborts: a malformed construct records an error (kind, token) and the parser
  resynchronises at the next statement. A nesting deeper than 200 records one `TooDeep`
  error and ends the parse instead of overflowing the stack.
- `c` is an ordinary name except in `c fn` and after `of`.
- `>>` closes two type argument lists (`List<List<int>>`).

Not done yet: no semantic checks, and string and f-string contents are not parsed (an
`FStr` node is the literal token).

## Layout

| file | role |
| --- | --- |
| `src/ast.cf` | `enum Nk`, `enum Ec`, payload documentation |
| `src/parser.cf` | class `Ast`, `parse` |
| `src/tree_shape.cf` | compact text form of a tree (`shape`, `miss_parse`, `miss_error`) |
| `src/tree_dump.cf` | printers for debugging |
| `tests/test_*.cf` | one program per area; each exits with its number of failed checks |
| `tools/corpus.py` | parses the std library, the examples, and the lexer and parser sources |

The shape form prints one letter per node (see `node_char`) and its operands in
parentheses: `1 + 2 * 3` is `P(;([+](i[*](ii))))`.

## Running

```bash
cd coffee/parser
coffee test --test-mode -I ../lexer/src
COFFEE=$(command -v coffee) python3 tools/corpus.py
```

The lexer is a package dependency (`coffee.toml`); `-I ../lexer/src` is for `coffee test`,
which compiles each test as a single file.
