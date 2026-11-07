# 命令行参考:`coffee2`

```
coffee2 input.cf [-o output] [-I dir] [-O0|-O1|-O2|-O3] [--emit-obj] [--emit-flat] [--have-c] [--lines file] [--stats] [--lint-perf] [--incremental dir] [-j n] [--static] [-g] [--align] [--verify] [--time] [--target=x86_64-linux]
coffee2 -c header.h [-o output.cfc]
```

`coffee2` 是自举编译器(`coffee/driver/src/main.cf`)。它只做编译。项目模式(`coffee.toml`)、依赖获取和 `std install` 不在这里,它们留给以后的工具链程序。

## 选项

| 选项 | 含义 |
|---|---|
| `input.cf` | 源文件。必须有且只有一个。 |
| `-o output` | 输出路径。省略时见下表。 |
| `-I dir` | 增加一个模块搜索目录和 `.cfc` 搜索目录,可重复。 |
| `--target=名字` | 为哪台机器生成代码。现在只有 `x86_64-linux`(也是默认,可写成 `x86_64`)。`aarch64-linux` 认得但还没有代码生成器,会报错;别的名字报未知目标。 |
| `-O0` | 关闭 IR 优化器。 |
| `-O1` `-O2` `-O3` | 打开 IR 优化器。默认 `-O1`。 |
| `--emit-obj` | 写出 `.o` 后停止,不链接。 |
| `-c` (`--gen-cfc`) | 输入是 C 头文件(`.h`),写出 `.cfc`:每个函数声明一行 `c fn`。输出默认是 `<头文件去掉扩展名>.cfc`,`-o` 指定别的。不编译。类型对应:`char`/`short`/`int` 是 `int(1)+`/`int(2)+`/`int(4)+`,`unsigned` 加上写成 `-`,`long`/`long long`/`size_t`/`ssize_t` 是 `int`(`unsigned long` 是 `int(8)-`),`float` 是 `float(4)`,`double` 是 `float`,`_Bool` 是 `bool`,`const char *` 参数是 `string`、结果是 `object`(和 `library/cfc/libc.cfc` 一样),`void *`、别的指针、函数指针、数组参数是 `object`,`...` 是 `args: object`,`enum` 是 `int(4)+`,`struct`/`union`(含 `typedef` 出来的名字)是 `type 名: object`,指向它的指针写成这个名字。写不出的(按值的结构体、`long double`)变成 `/#/ skipped` 注释,`static` 函数不输出。头文件是当文本读的:不预处理、不跟 `#include`、不展开宏。 |
| `--have-c` | 编译前,为输入文件里带函数体的 `c fn` 写 `<输入去掉.cf>.h`(没有就只给出警告)。`int` 写成 `long long`,`int(4)+` 是 `int`,`int(N)-` 是对应的 `unsigned`,`float` 是 `double`,`str` 是 `const char*`,`object` 是 `void*`,`type X: object` 声明的名字是 `X*`(头文件里有 `typedef struct X X;`)。只看输入文件本身,不看它导入的模块;类不导出。 |
| `--lines file` | 把每个函数里"某段机器码从哪一行源码开始"的表写到 `file`：`F 函数名`，然后每行 `L 函数内偏移 文件:行`。只在单文件编译（不带 `--incremental`）时给出；运行时库里的代码标 `(runtime)`。配合 `-g`（符号表）和 `driver/tools/cfline.sh exe file 地址` 把地址换成 `函数+偏移 文件:行`。 |
| `--emit-flat` | 写出展开了所有 `use` 的单文件源码后停止。诊断里的行号指的是这份文本。 |
| `--incremental dir` | 每个源文件一个 `.o`,放在 `dir`,没变的复用;再链接。前端仍检查整个程序。文件的记号(注释和空行不算)、别的文件的接口(函数头、类字段和方法头、枚举、别名、模块级 `let`、泛型整体)、编译器、`.cfc`、优化级别任何一样变了,才重编。这个模式下优化器不跨文件内联。 |
| `-j n` | 增量构建里同时编译 `n` 个文件(进程,共享前端的结果);默认是在线的处理器数,最多 16。产物和 `-j 1` 逐字节相同。 |
| `--dynamic` | 强制用 `cc` 链接。 |
| `--stats` | 编译后在标准输出打印一张表:代码量最大的 30 个函数,每个函数的字节数、IR 指令数、基本块数、循环数、load/store/调用/分配数、陷阱数(越界和除零检查)、放进栈帧的值的个数、保存的寄存器数、栈帧大小,最后是合计。只统计程序本身,不含链接进来的运行库。`--incremental` 下不可用。 |
| `--lint-perf` | 编译后在标准输出打印性能和无意义行为的发现,每条是 `文件:行: perf\|pointless: in '函数': 说明`:循环里分配或 memcpy(每次迭代复制字符串/聚合)、循环里除以循环不变量、同一地址紧接着 load 两次、只写不读的变量、分配后只释放没用过的块、与 0 相加或乘 1、值与自身比较、栈帧里的值超过 16 个(寄存器压力)。用法见 [如何找热点和无意义的代码](../how-to/find-hot-spots.md)。 |
| `-g` | 输出文件保留符号表(函数和数据的名字、大小、地址)和节头,gdb、perf 能显示名字,并生成 DWARF(版本 4):每个函数一个子程序、一段行号序列、`.debug_frame` 调用帧信息(所以断在函数第一条指令也能 `bt`),gdb 能按源码行下断点、单步、`bt` 显示 `文件:行`,valgrind/callgrind 的报告里有 `文件:行`。文件名是各 `.cf` 源文件(相对路径加编译时的 `PWD`),不是展开后的文本。变量和类型:每个参数和局部变量放在栈帧里,`info args`、`info locals`、`print x`、`print *p`、`ptype` 可用(任何 `-O` 级别都一样,代价是变量不再进寄存器,所以 `-g` 的代码比不带 `-g` 的慢);类的变量显示为指向结构体的指针,`&类` 与类变量是同一个指针,`str` 显示为字符串,`buf`/切片/元组显示为结构体,带 payload 的 enum 显示为 `tag` 加各变体的字段(只有 `tag` 所指的变体有值)。`--static`:DWARF 由自带的链接器生成。`--dynamic`(`cc` 链接):链接后用可执行文件自己的符号表补 DWARF,有变量。`--incremental`:每个文件的目标文件带行号表,链接后补 DWARF,只有行号和回溯,没有变量(各文件分开编译,类型编号不一致)。JIT 见 `cfjit --gdb`。不给 `-g` 时输出没有符号表也没有节头(`--static`),用 `cc` 链接时加 `-s`。 |
| `--verify` | 每个函数翻译前跑 IR 验证器(`tea test` 加这个);不给时不跑,编译快约 9%。 |
| `--align` | 每个函数从 16 字节边界开始(`tea` 的发布构建加这个)。不给时函数紧挨着放,文件更小。 |
| `--static` | 不用 `cc` 和 C 库:程序加上 `rt/src/min.cf` 里用 Coffee 写的 C 库(`malloc`、`calloc`、`free`、`memcpy`、`strlen`、`strdup`、`strcmp`、`open`、`read`、`write`、`close`、`remove`、`access`、`chmod`、`fork`、`waitpid`、`system`、`clock_gettime`、`exit` 等,走 Linux 系统调用;没有 `printf`、`fopen`,因为它们要变长参数),由 `ld/` 的链接器在本进程里链成静态可执行文件。程序不能用别的 C 函数(`printf`、`fopen` 等),否则链接报 `undefined symbol`。可以和 `--incremental` 一起用。程序只导入 `min.cf` 有的 libc 函数时,不加选项也是这样链接;导入了别的 C 函数或别的 C 库就用 `cc`。 |
| `--time` | 每个阶段花的毫秒数:展开、改名、解析、哈希、前端、逐文件的降低/代码生成/写对象(或并行时合在一起)、链接。 |

