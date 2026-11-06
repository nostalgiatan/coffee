# jit

`cfjit prog.cir` runs a `.cir` module: it reads the IR object, generates machine code in memory, loads the flat image and calls `main`. No front end, no linker, no C library.

```bash
coffee2 prog.cf --emit-cir -o prog.cir      # program + runtime lowered as one module
coffee2 jit/src/main.cf -I jit/src -I driver/src -I ir/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src --static -o cfjit
cfjit prog.cir
```

`tests/compare.sh <coffee2> <cfjit> <stage dir>` runs each test file of a stage natively and through `cfjit` and compares exit code and output. All of `compat`, `lexer`, `parser`, `sema`, `types` and `own` agree (90 files).

`cfjit prog.cir --sandbox` runs the code confined to a 4 GB linear memory (every access goes through `gs` in 32 bits); a bad access, a division by zero or a stack overflow ends the run with a trap (exit 139 and the signal on stdout). Only `exit` and `mmap` work as system calls so far. `SANDBOX=1 tests/compare.sh ...` compares exit codes.

Limits of this first version: the image is mapped read-write-execute in one piece; a module may only call system calls (the runtime is lowered into it), anything else is refused; no sandbox yet. See `docs/explanation/jit-design.md`.
