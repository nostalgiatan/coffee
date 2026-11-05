# 命令行参考:`coffee2`

```
coffee2 input.cf [-o output] [-I dir] [-O0|-O1|-O2|-O3] [--emit-llvm] [--emit-obj] [--mc] [--emit-flat]
```

`coffee2` 是自举编译器(`coffee/driver/src/main.cf`)。它只做编译。项目模式(`coffee.toml`)、依赖获取和 `std install` 不在这里,它们留给以后的工具链程序。

## 选项

| 选项 | 含义 |
|---|---|
| `input.cf` | 源文件。必须有且只有一个。 |
| `-o output` | 输出路径。省略时见下表。 |
| `-I dir` | 增加一个模块搜索目录和 `.cfc` 搜索目录,可重复。 |
| `-O0` | 关闭 IR 优化器,交给 `clang -O0`。 |
| `-O1` `-O2` `-O3` | 打开 IR 优化器;数字同时传给 `clang`。默认 `-O1`。 |
| `--emit-llvm` | 写出 LLVM 文本后停止,不调用 `clang`。 |
| `--mc` | 用 x86-64 机器码后端代替 LLVM 文本,再用 `clang` 链接。 |
| `--emit-obj` | 等于 `--mc`,但写出 `.o` 后停止。 |
| `--emit-flat` | 写出展开了所有 `use` 的单文件源码后停止。诊断里的行号指的是这份文本。 |

## 默认输出

| 选项 | 没有 `-o` 时 |
|---|---|
| 无 | `a.out` |
| `--emit-llvm` | `<input>.ll` |
| `--emit-obj` | `<input>.o` |
| `--emit-flat` | `<input>.flat` |

## 模块与 C 库

导入的 Coffee 模块先在源文件所在目录找,再依次在各 `-I` 目录找。模块文本放在导入它的文件前面,每个模块只放一次,`use` 行去掉;`use ... of c` 行保留。

libc 和 libm 的签名内置在编译器里。其他 C 库要有 `<库名>.cfc`,在源文件目录、其 `lib/`、`-I` 目录或其 `lib/` 里。链接命令带上找到它的目录的 `-L` 和 rpath,以及 `-l<库名>`。

## 退出码

| 值 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 前端报错、不能翻译的函数、链接失败 |
| 2 | 命令行用法错误 |

## 限制

- 只在 Linux 和 Android 上能用:参数从 `/proc/self/cmdline` 读取。
- 机器码后端只支持 x86-64。
- 诊断只有错误码和展开后源码的行号,没有文件名。
- 链接固定调用 `clang`。

## 相关

- 操作步骤:[构建编译器](../how-to/build-the-compiler.md)、[链接 C 库](../how-to/link-a-c-library.md)、[生成机器码](../how-to/emit-machine-code.md)
