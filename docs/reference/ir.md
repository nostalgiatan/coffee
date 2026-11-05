# IR 参考

`coffee/ir` 的中间表示是带块参数的类型化 SSA,没有 phi。一个函数是一个 `Fun`,所有信息存成并列的整数列。设计动机见 [IR 的设计](../explanation/ir-design.md)。

## 指令的列

每条指令有一个编号 `i`,下面各列用 `i` 做下标:

| 列 | 含义 |
|---|---|
| `op` | 操作码(`Op`) |
| `rep` | 结果的表示(`Rp`) |
| `ty` | 附加的类型信息。`Alloc` 和 `Slot` 里是字节数。 |
| `a` `b` `c` | 操作数,具体含义由操作码决定 |
| `aux` | 附加整数:常量值、字段偏移、比较谓词、被调用函数号、边号 |
| `nx` `pv` | 同一个块里的下一条、上一条 |
| `uses` | 使用次数,每个优化遍都保持正确 |
| `fwd` | 被替换成另一条指令时指向它 |
| `blk` | 所在的块 |

块有 `bfirst` `blast`(指令链)和 `pfirst` `plast` `npar`(块参数链)。边存在 `efrom` `eto` `eargs` `eargn` 里,边上传的参数放在 `pool`。

## 表示 `Rp`

标量:`Void` `I8` `I16` `I32` `I64` `F32` `F64` `Ptr`。聚合(切片、`buf<T>`、元组)是编号不小于 `rp_first_agg()` 的表示,按元素列表驻留。

## 操作码

| 组 | 操作 |
|---|---|
| 值 | `Param` `Const` `StrLit` `Undef` |
| 整数运算 | `Add` `Sub` `Mul` `SDiv` `UDiv` `SRem` `URem` `And` `Or` `Xor` `Shl` `AShr` `LShr` `Neg` `Not` |
| 浮点运算 | `FAdd` `FSub` `FMul` `FDiv` `FRem` `FNeg` |
| 比较 | `Cmp`(整数,谓词 `Eq` `Ne` `SLt` `SLe` `SGt` `SGe` `ULt` `ULe` `UGt` `UGe`) `FCmp`(谓词 `OEq` `ONe` `OLt` `OLe` `OGt` `OGe`) |
| 转换 | `Trunc` `SExt` `ZExt` `FpToSi` `FpToUi` `SiToFp` `UiToFp` `FpExt` `FpTrunc` `Bitcast` `PtrToInt` `IntToPtr` |
| 聚合 | `Extract` `Insert` |
| 内存 | `Slot` `Load` `Store` `Field` `Index` |
| 调用 | `Call`(Coffee 函数) `CCall`(C 函数) `CallI`(间接) |
| 所有权 | `Alloc` `Free` `Drop` `Clone` `OwnMove` |
| 终结 | `Br` `CondBr` `Switch` `Ret` `Unreachable` |

操作的性质(纯、可交换、读、写、终结、消耗、陷阱、分配)存在 `OpTab` 里。打印、验证、折叠都查这张表。

## 与所有权相关的操作

- `Alloc`:`ty` 是字节数,结果是 `Ptr`。
- `Free`:操作数 `a` 是指针。`aux` 是块大小加一,0 表示大小未知。后端可以忽略它。
- `Drop`、`Clone`:`aux` 是胶水函数号(`drop.N`、`clone.N`)。
- `Slot`:栈上的一块,`ty` 是字节数。可以出现在任何块里;LLVM 后端把它们提到入口块。

## 优化器

`opt.cf`(单函数):

| 遍 | 作用 |
|---|---|
| `fold_branches` | 条件恒定的 `CondBr` 变成 `Br` |
| `drop_unreachable` | 删除到不了的块 |
| `merge_blocks` | 只有一个前驱的块并入前驱 |
| `own_opt` | 只被紧随其后的 `Free` 使用的 `Alloc` 一起删除;`y = clone x` 后立刻 `drop x` 时 `y` 就是 `x` |
| `stack_alloc` | 在同一块内分配又释放,中间只有 `Load` `Store` `Field` `Index` 的 `Alloc` 改成 `Slot` |
| `dead_code` | 无人使用的指令和块参数 |

`modopt.cf`(整个模块,被调用者先处理):内联、每个函数的效应摘要(读、写、分配、未知、不返回、可能不终止)、删掉结果无人使用且只读会终止的调用、由摘要推出的函数属性。

这些遍都作用在 IR 的列上,不碰 LLVM 文本。

## 验证器

`verify.cf` 检查:每个块以终结指令结束,边的参数个数和类型,操作的类型,支配关系。优化遍之后会再运行一次。

## 相关

- 阶段与目录:[编译流水线参考](pipeline.md)
