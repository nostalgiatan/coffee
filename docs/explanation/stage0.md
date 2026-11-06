# 为什么用固定点检查自举,以及第一代为什么退出

Coffee 的编译器有过两代。第一代用 Rust 写成,编到 LLVM。第二代用 Coffee 写成,在 `coffee/` 里,叫自举编译器,现在是唯一的编译器。第一代已经移出主目录,最后一个版本在 git 标签 `rust-stage0-final`。

## 固定点检查在证明什么

编译器编译自己的源码得到 `gen1`,`gen1` 再编译同一份源码得到 `gen2`。比较两次输出的目标文件,如果逐字节相同,就说明编译器的输出不再依赖是谁编译了它。

这个检查能发现的是编译器自己的前后不一致,比如某个构造在被编译时走一条路径,生成的程序里走另一条。它不能证明编译器是对的:如果两代对某个构造犯了同样的错,两份文本仍然相同。所以固定点检查要和其他测试一起用,不能单独当作正确性的证据。

## 第一代怎么退出的

1. 冻结。第一代只修阻碍构建和测试的问题,不加新功能。
2. 提交种子。一份固定点的目标文件放在 `coffee/driver/seed/`,`coffee/driver/tools/build-from-seed.sh` 用它加系统的 `cc` 构建第二代,不需要 Rust 和 LLVM。
3. 补齐工具。测试运行器和构建工具(`tea`)用 Coffee 写成;第一代集成测试里适用于编译器的部分移植到 `coffee/compat`(1023 个用例)。
4. 归档。第一代的源码、测试和 `Cargo.toml` 从主目录删除,打上标签 `rust-stage0-final`。

## 第一代有而现在没有的功能

这些没有迁移,需要时要在第二代或 `tea` 里重新做:

- 从 `.h` / `.c` 生成 `.cfc`(`-c`),以及 `[dependencies.c_libraries]` 的依赖图和自动生成;现在 `.cfc` 要手写,libc 和 libm 的内置。
- 从 `c fn` 导出生成 `.h`(`--have-c`)。
- 包依赖:`coffee fetch`、`[dependencies.packages]`、`coffee std install`、内嵌的 std(`library/std` 还在,但不会自动导入)。
- 驱动的 `--static`、`--static-lib` 选项(静态可执行文件现在由 `cfld` 做,见 `coffee/ld/README.md`,只是驱动还没接上,默认仍用 `cc`)、`--target` 交叉编译、`--jit`、`--emit-llvm` / `--emit-bc`、`--emit-ast`、`--show-memory`。
- 运行时陷阱:算术溢出和除零不再检查(越界下标仍然陷阱)。
- `coffee run` / `coffee test` 的项目模式:由 `tea run` / `tea test` / `tea build` 代替。

## 相关

- 操作步骤:[构建编译器](../how-to/build-the-compiler.md)
