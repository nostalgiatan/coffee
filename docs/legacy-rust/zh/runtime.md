# 运行时

**没有** Coffee 运行时 crate（`src/runtime/` 从未接入，已删除）。中止/`raise` 是代码生成 + libc，不是 `coffee_panic` 库。

实际发生的事：

- **`raise`：** LLVM 调 libc 的 `fprintf` / `exit`，或函数上的 `#监听器`。见 `src/backend/mir_raise.rs`。
- **`declare_runtime_functions`：** 后端可能调用的 C 符号的 LLVM 声明（`puts`、`exit`、`malloc`、`free` 等）。用户源码里直接写 libc 名仍须 `use … in libc of c`；一般程序用 **std**（`use print in std`）。C 边界在 `library/std/src/sys.cf`。
- **官方 std：** `library/std` 的 Coffee 源码，嵌在 CLI 里（`coffee std install`），不是 Rust 模块。`IntBuf` 在 `mem`。除非 toml 写了 `[dependencies.packages.std]`，编译会前置 std 导入根。

**`buf`：** 拥有的 malloc 指针；drop/`rm` 会 `free`。`object` 载荷不 `free`。

**Last-use 搬走：** `src/types/last_use.rs`（检查器），再走现有 `mv` 代码生成。

**复合 drop：** 定长资源数组 `[T; N]` 与元组里的资源元素随容器 drop。class 的 `[T]` 切片字段按胖指针长度循环 drop 元素，不 `free` 缓冲区。

**深 clone：** 类字段里的嵌套 `str` / class / 资源数组 / 元组会再 clone；`object`、引用、切片字段仍浅拷贝。见 `src/backend/memory_ops/clone.rs`。`clone buf` 是类型错误。
