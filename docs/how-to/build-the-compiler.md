# 如何构建自举编译器 `coffee2`

目标:得到可执行文件 `coffee2`,并确认它能编译自己。

前提:已经有 Rust 版编译器(stage0),`clang` 和 LLVM 21.1 在 `PATH` 上。

## 构建 stage0

在仓库根目录:

```bash
cargo build --release
```

得到 `target/release/coffee`。

## 用 stage0 编译 `coffee2`

在 `coffee/` 目录下:

```bash
I="-I driver/src -I ir/src -I types/src -I sema/src -I parser/src -I lexer/src"
../target/release/coffee --test-mode $I driver/src/main.cf --bin -o coffee2
```

这一步要几分钟。我量过一次 Rust 版编译这份源码的耗时,约 4 分钟,所以第一次别以为它卡住了。

## 验证它能复现自己

用脚本一次做完:

```bash
driver/tools/bootstrap.sh ../target/release/coffee /tmp/coffee-boot
```

脚本做四件事:

1. stage0 编译出 `gen2`。
2. `gen2` 把驱动自己的源码编译成 LLVM 文本 `gen2.ll`。
3. `clang` 把 `gen2.ll` 编成 `gen3`,`gen3` 再编译一遍,得到 `gen3.ll`。
4. 比较两份文本。

最后一行输出 `fixed point: gen2 and gen3 produce identical LLVM text` 就是通过。两份文本不同,脚本会停在 `cmp` 报错。

## 验证机器码后端也能复现自己

在脚本留下的工作目录里,用 `gen2` 以 `--emit-obj` 编译驱动,链接后再编译一遍,比较两个 `.o`:

```bash
cd /tmp/coffee-boot
I="-I <coffee>/driver/src -I <coffee>/ir/src -I <coffee>/types/src -I <coffee>/sema/src -I <coffee>/parser/src -I <coffee>/lexer/src"
./gen2 main.cf $I --emit-obj -o a.o
clang -w -o gen4 a.o -lm
./gen4 main.cf $I --emit-obj -o b.o
cmp a.o b.o && echo identical
```

把 `<coffee>` 换成仓库里 `coffee/` 目录的绝对路径。

## 相关

- 命令行选项:[命令行参考](../reference/cli.md)
- 为什么要比较两次:[为什么用固定点检查自举](../explanation/stage0.md)
