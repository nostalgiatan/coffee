# AST 表达式不再用字符串（第一波）

日期：2026-09-04  
约束：不改 `mv`/`clone`/`rm`/`copy`/`clean` 语义；不发明 try/catch。

## 问题

解析器已经能产出 `Expression`，但 `let` / `return` / `raise` 仍把右值存成 `String`。语义分析、类型检查、codegen 再 `parse_expression` 一次。失败时走字面量或 `compile_source_as_expr`，AST 前后两套表达式世界。

## 做法

1. `VariableDecl.value`、`ReturnStmt.value`、`RaiseStmt.error_expr` 改为 `Expression`（`Option<Expression>`）。解析时 slice 完立刻 `expr::parse_expression`。
2. analyzer / type checker / codegen 只 `analyze_expression` / `check_expression` / `compile_expr`。
3. 数组 `let` 的 `[…]` 走 `Expression::ArrayLiteral`，不再对 initializer 做字符串切分（`parse_array_literal` 仅作测试遗留）。
4. 下一波（本 spec 不一次做完）：删 `#[cfg(test)] compile_expression_str`、字段访问里的 `compile_source_as_expr`、把 `expr.rs` 切片解析换成与 statement 相同的 nom 下降。

## 成功标准

`let x: int = 1 + 2` 从 parse 到 LLVM 只出现一棵 `Expression` 树。`pattern_matching_tests`、`class_tests`、`control_flow_tests`、`type_check_tests`、`functions_tests` 全绿。
