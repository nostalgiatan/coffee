# Coffee 内存契约（Plan B）设计

日期：2026-09-04  
前置：类型洞先合进当前树，再改本契约（与 `checker.rs` 冲突最少）。

## 目标

减轻每个 `let` 都要 `rm` 的仪式，同时让资源所有权比现在更硬：赋值不再悄悄复制资源。

## 语言规则（已锁定）

1. **值类型**（`int` / `float` / `bool`，以及只含值类型的元组/定长数组）：离开作用域自动结束，不必 `rm`。
2. **资源类型**（`class` 实例、`str`、slice、`object` C 句柄）：`b = a` 是编译错误；必须 `mv a b` 或 `clone a`。
3. **`rm` 只用于提前释放**；作用域出口由编译器插入等价清理。
4. **删除**用户侧 `copy` 与 `clean out`。
5. **用户源码不再写 `malloc`/`free`** 作为 Coffee 内存原语。C 库的 `malloc`/`free` 仍可通过 `use … in libc of c` 得到 `object` 句柄，由 `rm`/`mv`/`clone` 管句柄，不把指针当成 `int(4)+`。
6. **`clone` 优先表达式**：`let b: T = clone a`（语句 `clone a b` 可暂留一层兼容或一并删，实现时选一种并改完全部测试）。

## 非目标

- 改 C ABI 或 `.cfc` 格式
- 形式化健全性

（过程内借用检查已于 2026-09-05 落地，见 `2026-09-05-borrow-checker.md`。）

## 实现顺序（合进 master 后再开）

1. Checker：资源赋值报错；值类型读路径不再要求 Alive+rm。
2. Codegen：函数/块出口对仍 Alive 的值类型不生成 `free`；资源类型生成析构/`free`（`object` 不 Coffee-drop 载荷，只丢句柄变量）。
3. Parser：拒绝 `copy`、`clean out`（或解析后诊断）。
4. 测试：`memory_tests` 与全套夹具去掉值类型上强制 `rm`；资源测试改为 `mv`/`clone`/`rm`。

## 成功标准

- `let x: int = 1` 无 `rm` 能编译。
- `let p: Point = …` 后 `q = p` 报错；`mv p q` 或 `clone` 通过。
- 现有 C `object` + `malloc` 测试仍绿（libc 导入，不是语言关键词 `alloc`）。
- `copy` / `clean out` 源码为错误。
