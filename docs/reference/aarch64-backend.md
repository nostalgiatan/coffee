# aarch64 后端参考

`--target=aarch64-linux`(可写成 `aarch64`)为 aarch64 生成代码。现在能做的是**整数子集加浮点**:函数、调用、循环、算术、内存、聚合、字符串、类、`buf`;还没有的见末尾。输出是 `--emit-obj` 的可重定位 ELF64(机器号 183);加 `--static` 由 `cfld` 的链接器链进 `rt/src/os_linux_aarch64.cf` 的运行时,得到可以直接运行的 aarch64 Linux 可执行文件(见 [运行时按目标分开](../explanation/target-runtime.md))。

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

整数和指针值由线性扫描分配到 x19 到 x27(被调用者保存,九个),活跃范围的计算和 x86 共用(`gen_x86_regs.cf` 的 `g_live_ranges`:按循环深度加权的使用数、合并循环参数);跨调用的值也放在这些寄存器里,函数在序言里把用到的保存在帧里 `[x29+16(+8)+8k]`、尾声里恢复。寄存器不够时使用最少的值住帧。浮点、聚合和 `Slot` 仍在帧里。

| 用途 | 寄存器 |
|---|---|
| 值的住所(被调用者保存) | x19 到 x27 |
| 操作数和结果的暂存 | x9, x10, x11 |
| 地址的暂存 | x12, x13, x15 |
| 间接调用的目标 | x16 |
| 汇编器构造大立即数 | x17 |
| 浮点值的住所 | v8 到 v15(被调用者保存,用于跨调用存活的值)、v16 到 v31(其余的值) |
| 浮点运算的暂存、调用的浮点参数和结果 | v0 到 v7 |
| 帧指针、链接寄存器 | x29, x30 |
| 留给沙箱的线性内存基址 | x28 |

窄整数和 x86 一样:位置里只有低位有意义,用到时按宽度扩展。

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

## 链接

`ld/src/link.cf` 在 `set_ld_arch(1)`(驱动在 `--target=aarch64-linux` 时设)下:只收机器号 183 的目标文件;没有 `_start` 时补一个(`mov x29,#0; ldr x0,[sp]; add x1,sp,#8; bl main; mov x8,#94; svc #0`);重定位支持 `ABS64`(257)、`ABS32`(258)、`PREL32`(261)、`CALL26`(283)、`JUMP26`(282)、`ADR_PREL_PG_HI21`(275)、`ADD_ABS_LO12_NC`(277)、`LDST8/16/32/64/128_ABS_LO12_NC`(278、284、285、286、299)、`ADR_GOT_PAGE`(311)、`LD64_GOT_LO12_NC`(312)。`exe.cf` 的可执行文件头写 `machine_of(ld_arch)`。

## 怎样验证

装了 `qemu-aarch64` 时,下面的脚本把程序用 `--static` 链接后在 qemu 下运行;没有时 `a64diff.py`、`a64cases.py` 加 `--sim` 用解释器。

- `ir/tests/test_a64.cf`(`tea test`):每个编码函数写出的字,和 LLVM 汇编器(`llvm-mc -triple=aarch64`)对同一条指令的结果比较;分支修补;`--target=aarch64-linux` 编出的目标文件是机器号 183 且有序言、`bl`、`ret`。
- `ir/tools/a64diff.py <coffee2>`:`ir/tools/a64progs/*.cf` 和一个生成的 6000 个值同时活着的大帧程序,各自原生(x86-64)运行一次、再编成 aarch64 运行,退出码和标准输出必须相同。
- `ir/tools/a64cases.py <coffee2>`:`compat/tests` 里 191 个 `exits(...)` 用例。qemu 下 191 个全部通过。
- `ir/tools/a64suites.py <coffee2> <阶段目录>...`:每个阶段的 `tests/test*.cf` 编成 aarch64 在 qemu 下运行,退出 0 为通过。`lexer parser sema types own` 共 32 个全过,`ld` 9 个里 7 个过(其余 2 个要运行链接出的 x86 程序),`rt/test_fmt` 通过(运行时的 `atof` 是按 Go 的十进制算法写的,与 strtod 一样舍入到最近偶数)。会调用 `cc`、运行 x86 程序或用 `system` 的测试(`ir` 的 `expect_mc` 系列、`compat` 的 harness、`driver`、`tea`)在 qemu 下本来就不能跑,不算失败。
- 整个编译器(`driver/src/main.cf`)用 `--target=aarch64-linux --static` 编成 aarch64 程序(1.6 MB),在 qemu 下运行它编译同一个程序,x86 目标文件与原生编译器逐字节相同;它用 `--target=aarch64-linux --static` 编译自己得到的文件与第一次逐字节相同(6 秒)。
- `ir/tools/a64sim.py`:不用 qemu 的解释器,用 `llvm-objdump` 反汇编目标文件执行。

## 浮点

浮点值和整数一样由线性扫描分配到寄存器(活跃区间与 x86 共用 `g_live_ranges`):跨调用的进 v8 到 v15(只保存低 64 位,序言和尾声保存用到的),其余进 v16 到 v31;放不下的留在帧里,用时载入 v0、v1。运算、比较、转换、取放内存、传参、返回都直接用 v 寄存器。

与 x86 不同的语义:`fcvtzs`/`fcvtzu` 饱和,NaN 转整数得 0(x86 的 `cvttsd2si` 给「不确定值」`INT64_MIN`);负数转无符号得 0;这些在 IR 里是未定义的,所以两个目标的差别不算错误。比较的非序语义相同(NaN 与一切比较为假,`!=` 为真)。`fmod` 不是指令,`legalize` 把 `FRem` 换成对 libm `fmod` 的调用,静态运行时没有它。

## 按常数的无符号除法、远条件分支

64 位无符号数除以或取余常数(非 2 的幂,小于 2^31)在 `legalize` 里换成 `UMulHi`(aarch64 的 `umulh`,x86 的 `mul`)加位移:`t = mulhu(x, m)`,`q = (t + ((x - t) >> 1)) >> (l - 1)`,`m = floor(2^64 (2^l - d) / d) + 1`(Granlund–Montgomery 的不带 65 位的形式)。x86 也用它,所以无符号常数除法在两个目标上都不再有除法指令。

条件分支(`b.cc`、`cbz`、`cbnz`)只能跳 ±1 MB。`a64_end_fun` 发现跳不到时置 `a64_range_miss`,`ga_gen_fun` 把函数回退重做一遍,这次每条条件分支写成反条件跳过一条 `b`(±128 MB)。生成的 40000 条语句的函数(目标文件 3.8 MB)在 qemu 下与原生结果一致。

## 还没有的

- **向量循环**(`VLoop`):需要 NEON 的 `gen_a64_vec.cf`。
- **沙箱**:基址寄存器形式(`cfjit --sandbox=base`)在 aarch64 上是 `[x28, w偏移, uxtw]`,不多花指令;还没做,`Asm.sbx` 在这个目标上不起作用。
- 跳转表、`CallI` 以外的尾调用。
- 静态运行时的 `printf` 只做整数、字符、字符串(`%d %i %u %x %X %o %c %s %p %%`,标志 `-` `0`、宽度、长度字母),最多 5 个参数,没有 `%f`;`sprintf`、`fprintf`、`strtod`、`qsort` 等其余 libc 函数没有,用到它们的程序要 `cc`,而 `cc` 不是交叉编译器。
- 动态链接(`cc`)和 TLS、IFUNC 等 glibc 的东西。
- `cfjit`、`tea` 的 `--target` 透传。
