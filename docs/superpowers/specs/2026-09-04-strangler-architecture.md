# Coffee 绞杀式改造：架构契约

日期：2026-09-04  
**Status (2026-09-05):** 单文件与工程共用 `CompilationPipeline`（`CompilerFrontend` 为别名）；表达式走 AST `compile_expr`。内存契约与借用检查见后续 spec，不是本文件的非目标清单现状。

选择：**A** — 在原仓库换骨架，不绿场、不同时改语法/内存。

## 目标

安全 Coffee 的编译路径变成：

`源码 → AST（表达式是树，不是字符串）→ 前端一次（parse/语义/类型）→ Codegen 只吃 AST → LLVM`

驱动（单文件 vs `coffee.toml`）只选择输入文件和输出形态，**不选择另一套语义**。

## 非目标（下一份计划）

- 改 `mv`/`clone`/`copy`/`rm`/`clean` 的语言契约
- 拿掉用户 `malloc`/`free`
- 资源赋值 `b = a` 报错
- 把 `analyzer.rs` 切成二十个文件（没有 lowering 层时切文件是搬家）
- 换 nom / 换 inkwell / 换 LLVM 版本

## 现状（已证实）

- 单文件与工程都调用 `CompilerFrontend`；`src/compiler/pipeline.rs` 未 `mod`，是死代码。
- 工程入口只数 `Statement::Main`（`main(run())`），**不认** `fn main()`。对照测试因此失败。
- 项目模式 `compile_project()` 不处理 `--emit-llvm`。
- `Expression` 树已存在，但 `if`/`while`/`match` 条件、match arm、`Assignment`、`FunctionBody::Expression`、`ConstructorCall.args` 仍是 `String`。
- Codegen 大量 `compile_expression_str` / `compile_body_line`；DEBUG 在盯字符串怎么切。match 缺 terminator 已补过，枚举构造仍是 E700。

## 分层（名称不要夸大）

1. **Parser AST** — 补全：凡是值的地方都是 `Expression`，凡是语句块都是 `Vec<Statement>`。
2. **Frontend** — 唯一 `CompilationPipeline`；`CompilerFrontend` 只持有 `Session` 并转发。
3. **Codegen** — `compile_expr(&Expression)` 分发；禁止生产路径上的 `expr.to_string()` 再解析。
4. **Typed HIR（本阶段末可选）** — 在 AST 节点上挂已解析 `Type`，给下一阶段内存规则用。这还不是 SSA MIR；在表达式树稳定前不要发明第三套 IR。

## 入口规则（统一后）

一个入口文件恰好有一种入口：

- `fn main(...) => ...`（函数名为 `main`），或
- `main(callee(...))`（`Statement::Main`）

两者都算 entry。冲突（两种同时出现，或两个 `fn main`）为错误。测试里的 `fn main() => int:` 在工程模式下必须能编。

## 成功标准

- `tests/frontend_parity_tests.rs` 单文件与工程对同一 `fn main` 夹具均 exit 0；`--emit-llvm` 在两种模式下都写出 `.ll`。
- `compile_expression_str` / `compile_body_line` 从 **生产路径** 删除（测试里可暂留 `#[cfg(test)]` 对照，但默认编译不用）。
- 现有 `cargo test` 不比改造前差；E700 枚举构造可另开任务，不作为本阶段门禁（已 `#[ignore]`）。
- 无语言关键词变更。

## 执行顺序

Phase 1 驱动与管线 → Phase 2 AST 补全 → Phase 3 codegen 吃树 →（可选）Typed HIR 挂钩。内存系统在这之后另写计划。
