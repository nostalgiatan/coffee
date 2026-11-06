# IR 对象格式(.cir)

状态:已实现,在 `coffee/ir/src/cir.cf`,往返测试在 `coffee/ir/tests/test_cir.cf`。设计动机见 [JIT 后端的设计](../explanation/jit-design.md)。

## 约定

- 小端。除非另说明,整数用 zigzag 加 LEB128(`varint`):`zz(v) = (v << 1) ^ (v >> 63)`,每字节低 7 位有效,最高位表示后面还有。
- `bytes(n)` 是 `n` 个原始字节。
- 一个文件是头加若干节,节按下表顺序出现,没有对齐填充。

## 头

| 字段 | 类型 | 含义 |
|---|---|---|
| magic | bytes(4) | `CIR1` |
| flags | varint | 位 0:IR 已优化。位 1:带 `main` |
| nsec | varint | 节数 |
| 每节: tag, len | bytes(4), varint | 节标签和字节长度 |
| sum | bytes(4) | 以上所有节内容的 FNV-1a(32 位) |

## 节

标签四个字符,读入时按标签找,不认识的标签跳过(长度在头里),所以老读者能读新文件里它认识的部分。

### `NAME`:名字表

`count`(varint),然后每个名字 `len`(varint) 加 `bytes(len)`。名字号是它在表里的序号,从 0 起。它是调用目标和函数名字号的值域。

### `REPS`:聚合表示

`count`(varint),每项 `k`(varint) 加 `k` 个 varint(元素表示)。第 `i` 项的表示号是 `8 + i`。0 到 7 是标量,不入表。

### `STRS`:字符串

`len`(varint) 加 `bytes(len)`。内容与 `sbytes` 相同:每个字符串是 8 字节小端长度、正文、一个 NUL。读入时扫描一遍重建每个字符串的偏移。

### `GLOB`:全局变量

`count`(varint),每项 `name`(varint,名字号) 加 `size`(varint)。

### `FUNS`:函数

`count`(varint),然后每个函数:

| 字段 | 含义 |
|---|---|
| id | 名字号,未知为 -1(编码时加 1) |
| ret | 返回表示 |
| ni, nb, ne, np | 指令数、块数、边数、池大小 |
| nm | 名字长度加字节 |
| 列 | 见下 |

列的顺序固定:

1. 指令列(各 `ni` 项),顺序 `op rep ty a b c aux nx pv uses fwd blk`,每项 zigzag varint。`nx` 和 `pv` 存与自己下标的差(链接大多指向相邻指令),`blk` 存与前一项的差,其余原样。
2. 块列(各 `nb` 项):`bfirst blast pfirst plast npar`。
3. 边列(各 `ne` 项):`efrom eto eargs eargn`。
4. 池(`np` 项)。

`uses` 照存(值很小,一个字节),省得读入后再数一遍。`vn`、`attrs`、`f_fx`、`f_done` 不入盘,`restore` 会重置它们。

## 读入时的检查

读入器在使用前验证,任一失败返回错误,不执行:

- 魔数、节长度之和等于文件长度、校验和相符。
- 所有名字号 < `NAME.count`,表示号 < `8 + REPS.count`,字符串号在 `STRS` 里。
- 每个函数的 `ni nb ne np` 不超过剩余字节数(防止构造巨大分配)。
- `nx`、`pv`、`bfirst`、`blast` 在 -1 到 `ni` 之间,边的端点在 -1 到 `nb` 之间(-1 是被删掉的边)。块链上的指令都属于本函数的某个块,链不成环。链外的指令是死的,可以带任何值,不检查。
- 当 `flags` 为已优化时,可以再运行 `verify`(`--verify` 打开),发现问题则拒绝。

## 与 `Lib` 的对应

`FUNS` 的每个函数对应 `Lib.save` 存入的一行,`id` 对应 `f_id`,`ret` 对应 `f_ret`。读入就是反过来对每个函数调用 `Lib` 的列写入,再重建 `Names`、`Reps`、`sbytes`、`gl_*`。`ext_slot`(其他文件的小函数,只为内联)不入盘:要写的是整个模块优化后留下的函数。
