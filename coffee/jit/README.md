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

## As a library, for other languages

`jit/tools/build-lib.sh <coffee2> <dir>` builds `libcfjit.a`, `libcfjit.so` and `cfjit.h` (`jit/include/cfjit.h` is the reference: the functions, the codes they return, the rules). `--pic` makes the compiler put the target's system call stub (`tg_syscall_stub` in `ir/src/target.cf`) into the object, so `cc` only links the shared object; the library otherwise uses the host's C library. The API is plain C with `long` and pointers, so any language with a C FFI can use it:

| call | meaning |
|---|---|
| `cfjit_new()` / `cfjit_free(ctx)` | a context (up to 16); free unmaps its modules |
| `cfjit_register(ctx, "name", fn)` | offers a C function of the host to modules loaded later |
| `cfjit_load(ctx, bytes, n, err, errn)` | loads a `.cir` (from memory, so it can come from a file, a network or a database); a module's number or a negative code and the reason |
| `cfjit_sym(ctx, "name")` | the address of a `c fn` (callable with the C convention), or of a datum (`coffee_g_<name>` for a module-level `let`), or of a host function |
| `cfjit_replace(ctx, n, bytes, len, err, errn)` | the new module is linked first; the other modules' calls into the old one are rebound to the same names in the new one; the old one stays mapped |
| `cfjit_unload(ctx, n)` | unmaps a module; refused (-2) while another module calls into it |

`tests/embed_test.c` (C) and `tests/embed_test.py` (Python, ctypes, the host function a Python callback) are complete examples; both run in `tea test`. A module declares what it takes from the host in a `.cfc` (`c fn host_scale(x: int) => int:`) and imports it with `use host_scale in host of c`; what it gives is a `c fn`. Types that cross are the C ones: `int`, `float`, `bool`, `string` (a C string), `object` (a pointer).

Rules: one thread in the library at a time (the compiler's switches are global); a module's code runs on the caller's thread and stack; `cfjit_replace` leaves the old module mapped, so the host frees it with `cfjit_unload` when nothing runs in it (replacing is per module, not per function: a call inside a module is direct, a call between modules goes through a slot); each module has its own heap, so memory allocated in one can be freed in another but the allocators do not share state; a program without `main` that is built as a library gets its module-level initializers from a function named `cf_init` (`embed.cf` calls it on first use).

Data symbols are visible to the host (`cfjit_sym`), not shared between modules: the language has no way to say a name is data from elsewhere, so modules share through `c fn` accessors. The sandbox still refuses calls outside the module; `docs/explanation/sandbox-design.md` has the design for host calls from it.

Not yet: unloading a module's data separately, other systems than Linux (macOS and Windows need their own loaders; see `docs/explanation/jit-design.md`), a JIT for aarch64 (the aarch64 backend session's step 7).

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
