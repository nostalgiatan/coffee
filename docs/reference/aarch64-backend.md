# aarch64 后端参考

`--target=aarch64-linux`(可写成 `aarch64`)为 aarch64 生成代码。现在能做的是**整数子集加浮点**:函数、调用、循环、算术、内存、聚合、字符串、类、`buf`;还没有的见末尾。输出是 `--emit-obj` 的可重定位 ELF64(机器号 183);链接和运行时(`rt/src/min.cf` 的系统调用)按目标另做,不在这里。

文件:`ir/src/asm_a64.cf`(指令编码,自由函数 `a64_*(a: &Asm, ...)`,写进 x86 汇编器用的同一个 `Asm` 缓冲区,标号、跳转修补表和重定位表共用)、`target_a64.cf`(目标描述,函数名以 `ta_` 开头:参数寄存器、暂存寄存器、条件码对应)、`gen_a64_regs.cf`(帧、操作数的装载、边上的复制)、`gen_a64_call.cf`(调用约定)、`gen_a64_isel.cf`(每个操作的指令)、`gen_a64_fun.cf`(一个函数和目标文件)。`target.cf` 的 `gen_function` 按 `target_arch` 选 `g_gen_fun` 或 `ga_gen_fun`,`target_object` 选目标文件的写法。

## 目标文件

用 x86-64 的 `write_elf` 写,再把 `e_machine` 改成 183。重定位类型写在 `rl_ty` 里,`write_elf` 原样放进 `.rela.text.<函数>`:

| 用途 | 重定位 |
|---|---|
| 调用函数 | `R_AARCH64_CALL26`(283),加数 0 |
| 取符号或 `.rodata` 里的字符串的地址 | `adrp` 配 `R_AARCH64_ADR_PREL_PG_HI21`(275),`add` 配 `R_AARCH64_ADD_ABS_LO12_NC`(277),加数是偏移 |

符号 1 仍是 `.rodata` 节符号。函数按 4 字节对齐(`set_target` 调 `fn_align_at_least(4)`)。

## 栈帧

以 x29 为基准,向上(x29 = sp = 帧的最低处):

| 位置 | 内容 |
|---|---|
| `[x29]`、`[x29+8]` | 调用者的 x29 和 x30 |
| `[x29+16]` | 返回聚合的函数收到的隐藏指针(只有那时才有) |
| 其上 | 每个值一个 8 字节对齐的位置;聚合按内存布局;`Slot` 的区域 |
| 最上 | 边复制用的暂存区 |

序言是 `sub sp, sp, #帧; stp x29, x30, [sp]; mov x29, sp`,尾声是 `ldp; add sp, sp, #帧; ret`。帧大小向上取整到 16。偏移超出 `ldr` 的缩放立即数(32760 字节)时改用 `ldur`,再不行用 x17 装偏移,所以帧大小没有上限。

## 寄存器

现在**每个值都住在帧里**(没有寄存器分配),运算时装进暂存寄存器:

| 用途 | 寄存器 |
|---|---|
| 操作数和结果 | x9, x10, x11 |
| 地址 | x12, x13 |
| 间接调用的目标 | x16 |
| 汇编器构造大立即数 | x17 |
| 浮点运算 | v0, v1 |
| 帧指针、链接寄存器 | x29, x30 |
| 留给沙箱的线性内存基址 | x28 |

浮点值的位模式存在帧里的整数位置,运算前 `fmov` 进 v 寄存器。窄整数和 x86 一样:位置里只有低位有意义,用到时按宽度扩展。

## 调用约定

AAPCS64:整数和指针放 x0 到 x7,浮点放 v0 到 v7,其余按 8 字节一个放栈上(调用前 `sub sp`,16 字节对齐);结果在 x0 或 v0。聚合参数传指向调用者帧里副本的指针,被调用者复制进自己的帧;返回聚合的函数收到指向结果位置的指针,放在 x8(间接结果寄存器)。变参 C 函数(`printf`)在 Linux 上与普通调用相同。函数把 x19 到 x28 全部留给调用者,没有保存它们的代码,因为没有用到。

