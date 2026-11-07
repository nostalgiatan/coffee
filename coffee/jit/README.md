# jit

`cfjit prog.cir` runs a `.cir` module: it reads the IR object, generates machine code in memory, loads the flat image and calls `main`. No front end, no linker, no C library.

```bash
coffee2 prog.cf --emit-cir -o prog.cir      # program + runtime lowered as one module
coffee2 jit/src/main.cf -I jit/src -I driver/src -I ir/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src --static -o cfjit
cfjit prog.cir
```

`tests/compare.sh <coffee2> <cfjit> <stage dir>` runs each test file of a stage natively and through `cfjit` and compares exit code and output. All of `compat`, `lexer`, `parser`, `sema`, `types` and `own` agree (90 files).

`cfjit prog.cir --sandbox` runs the code confined to a 4 GB linear memory (every access goes through `gs` in 32 bits); a bad access, a division by zero or a stack overflow ends the run with a trap (exit 139 and the signal on stdout). `exit`, `mmap`, and `read`/`write` on the allowed standard streams (`--allow=stdin,stdout,stderr`, a subset; default all three) work as system calls; the rest fail with -ENOSYS. Quotas: `--mem=MB` limits the heap, `--cpu=MS` ends the run (trap, signal 14) after that much wall time, also when it waits in a read.

Before the sandbox code runs, `ir/src/sbxcheck.cf` checks its machine code (only what the generator emits, memory only through `gs` or the frame, direct jumps to instruction starts); a refusal exits with 2.

`SANDBOX=1 tests/compare.sh ...` compares exit codes and output.

`cfjit prog.cir --cache` keeps the generated image next to the `.cir` (`prog.cir.cim`, `prog.cir.sbx.cim`) and uses it while the `.cir` is unchanged (`tea jit` passes it).

`cfjit` runs the IR checker on every function and refuses a module that fails it (`--no-verify` skips it); `--no-run` stops after the image is made. `cfjit prog.cir --time` prints the start-up phases (read_cir, prune, generate, image) in milliseconds on stdout. `coffee2 -O0 --emit-cir` makes the unoptimized module.

Limits of this first version: the image is mapped read-write-execute in one piece; a module may only call system calls (the runtime is lowered into it), anything else is refused; no sandbox yet. See `docs/explanation/jit-design.md`.
