# riscv64 后端参考

`--target=riscv64-linux`(可写成 `riscv64`)为 RV64GC 的 Linux 生成代码(用到 I、M、F、D,不用压缩指令)。输出是 `--emit-obj` 的可重定位 ELF64(机器号 243,`e_flags` = 4,双精度浮点 ABI);加 `--static` 由 `cfld` 链进 `rt/src/os_linux_aarch64.cf` 的运行时(riscv64 和 aarch64 用同一张通用系统调用号表,`exit_group` 是 94),得到可以直接运行的可执行文件。没有 riscv 机器时用 `qemu-riscv64` 运行。

文件:`ir/src/asm_rv.cf`(指令编码 `rv_*`,写进与 x86、aarch64 共用的 `Asm` 缓冲区)、`target_rv.cf`(寄存器分工,函数名以 `tr_` 开头)、`gen_rv_regs.cf`(帧、操作数装载、边上的复制)、`gen_rv_call.cf`(调用约定)、`gen_rv_isel.cf`(每个操作的指令)、`gen_rv_fun.cf`(一个函数和目标文件)。结构与 [aarch64 后端](aarch64-backend.md)相同,`target.cf` 的 `gen_function` 和 `target_object` 按 `target_arch`(2)选择。

## 与 aarch64 不同的地方

- **没有标志寄存器。** 比较和分支是一条指令(`beq`、`blt`、`bltu` 等);生成器把 IR 的比较直接做成这些分支,需要布尔值时用 `slt`/`sltu`/`xor`+`sltiu`。
- **指令定长 4 字节,立即数窄。** 12 位立即数放得下就用 `addi`/`ld`/`sd`,否则 `lui`+`addi`(+`slli`)造常量;太大的帧偏移通过 t6(x31)组出来。
- **分支够不着时重生成。** 条件分支只到 ±4 KiB,`jal` 到 ±1 MiB。`gr_gen_fun` 先用短形式;函数里有够不到的分支就用更长一级的形式(条件取反跳过一条 `jal`,再不够用 `auipc`+`jalr`)重新生成整个函数。`ir/tools/a64cases.py`/`a64diff.py` 里的 `gen_bigfunc`(约 1.6 MB 的单个函数)覆盖了这一点。
- **调用与全局地址。** 调用用 `auipc`+`jalr`,重定位 `R_RISCV_CALL_PLT`(19);静态链接取地址用 `lui`+`addi`,重定位 `R_RISCV_HI20`(26)与 `R_RISCV_LO12_I`(27),所以程序必须链接在低 2 GiB 内(`cfld` 的基址满足)。
- **链接器。** `ld/src/link.cf` 的 `apply_rv` 处理重定位 1、2(绝对 32/64 位)、18/19(调用)、26/27/28(HI20、LO12_I、LO12_S);`_start` 28 字节,`cf_syscall` 是 `ecall; ret`(`ld/src/stub.cf` 的 arch 2)。

## 约定(LP64D)

| | |
|---|---|
| 整数、指针参数 | a0 到 a7;其余走栈,每个 8 字节 |
| 浮点参数 | fa0 到 fa7(f10 到 f17) |
| 返回 | a0 或 fa0;返回聚合的函数在 a0 收到隐藏指针,其余参数后移 |
| 变参 | 浮点也走整数寄存器 |
| 被调用者保存 | s0 到 s11、fs0 到 fs11(用到才存) |
| 生成器暂存 | t0 到 t2 放操作数和结果,t3、t4 放地址,t5 放间接调用的目标,t6 放汇编器造的大立即数 |

值的放法与别的目标一样:每个值在帧里有 8 字节对齐的位置,忙的整数值分到一小组被调用者保存寄存器里(`tr_pool_reg`),浮点值以位的形式放在帧里,用到时装进浮点寄存器。帧以 s0 为基准,向上:`[s0]` 旧 s0,`[s0+8]` ra,返回聚合时 `[s0+16]` 是隐藏指针,然后是保存的寄存器、各个值的位置、`slot` 区和边复制的暂存区。

## 验证

- 191 个 `compat` 的 `exits` 用例用 `--target=riscv64-linux --static` 编译,在 qemu-riscv64 下退出码都对(`ir/tools/a64cases.py --target=riscv64-linux`)。
- `ir/tools/a64progs/` 的 11 个程序和两个生成的大程序(6000 个同时活着的值;1.6 MB 的函数)在 qemu-riscv64 下的退出码和输出与原生 x86-64 版本逐字相同(`a64diff.py --target=riscv64-linux`)。
- 各阶段的测试(lexer、parser、sema、types、own、ld、rt,以及 ir 里不起子进程的)作为 riscv64 程序在 qemu 下全部通过(`a64suites.py --target=riscv64-linux`);ir 有 18 个测试要起原生子进程或链接 C,aarch64 上同样不能这样跑。
- 整个编译器编成 riscv64 对象(362 000 行反汇编)用 `llvm-objdump --mattr=+m,+f,+d` 没有一条无效指令;编成 riscv64 可执行文件后在 qemu 下运行,用它编出的 `--static` 编译器与原生编出的**逐字节相同**。
- 单元测试 `ld/tests/test_stub.cf` 检查 riscv64 的 `cf_syscall`(`ecall`、`ret` 的编码)。

## 性能

没有 riscv 真机,所以拿 clang 当参照:`ir/tools/rvbench/run.sh <coffee2>` 把 6 个小核(`fib`、`sieve`、`matmul`、`mandel`、`collatz`、`hash`,各有 Coffee 和 C 两份)编成 riscv64,在 `qemu-riscv64 -one-insn-per-tb -d exec` 下数**执行的指令条数**,和 `clang -O2`、`-O1` 的同一核比较,退出码必须一致。指令数不是时间,但同一条流水线上的差别大致和它成正比,而且不受模拟器速度影响。

| 核 | Coffee | clang -O2 | 比值 | 优化前 |
|---|---|---|---|---|
| fib | 8.53M | 7.19M | 1.19 | 1.19 |
| sieve | 9.06M | 7.64M | 1.19 | 1.73 |
| matmul | 3.72M | 2.02M | 1.84 | 2.13 |
| mandel | 11.6M | 7.43M | 1.56 | 2.10 |
| collatz | 16.2M | 10.7M | 1.52 | 2.27 |
| hash | 9.08M | 3.88M | 2.34 | 2.56 |

做过的优化:
- **浮点常量和循环里要放进寄存器的整数常量**只在函数入口装一次(`g_hoist_use`);相等的常量共用一个寄存器;循环里有调用的函数不提升(被调用者保存的寄存器每次调用都要存取)。
- **立即数形式**:`andi`/`ori`/`xori`,零用 `x0`;移位、扩展、除法直接用操作数所在的寄存器,不再先 `mv`。
- **块参数的并行复制**用寄存器完成(先存帧里的,再按依赖顺序移动,环用临时寄存器打破),不再绕道栈帧。

剩下的差距主要在 IR 一层,对所有目标都有用:循环不变量外提和归纳变量的强度削弱(`matmul` 里 `i*n+k` 每轮重算,clang 把它变成指针自增)、布尔循环变量(`hash` 的 `go`)、riscv 没有缩放寻址(`slli`+`add` 多一条;`Zba` 的 `sh3add` 可省,基线 RV64GC 没有)。

## 还没有

向量循环(`set_vector_loops(false)`)、沙盒/JIT 加载器(`cfjit --sandbox` 只有 x86-64 和 aarch64)、调试信息(`-g`)的 riscv 变体、`--pic`、压缩指令和浮点以外的扩展。
