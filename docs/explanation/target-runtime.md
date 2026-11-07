# 运行时按目标分开

`--static` 的程序带着一个很小的 C 库(`rt/src/min.cf`),它不依赖任何系统库,所以系统调用必须按目标写。这篇文章说明现在怎么分,以及加一个新系统或新架构要做什么。

## 现在的布局

- `rt/src/min.cf`:各系统共有的部分,堆、`memcpy`、`strlen`、`strdup`、`strcmp`、`calloc`。它只用 `peek`/`poke` 和一个函数 `cf_mmap(n)`。
- `rt/src/os_<系统>_<架构>.cf`:这个系统自己的部分。现在有 `os_linux_x86_64.cf` 和 `os_linux_aarch64.cf`。文件里有一行 `/#/ @split`:它前面(只有 `cf_mmap`)接在 `min.cf` 的状态变量之后,后面(`write`、`exit`、`open`、`read`、`close`、`remove`、`access`、`chmod`、`clock_gettime`、`fork`、`waitpid`、`execve`、`_exit`、`fflush`、`system`)接在 `min.cf` 的末尾。
- `rt/gen.sh` 把这些文件做成 `ir/src/minrt.cf` 里的文本函数;`ir/src/target.cf` 的 `min_rt_text()` 按当前目标把它们拼起来。x86-64 Linux 拼出来的文本和分开之前逐字节相同。

## 一个系统要提供的函数

名字和含义固定,和 C 库的同名函数一致:`cf_mmap(n)`(映射 `n` 字节、清零的内存,返回地址)、`write`、`exit`、`open`、`read`、`close`、`remove`、`access`、`chmod`、`clock_gettime`、`fork`、`waitpid`、`execve`、`_exit`、`fflush`、`system`。

aarch64 Linux 用的是通用的系统调用表,没有 `open`、`fork`、`unlink`:用 `openat`/`unlinkat`/`faccessat`/`fchmodat`(目录取 `AT_FDCWD`,即 -100)和 `clone`(旗标 `SIGCHLD`)代替,`exit` 是 `exit_group`(94)。

## `cf_syscall`

程序里的内置 `syscall(n, a, b, c, d, e, f)` 编成对 `cf_syscall` 的调用,由链接器补上这个函数:把 C 调用约定的参数搬进系统调用指令用的寄存器。机器码按架构不同,放在 `ld/src/link.cf` 的 `syscall_stub()`,由驱动用 `--target=` 通过 `set_ld_arch` 选。x86-64 是 `syscall`,aarch64 是 `svc #0`(`x8` 放号码,`x0`..`x5` 放参数),riscv64 是 `ecall`(`a7` 放号码,`a0`..`a5` 放参数)。

## 还没有的

- riscv64:同 aarch64 用通用系统调用表和 `os_linux_aarch64.cf`;`cfld` 认机器号 243 和 `R_RISCV_*` 重定位,补 riscv64 的 `_start`,见 [riscv64 后端参考](../reference/riscv64-backend.md)。
- aarch64:已经可以链接和运行(`cfld` 认机器号 183 和 `R_AARCH64_*` 重定位,补 aarch64 的 `_start`),见 [aarch64 后端参考](../reference/aarch64-backend.md)。
- 沙箱镜像里的 `cf_syscall`(`ir/src/image.cf`)只有 x86-64 的版本。

## Windows

Windows 没有稳定的系统调用号,程序必须通过 `kernel32.dll` 的导出函数(`VirtualAlloc`、`WriteFile`、`ReadFile`、`CreateFileW`、`ExitProcess` 等)和系统打交道。所以一个 Windows 目标要的东西比加一个 Linux 架构多:

1. **可执行文件格式**:`cfld` 要写 PE/COFF,有导入表,要读 COFF 目标文件(或让后端直接写 COFF)。
2. **调用约定**:Win64 的整数参数在 `rcx rdx r8 r9`,调用者留 32 字节影子空间,`rsi`/`rdi`/`xmm6`..`xmm15` 由被调用者保存。`g_do_call` 和寄存器描述(`target_x86.cf` 里的池、调用者保存的槽数、写掩码)都要有 Windows 版本。
3. **运行时**:`os_windows_x86_64.cf` 用导入的函数实现上面那组名字。`min.cf` 不用改。`fork`/`waitpid`/`execve`/`system` 在 Windows 上要换成 `CreateProcess` 一类的实现,或者先不提供。
4. **`syscall` 内置**:Windows 没有,用到的程序在这个目标上是编译错误。

这四项互相依赖,适合在 aarch64 后端之后作为一个单独的工作。
