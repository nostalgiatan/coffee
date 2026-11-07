# tea 参考

`tea` 是编译器外面的工具。命令是模块:每个命令一个文件,在 `coffee/tea/src/` 下。

```
tea <command> [options]
```

## 命令

| 命令 | 作用 |
|---|---|
| `test` | 运行当前目录 `tests/` 里的测试。 |
| `build` | 编译 `coffee.toml` 的入口,得到 `target/build/<包名>`。 |
| `run` | 先 `build`,再运行程序;`--` 后面的词交给程序。 |
| `jit` | 用 JIT 运行程序:`coffee2 --emit-cir` 做 `.cir`,`cfjit` 在内存里生成代码并运行。 |

## tea test

```
tea test [-I dir]... [--compiler path] [-j N] [file.cf]...
```

| 选项 | 含义 |
|---|---|
| `-I dir` | 增加一个模块搜索目录,可重复,传给 `coffee2`。 |
| `--compiler path` | 用哪个 `coffee2`。默认 `PATH` 里的 `coffee2`。 |
| `-j N` | 同时运行几个测试。默认 4。 |
| `file.cf` | 只运行这些文件。省略时运行 `tests/*.cf`。 |
| `--one file.cf` | 内部使用:编译并运行一个测试,结果写进它的目录。 |

## tea build 和 tea run

```
tea build [-O0..-O3] [-I dir]... [--compiler path] [--debug]
tea run   [-O0..-O3] [-I dir]... [--compiler path] [--debug] [-- 程序参数...]
```

编译 `[build] main`(默认 `src/main.cf`),模块搜索目录是 `[build] include` 加上 `-I`,输出放在 `target/build/<[package] name>`。`run` 返回程序的退出码;编译失败时 `build` 和 `run` 返回 1。

默认是发布构建:输出文件没有符号表,函数从 16 字节边界开始(给 `coffee2` 加了 `--align`;`--debug` 构建不对齐,`coffee2` 直接调用也不对齐,文件更小)。`--debug` 输出到 `target/debug/<包名>`,带 `-g`(保留函数和数据的名字,供 gdb、perf 用),没给优化级别时用 `-O0`,对象缓存在 `target/obj-debug`。

## coffee.toml

`tea` 读当前目录的 `coffee.toml`:

```toml
[package]
name = "ir"

[build]
main = "src/main.cf"
include = ["../ir/src", "src"]

[test]
include = ["kit"]
```

`[build] include` 同时用于构建和测试;`[test] include` 只加给测试。列表可以写成一行或分成多行,注释以 `#` 开头。`coffee2` 不读 toml,只认命令行参数,`tea` 拼好 `-I` 再交给它。

两段都没有列表时,`tea` 读旧的 `tea.toml` 里的 `[test] include` 并提示迁移。

## 输出

每个测试一行:`ok`、`FAILED (compile)` 或 `FAILED (exit N)`。失败的测试后面跟着对应日志的最后 30 行。最后一行是 `test result: ok. N passed; 0 failed`,或 `FAILED`。

退出码:全部通过 0,有测试失败 1,命令行错误 2。

## 目录

每个测试一个目录 `target/test/<名字>/`:

| 文件 | 内容 |
|---|---|
| `bin` | 编译出的程序。 |
| `compile.log` | 编译器的输出。 |
| `run.log` | 程序的标准输出和标准错误。 |
| `status` | `compile`,或程序的退出码。 |

程序在这个目录里运行。`target/test/list` 是本次运行的测试清单。

## 相关

- 操作:[如何运行测试](../how-to/run-the-tests.md)

## tea prof

```
tea prof [file.cf] [-I dir]... [-O0|-O1] [--jit] [--hz N] [--top N] [--folded 文件] [--compiler path] [--prof path] [--cfjit path] [-- 程序参数...]
```

用 `-g` 编译程序(行号表放在 `target/prof/<文件名>.lines`),在采样剖析器 `cfprof` 下运行,打印热点。省略文件时用 `[build] main`。`--jit` 让程序经 `cfjit` 运行,剖析的是生成的机器码(函数名来自 `cfjit --perf-map`,行号来自 `cfjit --lines`)。默认 `-O1`。每次都重新编译。返回程序的退出码。`cfprof` 的选项见 [cfprof](cfprof.md)。

## tea jit

```
tea jit [file.cf] [-I dir]... [--compiler path] [--jit path] [--no-run] [-- 程序参数...]
```

省略文件时用 `[build] main`。`.cir` 放在 `target/jit/<文件名>.cir`。它只在以下情况重做:文件不存在、`coffee2` 比它新、入口所在目录或某个 include 目录里有更新的 `.cf` 文件(按修改时间判断,和 make 一样)。`--no-run` 只做 `.cir`。`cfjit` 带 `--cache` 运行:生成好的映像存在 `.cir` 旁边的 `.cir.cim`,`.cir` 内容不变就直接用它。`cfjit` 默认从 `PATH` 找,`--jit path` 指定。返回程序的退出码;前端失败时返回 1。
