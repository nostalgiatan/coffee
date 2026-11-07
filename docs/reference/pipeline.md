# 编译流水线参考

源文件进入后依次经过下面的阶段。每个阶段是 `coffee/` 下的一个目录,用 Coffee 写成,有自己的 `README.md` 和 `tests/`。

| 阶段 | 目录 | 输入 | 输出 | 报告的问题 |
|---|---|---|---|---|
| 词法 | `lexer` | 源文本 | 词表(`kind` `start` `end` `line` `col` 五列) | 词法错误 |
| 语法 | `parser` | 词表 | 语法树 | 解析错误码 |
| 语义 | `sema` | 语法树 | 解析了名字的树 | E101 E103 E106 E200 E201 E600,未使用变量警告 |
| 类型 | `types` | 语法树和名字 | 每个表达式的类型、存储表 | 类型错误 |
| 所有权 | `own` | 带类型的树 | 移动点、作用域出口的状态 | E300 E301 E302 E303 E400 E401 |
| 降级 | `ir`(`lower.cf`) | 带类型的树 | 每个函数一个 `Fun` | 不支持的构造 |
| 优化 | `ir`(`opt.cf` `modopt.cf`) | `Fun` | `Fun` | 无 |
| 后端 | `ir`(`gen.cf` `asm.cf` `elf.cf`) | `Fun` | ELF 目标文件 | 不能翻译的函数 |
| 驱动 | `driver` | 命令行 | 可执行文件或中间产物 | |

每个阶段的规则:

- 类型规则:[types/RULES.md](../../coffee/types/RULES.md)
- 存储表(每种类型的表示、大小、所有权、drop 与 clone):[types/STORAGE.md](../../coffee/types/STORAGE.md)
- 所有权与借用规则:[own/RULES.md](../../coffee/own/RULES.md)

## 阶段之间的约定

- 词表和语法树都是整数列(`buf<int>`),没有每个节点一次的分配。
- 第一代要求导入关系是一棵树,所以 `driver/src/cstr.cf` 和 `files.cf` 自己不导入任何模块,由使用者(驱动、`tea`)按顺序各列一次。自举编译器自己的展开器没有这个限制。
- 导入在驱动里展开:被导入模块的文本放到导入者前面,各阶段看到的是一份单文件源码。
- 后端只有一个:`gen.cf` 写 x86-64 机器码。LLVM 文本后端已经移除,历史版本里可以找到。

## 相关

- IR 的格式:[IR 参考](ir.md)
- 机器码后端:[x86-64 后端参考](x86-backend.md)、[aarch64 后端参考](aarch64-backend.md)