## 指令选择

- `add`/`sub` 带常数用立即数(12 位,或 12 位左移 12),其余装常数再用寄存器形式;常数用 `movz`/`movn`/`movk`。
- 乘法 `mul`,除法 `sdiv`/`udiv`,取余用 `sdiv` 加 `msub`;`SMulHi`(`legalize` 做的按常数除)是 `smulh`。
- 比较用 `cmp` 加 `cset`,紧跟其后只用于分支的比较融合成 `cmp` 加 `b.cc`。浮点比较 `fcmp`,有序谓词的条件码(无序时都为假):`<` mi、`<=` ls、`>` gt、`>=` ge、`==` eq,`!=`(有序且不等)是 `cset mi` 与 `cset gt` 取或。
- `Index`/`Field` 只供一个标量 `load`/`store` 用时折进地址:`[基址, 立即数]`、`[基址, 索引]` 或 `[基址, 索引, lsl #k]`(元素大小等于访问大小时)。
- 窄整数的扩展:`sxtb`/`sxth`/`sxtw`、`uxtb`/`uxth`、`mov w, w`。
- 浮点到整数是 `fcvtzs`/`fcvtzu`(饱和,与 x86 的越界结果不同),整数到浮点是 `scvtf`/`ucvtf`(一条指令,无符号 64 位不用 x86 那段序列)。
- 向量循环(`VLoop`)还没有代码:`set_target` 关掉 `vectorize`,所以不会出现。

## 怎样验证

没有 aarch64 机器或 qemu 时:

- `ir/tests/test_a64.cf`:每个编码函数写出的字,和 LLVM 汇编器(`llvm-mc -triple=aarch64`)对同一条指令的结果比较;分支修补;`--target=aarch64-linux` 编出的目标文件是机器号 183 且有序言、`bl`、`ret`。
- `ir/tools/a64sim.py`:用 `llvm-objdump` 反汇编目标文件,在 Python 里执行代码生成器会产生的那部分指令,C 库函数(`malloc`、`printf` 等)用 Python 实现。
- `ir/tools/a64diff.py <coffee2>`:`ir/tools/a64progs/*.cf` 和一个生成的 6000 个值同时活着的大帧程序,各自原生(x86-64)运行一次、再编成 aarch64 在模拟器里运行,退出码和标准输出必须相同。
- `ir/tools/a64cases.py <coffee2>`:`compat/tests` 里 191 个 `exits(...)` 用例在模拟器里运行;其中 2 个因模拟器没实现 `getenv` 和步数上限而不算(189 个通过)。
- 整个编译器(`driver/src/main.cf`)编成 aarch64 目标文件,约 2.6 MB,`llvm-objdump` 全部能解码。

## 还没有的

- **寄存器分配**:每个值在帧里,代码比 x86 的慢得多、大得多(一次加法是两次装载、一次加、一次存储)。x86 的分配器(`gen_x86_regs.cf`)按 x86 的寄存器池和调用写掩码写成,要抽出与目标无关的部分才能共用。
- **向量循环**(`VLoop`):需要 NEON 的 `gen_a64_vec.cf`。
- **沙箱**:基址寄存器形式(`cfjit --sandbox=base`)在 aarch64 上是 `[x28, w偏移, uxtw]`,不多花指令;还没做,`Asm.sbx` 在这个目标上不起作用。
- 按常数的无符号除法(`legalize` 也没有)、跳转表、`CallI` 以外的尾调用;条件分支只能跳 ±1 MB,更大的函数 `a64_end_fun` 返回 false(报告为生成失败)。
- 链接与运行:`ld/` 和 `elf.cf` 的 aarch64 目标(机器号、重定位、入口桩),`rt/src/min.cf` 的 aarch64 系统调用,`--static`。
- `cfjit`、`tea` 的 `--target` 透传。
