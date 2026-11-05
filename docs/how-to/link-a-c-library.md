# 如何让 Coffee 程序调用自己的 C 库

目标:在 Coffee 里调用一个不属于 libc 和 libm 的 C 函数。

## 写 `.cfc` 文件

编译器不读 C 头文件,它读 `.cfc` 文件。每个导出的函数一行,写成 Coffee 的 `c fn` 形式,类型用 Coffee 的写法。库叫 `greet`,文件就叫 `greet.cfc`:

```
c fn twice(x: int(4)+) => int(4)+:
```

`int(4)+` 是 4 字节有符号整数,对应 C 的 `int`。

如果手头有头文件,可以让 stage0 生成 `.cfc`:`coffee -c header.h [-I path]`。自举编译器目前不能生成。

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

## 注意

- 同名函数在 libc 或 libm 的内置表里已经有时,保留内置表的签名。
- 找不到库文件时,错误出现在链接阶段。

## 相关

- 所有选项:[命令行参考](../reference/cli.md)
