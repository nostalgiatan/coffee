# cfprof

采样剖析器。只依赖 `ptrace`,不需要 `perf`(`perf_event_paranoid` 高时 `perf` 用不了),也不依赖 `valgrind`。

```
cfprof [--hz N] [--top N] [--seconds N] [--lines 文件] [--folded 文件] [--syms elf] [--] 程序 [参数...]
```

运行程序,每秒约 `--hz` 次(默认 997)停住它,读寄存器,沿 `rbp` 链回溯调用栈(编译器生成的代码都保留帧指针),然后报告时间花在哪里。

| 选项 | 含义 |
|---|---|
| `--hz N` | 采样频率。 |
| `--top N` | 每张表的行数,默认 25。 |
| `--seconds N` | 到时间就杀掉程序并报告。用来剖析卡住的程序:报告里就是它停在哪个函数、哪一行。退出码 124。 |
| `--lines 文件` | 行号表,用 `coffee2 prog.cf -g --lines 文件` 或 `cfjit prog.cir --lines` 生成。有了它才有"按源码行"的表。 |
| `--folded 文件` | 写折叠栈(`a;b;c 次数`,最外层在前),可交给 flamegraph.pl 或 speedscope。 |
| `--syms elf` | 符号表来自这个 ELF 文件,默认是被运行的程序。 |

函数名来自程序自己的符号表(要 `-g`,否则符号不保留)和 `/tmp/perf-PID.map`(`cfjit --perf-map` 写的,JIT 代码用)。位置无关的可执行文件(`--dynamic`)的加载地址从 `/proc/PID/maps` 取。

输出四张表:

1. **self%**:停在这个函数自己代码里的样本比例,按它排序。
2. **incl%**:栈里有这个函数的样本比例(含它调用的)。
3. **called from**:self 最高的几个函数各自被谁调用。
4. **line**:停在哪个源码行(要 `--lines`)。

限制:样本在函数序言(`push rbp; mov rbp, rsp`)里时按序言处理,在 `ret` 之前的 `pop rbp` 之后会少记直接调用者;C 库里没有帧指针的函数会让回溯提前结束;每次采样要停住程序几个系统调用,不适合剖析几毫秒的程序。
