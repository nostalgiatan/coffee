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

字符串释放改成绕过 free 的分支(不再算地址);保存的寄存器用 push/pop(入口 1 到 2 字节,出口同),有多个 return 的函数共用一个出口。编译器自身 1045216 → 996816 字节(比这一批开始前 1163200 小 14%)。

## 合并路径(前三项)
自编译(callgrind,driver/src/main.cf):19.63G → 16.01G Ir(-18%);编译器 990760 → 989120 字节,不动点成立。
- `prints_numbers` 不再整程序重新解析+检查,只做词法扫描(有 f-string 就附加数字运行时,链接器丢掉用不到的)。
- 验证器默认不跑,`--verify` 开启(`tea test` 加上)。
- `main.cf` 与 `incr.cf` 的链接尾部合成 `link_objects`。

## 符号哈希、glue 并入模块路径、前端预检
自编译 16.01G → 13.65G Ir(再 -15%);编译器 990744 字节,不动点成立,基准输出与体积不变。
- `Syms` 加按哈希分链的索引(`find_span`/`find_in_module` 走链,`truncate` 摘链),结果与线性扫描相同。
- drop/clone glue 先入模块再 `module_opt`,不再当场优化当场生成;删了 `emit_current`。体积/速度无变化,少一条路径。
- `renamed_aliases` 先按行预检 `use ... as`,没有就不词法分析;`keyword_scan` 先按长度和首字母排除。

## 内联按净体积
旧规则:被调函数 ≤ 40 条指令就内联。扫描(编译器自身体积 / 自编译 Ir):上限 0→780440,8→805000,16→843208,24→894296,40→990744,64→1207496;但上限小时 objects、matmul 变慢 25–30%。
新规则(`modopt.cf`):函数体 ≤ 参数数 + 4 条随处内联(拷贝和调用差不多大);更大的(≤ 40)只在调用点处于循环里时内联。
结果:编译器 990744 → 825096 字节(-17%),自编译 13.65G → 12.86G Ir,基准运行时间不变(matmul 0.19–0.20)。参数 plain 0–8、loop 24–40 体积相同,没有再调的余地。
`track.sh` 记录每次提交的自编译 Ir 与编译器体积到 `TRACK.tsv`。

## 链接期相同代码折叠(试过,没留)
在 `cfld` 里 gc 之后加了一步:内容和重定位目标都相同的 text 节只留一份(迭代到不再变)。编译器自身只折叠了 16 个节,体积 828760 → 827032(-0.2%),而折叠代码本身约 +2KB,净零;基准程序体积不变。结论:现在的代码生成里相同函数很少,不值得留,已还原。

## 溢出统计(只量,没改)
临时在 `alloc_regs` 末尾按函数打印(`--emit-ir` 时),整数值:
- 编译器自编译:85464 个值里 10884 个(12.7%)没拿到寄存器,按循环深度加权的使用占 24.8%(2.52M / 10.15M)。
- 基准程序里的数字被运行时(fmt)的函数占满,不能代表程序本身。
- 静态:编译器二进制 193689 条指令里 34425 条(17.8%)带 `(%rbp)` 内存操作数,其中 33160 条是 mov(含聚合值的栈槽,不全是溢出)。
- 可分配的整数寄存器 8 个(r10,r8,r9,rbx,r12–r15);r11 在 `gen.cf` 里只有间接调用和一处 `ld(11,…)` 用到,几乎是空闲的。

## r11 作第 9 个可分配寄存器
r11 原来只在间接调用(目标地址)用到,现在是调用者保存的池里的第 4 个(掩码第 3 位),和 r10 一样可分配,但参数/实参仍不能用它(间接调用先把目标装进 r11)。
编译器 825096 → 813928 字节(-1.4%),自编译 12.86G → 12.58G Ir(-2.2%);基准运行时间不变或略好(hash 0.29→0.28,matmul 0.19→0.18),基准代码体积 +0.7%(r11 要 REX 前缀)。不动点成立。

## rdi 作第 10 个可分配寄存器
rdi 只在这些地方当临时寄存器:调用实参、sret、聚合值的 Store 和 Ret、Alloc/Free(后几项本来就是调用点)。池里它排在 r11 之后(掩码第 4 位),不给参数和实参用;有实参(或返回聚合值)的调用写 rdi,聚合 Store/Ret 作为只写 rdi 的点(`uses_rdi`)。
编译器 813928 → 808984 字节(-0.6%),自编译 12.58G → 12.41G Ir(-1.4%);基准运行时间不变或略好,基准体积略降(collatz 37328→37168),不动点成立。
rsi 不行:`src(…, 6)` 和 `addr_of(…, 1, 6)` 在每个地址在栈上的 Load/Store 里都用它当地址临时寄存器,要先把临时寄存器的选取改成动态的。

## 区间拆分(围绕调用,试过,没留)
先量了没拿到寄存器的值的加权使用:75.6% 是跨调用且在循环里的值(只能用 5 个被调用者保存寄存器),23.8% 是循环里不跨调用的,冷值几乎为零。
试的做法:跨调用的热值(权重 ≥ 8)在 5 个被保存寄存器用完后改放 r10/r11,定义处同时存到栈槽,每次可能改写该寄存器的调用后重新装入;参与合并(coalesce)的值不用(它们共用寄存器,重装会盖掉别的值)。
结果:不动点成立,但变差——编译器 808984 → 856696 字节(+6%),自编译 12.41G → 12.82G Ir(+3%),基准明显变慢(fib 0.13→0.24,hash 0.27→0.59,acc 0.31→0.60)。
原因:SSA 值溢出后用内存操作数本来就便宜;定义处多一条 store、每个调用后多一条 load,比留在栈上更贵。已还原。

## 驱逐按使用密度(借自 LLVM)
分配器驱逐/溢出时比较的不再是加权使用总数,而是总数除以区间长度(`dn = wt * 1024 // (hi - lo + 2)`):长而稀的区间让位给短而密的。
编译器 808984 → 786440 字节(-2.8%),自编译 12.41G → 12.23G Ir(-1.5%),基准代码体积约 -1%,运行时间不变(在噪声内)。不动点成立,11 个套件全过。
