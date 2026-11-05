# 教程:写、编译并运行第一个 Coffee 程序

这篇教程带你从一个空目录开始,写一个会打印斐波那契数的程序,把它编译成可执行文件并运行,最后换一种方式生成机器码再运行一次。大约需要十分钟。

开始之前,你需要一个能用的 `coffee2`。如果还没有,先按 [构建编译器](../how-to/build-the-compiler.md) 做出来,然后回到这里。你还需要 `clang`,编译器用它来链接。

## 1. 写程序

新建一个目录,在里面创建 `hello.cf`:

```
use printf in libc of c

fn fib(n: int) => int:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)

fn main() => int:
    for i in 0..10:
        printf("fib(%d) = %d\n", i, fib(i))
    return 0
```

第一行从 C 的标准库里取 `printf`。Coffee 用缩进表示块,`main` 的返回值就是进程的退出码。

## 2. 编译

```bash
coffee2 hello.cf -o hello
```

没有任何输出就是成功了。目录里多了一个可执行文件 `hello`。

## 3. 运行

```bash
./hello
```

你会看到从 `fib(0) = 0` 到 `fib(9) = 34` 的十行输出。

## 4. 看一眼编译器生成了什么

默认情况下,编译器直接生成 x86-64 机器码。加上 `--emit-obj`,它停在目标文件这一步:

```bash
coffee2 hello.cf --emit-obj -o hello.o
objdump -d hello.o | head -30
```

看到的是每个函数的指令。链接这一步交给系统的 `cc`。

```bash
coffee2 hello.cf --emit-llvm -o hello.ll
head hello.ll
```

文件开头是一些 `declare` 行,后面是每个函数的定义。你不需要读懂它,只要知道前面的编译就是先生成这个文本,再交给 `clang`。

## 5. 换成 LLVM 文本

还有第二条路:生成 LLVM 文本,再交给 `clang`。

```bash
coffee2 hello.cf --emit-llvm -o hello.ll
head hello.ll
clang hello.ll -o hello_llvm -lm
./hello_llvm
```

输出和刚才一样。文件开头是一些 `declare` 行,后面是每个函数的定义。这条路径只用来做对照,不是默认。

## 你完成了什么

你写了一个 Coffee 程序,用 `coffee2` 编译并运行了它,看到了 LLVM 文本,也用机器码后端生成过可执行文件。

接下来可以:

- 让程序调用你自己的 C 库,见 [链接 C 库](../how-to/link-a-c-library.md)。
- 查看 `coffee2` 的全部选项,见 [命令行参考](../reference/cli.md)。
- 想知道语言的全部语法,见仓库根目录的 [SYNTAX.md](../../SYNTAX.md)。
