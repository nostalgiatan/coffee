# riscv64 后端参考

`--target=riscv64-linux`(可写成 `riscv64`)为 RV64GC 的 Linux 生成代码(用到 I、M、F、D,不用压缩指令)。另有两个带扩展的目标:`riscv64-linux-rva22`(再用 Zba、Zbb:`sh1add`/`sh2add`/`sh3add` 做缩放地址,`add.uw`、`sext.b`、`sext.h`、`zext.h` 做扩展)和 `riscv64-linux-rva23`(再加向量扩展 V:向量循环)。扩展目标的程序要在支持它们的处理器上运行,qemu 用 `-cpu rva22u64` / `-cpu rva23u64`。输出是 `--emit-obj` 的可重定位 ELF64(机器号 243,`e_flags` = 4,双精度浮点 ABI);加 `--static` 由 `cfld` 链进 `rt/src/os_linux_aarch64.cf` 的运行时(riscv64 和 aarch64 用同一张通用系统调用号表,`exit_group` 是 94),得到可以直接运行的可执行文件。没有 riscv 机器时用 `qemu-riscv64` 运行。

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

没有 riscv 真机,所以拿 clang 当参照:`ir/tools/rvbench/run.sh <coffee2>`(用 `TARGET=` 选目标)把 8 个小核(各有 Coffee 和 C 两份)编成 riscv64,在 `qemu-riscv64 -one-insn-per-tb -d exec` 下数**执行的指令条数**,和 `clang -O2`(带同样的扩展)的同一核比较,退出码必须一致。指令数不是时间,但同一条流水线上的差别大致和它成正比,而且不受模拟器速度影响。数字是 Coffee 比 clang -O2 的倍数(越小越好):

| 核 | riscv64-linux | rva22 | rva23 |
|---|---|---|---|
| fib | 1.19 | 1.19 | 1.19 |
| sieve | 1.13 | 1.05 | 1.49 |
| matmul | 1.33 | 1.34 | 6.14 |
| mandel | 1.27 | 1.27 | 1.27 |
| collatz | 1.52 | 1.61 | 1.61 |
| hash | 2.08 | 2.27 | 2.27 |
| saxpy(向量循环) | 1.62 | 1.61 | **0.77** |
| isum(向量循环) | 1.76 | 1.76 | **0.51** |

向量循环在 rva23 上比 clang 的自动向量化指令更少,因为按程序需要的寄存器数选寄存器分组(`LMUL` 8、4、2、1 取最大的装得下的)。`matmul` 在 rva23 上慢是 clang 把内层循环向量化了,而 Coffee 的向量化器只认 `a[i]` 形式的单层循环;`sieve` 同理(clang 向量化了清零循环)。向量目标的循环强度削弱放在向量化之后(向量化器要看没改过的循环),`matmul` 因此比标量目标多了一轮优化,多出 20% 的指令。

做过的优化:
- **浮点常量和循环里要放进寄存器的整数常量**只在函数入口装一次(`g_hoist_use`);相等的常量共用一个寄存器;循环里有调用的函数不提升(被调用者保存的寄存器每次调用都要存取)。
- **立即数形式**:`andi`/`ori`/`xori`,零用 `x0`;移位、扩展、除法直接用操作数所在的寄存器,不再先 `mv`。
- **块参数的并行复制**用寄存器完成(先存帧里的,再按依赖顺序移动,环用临时寄存器打破),不再绕道栈帧。

IR 一层的三个优化(`opt.cf`),位集由 `set_strength` 决定(1 乘积、2 地址、4 线程化):x86-64、aarch64 开 1 和 4,riscv64 全开。x86-64 和 aarch64 上量过(qemu 数指令):`matmul` 少 8%,`hash` 少 7% 到 10%,`mandel` 多 0.7%,`sieve`、`collatz` 不变;x86-64 上 `ir/bench` 的 `matmul` 从 0.20 秒到 0.17 秒。位 2 在这两个目标上有缩放寻址,开了反而多指令(aarch64 的 `matmul` 多 9%),所以不开。`--strength=N` 可以按位覆盖目标的选择,用来量。
- **强度削弱**(`strength_reduce`,位 1 和 2):循环只有一个入口、归纳变量每圈加同一个循环不变量时,`iv * c` 变成循环头的新参数(进循环前算初值,每圈加 `step * c`);`base[iv + k]` 的 `index` 变成指针参数,每圈加常数步长(省掉 riscv 没有的缩放寻址要多出的 `slli` 加 `add`)。变步长只对乘积做,对地址做反而多一条。`matmul` 里的 `k * n` 和 `a[i*n+k]` 因此从每圈重算变成两次自增。
- **线程化**(`thread_forward`、`thread_const_params`,位 4):进入一个只转发参数的块的边直接去它的目标;给只测试参数的块传常量的边直接去测试取的那一边(`while go:` 里把 `go` 设成假再回到测试,现在直接出循环)。条件是这个块的值只被它自己的边读。
- 寄存器分配里的常量提升(见上)。

`ir/tools/a64progs/loops.cf` 覆盖这些循环(常量、负、变步长,嵌套,标志循环,窄整数),和原生版本比输出。

