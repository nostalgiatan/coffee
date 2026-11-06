# Benchmarks

Run time and code size of the code the self-hosted compiler generates. `run.sh <coffee2>` builds each
program with `--static`, runs it five times and prints the best user time; it also compares the output
with `expected.txt`, so a miscompiled program shows up as WRONG. The expected outputs of `collatz`,
`mandel`, `sort`, `strings` (full size) and of reduced versions of `matmul`, `hash`, `bits` and
`objects` were checked against Python.

| program | what it exercises |
|---|---|
| `fib` | recursion, calls and returns |
| `acc` | small functions that are inlined, a remainder by a constant |
| `sieve` | a 160 MB buffer: limited by memory, not by the code |
| `collatz` | integer branches, shifts and multiplies |
| `mandel` | floating point (every value goes through general registers) |
| `sort` | recursion and loads and stores in a buffer (a quicksort, fifteen times) |
| `hash` | an open-addressing hash set: probing loops over a 2 MB buffer |
| `objects` | allocation and release of small objects, an owner replaced each time |
| `strings` | f-strings, concatenation, `len` and indexing |
| `matmul` | triple loop over three buffers |
| `bits` | a loop of xors, shifts and multiplies kept in registers |

Do not read much into the `code` column of the programs that print: they link the number formatting
runtime, about 50 KB. Times are the best of five user times on a machine that was not idle (load
average 3 to 6), so differences under about 5% are noise.

## Baseline, 2026-11-06

Compiler built from the commit that adds this directory, x86-64, 32 cores.

| program | run (s) | code (B) |
|---|---|---|
| fib | 0.13 | 1616 |
| acc | 0.37 | 2008 |
| sieve | 0.35 | 1720 |
| collatz | 0.30 | 51832 |
| mandel | 0.44 | 52216 |
| sort | 0.56 | 54032 |
| hash | 0.31 | 51656 |
| objects | 0.23 | 51848 |
| strings | 1.05 | 53856 |
| matmul | 0.36 | 52368 |
| bits | 0.39 | 51496 |

The compiler compiling itself (`driver/src/main.cf` with `--static`) takes about 1.4 s of user time
and the executable is 1.47 MB.

## Found while building these

- A number above the largest `int` used to become `-1` silently (`let h = 14695981039346656037`).
  It is now a type error unless the context is an unsigned 8-byte int.

xmm 家之后:mandel 0.44 → 0.26 秒,其余不变。

xmm 家与 fmt_int 改写之后:strings 1.09 → 0.34 秒(整数转文字原来每个数字拼接一次,现在只分配两次)。

优化器一轮之后(GVN、判定条件折叠、循环不变量外提、短路条件用跳转、循环旋转与冷块置后),callgrind 指令数(百万):
fib 2276、acc 7600、sieve 866→784、mandel 2420→2306、objects 6600→5560、sort 3721→3588、matmul 6070→4491、hash 8201→8401、bits 7500、collatz 2690→2864、strings 8873→7341。
collatz 与 hash 略差:块参数的复制(`lea 1(%r9),%r13; mov %r13,%r9`)还没合并。

块参数合并之后(循环里算出的值和它要传给的参数共用寄存器),callgrind 指令数(百万):
mandel 2305、objects 5440、sort 3440、matmul 4229、collatz 2732、strings 7226、hash 8401、其余不变。
