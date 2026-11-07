# jit

`cfjit prog.cir` runs a `.cir` module: it reads the IR object, generates machine code in memory, loads the flat image and calls `main`. No front end, no linker, no C library.

```bash
coffee2 prog.cf --emit-cir -o prog.cir      # program + runtime lowered as one module
coffee2 jit/src/main.cf -I jit/src -I driver/src -I ir/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src --static -o cfjit
cfjit prog.cir
```

`tests/compare.sh <coffee2> <cfjit> <stage dir>` runs each test file of a stage natively and through `cfjit` and compares exit code and output. All of `compat`, `lexer`, `parser`, `sema`, `types` and `own` agree (90 files).

`cfjit prog.cir --sandbox` runs the code confined to a 4 GB linear memory (every access goes through `gs` in 32 bits; `--sandbox=base` does the same with the start of the memory in a register, the form other processors can have); a bad access, a division by zero or a stack overflow ends the run with a trap (exit 139 and the signal on stdout). `exit`, `mmap`, and `read`/`write` on the allowed standard streams (`--allow=stdin,stdout,stderr`, a subset; default all three) work as system calls; the rest fail with -ENOSYS. Quotas: `--mem=MB` limits the heap, `--cpu=MS` ends the run (trap, signal 14) after that much wall time, also when it waits in a read.

Before the sandbox code runs, `ir/src/sbxcheck.cf` checks its machine code (only what the generator emits, memory only through `gs` or the frame, direct jumps to instruction starts); a refusal exits with 2.

`SANDBOX=1 tests/compare.sh ...` compares exit codes and output.

`cfjit prog.cir --cache` keeps the generated image next to the `.cir` (`prog.cir.cim`, `prog.cir.sbx.cim`) and uses it while the `.cir` is unchanged (`tea jit` passes it).

`cfjit` runs the IR checker on every function and refuses a module that fails it (`--no-verify` skips it); `--no-run` stops after the image is made. `cfjit prog.cir --time` prints the start-up phases (read_cir, prune, generate, image) in milliseconds on stdout. `coffee2 -O0 --emit-cir` makes the unoptimized module.

## Linking in memory

The image is laid out in pages with their own rights: the code and the stubs of the external functions (read, execute), the rodata (read), the data and the slots the stubs jump through (read, write). Nothing is writable and executable at once. A call of a function the module does not define goes through a stub and a slot; the loader fills the slots with addresses from a table of names (`Exports` in `src/loader.cf`), which holds what the modules loaded before define.

`cfjit main.cir --with=lib.cir --with=more.cir` loads the modules of `--with=` first, in order, each able to call what the ones before it define, and then `main.cir`. A module is a `.cir` made by `coffee2 --emit-cir`; it needs no `main`. What it exports are its functions (`c fn` keep their exact names); the program that calls one declares it in a `.cfc` (`c fn twice(x: int) => int:`) and imports it with `use twice in lib of c`. A call nothing offers stops the load with the name (`cfjit: the module calls 'twice', which nothing offers`). `--cache` keeps the symbols and the imports of the image (format `CIM2`), so a kept image links the same way.

Each module has its own copy of the runtime (its own heap): memory one module allocates can be freed in another, but a pointer into a module's data is that module's business. The sandbox (`--sandbox`) still refuses any call outside the module.

Not yet: data symbols shared between modules, unloading a module, replacing a function in a loaded module, host functions given by an embedding program (the table is there, `Exports.add(name, address)`; the command line has no way to name one), other systems than Linux.

## Debugging aids

- `cfjit prog.cir --lines` writes `prog.cir.lines` (`F name`, `L offset line`; the line is a line of
  `coffee2 --emit-flat` output, the offset is from the start of the function).
- `cfjit prog.cir --perf-map` writes `/tmp/perf-PID.map` once the image is loaded, so `perf` names JIT addresses.
- `coffee2 prog.cf --lines file -g` does the same for AOT code, with `file:line` instead of flat lines;
  `coffee/driver/tools/cfline.sh exe file addr` looks an address up.
- `coffee2 prog.cf -g` also makes DWARF (lines, call frame information, and with `--static`/`--dynamic` parameters, locals and types), see `docs/reference/cli.md`.
- JIT code under gdb: `coffee2 prog.cf -g --emit-cir -o prog.cir` writes `prog.cir.map` (the line map, the variables and the types) beside the module. `cfjit prog.cir --gdb` generates the code (not from the cache), maps it at the fixed address `0x40000000`, writes `prog.cir.dbg` (an ELF file with the DWARF of the generated code) and `prog.cir.gdb` (`add-symbol-file prog.cir.dbg 0x40000000`). Then:

      gdb -x prog.cir.gdb -ex run --args cfjit prog.cir --gdb
      (gdb) break sum
      (gdb) continue

  With a debugger attached `cfjit --gdb` raises SIGTRAP once the code is in memory, so `run` stops there and the breakpoints can be set before `continue`. Run it from the directory of the `.cir` (the script names `prog.cir.dbg` as given).
- Not yet: line marks in `--sandbox` code maps, variables in `--incremental` builds (lines work), a JIT that registers its code through the GDB JIT interface, variables in registers (`-g` keeps them in the frame).
