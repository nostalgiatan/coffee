# Coffee 文档

Coffee 是带 Python 式缩进语法的静态类型语言,有双向的 C 互操作。编译器有两代:自举编译器(`coffee/` 目录,用 Coffee 写成)是现在的主线,Rust 写的第一代是 stage0,只负责引导和运行测试。这套文档描述自举编译器。

文档按 [Diátaxis](https://diataxis.fr) 分成四类。

## 教程

跟着做,学会基本用法。

- [写、编译并运行第一个程序](tutorials/first-program.md)

## 操作指南

已经知道要做什么,查怎么做。

- [构建自举编译器](how-to/build-the-compiler.md)
- [链接 C 库](how-to/link-a-c-library.md)
- [生成机器码](how-to/emit-machine-code.md)
- [运行测试](how-to/run-the-tests.md)
- [添加测试](how-to/add-a-test.md)

## 参考

查事实。

- [命令行](reference/cli.md)
- [编译流水线](reference/pipeline.md)
- [IR](reference/ir.md)
- [x86-64 后端](reference/x86-backend.md)
- [tea](reference/tea.md)
- 语言语法:[SYNTAX.md](../SYNTAX.md)(按 Rust 版整理,自举编译器接受它的子集)
- 类型存储表:[coffee/types/STORAGE.md](../coffee/types/STORAGE.md)
- 所有权规则:[coffee/own/RULES.md](../coffee/own/RULES.md)

## 说明

理解为什么。

- [固定点检查与 Rust 版的退出路线](explanation/stage0.md)
- [所有权与释放怎么落到代码上](explanation/ownership-and-drop.md)
- [IR 的设计](explanation/ir-design.md)
- [机器码后端的取舍](explanation/backend-choices.md)

## 存档

- [docs/superpowers](superpowers):按日期记录的设计文稿和计划。
- [docs/legacy-rust](legacy-rust/README.md):Rust 版编译器的文档,停在 0.3.9。
