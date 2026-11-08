# 如何让 Coffee 程序调用自己的 C 库

目标:在 Coffee 里调用一个不属于 libc 和 libm 的 C 函数。

## 写 `.cfc` 文件

编译器读 `.cfc` 文件,不直接读 C 头文件。每个导出的函数一行,写成 Coffee 的 `c fn` 形式,类型用 Coffee 的写法。库叫 `greet`,文件就叫 `greet.cfc`:

```
c fn twice(x: int(4)+) => int(4)+:
```

`int(4)+` 是 4 字节有符号整数,对应 C 的 `int`。

有头文件时让编译器生成:

```bash
coffee2 -c greet.h
```

写出 `greet.cfc`。生成器把头文件当文本读(不预处理,不跟 `#include`),结构体变成不透明的 `type 名: object`,写不出的函数留成 `/#/ skipped` 注释,生成后看一眼再用。没有头文件就手写(格式见 `library/cfc/libc.cfc`)。

反过来,Coffee 写的库要给 C 调用:`coffee2 --have-c lib.cf --emit-obj` 写出 `lib.h`(只含带函数体的 `c fn`),再 `cc main.c lib.cf.o`。

## 放在编译器找得到的位置

`greet.cfc` 放在下面任意一处:

- 程序源文件旁边
- 源文件旁边的 `lib/`
- 某个 `-I` 目录,或它的 `lib/`

库文件本身(`libgreet.so`,或静态库)放在同一个目录,链接时会按这个目录加 `-L`、rpath 和 `-lgreet`。

## 在程序里使用

```
use printf in libc of c
use twice in greet of c

fn main() => int:
    printf("%d\n", twice(21))
    return 0
```

## 编译并运行

```bash
cc -shared -fPIC -o lib/libgreet.so greet.c
coffee2 use_c.cf -o use_c
./use_c
```

输出 `42`。

## 不用 `cc` 链接

库以目标文件(`greet.o`)或静态库(`libgreet.a`)的形式放在 `.cfc` 同一目录时,`coffee2` 用自带的链接器 `cfld` 链接,机器上不需要 `cc`。这对三个目标都成立(`--target=aarch64-linux`、`--target=riscv64-linux` 要加 `--static`;x86-64 自动选择)。C 部分用目标对应的编译器做,例如 `clang --target=riscv64-linux-gnu -ffreestanding -O1 -c greet.c -o greet.o`。

条件:库只依赖内置 C 库(`library/cfc/libc.cfc` 里 `rt/src/min.cf` 提供的那部分)。依赖 glibc 其余部分的库(`printf`、`snprintf`、`libm`)或只有 `.so` 的库,仍然走 `cc`;在没有 `cc` 的机器上这类程序链接失败。回调(C 调 Coffee 函数值)、浮点、超过 8 个整数参数的调用在三个目标上都测过(`driver/tests/test_static_lib.cf`)。

## 注意

- 同名函数在 libc 或 libm 的内置表里已经有时,保留内置表的签名。
- 找不到库文件时,错误出现在链接阶段。

## 相关

- 所有选项:[命令行参考](../reference/cli.md)
