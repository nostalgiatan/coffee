# 如何运行测试

目标:运行某一阶段的测试,或者全部测试。

测试是 Coffee 写的,由 `tea test` 运行(stage0 的 `coffee test` 也行)。每个阶段的测试在自己目录的 `tests/` 下,每个文件的 `main` 返回失败的个数,0 表示全部通过。

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

每个阶段依赖哪些阶段写在它自己的 `tea.toml` 里,不需要再列 `-I`。选项见 [tea 参考](../reference/tea.md)。

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

还没通过的用例放在 `coffee/compat/pending/`,不会运行。分类和没有移植的部分见 `coffee/compat/README.md`。

## 用 stage0 运行

stage0 的 `coffee test` 仍然可用,在该阶段的目录里运行,`-I` 要列出它依赖的阶段:

| 目录 | 命令 |
|---|---|
| `coffee/lexer` | `coffee test -I src` |
| `coffee/parser` | `coffee test --test-mode -I ../lexer/src` |
| `coffee/sema` | `coffee test --test-mode -I ../parser/src -I ../lexer/src -I src` |
| `coffee/types` | `coffee test --test-mode -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |
| `coffee/own` | `coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |
| `coffee/ir` | `coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |
| `coffee/driver` | `coffee test --test-mode -I ../ir/src -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |

`--test-mode` 跳过进程锁。不加它,同一目录下并发运行两个 `coffee` 会失败。stage0 的运行器在阶段目录里留下 `ir_lower_test*` 之类的临时文件,已经加入 `.gitignore`。

## 运行类测试怎么跑

`coffee/ir` 的运行类测试把程序编成目标文件,用 `clang -fsanitize=address` 链接,再运行并比较退出码。临时文件写在当前目录。`tea` 让每个测试在自己的目录里运行,所以它们可以并行。

一次传多个文件路径时,我只看到第一个被运行,所以逐个文件运行。

## 内存错误

运行类测试用 AddressSanitizer 编译。内存泄漏或重复释放时进程以退出码 99 结束,测试输出 `FAIL: got 99`。

## 相关

- 写新测试:[如何添加测试](add-a-test.md)
