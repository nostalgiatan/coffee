# 模块边界与代理文件租约

日期：2026-09-04  
目的：并行子代理不要再挤在 `codegen.rs` / `expressions.rs` / `analyzer.rs` 上。

## 优先级（Task 7–9 完成之后）

0. **立刻拆分** `codegen.rs` / `expressions.rs`（计划：`docs/superpowers/plans/2026-09-04-split-after-task7.md`）。未拆之前不要派 E700 / Pattern / 内存。
1. 工程 `.o` 走完整 `CompilationPipeline`（driver 租约，可与拆分的 S2 并行）。
2. E700 枚举构造（codegen-expr，拆完 `expr/` 之后）。
3. 类型检查补全（types）。
4. match `Pattern` 枚举（parser + `match_gen.rs`）。
5. 内存契约 / C 句柄（c-builtins，另开 spec）。

## 规则

同一时刻 **一类文件只允许一个代理写入**。读可以交叉。合并前用 `git status` 确认租约。

## 当前租约（目录级）

| 租约名 | 可写路径 | 典型任务 |
|--------|----------|----------|
| **driver** | `src/main.rs`, `src/compiler/**` | 入口、emit、pipeline、工程 `.o` |
| **parser** | `src/parser/**` | AST 形状、`Pattern` 枚举 |
| **sema** | `src/semantic/analyzer.rs` | 语句语义、作用域 |
| **c-builtins** | `src/semantic/c_builtins.rs`, `src/c/**` | libc 表、`.cfc`、malloc 句柄 |
| **types** | `src/types/**` | 类型检查补全、方法签名 |
| **codegen-coord** | `src/backend/codegen.rs` | **仅** `CodeGenerator` 字段、`compile_program` |
| **codegen-expr** | `src/backend/expressions.rs` | `compile_expr` / 删字符串路径 |
| **codegen-stmt** | `src/backend/statements.rs`, `variables.rs`, `memory_ops.rs` | let/assign/rm |
| **codegen-cf** | `src/backend/control_flow.rs`（目标：if/while/for） | 控制流 |
| **codegen-match** | 目标：`src/backend/match_gen.rs`（尚未拆出，现仍在 `codegen.rs`） | match IR |
| **codegen-fn** | `src/backend/functions.rs`, `classes.rs` | 调用、类 |
| **tests-parity** | `tests/frontend_parity_tests.rs`, `tests/common/mod.rs` | 单文件 vs 工程 |
| **tests-lang** | `tests/*_tests.rs` 其余 | 语言行为 |
| **docs** | `docs/**`, `CLAUDE.md`, `SYNTAX.md`, `IFLOW.md` | 文档 |

互斥：`codegen-coord` 与 `codegen-match` 在 **match 未拆出之前不能并行**（都写 `codegen.rs`）。Task 7 结束前不要再派任何 `codegen-*`。

## 目标拆分（Task 7 落地后做，一次一文件）

`codegen.rs` 里仍堆着 `compile_if` / `while` / `for` / `match` / `function` / `main_entry`。`control_flow.rs` 现在几乎只有 `LoopContext`。拆完后：

- `control_flow.rs` ← `compile_if`, `compile_while`, `compile_for`
- `match_gen.rs` ← `compile_match`
- `functions.rs` 或 `codegen.rs` 薄封装 ← `compile_function`, `compile_main_entry`
- `codegen.rs` 只保留结构体、`compile_program*`、公共 helper（`expr_to_legacy_str` 应已删除）

`expressions.rs`（~2500 行）再按变体拆到 `src/backend/expr/{literal,binary,call,member,index}.rs`，由 `expr/mod.rs` 的 `compile_expr` 分发。

## 已做的拆分

- `src/semantic/c_builtins.rs`：libc/libm 内置表从 `analyzer.rs` 挪出。改 malloc 签名的代理只租 **c-builtins**，不碰 `analyze_statement`。
