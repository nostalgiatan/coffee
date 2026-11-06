# 如何运行测试

目标:运行某一阶段的测试,或者全部测试。

测试是 Coffee 写的,由 `tea test` 运行。每个阶段的测试在自己目录的 `tests/` 下,每个文件的 `main` 返回失败的个数,0 表示全部通过。

## 用 tea 运行

先构建 `tea`(和 `coffee2` 一起,不需要 Rust):

```bash
coffee/driver/tools/build-from-seed.sh /tmp/coffee-seed
```

在某个阶段的目录里运行,指定刚构建的编译器:

```bash
cd coffee/ir
/tmp/coffee-seed/tea test --compiler /tmp/coffee-seed/coffee2
```

每个阶段依赖哪些阶段写在它自己的 `coffee.toml` 里,不需要再列 `-I`。选项见 [tea 参考](../reference/tea.md)。

只运行一个文件,把路径放在命令末尾:

```bash
/tmp/coffee-seed/tea test --compiler /tmp/coffee-seed/coffee2 tests/test_opt.cf
```

## 移植过来的 Rust 测试

`coffee/compat` 里是从 Rust 编译器的集成测试移植来的用例,用同样的方式运行:

```bash
cd coffee/compat
/tmp/coffee-seed/tea test --compiler /tmp/coffee-seed/coffee2
```

兼容用例(1023 个)全部通过。没有移植的部分和两个编译器的差别见 `coffee/compat/README.md`。

## 运行类测试怎么跑

`coffee/ir` 的运行类测试把程序编成目标文件,用 `clang -fsanitize=address` 链接,再运行并比较退出码。临时文件写在当前目录。`tea` 让每个测试在自己的目录里运行,所以它们可以并行。

一次传多个文件路径时,我只看到第一个被运行,所以逐个文件运行。

## 内存错误

运行类测试用 AddressSanitizer 编译。内存泄漏或重复释放时进程以退出码 99 结束,测试输出 `FAIL: got 99`。

## 相关

- 写新测试:[如何添加测试](add-a-test.md)
