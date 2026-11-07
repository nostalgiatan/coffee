# Windows 目标参考

`--target=x86_64-windows`(可写成 `x86_64-windows-gnu`)为 64 位 Windows 生成代码:x86-64 的指令选择不变,调用约定换成 Win64,目标文件换成 COFF。`--emit-obj` 得到 `.obj`;不加 `--emit-obj` 时由 MinGW-w64 的 `x86_64-w64-mingw32-gcc` 链接(它提供 C 运行库),得到 `.exe`。`--static` 对 Windows 没有意义(没有稳定的系统调用号,运行时要走导入函数),会报错。

没有 Windows 的机器上用下面的办法验证,**没有**在真正的 Windows 上运行过:

- 调用约定:`--target=x86_64-linux-ms` 是同一套 Win64 约定放进 ELF 对象,`--static` 链接时起始代码和 `cf_syscall` 也按 Win64。191 个 `compat` 的 `exits` 用例和 `a64progs/` 里的 11 个程序在 Linux 上直接运行(`ir/tools/winabi.py`),输出与 System V 版本逐字相同;和 gcc 的 `__attribute__((ms_abi))` 函数互相调用(`ir/tools/winabi/run.sh`)。
- 对象格式:`ir/tests/test_coff.cf` 从字节读回头、节、符号、字符串表和重定位;`llvm-objdump`、`lld-link`、MinGW 的 `ld` 都接受生成的对象;编译器自己的 1583 个函数编成一个 COFF 对象,除了 `fork`、`clock_gettime` 这类 POSIX 函数,所有引用都能解析;191 个用例用 `winabi.py link` 全部链接成功。

## 调用约定(Win64)

| | System V(其他目标) | Win64 |
|---|---|---|
| 整数、指针参数 | rdi, rsi, rdx, rcx, r8, r9 | rcx, rdx, r8, r9 |
| 浮点参数 | xmm0 到 xmm7,与整数分开数 | xmm0 到 xmm3,**按位置**:第三个参数是 r8 或 xmm2,不管前两个是什么 |
| 其余参数 | 栈,8 字节一个 | 栈,但调用者在栈参数**下面**留 32 字节影子空间 |
| 返回聚合的隐藏指针 | rdi | rcx(占第一个位置) |
| 变参调用 | `al` = 用到的 xmm 个数 | 前四个位置的浮点同时放进对应的整数寄存器 |
| 被调用者保存 | rbx, rbp, r12 到 r15 | 另有 rsi, rdi, xmm6 到 xmm15 |

做法:所有函数(Coffee 调 Coffee 也是)都用 Win64 的参数位置,只有**导出的 `c fn`**(C 能调用的)额外保存 rsi、rdi 和 xmm6 到 xmm15——其余函数只被 Coffee 代码调用,调用者本来就当它们是调用者保存的。`c fn` 的序言多压两个寄存器,在帧底留 160 字节用 `movups` 存 xmm,尾声先还原再弹栈。`target_x86.cf` 的 `tg_win64()`、`tg_int_args()`、`tg_xmm_args()`、`tg_shadow()` 是各处读的事实;`gen.cf` 的 `int_reg(k)` 给出第 k 个位置的寄存器;`g_do_call`(`gen_x86_call.cf`)先算每个参数去哪里(`g.al`),再按它放。

每次调用后调用者栈上多出的 32 字节影子空间在调用后一起弹掉;栈参数个数为奇数时仍先垫 8 字节,调用点的 rsp 保持 16 对齐。写寄存器的掩码(`g_site_mask`)里 r8、r9 现在是第三、第四个参数。

帧不小于一页(4096 字节)时,序言按页从上往下逐页 `sub rsp, 4096` 并读一下 `[rsp]`,使栈保护页按顺序被触及(Windows 不允许一次跳过多页)。

## COFF 对象(`ir/src/coff.cf`)

`write_coff(o, a)` 与 `write_elf` 吃同一份 `Obj` 和 `Asm`,每个函数一个节 `.text$<名字>`(名字超过 8 字节的节名放进字符串表,头里写 `/偏移`),所以 `--gc-sections` / `/opt:ref` 能丢掉没人引用的函数;`.rdata`(含全零的 256 字节块和字符串)、`.data`;符号表头两项是 `.rdata` 的节符号加辅助记录,之后是 `Obj` 里的符号,编号和 ELF 写法一致(`2 + k`),代码生成器的重定位里 `.rodata`(1)换成 0。加 `--pic` 时多一个 `.drectve`,列出 `c fn` 的 `/EXPORT:`。

| 代码生成器 | COFF |
|---|---|
| `R_X86_64_PLT32`、`R_X86_64_PC32` | `IMAGE_REL_AMD64_REL32`(4):COFF 把加数放在指令里,值是 ELF 加数加 4 |
| `R_X86_64_32` | `IMAGE_REL_AMD64_ADDR32`(2) |
| `R_X86_64_64` | `IMAGE_REL_AMD64_ADDR64`(1) |

一个函数的重定位超过 65534 个时用 `IMAGE_SCN_LNK_NRELOC_OVFL`。节数上限 65279。

## 还没有的

- **展开信息**(`.pdata`、`.xdata`):没有它,异常(结构化异常、调试器的栈回溯)不能穿过生成的函数;普通运行不需要。xmm 保存区的展开码(`UWOP_SAVE_XMM128`)与帧指针一起用时的偏移基准需要在真的 Windows 上核对,所以没有凭记忆写。
- 行表(`.cf_lines`)、DWARF。
- **加载器和运行时**:`cfjit` 的 `VirtualAlloc`/`VirtualProtect`、陷阱用向量异常处理、`os_windows_x86_64.cf`(走 `kernel32` 导入)。`--static` 的纯 Coffee 运行时因此不能用于 Windows。
- 沙盒模式(`--sandbox`)只有 Linux 的加载器。
