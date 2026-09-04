# Coffee 编译器健康地图

日期：2026-09-04  
HEAD：`2cbc64f`

类型检查与 C 句柄已合入。错误码 E100–E112 的诊断改动可能仍在工作区未提交。

Coffee 赋值单向 coerce；`object` 是 C 不透明指针。`malloc`/`free` 等不再用 `int(4)+` 截断指针。