## 默认输出

| 选项 | 没有 `-o` 时 |
|---|---|
| 无 | `a.out` |
| `--emit-obj` | `<input>.o` |
| `--emit-flat` | `<input>.flat` |

## 模块与 C 库

导入的 Coffee 模块先在源文件所在目录找,再依次在各 `-I` 目录找。模块文本放在导入它的文件前面,每个模块只放一次,`use` 行去掉;`use ... of c` 行保留。

libc 和 libm 的签名内置在编译器里。其他 C 库要有 `<库名>.cfc`,在源文件目录、其 `lib/`、`-I` 目录或其 `lib/` 里。链接命令带上找到它的目录的 `-L` 和 rpath,以及 `-l<库名>`。

## 退出码

| 值 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 前端报错、不能翻译的函数、链接失败 |
| 2 | 命令行用法错误 |

## 限制

- 只在 Linux 和 Android 上能用:参数从 `/proc/self/cmdline` 读取。
- 机器码后端只支持 x86-64。
- 诊断只有错误码和展开后源码的行号,没有文件名。
- 链接默认调用 `cc`;`--static` 用自带的链接器。

## 相关

- 操作步骤:[构建编译器](../how-to/build-the-compiler.md)、[链接 C 库](../how-to/link-a-c-library.md)、[生成机器码](../how-to/emit-machine-code.md)
