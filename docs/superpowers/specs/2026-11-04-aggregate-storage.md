# 聚合值的存储：class 与枚举都是堆上的拥有指针（2026-11-04）

状态：**提案，未实现。** 与 [embedded-vm-direction](2026-11-04-embedded-vm-direction.md) 的 D4/D5 一致。

## 问题（2026-11-04 实测）

class 实例和带 payload 的枚举值在 IR 里已经是 `ptr`，但**存储是栈上的 `alloca`**，函数返回时指针就悬空了。

```
fn mk() => P:
    return P { x: 1, y: 2 }
fn main() => int:
    let a: P = mk()
    return sum(a)        /#/ 期望 3
```

`mk` 的 IR 是 `%P_literal = alloca %P ... ret ptr %P_literal`。同一个可执行文件连续运行四次，退出码是 49、81、97、193。带 payload 的枚举（`emit_enum_variant_call`）同理，已记为被忽略的测试 `test_enum_returned_from_function`。

这类 bug 没被抓到，是因为多数测试只断言"能编译"。

## 决定

1. **标量按值**：`int` / `float` / `bool` 以及只含标量的元组、定长数组，仍是值。
2. **聚合是拥有指针**：class 实例、带 payload 的枚举值，构造时在堆上分配，值本身就是指向该块的指针。这是现状的表示（`ptr`），改变的只是存储位置。
3. **所有权沿用 Plan B**：这些类型已经是 resource（`Type::is_resource` 对 `NamedType` 返回 true），不能用 `=` 复制，要 `mv` / `clone`，最后一次使用可隐式 move。`mv` 只搬指针。
4. **drop 负责释放**：作用域结束、`rm`、被 `mv` 走之前的覆盖，都由 drop glue 先递归 drop 字段，再 `free` 该块。
5. **只含单元变体的枚举**（如 `Color`）继续是整数 tag，不分配。

## 为什么是这个形状

- **统一表示**：IR 或解释器只需要"标量"和"指针"两类值，ABI 简单，对"体积最小"的 VM 有利。
- **没有 GC 和引用计数**：单一所有者保证释放时机确定，也不会出现环。
- **递归类型自然成立**：AST 这类 `enum Expr: Add(Expr, Expr)` 的字段就是指针。
- **分配策略成为优化**：语义不变，之后可以用逃逸分析把"证明不逃逸"的值放回栈上，对应 D5（证明成立就编译成最直接的访问）。

## 不做什么

- 不把标量也装箱。每个 `int` 一次分配会毁掉性能，也让优化器的工作变复杂。
- 不引入引用计数或 GC。

## 已知代价与风险

| 项 | 说明 | 处理 |
|---|---|---|
| 每次构造一次分配 | 小对象开销明显 | 后续用逃逸分析回到栈上；嵌入式场景换成自带的 bump / 线性内存分配器 |
| 依赖 `malloc`/`free` | 与沙箱方向（D6）冲突 | 分配接口收敛到一处，沙箱阶段换成线性内存分配器 |
| 深链表的递归 drop | 很长的链会栈溢出 | drop glue 对递归字段改成循环，或设深度上限；先记为风险，实现时验证 |
| 现有 `is_heap_allocated` 标志 | 目前只有走 `malloc` 的局部才被释放，其余当栈对象 | 聚合构造统一走堆后，这个标志对 class / 枚举不再需要，要整理其引用点（`statements.rs`、`memory_ops/compile.rs`、`context.rs`） |
| 现有测试可能编码了栈语义 | 例如对 class 局部 `rm` 的预期 | 改动前先跑全量，逐个区分"测试错了"和"实现错了" |
| 内存泄漏 | 构造改堆后，漏掉 drop 的路径就会泄漏 | 加运行测试，用 `--show-memory` 或统计 `malloc`/`free` 次数断言配平 |

## 实施顺序（每步先写会失败的运行测试）

1. **返回 class**：`mk()` 返回 `P { .. }` 后 `sum(a)` 得 3；连续多次运行结果稳定。
2. **返回枚举**：取消 `test_enum_returned_from_function` 的 `#[ignore]`。
3. **配平**：构造数与释放数相等（含作用域结束、`rm`、`mv` 之后、`return` 提前退出）。
4. **嵌套**：class 字段是 class / 枚举，枚举 payload 里是 class / 枚举，drop 递归释放。
5. **清理 `is_heap_allocated`** 对聚合类型的特判。

## 未决

- class 字段里的聚合是内嵌值还是指针。本文默认**指针**（与"聚合都是拥有指针"一致），内嵌是以后的优化。
- 带 payload 的枚举，每个变体按各自大小分配，还是统一按最大变体分配。默认按各自大小。
- `clone` 对枚举的深拷贝规则，沿用 `src/backend/memory_ops/clone.rs` 的 class 路径，需要确认。