剩下的差距:`hash` 的活跃值超过 11 个寄存器,有溢出;`collatz` 里 `n // 2` 要 4 条指令(整数是有符号的,不能像 C 那样假定不溢出);riscv 没有缩放寻址(`Zba` 的 `sh3add` 可省,基线 RV64GC 没有)。

## 向量循环(rva23)

`gen_rv_vec.cf` 把 `VLoop` 做成 RVV 1.0。向量长度由机器决定:每圈 `vsetvli` 取剩下的元素数得到 `vl`,所以循环一直做到末尾,后面的标量循环无事可做。缓冲区各占一个指针寄存器(a2 到 a7 和 t3),每圈按 `vl` 往前走;不变量先 `vmv.v.x` 铺满;归约的累加器用 `tu`(尾部不动),最后 `vred*.vs` 与槽里的值合并。编码用 `llvm-mc` 核对过,全部程序在 qemu 的 rva23u64 下与原生版本输出相同。

## 沙盒

`cfjit prog.cir --target=riscv64 --sandbox[=base]` 生成、验证并在 4 GB 线性内存里运行 riscv64 代码(`cfjit` 要编成 riscv64:`coffee2 jit/src/main.cf ... --static --target=riscv64-linux`,在 qemu-riscv64 下运行)。只用基线 RV64GC,没有 `gs` 形式。和 aarch64 同一个模型,差别在寄存器和访存形式:

- **寄存器。** s10(x26)放区域起点,s11(x27)是影子帧指针(向上走,函数入口加、出口减;聚合和取过地址的槽在影子帧里,在 s11 下方);这两个不在寄存器池里(池从 11 个减到 9 个)。t6(x31)是访存地址的暂存。宿主栈上只有返回地址、保存的寄存器、溢出的标量。
- **访存。** 每个受限访存是五条指令之内的固定形式:`[addiw t6, base, disp]`、`slli t6, <t6 或 base>, 32`、`srli t6, t6, 32`、`add t6, t6, s10`,然后 `ld/sd/lw/.../fld/fsd rt, 0(t6)`。不管 `base` 里是什么,地址落在区域里。栈帧上的访问是 `imm(s0)`(帧不超过 2000 字节,位移是 12 位),保存的 s0/ra 在 `s0+0`、`s0+8`。
- **调用。** 只调模块里的函数、`cf_syscall` 和导入入口;参数在栈上的调用、函数值调用、函数地址不生成(或在做镜像时拒绝)。导入入口 32 字节(`auipc t6,0; ld t6,16(t6); jalr x0,0(t6)`,后面是槽);`read`/`write` 的桩同样。
- **验证器**(`sbxcheck_rv.cf`)按 6 条共用规则拒绝:不是 32 位指令或是 `ecall`、原子、栅栏、向量的(decode);访存不是 `0(t6)` 形式也不是 `imm(s0)`(unguarded);向 `s0+0..15` 存、通过 sp 访存(frame);写 ra、sp、gp、tp、s0、s10,除了帧自己的那几条固定字(protected);`jalr` 不是 `auipc` 之后的第二条、跳转目标不在代码里、代码走出末尾(branch);跳进 `srli` 之后的 `add` 或访存本身(adjacent,那时 t6 是任意值)。共用的反例集在 `sbxcases.cf`(目标字母 `v`)。
- **加载器**(`sandbox_rv.cf`)与 aarch64 共用 `sandbox_run_a64`(多一个 `rv` 参数),只是桩换成 riscv64 的:进入、陷阱、退出、`read`/`write` 的宿主桩(唯一的 `ecall`)、SIGSEGV/SIGBUS/SIGFPE/SIGILL 的处理(pc 在 `ucontext+176`,出错地址在 `siginfo+16`)、SIGALRM 配额。宿主调用跳板是 `host_tramp_rv`。
- **测试。** `jit/tests/test_sandbox_rv.cf`(和 `test_sandbox_a64.cf` 同一批用例,qemu-riscv64 不在时跳过);`ir/tools/a64cases.py --target=riscv64-linux --sandbox=<cfjit>` 把 191 个 `exits` 用例放进沙盒:184 个与原生退出码相同,其余 7 个(函数值 4 个,与 libc 同名的函数 3 个)和 aarch64 的结果相同。

## JIT 加载(不带沙盒)

`cfjit prog.cir --target=riscv64`(riscv64 版的 `cfjit`,qemu-riscv64 下运行)把映像放在任意地址:符号地址用 `auipc`+`addi`(`rv_pcrel`,重定位种类 23、24,距离由 `make_image` 填),调用是 `auipc`+`jalr` 一对(种类 19);外部函数的桩是 `auipc t6; ld t6, 槽; jalr x0, 0(t6); ebreak`(16 字节),槽由加载器填;代码变成可执行后用 `riscv_flush_icache`(系统调用 259)刷指令缓存。`--with=` 模块照常工作。测试:`jit/tests/test_jit_rv.cf`;191 个 `exits` 用例用 `a64cases.py --sandbox=<cfjit> --plain` 跑:188 个退出码正确,其余 3 个是与 libc 同名函数的用例(前端就不收)。

## 还没有

基线 RV64GC 的向量循环(没有向量扩展,`set_vector_loops(false)`;rva23 有)、浮点之外的循环变量强度削弱到 x86-64 和 aarch64(要先在那两个目标上量)、调试信息(`-g`)的 riscv 变体、`--pic`、压缩指令和浮点以外的扩展。
