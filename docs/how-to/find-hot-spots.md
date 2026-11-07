# 如何找热点和无意义的代码

目标:找出程序(或编译器自己)的时间花在哪里,以及哪些代码做了没有意义的事。

## 找热点

```bash
tea prof                 # 当前项目的入口,-O1,打印热点
tea prof --jit           # 同一个程序,经 cfjit 运行,剖析生成的机器码
tea prof app.cf --top 10 --hz 2000 -- 参数
```

不用 `tea` 时:

```bash
coffee2 app.cf --static -g -O1 --lines app.lines -o app
cfprof --lines app.lines -- ./app
```

先看 self% 最高的函数,再看它的 called from 和 line 表,确定是哪个调用点、哪一行。需要火焰图时加 `--folded out.txt`。程序卡住或太慢时用 `--seconds 5`,到时间杀掉并报告停在哪里。

## 检查生成的代码

```bash
coffee2 app.cf --stats        # 每个函数的代码量、load/store、调用、分配、栈帧里的值
coffee2 app.cf --lint-perf    # 循环里的分配和复制、循环不变的除法、只写不读的变量……
```

`--stats` 的 spills 列高的函数,寄存器不够用,把它拆小。`--lint-perf` 的每条发现带 `文件:行`,按函数列出。发现不一定是错:`perf` 类是代价的提示,`pointless` 类基本可以删。

## 剖析编译器自己

```bash
coffee/driver/tools/profile-compiler.sh   # 用 -g 编译 coffee2,再剖析它编译自己
```

## 参考

- [cfprof](../reference/cfprof.md)
- [命令行](../reference/cli.md)(`--stats`、`--lint-perf`、`-g`)
- [tea](../reference/tea.md)(`tea prof`)
