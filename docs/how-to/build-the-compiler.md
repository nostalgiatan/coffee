# 如何构建自举编译器 `coffee2`

目标:得到可执行文件 `coffee2`,并确认它能编译自己。

前提:x86-64 Linux 和系统的 C 编译器 `cc`。不需要 Rust 和 LLVM。

## 不用 Rust 和 LLVM:从种子构建

x86-64 Linux 上,只要有系统的 C 编译器(默认 `cc`,可用 `CC` 指定):

```bash
coffee/driver/tools/build-from-seed.sh /tmp/coffee-seed
```

得到 `/tmp/coffee-seed/coffee2`,约半分钟。脚本把仓库里的种子目标文件 `coffee/driver/seed/coffee2-x86_64.o.gz` 链接成编译器,用它编译源码,再用结果编译一遍,要求两次的目标文件逐字节相同。种子比源码旧时,加 `--refresh` 更新它:

```bash
coffee/driver/tools/build-from-seed.sh /tmp/coffee-seed --refresh
```

## 验证它能复现自己

脚本已经做了:种子编出 `gen1.o`,`gen1` 再编出 `gen2.o`,两者逐字节相同才算通过。目标文件格式变了的那一次,种子的输出是旧格式,脚本会多编一轮(`gen3.o`),要求 `gen2.o` 和 `gen3.o` 相同。

## 第一代(Rust)

第一代编译器已经移出主目录。它的最后一个版本在 git 标签 `rust-stage0-final` 里:`git checkout rust-stage0-final` 后用 `cargo build --release` 构建(需要 LLVM 21.1)。种子是从它引导出来的,日常构建不再需要它。

## 相关

- 命令行选项:[命令行参考](../reference/cli.md)
- 为什么要比较两次:[为什么用固定点检查自举](../explanation/stage0.md)
