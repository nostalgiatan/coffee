# 类型系统完整性审查（2026-09-04）

HEAD：`2cbc64f`（本文件记录的闭合项含之后工作区改动）  
证据以 `cargo test` 输出为准，见审查完成时的测试命令。

**结论：Coffee 当前子集（含 C `object` 句柄）的审查项已闭合到可验收程度；仍不是形式化健全类型系统。**

## 已闭合（有实现 + 有测试或明确职责边界）

| 项 | 状态 |
|----|------|
| 语句全覆盖（含 Main/Class/Enum/Import/break） | 走进 checker |
| `let` / 赋值（含 `a.b.c`）左右值 | 有测试 |
| if/while 条件 bool | 有测试 |
| for-in 非数组报错；数组元素类型 | 有测试 |
| `for i in a..b` 要求 a/b 为 int | 有测试 |
| match 枚举穷尽 + 载荷类型 | 有测试 |
| match int/float/string 需 catch-all；bool 需 true+false 或 `_`；元组需 catch-all | 有测试 |
| match guard 必须是 bool | 有测试 |
| 结构体字面量字段、构造参数、未知字段/方法 | 有测试 |
| Call 参数；C 调用返回句柄而非 Function 值 | 有测试 |
| `raise` 仅 `NamedType`（错误类 / `TestError(...)`）；int/bool/string 拒绝 | 有测试 |
| Coffee 赋值单向 `expected.can_coerce_from(found)` | 实现 |
| `object` 入参：`is_c_handle_value()`（不含 bool） | 有测试 |
| `object` 出参：仅 pointer-sized int / string / ref / object（拒绝 `int(4)+`） | 有测试 |
| 空 `[]` 对 slice 注解 `[T]` 取注解元素类型 | 有测试 |
| `check_import` 只记账；模块存在性在 pipeline | 有意分工 |
| `rm`/`mv` 对 `object` 不按 Coffee drop 卡死 | 实现 |
| `printf` 可变参多余实参：字符串按 ptr 传递；bool 拒绝 | 有测试 |
| 诊断 E100–E112 | 工作区 `diagnostics.rs` + `type_error_kind_tests` |

## 有意保留（本轮不实现）

1. **字面量 AST 仍是 `Literal(String)`** — 检查走 `infer_value_type` 是字面量表示，不是漏检路径。
2. **赋值左值仍是点号字符串** — `a.b.c` 已按路径检查；改成 `Expression` LHS 是 AST 重构，不是类型漏洞。
3. **无引用/生命周期证明** — C 用 `object` + `free`。
4. **形式化 soundness**（别名、未初始化等）否定。

## 一句话

可修的类型与 `printf` 可变参代码生成已落地；不要对外说「形式化健全」。
