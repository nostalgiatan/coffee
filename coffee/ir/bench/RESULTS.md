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

`bench/flaws.py <coffee2>` 数每个基准里可避免的指令模式(连环 mov、lea 后接 mov、test 紧跟置标志的运算、mov $0 等)和总指令数。
常数下标进寻址模式、`x % 2^k == 0` 改为掩码测试、两个活寄存器相加用一条 lea 之后:指令总数 6448 → 6185(含库代码),collatz 0.19 → 0.15 秒。

聚合值:使用计数在 dead_code 前重数(之前有残留计数,没人用的 `insert` 删不掉);只被 `extract` 拆开的聚合 `load`(`&buf` 的指针和长度)改为逐元素 `load`。
callgrind(百万):sort 3440 → 2633、matmul 4229 → 3966、strings 7226 → 6935;总指令数(flaws.py)6185 → 6081。

边界检查:循环计数器(从小常数起、每次加小常数)在 `i < K'` 的分支里,对 `ult i, K`(K' ≤ K)的检查不再生成。sieve 784 → 744、strings 6935 → 6887(百万指令)。下标是算出来的(`i * n + k`)时仍保留检查。

字符串路径:新内建 `str_alloc(n)`(n 字节文本的新字符串)和 `str_put(s, i, c)`,`__cf_fmt_int` 直接写进结果字符串(不再经过 buf、`as str` 的 strlen 和拷贝),位数用乘 10 的比较阶梯数;`x % m`(m 非 2 的幂)改为 `x - x / m * m`,让同一个 x 的商只算一次。
strings 6887 → 3882、collatz 2732 → 1812(百万指令);wall 0.26 → 0.15 秒。

浮点数文字:大数只用需要的 limb 数(按指数算),不再固定 48 个。200000 次 `f"{x}"`:47.0G → 8.4G 指令(5.6 倍),输出与之前逐位相同。真正的快路径(Ryu 一类)还没做。

浮点数文字的快路径:Grisu3(64 位有效数乘 10 的缓存幂,表 87 项以十六进制文本放在函数里),拿不准时退回大数;`__cf_layout` 直接写结果字符串。200000 次 `f"{x}"`:47.0G → 0.88G 指令(约 53 倍)。差分检查:200 万个随机数加全部 2 的幂、非规格化数,快路径与大数结果逐位相同(0 个不一致),约 99.3% 走快路径。`rt/tests/test_fmt.cf` 留了一个 3 万例的版本。

体积细节:mov 0 用 2 字节 xor;没人用的调用结果不存;帧里的存储直接从家寄存器出;字面量文字内的偏移一条 lea;失败检查(`call; unreachable`)每个函数只留一个块;汇编器去掉 `mov a,b; mov b,a` 和刚存又读的同一帧位置。编译器自身 1163200 → 1059216 字节(-9%)。
