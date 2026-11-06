# 如何运行测试

目标:运行某一阶段的测试,或者全部测试。

测试是 Coffee 写的,由 stage0 的 `coffee test` 运行。每个阶段的测试在自己目录的 `tests/` 下,每个文件的 `main` 返回失败的个数,0 表示全部通过。

## 运行一个阶段

在该阶段的目录里运行,`-I` 要列出它依赖的阶段:

| 目录 | 命令 |
|---|---|
| `coffee/lexer` | `coffee test -I src` |
| `coffee/parser` | `coffee test --test-mode -I ../lexer/src` |
| `coffee/sema` | `coffee test --test-mode -I ../parser/src -I ../lexer/src -I src` |
| `coffee/types` | `coffee test --test-mode -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |
| `coffee/own` | `coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |
| `coffee/ir` | `coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |
| `coffee/driver` | `coffee test --test-mode -I ../ir/src -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src` |

`--test-mode` 跳过进程锁。不加它,同一目录下并发运行两个 `coffee` 会失败。

## 只运行一个文件

在命令末尾加上文件路径:

```bash
coffee test --test-mode -I ../types/src -I ../sema/src -I ../parser/src -I ../lexer/src -I src tests/test_opt.cf
```

## 运行类测试怎么跑

`coffee/ir` 的运行类测试把程序编成目标文件,用 `clang -fsanitize=address` 链接,再运行并比较退出码。

一次传多个文件路径时,我只看到第一个被运行,所以逐个文件运行。

## 内存错误

运行类测试用 AddressSanitizer 编译。内存泄漏或重复释放时进程以退出码 99 结束,测试输出 `FAIL: got 99`。

## 相关

- 写新测试:[如何添加测试](add-a-test.md)
