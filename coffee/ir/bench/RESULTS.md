# Benchmarks: compile time and run time

Measured 2026-11-05 on x86_64 Linux, 32 cores, `clang` 21, Rust `coffee` release build, best of 3
(`bench/run.sh`; the IR text comes from `bench/emit.cf`). One machine, one run of the script: treat
differences under ~10% as noise. Not measured on the Termux/aarch64 development device.

Programs: `fib` (`fib(38)`, recursion), `acc` (400M calls of a small method and a small function in a loop),
`sieve` (sieve of Eratosthenes on a 20M `buf<int>`).

## Run time of the compiled program (seconds)

| program | Rust coffee -O0 | -O2 | -O3 | our IR, opt off, clang -O0 | opt on, clang -O0 | opt off, clang -O2 | opt on, clang -O2 |
|---|---|---|---|---|---|---|---|
| fib   | 0.196 | 0.131 | 0.130 | 0.154 | 0.156 | 0.070 | 0.068 |
| acc   | 1.427 | 0.316 | 0.316 | 0.699 | 0.620 | 0.252 | 0.253 |
| sieve | 0.919 | 0.802 | 0.794 | 0.802 | 0.823 | 0.707 | 0.690 |

- Our IR text compiled by clang is as fast as or faster than the Rust compiler's code at the same level
  (acc -O0 is 2x faster: the Rust backend keeps every local in memory).
- Our own optimizer pays off only when clang does not optimize (acc at -O0: -11%); with clang -O2 it
  changes nothing measurable, because LLVM already inlines and cleans up these programs. Its value
  has to come from what LLVM cannot know (ownership: alloc/free and clone/drop pairs) and from a
  back end that is not LLVM.

## Compile time

- Our front end + IR + optimizer + text emission (parse, sema, types, lowering, `optimize`), per small
  program: about 17 ms, optimizer on or off alike (300 runs take 4-5 s; `time()` has a 1 s
  resolution). This is the compiler itself compiled at -O0 by the Rust compiler.
- `clang` on the produced `.ll`: 60-80 ms (process start dominates).
- Rust `coffee` on the same sources: 60-100 ms to a linked binary.
- The Rust compiler on the self-hosted pipeline (lexer, parser, sema, types, own, ir; about 8k lines
  of Coffee): one `coffee test` file that imports it takes about 15 s to compile and run. The whole
  `coffee/ir` test suite (13 files) takes about 2 min. Both include the AddressSanitizer build.

## Found while measuring

- Rust back end: `-O1..-O3` objects used a non-PIC relocation model on Linux, and linking failed
  against a PIE-default linker (`R_X86_64_32 against .rodata.str1.1`). Native targets are now always PIC.
- IR inliner: a directly recursive callee was inlined 16 times into its caller (fib: 812 -> 6343 bytes).
  Such callees are no longer inlined (`test_module.cf` #13).
