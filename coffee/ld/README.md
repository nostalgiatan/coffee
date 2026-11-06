# ld — a static linker for the self-hosted compiler

Links the x86-64 ELF objects (`.o`, written by `ir/src/elf.cf`) and `ar` archives into a static
executable without `cc`. No LLVM, no Rust, no libc: it is a Coffee program built by `coffee2`.

    coffee2 src/main.cf -I src -I ../ir/src -I ../driver/src -o cfld
    coffee2 prog.cf --emit-obj -o prog.o
    cfld prog.o -o prog              # a `main` without `_start` gets one that calls it and exits
    cfld [-o out] [-L dir]... file.o file.a -lname ...

The driver still links with `cc`; nothing here changes its default.

## Layout

| file | what |
|---|---|
| `src/link.cf` | `Linker`: reading objects/archives, symbol resolution, layout, `build_image`, `apply_relocs` |
| `src/exe.cf` | `link_exe`: the ELF64 executable around the image (headers, segments, entry) |
| `src/main.cf`, `src/args.cf` | the `cfld` command |
| `src/col.cf` | growable integer columns, little-endian reads/writes |
| `kit/kit.cf` | for the tests: writer of arbitrary objects (`Mk`) and archives (`Ar`), run a program |

## The two halves

`Linker.layout` + `build_image` produce a flat memory image (`Txt`) whose addresses are `rva`s from
the load address; `Linker.apply_relocs(mem, base)` patches it for any `base`. The latter touches no
file, so a JIT can use it: add objects, `layout`, `build_image`, `apply_relocs` with the address the
image was mapped at (`tests/test_image.cf` does this for two bases). `link_exe` is the file writer
on top: it applies the relocations for `0x400000` and cuts the image into `PT_LOAD` pieces
(text, rodata + GOT, data + bss; each starts at the same offset modulo the page size in the file
and in memory, so the file has no page padding).

## What works

- Sections are joined into four groups by flags: text, rodata (read-only, with the GOT), data, bss.
- Relocations: `R_X86_64_64`, `PC32`, `PLT32`, `GOTPCREL`, `GOTPCRELX`, `REX_GOTPCRELX` (a GOT slot,
  no relaxation), `32`, `32S`, `PC64`; range-checked.
- Symbols: global, weak (strong wins in either order), local, section, absolute, common (merged,
  the larger wins); duplicate and undefined names are errors.
- Archives with a symbol index: members are pulled in until nothing is needed, in command-line order
  (tested against the glibc `libc.a` index: only the pulling, not a working link).

## Dropping what nothing reaches

The compiler writes one `.text.<name>` section (and `.rela.text.<name>`) per function, and
`Linker.gc(entry)` (called by `link_exe`) keeps a text section only when `_start` reaches it through
relocations; data sections are all kept, and so is what they point at. `cc` gets `-Wl,--gc-sections`
from the driver for the same effect. (`tests/test_gc.cf`)

## Not yet (for linking against glibc's `libc.a` / `-l` libraries)

- TLS sections and relocations, IFUNC (`R_X86_64_IRELATIVE`), `.init_array`/`.fini_array` and the
  `__init_array_start`-style linker-defined symbols, `_GLOBAL_OFFSET_TABLE_`, `__ehdr_start`.
- `crt1.o`/`crti.o`/`crtn.o` and `libgcc.a`; linker scripts; shared libraries (`-l` finds only `.a`).
- `COMDAT`/`.group`, `SHF_MERGE` string merging, `.eh_frame_hdr`.
- Relaxation of GOTPCREL, a symbol table in the output.

## Tests

    cd ld && tea test --compiler <dir>/coffee2 -j 8

Each test builds objects (with `kit` or with the compiler's own `Asm`/`write_elf`), links them with
`Linker`, writes the executable, runs it and compares the exit code.
