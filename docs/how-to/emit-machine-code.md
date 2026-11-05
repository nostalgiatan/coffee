# 如何生成机器码而不是 LLVM 文本

目标:用自举编译器自己的 x86-64 后端生成目标文件或可执行文件,不经过 LLVM。

前提:x86-64 Linux。后端目前只支持这一个目标。

## 直接生成可执行文件

```bash
coffee2 prog.cf -o prog
```

## 只生成目标文件

```bash
coffee2 prog.cf --emit-obj -o prog.o
```

不给 `-o` 时,输出文件叫 `prog.cf.o`。

然后自己链接:

```bash
clang prog.o -o prog -lm
```

链接依然用 `clang`。后端自己写 ELF 目标文件,但没有写链接器。

## 检查生成的指令

```bash
objdump -d prog.o
```

## 调试

- 编译时出现 `machine code: N functions could not be translated`,说明有 N 个函数用到了后端没处理的 IR 操作。
- 想知道某个问题是不是后端造成的,把同一个程序加 `--emit-llvm` 走 LLVM 后端,链接后对比结果。

## 相关

- 后端的约定和限制:[x86-64 后端参考](../reference/x86-backend.md)
- 设计取舍:[后端的取舍](../explanation/backend-choices.md)
