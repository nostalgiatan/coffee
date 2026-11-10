# 如何构建自举编译器 `coffee2`

目标:得到可执行文件 `coffee2`,并确认它能编译自己。

前提:x86-64 或 aarch64 的 Linux。不需要 C 编译器、Rust 和 LLVM。

## 不用 Rust 和 LLVM:从种子构建

脚本按机器选种子(`uname -m`):

```bash
coffee/driver/tools/build-from-seed.sh /tmp/coffee-seed
```

得到 `/tmp/coffee-seed/coffee2`、`tea`、`cfjit` 和 `cfprof`,约半分钟。种子是编译器的静态可执行文件:`coffee/driver/seed/coffee2-x86_64.gz` 和 `coffee2-aarch64.gz`(后者由 x86-64 的编译器用 `--target=aarch64-linux --static` 交叉编出);脚本用它编译源码得到 `gen1`,`gen1` 再编译一遍得到 `gen2`,要求两者逐字节相同。种子比源码旧时,加 `--refresh` 更新它:

```bash
coffee/driver/tools/build-from-seed.sh /tmp/coffee-seed --refresh
```

在 x86-64 上 `--refresh` 同时刷新两个种子:脚本把刚编出的编译器再交叉编成 aarch64,和已提交的 aarch64 种子比较,不同就重写。在 aarch64 上只刷新 aarch64 种子。aarch64 种子在 aarch64 上编译源码得到 `gen1`,要求与种子逐字节相同(CI 的 `seed-aarch64` 任务在真机上做这件事;本机可在 qemu-aarch64 下手动做)。编译器的默认目标是它运行所在的机器(读自己的 ELF 头),所以 aarch64 上的 `coffee2` 默认生成 aarch64 代码;`--target=` 照常覆盖。

## 验证它能复现自己

脚本已经做了:`gen1` 和 `gen2` 逐字节相同才算通过。种子比源码旧、输出不同的那一次,脚本会多编一轮(`gen3`),要求 `gen2` 和 `gen3` 相同。

## 第一代(Rust)

第一代编译器已经移出主目录。它的最后一个版本在 git 标签 `rust-stage0-final` 里:`git checkout rust-stage0-final` 后用 `cargo build --release` 构建(需要 LLVM 21.1)。种子是从它引导出来的,日常构建不再需要它。

## 相关

- 命令行选项:[命令行参考](../reference/cli.md)
- 为什么要比较两次:[为什么用固定点检查自举](../explanation/stage0.md)
